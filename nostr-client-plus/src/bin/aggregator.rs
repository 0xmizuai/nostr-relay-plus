use anyhow::Result;
use mongodb::{Client as DbClient, Collection};
use nostr_client_plus::client::Client;
use nostr_client_plus::db::RawDataEntry;
use nostr_client_plus::event::UnsignedEvent;
use nostr_client_plus::job_protocol::{AggregatePayload, Kind, ResultPayload};
use nostr_client_plus::utils::get_timestamp;
use nostr_crypto::eoa_signer::EoaSigner;
use nostr_crypto::sender_signer::SenderSigner;
use nostr_plus_common::relay_event::RelayEvent;
use nostr_plus_common::relay_message::RelayMessage;
use nostr_plus_common::sender::Sender;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing_subscriber::FmtSubscriber;

mod utils;
use crate::utils::{get_private_key_from_name, get_single_tag_entry};

type AggrMessage = (String, (Sender, ResultPayload));

#[tokio::main]
async fn main() {
    // Command line parsing
    let args: Vec<String> = std::env::args().collect();
    let relay_url = match args.len() {
        1 => String::from("ws://127.0.0.1:3033"),
        2 => args[1].clone(),
        _ => {
            eprintln!("Too many arguments");
            return;
        }
    };

    // Logger setup
    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    // Configure DB from env
    let db_url = std::env::var("MONGO_URL").expect("MONGO_URL is not set");
    let db = DbClient::with_uri_str(db_url)
        .await
        .expect("Cannot connect to db")
        .database("test-preprocessor");
    let collection: Collection<RawDataEntry> = db.collection("tmp_results");

    // Get a silly private key based on a string identifying the service.
    let private_key = get_private_key_from_name("Aggregator").unwrap();

    // Create client
    let signer = EoaSigner::from_bytes(&private_key);
    let mut client = Client::new(SenderSigner::Eoa(signer));
    let mut relay_channel = client
        .connect_with_channel(relay_url.as_str())
        .await
        .unwrap();

    // Build a pipeline through channels: listener -> aggregator
    let (aggr_tx, mut aggr_rx) = mpsc::channel::<AggrMessage>(100);

    /*
     * Results listener
     */
    let listener_handler = tokio::spawn(async move {
        tracing::info!("Results listener started");

        while let Some(msg) = relay_channel.recv().await {
            match msg {
                RelayMessage::Event(ev) => match ev.event.kind {
                    Kind::RESULT => {
                        if let Ok(payload) = get_result_payload(&ev) {
                            if let Ok(Some(job_id)) = get_single_tag_entry('e', &ev.event.tags) {
                                let sender = ev.event.sender;
                                let value = (job_id, (sender, payload));
                                if aggr_tx.send(value).await.is_err() {
                                    tracing::error!("Cannot send to aggregator task");
                                }
                            } else {
                                tracing::error!("invalid tag");
                            }
                        } else {
                            tracing::error!("invalid content");
                        }
                    }
                    _ => tracing::error!("Wrong kind of event received"),
                },
                _ => tracing::info!("Non-Event message: ignored"),
            }
        }
        tracing::error!("Error while listening for relay events: task has exited");
    });

    /*
     * Results aggregator
     */
    let aggregator_handler = tokio::spawn(async move {
        tracing::info!("Results aggregator started");

        // keep track of results for matching
        let mut aggr_book: HashMap<String, (Vec<Sender>, ResultPayload)> = HashMap::new();

        while let Some((job_id, (sender, new_payload))) = aggr_rx.recv().await {
            match aggr_book.entry(job_id.clone()) {
                Entry::Occupied(mut entry) => {
                    let (workers, payload) = entry.get_mut();
                    if *payload != new_payload {
                        // ToDo: here the logic can be complicated. For example, how do we
                        //  handle resolutions now? Do we need a separate book to handle
                        //  controversies?
                        tracing::error!("payloads for {} are different", job_id);
                        continue;
                    }

                    // Ok, they are the same
                    if workers.len() == 2 {
                        // the current one will be the 3rd one, so we can remove the entry and
                        // send it for publishing.
                        // ToDo: it's better to publish from here and then remove, in case publish
                        //  fails. But how do we handle publishing failures? How do they get
                        //  rewarded?
                        workers.push(sender);
                        let ev =
                            get_aggregation_event(workers, new_payload, &job_id, client.sender());
                        if client.publish(ev).await.is_err() {
                            tracing::error!("Cannot publish Aggr");
                        }
                        entry.remove(); // ToDo: should we remove if publish failed?
                    } else {
                        workers.push(sender);
                    }
                }
                Entry::Vacant(entry) => {
                    entry.insert((vec![sender], new_payload));
                }
            }
        }
    });

    listener_handler.await.unwrap();
    aggregator_handler.await.unwrap();
    tracing::info!("Shutting down gracefully");
}

fn get_result_payload(ev: &RelayEvent) -> Result<ResultPayload> {
    let res: ResultPayload = serde_json::from_str(ev.event.content.as_str())?;
    Ok(res)
}

fn get_aggregation_event(
    workers: &Vec<Sender>,
    payload: ResultPayload,
    job_id: &String,
    this_sender: Sender,
) -> UnsignedEvent {
    let content = AggregatePayload {
        header: payload.header,
        winners: workers.clone(),
    };
    UnsignedEvent::new(
        this_sender,
        get_timestamp(),
        Kind::AGG,
        vec![vec!["e".to_string(), hex::encode(job_id)]],
        serde_json::to_string(&content).expect("content serialization failed"),
    )
}
