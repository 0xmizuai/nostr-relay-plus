use anyhow::{anyhow, Result};
use chrono::Utc;
use futures_util::TryStreamExt;
use lazy_static::lazy_static;
use mongodb::bson::{doc, to_bson, Document};
use mongodb::{Client as DbClient, Collection};
use nostr_client_plus::__private::config::{load_config, AggregatorConfig};
use nostr_client_plus::__private::metrics::get_metrics_app;
use nostr_client_plus::client::Client;
use nostr_client_plus::db::FinishedJobs;
use nostr_client_plus::job_protocol::{
    AssignerTask, AssignerTaskStatus, ClassifierJobOutput, Kind, NewJobPayload, ResultPayload,
};
use nostr_client_plus::redis::RedisEventOnWire;
use nostr_client_plus::request::{Filter, Request};
use nostr_crypto::eoa_signer::EoaSigner;
use nostr_crypto::sender_signer::SenderSigner;
use nostr_plus_common::binary_protocol::BinaryMessage;
use nostr_plus_common::logging::init_tracing;
use nostr_plus_common::relay_event::RelayEvent;
use nostr_plus_common::relay_message::RelayMessage;
use nostr_plus_common::sender::Sender;
use nostr_plus_common::wire::EventOnWire;
use prometheus::{IntCounter, IntGauge, Registry};
use redis::{Commands, Connection, RedisError};
use sha2::{Digest, Sha512};
use std::collections::hash_map::Entry;
use std::collections::{BTreeSet, HashMap};
use std::format;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

mod utils;
use crate::utils::{get_private_key_from_name, get_single_tag_entry};

type AggrMessage = (String, (Sender, ResultPayload, String));

