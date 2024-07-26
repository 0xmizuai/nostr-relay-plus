use anyhow::Result;
use mongodb::{Client as DbClient, Collection};
use nostr_client_plus::client::Client;
use nostr_client_plus::db::FinishedJobs;
use nostr_client_plus::job_protocol::{Kind, ResultPayload};
use nostr_client_plus::request::{Filter, Request};
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

// const N_WINNERS: usize = 3;

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    // Command line parsing
    let relay_url = std::env::var("RELAY_URL").unwrap_or("ws://127.0.0.1:3033".to_string());

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
        .database("mine");
    let collection: Collection<FinishedJobs> = db.collection("finished_jobs");

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
                    tracing::debug!("Another result for job {} found", job_id);
                    let (workers, payload) = entry.get_mut();

                    if *payload != new_payload {
                        // ToDo: here the logic can be complicated. For example, how do we
                        //  handle resolutions now? Do we need a separate book to handle
                        //  controversies?
                        tracing::error!("payloads for {} are different", job_id);

                        // ToDo: Currently we handle disputes by deleting the entry altogether.
                        //  If this was already at the second result, another one will come and
                        //  linger in the book because no other matches are expected to come.
                        //  Another approach needs to decided.
                        entry.remove();
                        continue;
                    }

                    // Ok, they are the same
                    workers.push(sender);
                    // We allow the case of len > N_WORKERS for cases where there are issues
                    // writing to the db or removing from book.
                    // The first N_WINNERS are still the same in the DB, regardless.

                    // TODO: parse JobType
                    let job_type = new_payload.header.job_type;
                    let n_winners = match job_type {
                        0 => 1,
                        1 => 3,
                        _ => unreachable!()
                    };

                    if workers.len() >= n_winners {
                        tracing::debug!("All the results for {} are consistent, sending", job_id);
                        let raw_data_id = new_payload.header.raw_data_id.clone();
                        let db_entry = FinishedJobs {
                            _id: raw_data_id,
                            workers: workers.clone(), // if we remove first, no need to clone but this is safer
                            timestamp: get_timestamp(),
                            result: new_payload,
                            job_type,
                        };
                        match collection.insert_one(db_entry, None).await {
                            Ok(_) => {
                                entry.remove(); // Remove only if we know it's safe in db
                            }
                            Err(err) => {
                                // We log and that's it, we keep the entry in book for later inspection
                                tracing::error!("Cannot write finished job to db: {}", err);
                            }
                        }
                    }
                }
                Entry::Vacant(entry) => {
                    tracing::debug!("First time we see job {}", job_id);
                    entry.insert((vec![sender], new_payload));
                }
            }
        }
    });

    // Send subscription, so everything will finally start
    let sub_id = "be4788ade0000000000000000000000000000000000000000000000000001111";
    let filter = Filter {
        kinds: vec![Kind::RESULT],
        since: Some(get_timestamp()),
        ..Default::default()
    };
    let req = Request::new(sub_id.to_string(), vec![filter]);
    client
        .subscribe(req)
        .await
        .expect("Cannot subscribe for Results");
    tracing::info!("Subscribed to Result");

    listener_handler.await.unwrap();
    aggregator_handler.await.unwrap();
    tracing::info!("Shutting down gracefully");
}

fn get_result_payload(ev: &RelayEvent) -> Result<ResultPayload> {
    let res: ResultPayload = serde_json::from_str(ev.event.content.as_str())?;
    Ok(res)
}
