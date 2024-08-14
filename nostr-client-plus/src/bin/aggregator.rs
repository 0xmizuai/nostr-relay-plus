use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use mongodb::bson::doc;
use mongodb::{Client as DbClient, Collection};
use nostr_client_plus::__private::config::{load_config, AggregatorConfig};
use nostr_client_plus::__private::metrics::get_metrics_app;
use nostr_client_plus::client::Client;
use nostr_client_plus::db::FinishedJobs;
use nostr_client_plus::job_protocol::{Kind, ResultPayload};
use nostr_client_plus::redis::RedisEventOnWire;
use nostr_client_plus::request::{Filter, Request};
use nostr_crypto::eoa_signer::EoaSigner;
use nostr_crypto::sender_signer::SenderSigner;
use nostr_plus_common::binary_protocol::BinaryMessage;
use nostr_plus_common::relay_event::RelayEvent;
use nostr_plus_common::relay_message::RelayMessage;
use nostr_plus_common::sender::Sender;
use nostr_plus_common::wire::EventOnWire;
use prometheus::{IntCounter, Registry};
use redis::{Commands, Connection, RedisError};
use sha2::{Digest, Sha512};
use std::collections::hash_map::Entry;
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tracing_subscriber::FmtSubscriber;

mod utils;
use crate::utils::{get_private_key_from_name, get_single_tag_entry};

type AggrMessage = (String, (Sender, ResultPayload, String));

