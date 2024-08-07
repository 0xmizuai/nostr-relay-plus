use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use mongodb::{Client as DbClient, Collection};
use nostr_client_plus::__private::config::{load_config, AggregatorConfig};
use nostr_client_plus::__private::metrics::get_metrics_app;
use nostr_client_plus::client::Client;
use nostr_client_plus::db::FinishedJobs;
use nostr_client_plus::job_protocol::{Kind, ResultPayload};
use nostr_client_plus::request::{Filter, Request};
use nostr_crypto::eoa_signer::EoaSigner;
use nostr_crypto::sender_signer::SenderSigner;
use nostr_plus_common::relay_event::RelayEvent;
use nostr_plus_common::relay_message::RelayMessage;
use nostr_plus_common::sender::Sender;
use prometheus::{IntCounter, Registry};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing_subscriber::FmtSubscriber;

mod utils;
use crate::utils::{get_private_key_from_name, get_single_tag_entry};

type AggrMessage = (String, (Sender, ResultPayload));

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::default();
    pub static ref FINISHED_JOBS: IntCounter =
        IntCounter::new("finished_jobs", "Finished Jobs").expect("Failed to create finished_jobs");
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    dotenv::dotenv().ok();
    // Command line parsing
    let relay_url = std::env::var("RELAY_URL").unwrap_or("ws://127.0.0.1:3033".to_string());
    let supported_version = std::env::var("VERSION").unwrap_or("v0.0.1".to_string());

    // Command line parsing
    let args: Vec<String> = std::env::args().collect();
    let config_file_path = match args.len() {
        1 => {
            return Err(anyhow!("Missing config file path"));
        }
        2 => args[1].clone(),
        _ => {
            return Err(anyhow!("Too many arguments"));
        }
    };

    // Get configuration from file
    let config: AggregatorConfig = load_config(config_file_path, "aggregator")?.try_into()?;

    // Logger setup
    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::DEBUG)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    // Metrics
    register_metrics();
    let socket_addr = config.metrics_socket_addr;
    let shared_registry = Arc::new(REGISTRY.clone());
    let (router, listener) = get_metrics_app(shared_registry, socket_addr.as_str()).await;
    let metrics_handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

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
                RelayMessage::Ok(ok_msg) => {
                    if ok_msg.accepted {
                        tracing::debug!("Event {} accepted", hex::encode(ok_msg.event_id))
                    } else {
                        tracing::error!(
                            r#"Event {} rejected: "{}""#,
                            hex::encode(ok_msg.event_id),
                            ok_msg.message
                        );
                    }
                }
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
            // TODO: parse JobType
            let job_type = new_payload.header.job_type;
            let n_winners = match job_type {
                0 => 1,
                1 => 3,
                _ => unreachable!(),
            };

            if new_payload.version != supported_version {
                tracing::error!("Version mismatch");
                continue;
            }

            if n_winners <= 1 {
                tracing::debug!("The job needs only one worker! {}", job_id);
                let raw_data_id = new_payload.header.raw_data_id.clone();
                let db_entry = FinishedJobs {
                    _id: raw_data_id,
                    workers: vec![sender.clone()], // if we remove first, no need to clone but this is safer
                    timestamp: chrono::Utc::now().timestamp() as u64,
                    result: new_payload.clone(),
                    job_type,
                };
                match collection.insert_one(db_entry, None).await {
                    Ok(_) => {
                        FINISHED_JOBS.inc();
                    }
                    Err(err) => {
                        // We log and that's it, we keep the entry in book for later inspection
                        tracing::error!("Cannot write finished job to db: {}", err);
                    }
                }
                continue;
            }

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

                    if workers.len() >= n_winners {
                        tracing::debug!("All the results for {} are consistent, sending", job_id);
                        let raw_data_id = new_payload.header.raw_data_id.clone();
                        let db_entry = FinishedJobs {
                            _id: raw_data_id,
                            workers: workers.clone(), // if we remove first, no need to clone but this is safer
                            timestamp: chrono::Utc::now().timestamp() as u64,
                            result: new_payload,
                            job_type,
                        };
                        match collection.insert_one(db_entry, None).await {
                            Ok(_) => {
                                FINISHED_JOBS.inc();
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
        since: Some(chrono::Utc::now().timestamp() as u64),
        ..Default::default()
    };
    let req = Request::new(sub_id.to_string(), vec![filter]);
    client
        .subscribe(req, Some(120)) // in case of reconnect, get messages 2 mins old
        .await
        .expect("Cannot subscribe for Results");
    tracing::info!("Subscribed to Result");

    listener_handler.await.unwrap();
    aggregator_handler.await.unwrap();
    metrics_handle.await.unwrap();
    tracing::info!("Shutting down gracefully");
    Ok(())
}

fn get_result_payload(ev: &RelayEvent) -> Result<ResultPayload> {
    let res: ResultPayload = serde_json::from_str(ev.event.content.as_str())?;
    Ok(res)
}

fn register_metrics() {
    REGISTRY
        .register(Box::new(FINISHED_JOBS.clone()))
        .expect("Cannot register finished_jobs");
}