const MONITORING_INTERVAL: Duration = Duration::from_secs(120);

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::default();
    pub static ref FINISHED_JOBS: IntCounter =
        IntCounter::new("finished_jobs_total", "Total Finished Jobs")
            .expect("Failed to create finished_jobs_total");
    pub static ref FAILED_JOBS: IntCounter =
        IntCounter::new("failed_jobs_total", "Total Failed Jobs")
            .expect("Failed to create failed_jobs_total");
    pub static ref RESULT_ERRORS: IntCounter =
        IntCounter::new("result_errors_total", "Total Result Errors")
            .expect("Failed to create result_errors_total");
    pub static ref PENDING_JOBS: IntGauge =
        IntGauge::new("pending_jobs", "Pending Jobs").expect("Failed to create pending_jobs");
    pub static ref UNCLASSIFIED_ERRORS: IntCounter =
        IntCounter::new("unclassified_errors_total", "Total Unclassified Errors")
            .expect("Failed to create unclassified_errors_total");
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
    // Only string true, will be considered as true
    let skip_pow_check = std::env::var("SKIP_POW_CHECK").unwrap_or("".to_string());

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
    init_tracing();

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
    let connection = DbClient::with_uri_str(db_url.clone())
        .await
        .expect("Cannot connect to db");
    let db = connection.database("mine_local");
    let collection: Collection<FinishedJobs> = db.collection("finished_jobs");
    let task_collection: Collection<AssignerTask> =
        connection.database("assigner").collection("tasks");

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
                                    UNCLASSIFIED_ERRORS.inc();
                                    continue;
                                }
                                let assign_event = assigned_event.unwrap();

                                // Check Result event validation
                                let validate_result = validate_result_event(
                                    ev.event.clone(),
                                    assign_event.clone(),
                                    skip_pow_check == "true",
                                );

                                // ToDo: because of the way `validate_result_event` is written,
                                //  it can be a failed job only when it's Ok(false). In case of
                                //  Error, the only thing we can say it's a Result Error.
                                //  `validate_result_event` needs to be re-written in a more compact way
                                //  and return Result<()>: validation is either a success or error.
                                match validate_result {
                                    Err(msg) => {
                                        tracing::error!("Validate error {}", msg.to_string());
                                        RESULT_ERRORS.inc();
                                        continue;
                                    }
                                    Ok(res) => {
                                        if !res {
                                            tracing::error!("Validate Result Event Failed");
                                            FAILED_JOBS.inc();
                                            continue;
                                        }
                                    }
                                }

                                let sender = ev.event.sender;
                                let value = (job_id, (sender, payload, assign_event.id));
                                if aggr_tx.send(value).await.is_err() {
                                    tracing::error!("Cannot send to aggregator task");
                                    UNCLASSIFIED_ERRORS.inc();
                                }
                            } else {
                                tracing::error!("invalid tag");
                                RESULT_ERRORS.inc();
                            }
                        } else {
                            tracing::error!("invalid content");
                            RESULT_ERRORS.inc();
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
        let mut event_ids: Vec<String> = vec![];

        while let Some((job_id, (sender, new_payload, assign_event_id))) = aggr_rx.recv().await {
            tracing::debug!("======================================");
            tracing::debug!("Assign Event id: {}", assign_event_id);
            tracing::debug!("Job id: {}", job_id);
            tracing::debug!("======================================");
            // TODO: parse JobType
            let job_type = new_payload.header.job_type;
            let n_winners = match job_type {
                0 => 1,
                1 => 3,
                _ => {
                    tracing::error!("Unknown job type");
                    UNCLASSIFIED_ERRORS.inc();
                    continue;
                }
            };
            let payload = new_payload.clone();

            if payload.version != supported_version {
                tracing::error!("Version mismatch");
                UNCLASSIFIED_ERRORS.inc();
                continue;
            }

            // job_type 1 is classifier
            if (job_type == 1) {
                event_ids.push(payload.header.raw_data_id.to_string());
                classifier_aggregate(&sender, &payload, &task_collection).await;
                finalize_classification(
                    assign_event_id,
                    &payload,
                    vec![payload.header.raw_data_id.to_string()],
                    &task_collection,
                    &collection,
                )
                .await;
                continue;
            }

            if n_winners <= 1 {
                tracing::debug!("The job needs only one worker! {}", job_id);
                let raw_data_id = payload.header.raw_data_id.clone();
                let db_entry = FinishedJobs {
                    _id: raw_data_id,
                    workers: vec![sender.clone()], // if we remove first, no need to clone but this is safer
                    timestamp: chrono::Utc::now().timestamp() as u64,
                    result: new_payload,
                    job_type,
                    assign_event_id,
                };

                match collection.insert_one(db_entry, None).await {
                    Ok(_) => {
                        FINISHED_JOBS.inc();
                    }
                    Err(err) => {
                        tracing::error!("Cannot write finished job to db: {}", err);
                        FAILED_JOBS.inc();
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
                        RESULT_ERRORS.inc();

                        // ToDo: Currently we handle disputes by deleting the entry altogether.
                        //  If this was already at the second result, another one will come and
                        //  linger in the book because no other matches are expected to come.
                        //  Another approach needs to decided.
                        entry.remove();
                        PENDING_JOBS.set(aggr_book.len() as i64);
                        continue;
                    }

                    // Ok, they are the same
                    workers.push(sender);
                    // We allow the case of len > N_WORKERS for cases where there are issues
                    // writing to the db or removing from book.
                    // The first N_WINNERS are still the same in the DB, regardless.

                    if workers.len() >= n_winners {
                        // Use job_type as 0 since this is a pow work
                        if reward(assign_event_id, workers, new_payload, &collection, 0).await {
                            entry.remove(); // Remove only if we know it's safe in db
                            PENDING_JOBS.set(aggr_book.len() as i64);
                        }
                    }
                }
                Entry::Vacant(entry) => {
                    tracing::debug!("First time we see job {}", job_id);
                    entry.insert((vec![sender], new_payload));
                    PENDING_JOBS.set(aggr_book.len() as i64);
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

async fn finalize_classification(
    assign_event_id: String,
    payload: &ResultPayload,
    event_ids: Vec<String>,
    collection: &Collection<AssignerTask>,
    finish_job_collection: &Collection<FinishedJobs>,
) {
    tracing::debug!("Finalizing classifications for events: {:?}", event_ids);
    let filter = doc! {
        "event_id": {
            "$in": event_ids,
        },
    };

    let mut cursor = collection
        .find(filter, None)
        .await
        .expect("Failed to finalize classifications");
    let mut tasks_by_event: HashMap<String, Vec<AssignerTask>> = HashMap::new();
    while let Some(task) = cursor.try_next().await.unwrap() {
        if !tasks_by_event.contains_key(task.event_id.as_str()) {
            tasks_by_event.insert(task.event_id.to_string(), vec![]);
        }
        tasks_by_event
            .get_mut(task.event_id.as_str())
            .unwrap()
            .push(task);
    }

    for event_id in tasks_by_event.keys() {
        tracing::debug!("Event id: {}", event_id);
        let mut processing_count = 0;
        let mut answers: HashMap<String, i32> = HashMap::new();
        for task in tasks_by_event.get(event_id).unwrap() {
            tracing::debug!("Worker id: {}", hex::encode(task.worker.to_bytes()));
            if task.result.is_empty() && task.status == AssignerTaskStatus::Pending {
                processing_count += 1;
                break;
            }
            let result_identifier = task.get_result_identifier();
            tracing::info!("result_identifier: {}", result_identifier.to_string());
            if !result_identifier.is_empty() {
                match answers.entry(result_identifier) {
                    Entry::Occupied(mut entry) => {
                        entry.insert(entry.get() + 1);
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(1);
                    }
                }
            }
        }
        if processing_count > 0 {
            tracing::debug!("Waiting for more results for event id: {}", event_id);
            continue;
        }
        tracing::debug!("All results has been collected for event id: {}", event_id);
        // max_count count number of results having same max count
        let mut max_count = 0;
        // max_answer is number of results having max answer
        let mut max_answer = 0;
        let mut answer: String = "".to_string();
        for result in answers.keys() {
            let count = answers.get(result).unwrap().to_owned();
            if count == max_answer {
                max_count += 1;
            } else if { count > max_answer } {
                max_count = 1;
                max_answer = count;
                answer = result.clone();
            }
        }
        let mut tasks_to_update: Vec<AssignerTask> = vec![];
        if max_count > 1 {
            tracing::debug!("There are {} results having same count: {}. We need to ask assign same task to more workers.", max_count, max_answer);
            // Now let's mark all tasks with results as RETRY
            for task in tasks_by_event.get(event_id).unwrap() {
                if !task.result.is_empty() {
                    tasks_to_update.push(AssignerTask {
                        worker: task.worker.clone(),
                        event_id: task.event_id.clone(),
                        status: AssignerTaskStatus::Retry,
                        created_at: task.created_at,
                        expires_at: task.expires_at,
                        result: task.result.clone(),
                    });
                }
            }
        } else {
            // Now let's award the winners
            let mut winners: Vec<Sender> = vec![];
            for task in tasks_by_event.get(event_id).unwrap() {
                let result_identifier = task.get_result_identifier();
                if result_identifier == answer {
                    tasks_to_update.push(AssignerTask {
                        worker: task.worker.clone(),
                        event_id: task.event_id.clone(),
                        status: AssignerTaskStatus::Succeeded,
                        created_at: task.created_at,
                        expires_at: task.expires_at,
                        result: task.result.clone(),
                    });
                    winners.push(task.worker.clone());
                } else if !task.result.is_empty() {
                    tracing::debug!("result_identifier: {}", result_identifier);
                    tracing::debug!("answer: {}", answer);
                    tasks_to_update.push(AssignerTask {
                        worker: task.worker.clone(),
                        event_id: task.event_id.clone(),
                        status: AssignerTaskStatus::Failed,
                        created_at: task.created_at,
                        expires_at: task.expires_at,
                        result: task.result.clone(),
                    });
                }
            }
            tracing::debug!("Winners: {:?}", winners);
            // job_type 1 as classifier
            reward(
                assign_event_id.clone(),
                &winners,
                payload.clone(),
                finish_job_collection,
                1,
            )
            .await;
        }
        // TODO(wangjun.hong): Replace below with a single update
        collection
            .insert_many(tasks_to_update, None)
            .await
            .expect("Failed to update tasks");
        let delete_filter = doc! {
            "event_id": event_id,
            "status": {
                "$in": [
                    "Pending",
                ],
            }
        };
        collection
            .delete_many(delete_filter, None)
            .await
            .expect("Failed to delete tasks");
    }
}

async fn reward(
    assign_event_id: String,
    workers: &Vec<Sender>,
    payload: ResultPayload,
    collection: &Collection<FinishedJobs>,
    job_type: u16,
) -> bool {
    tracing::debug!(
        "All the results for {} are consistent, sending",
        assign_event_id
    );
    let raw_data_id = payload.header.raw_data_id.clone();
    let db_entry = FinishedJobs {
        _id: raw_data_id,
        workers: workers.clone(), // if we remove first, no need to clone but this is safer
        timestamp: chrono::Utc::now().timestamp() as u64,
        result: payload,
        job_type,
        assign_event_id,
    };
    tracing::debug!(
        "Rewarding all workers for data: {}",
        raw_data_id.to_string()
    );
    match collection.insert_one(db_entry, None).await {
        Ok(_) => {
            FINISHED_JOBS.inc();
            true
        }
        Err(err) => {
            // We log and that's it, we keep the entry in book for later inspection
            tracing::error!("Cannot write finished job to db: {}", err);
            FAILED_JOBS.inc();
            false
        }
    }
}

async fn classifier_aggregate(
    sender: &Sender,
    payload: &ResultPayload,
    collection: &Collection<AssignerTask>,
) {
    let filter = doc! {
        "event_id": payload.header.raw_data_id.to_string(),
        "worker": hex::encode(sender.to_bytes()),
    };

    tracing::debug!(
        "Event id: {}, worker id: {}",
        payload.header.raw_data_id.to_string(),
        hex::encode(sender.to_bytes())
    );

    let task = collection.find_one(filter.clone(), None).await.unwrap();

    if (task.is_none()) {
        tracing::error!(
            "No task found for worker: {}",
            hex::encode(sender.to_bytes())
        );
        return;
    }

    tracing::debug!("Task found for worker: {}", hex::encode(sender.to_bytes()));

    let worker_task = task.unwrap();

    let current_timestamp = Utc::now().timestamp() as u64;
    let mongo_error_message = format!(
        "Failed to update task for event: {}, worker: {}",
        payload.header.raw_data_id.to_string(),
        hex::encode(sender.to_bytes())
    );

    tracing::debug!(
        "Current timestamp: {}, expires at: {}",
        current_timestamp,
        worker_task.expires_at
    );
    if ((worker_task.expires_at as u64) < current_timestamp) {
        tracing::error!(
            "Task: {} is timed out",
            payload.header.raw_data_id.to_string()
        );
        // Timeout should be AssignerTaskStatus::Timeout
        let update = doc! {"$set": {"status": "Timeout"}};
        collection
            .update_one(filter.clone(), update, None)
            .await
            .expect(mongo_error_message.as_str());
        return;
    }

    let update = doc! {"$set": {"result": payload.output.clone()}};
    collection
        .update_one(filter.clone(), update, None)
        .await
        .expect(mongo_error_message.as_str());
}

fn get_result_payload(ev: &RelayEvent) -> Result<ResultPayload> {
    let res: ResultPayload = serde_json::from_str(ev.event.content.as_str())?;
    Ok(res)
}

fn register_metrics() {
    REGISTRY
        .register(Box::new(FINISHED_JOBS.clone()))
        .expect("Cannot register FINISHED_JOBS");

    REGISTRY
        .register(Box::new(FAILED_JOBS.clone()))
        .expect("Cannot register FAILED_JOBS");
    REGISTRY
        .register(Box::new(RESULT_ERRORS.clone()))
        .expect("Cannot register RESULT_ERRORS");
    REGISTRY
        .register(Box::new(PENDING_JOBS.clone()))
        .expect("Cannot register PENDING_JOBS");
    REGISTRY
        .register(Box::new(UNCLASSIFIED_ERRORS.clone()))
        .expect("Cannot register UNCLASSIFIED_ERRORS");
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

fn validate_result_event(
    event: EventOnWire,
    assign_event: RedisEventOnWire,
    skip_pow_check: bool,
) -> Result<bool> {
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

        if (skip_pow_check) {
            return Ok(true);
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