const MONITORING_INTERVAL: Duration = Duration::from_secs(120);

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::default();
    pub static ref FINISHED_JOBS: IntCounter =
        IntCounter::new("finished_jobs_total", "Total Finished Jobs")
            .expect("Failed to create finished_jobs");
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

    // Check assigner_whitelist is not empty and then create a set out of it
    if config.assigner_whitelist.is_empty() {
        return Err(anyhow!("assigner_whitelist is empty"));
    }
    let assigner_whitelist: BTreeSet<Sender> = config
        .assigner_whitelist
        .into_iter()
        .map(|s| hex::decode(s).expect("Cannot decode"))
        .map(|s| Sender::from_bytes(&s).expect("Cannot convert from bytes"))
        .collect();

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

    // Configure Redis
    let redis_url = std::env::var("REDIS_URL").expect("REDIS_URL is not set");
    let redis_client = redis::Client::open(redis_url)?;
    let mut redis_con = redis_client.get_connection()?;

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
                    // Save 6002 event, for later 6003 verify
                    Kind::ASSIGN => {
                        let res = save_event_to_redis(&mut redis_con, ev.event.clone());
                        match res {
                            Ok(_) => {
                                tracing::debug!(
                                    "Write assigned job to redis: {:?}",
                                    hex::encode(ev.event.id)
                                );
                            }
                            Err(err) => {
                                tracing::error!("Cannot write assigned job to redis: {}", err);
                            }
                        }
                    }
                    Kind::RESULT => {
                        if let Ok(payload) = get_result_payload(&ev) {
                            if let Ok(Some(job_id)) = get_single_tag_entry('e', &ev.event.tags) {
                                // Get saved assign event from redis
                                let assigned_event: Result<RedisEventOnWire, RedisError> =
                                    redis_con.get(job_id.clone());
                                if let Err(err) = assigned_event {
                                    tracing::error!("Can't find assign_event from job_id, {}", err);
                                    continue;
                                }
                                let assign_event = assigned_event.unwrap();

                                // Check Result event validation
                                let validate_result =
                                    validate_result_event(ev.event.clone(), assign_event.clone());

                                match validate_result {
                                    Err(msg) => {
                                        tracing::error!("Validate error {}", msg.to_string());
                                        continue;
                                    }
                                    Ok(res) => {
                                        if !res {
                                            tracing::error!("Validate Result Event Failed");
                                            continue;
                                        }
                                    }
                                }

                                let sender = ev.event.sender;
                                let value = (job_id, (sender, payload, assign_event.id));
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
                RelayMessage::EOSE => tracing::debug!("EOSE message"),
                RelayMessage::Closed => tracing::warn!("Closed message"),
                RelayMessage::Notice => tracing::debug!("Notice message"),
                RelayMessage::Auth(_) => tracing::debug!("Auth message"),
                RelayMessage::Disconnected => {
                    tracing::error!("Client got disconnected, we are shutting down");
                    break;
                }
                RelayMessage::Binary(msg) => {
                    if let Err(err) = handle_binary(&msg[..]) {
                        tracing::error!("{}", err);
                    }
                }
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

        while let Some((job_id, (sender, new_payload, assign_event_id))) = aggr_rx.recv().await {
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
                    assign_event_id,
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
                            assign_event_id,
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
    let result_filter = Filter {
        kinds: vec![Kind::RESULT],
        // Subscribe ASSIGN events since 15 minutes ago
        since: Some(chrono::Utc::now().timestamp() as u64 - 900),
        ..Default::default()
    };
    let assign_filter = Filter {
        kinds: vec![Kind::ASSIGN],
        authors: assigner_whitelist.into_iter().collect(),
        // Subscribe ASSIGN events since 15 minutes ago
        since: Some(chrono::Utc::now().timestamp() as u64 - 900),
        ..Default::default()
    };
    let req = Request::new(sub_id.to_string(), vec![result_filter, assign_filter]);
    client
        .subscribe(req, Some(120)) // in case of reconnect, get messages 2 mins old
        .await
        .expect("Cannot subscribe for Results");
    tracing::info!("Subscribed to Result");

    // Spawn task to monitor subscriptions
    let subs_monitor_handle = tokio::spawn(async move {
        tracing::info!("Subscription monitor started");
        loop {
            tracing::info!("Querying subscriptions");
            if let Err(err) = client
                .send_binary(Message::Binary(BinaryMessage::QuerySubscription.to_bytes()))
                .await
            {
                tracing::error!("{}", err);
            }
            tokio::time::sleep(MONITORING_INTERVAL).await;
        }
        tracing::warn!("Subscription monitor stopped");
    });

    listener_handler.await.unwrap();
    aggregator_handler.await.unwrap();
    metrics_handle.await.unwrap();
    subs_monitor_handle.await.unwrap();
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

fn save_event_to_redis(redis_con: &mut Connection, event: EventOnWire) -> Result<()> {
    let key = hex::encode(event.id);
    let redis_event = RedisEventOnWire {
        id: key.clone(),
        sender: hex::encode(event.sender.to_bytes()),
        tags: event.tags,
        content: event.content,
    };

    redis_con.set(key.clone(), redis_event)?;
    Ok(())
}

fn validate_result_event(event: EventOnWire, assign_event: RedisEventOnWire) -> Result<bool> {
    let payload: ResultPayload = serde_json::from_str(event.content.as_str())?;
    let job_type = payload.header.job_type;
    // POW event check
    if job_type == 0 {
        // Check assignee is Result Sender
        if let Ok(Some(assignee)) = get_single_tag_entry('p', &assign_event.tags) {
            if let Ok(assignee_hex) = hex::decode(assignee) {
                if let Some(assignee_account) = Sender::from_bytes(&assignee_hex) {
                    if assignee_account != event.sender {
                        tracing::error!("Assignee mismatch");
                        return Ok(false);
                    }
                } else {
                    tracing::error!("Invalid p tag in assign_event");
                    return Ok(false);
                }
            } else {
                tracing::error!("Invalid p tag in assign_event");
                return Ok(false);
            }
        } else {
            tracing::error!("Missing p tag in assign_event");
            return Ok(false);
        }

        // Check pow result, Sha512(raw_data_id + output) is 00000....
        let mut hex_raw_data_id = hex::encode(payload.header.raw_data_id.hash());
        hex_raw_data_id.push_str(&payload.output);
        let mut hasher = Sha512::new();
        hasher.update(hex_raw_data_id);
        let result = hasher.finalize();
        let hex_esult = hex::encode(result);
        let pow_result_valid = hex_esult.starts_with("00000");
        if !pow_result_valid {
            tracing::error!("Pow result invalid");
            return Ok(false);
        }
    }

    Ok(true)
}

fn handle_binary(msg: &[u8]) -> Result<()> {
    let binary_msg = BinaryMessage::from_bytes(msg)?;
    match binary_msg {
        BinaryMessage::QuerySubscription => return Err(anyhow!("Unexpected QuerySubscription")),
        BinaryMessage::ReplySubscription(count) => {
            tracing::info!("Active subscriptions: {}", count)
        }
    }
    Ok(())
}
