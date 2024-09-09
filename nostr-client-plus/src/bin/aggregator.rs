use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use mongodb::bson::doc;
use mongodb::{Client as DbClient, Collection};
use nostr_client_plus::__private::config::{load_config, AggregatorConfig};
use nostr_client_plus::__private::metrics::get_metrics_app;
use nostr_client_plus::client::Client;
use nostr_client_plus::db::{ClassifierResult, FinishedJobs};
use nostr_client_plus::job_protocol::{
    AssignerTask, ClassifierJobOutput, Kind, ResultPayload
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
use std::sync::Arc;
use std::time::Duration;
use std::vec;
use serde_json::from_str;
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
        IntGauge::new("pending_jobs", "Pending Jobs")
            .expect("Failed to create pending_jobs");
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
    let db = connection.database("mine");
    let collection: Collection<FinishedJobs> = db.collection("finished_jobs");
    let classifier_result_collection: Collection<ClassifierResult> = db.collection("classifier_results");

    // Configure Redis
    let redis_url = std::env::var("REDIS_URL").expect("REDIS_URL is not set");
    let redis_client = redis::Client::open(redis_url)?;
    let mut redis_con = redis_client.get_connection()?;
    let mut redis_con_task = redis_client.get_connection()?;

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
                        };
                        // Let's increase pending jobs whenever we saw a new job to collect
                        // results from assigner
                        PENDING_JOBS.inc();
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

        while let Some((job_id, (sender, new_payload, assign_event_id))) = aggr_rx.recv().await {
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

            if new_payload.version != supported_version {
                tracing::error!("Version mismatch");
                UNCLASSIFIED_ERRORS.inc();
                continue;
            }

            if n_winners <= 1 {
                tracing::debug!("The job needs only one worker! {}", job_id);
                let raw_data_id = new_payload.header.raw_data_id.clone();
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

            // Falling below are logics for classification
            let raw_data_id = hex::encode(new_payload.header.raw_data_id.hash());

            // First fetch all existing tasks
            let saved_tasks: Result<Option<String>, RedisError> =
                (&mut redis_con_task).get(raw_data_id.clone());
            let mut tasks: Vec<AssignerTask> = match saved_tasks {
                Ok(val) => {
                    match val {
                        Some(val) => {
                            match from_str(val.as_str()) {
                                Ok(val) => {
                                    val
                                }
                                Err(err) => {
                                    tracing::error!("Failed parse tasks, error: {}", err);
                                    // Skip this task since the stored tasks are corrupted
                                    continue;
                                }
                            }
                        }
                        None => {
                            tracing::debug!("This is the first time we saw this event result");
                            Vec::new()
                        }
                    }
                }
                Err(err) => {
                    tracing::error!("Failed to fetch tasks from redis for data: {}, error: {}", raw_data_id.clone(), err);
                    // Skip this task since something wrong with redis
                    continue;
                }
            };

            // Then append new tasks in the end
            tasks.push(
                AssignerTask {
                    worker: sender,
                    event_id: raw_data_id.clone(),
                    result: new_payload,
                }
            );

            // When number of tasks equal to winner count, let's try to finalize
            if tasks.len() == n_winners {
                match finalize_classification(
                    &tasks,
                    assign_event_id,
                    &classifier_result_collection,
                    &collection,
                ).await {
                    Ok(_) => {
                        // Note, we don't remove the data from redis, the data can stay in
                        // redis for a while to be used as a reference later
                        tracing::debug!("Successfully finalized data: {}", raw_data_id);
                    }
                    Err(err) => {
                        tracing::error!("Failed to finalized data: {}, error: {}", raw_data_id, err);
                        RESULT_ERRORS.inc();
                    }
                };
                PENDING_JOBS.dec();
            } else {
                // Otherwise let's just update the values in redis
                match save_task_to_redis(&mut redis_con_task, raw_data_id.clone(), tasks) {
                    Ok(_) => {
                        tracing::debug!("Successfully updated data: {}", raw_data_id);
                    }
                    Err(err) => {
                        tracing::error!("Failed to updated data: {}, err: {}", raw_data_id, err);
                        RESULT_ERRORS.inc();
                    }
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

/**
This method tries to finalize all results returned for a single
event_id, we figure out a result which majority of results having
and mark that as the winner result, and only reward the workers
with that result
**/
async fn finalize_classification(
    tasks: &Vec<AssignerTask>,
    assign_event_id: String,
    classifier_result_collection: &Collection<ClassifierResult>,
    collection: &Collection<FinishedJobs>,
) -> Result<()> {
    tracing::debug!("Finalizing classifications for event: {}", assign_event_id);
    let mut answers: HashMap<String, i32> = HashMap::new();

    // Let's first go through all tasks and construct a map of
    // answer -> count
    for task in tasks.iter() {
        let identifier: String = match get_task_identifier(task) {
            Some(val) => {
                if val.is_empty() {
                    tracing::error!("Get empty identifier from: {}", task.result.output);
                    continue;
                }
                val
            }

            None => {
                tracing::error!("Get identifier as None from: {}", task.result.output);
                continue;
            }
        };
        match answers.entry(identifier) {
            Entry::Occupied(mut entry) => {
                entry.insert(entry.get() + 1);
            }
            Entry::Vacant(entry) => {
                entry.insert(1);
            }
        }
    }

    // Let's check if there's an answer which majority of the tasks
    // have, if we see multiple answers with same counts from all task results
    // we retry and wait for more results
    // max_count count number of results having same max count
    let mut max_count = 0;
    // max_answer is number of results having max answer
    let mut max_answer = 0;
    let mut answer = String::new();
    for (result, count) in answers {
        if count == max_answer {
            max_count += 1;
        } else if count > max_answer {
            max_count = 1;
            max_answer = count;
            answer = result;
        }
    }
    // This is when we are going to retry because there is not a single
    // answer occurs more than any others
    let mut accepted_result: Option<String> = None;
    if max_count == 1 {
        accepted_result = Some(answer);
    }
    let (result, winners) = get_winners(tasks, accepted_result);

    if !winners.is_empty() && !result.is_none() {
        reward(assign_event_id.clone(), &winners, result.unwrap(), classifier_result_collection,
        collection, 1).await;
    }

    Ok(())
}

/**
Go through all tasks and return all winners and the result payload
When result is None, no winners are returned
**/
fn get_winners(
    tasks: &Vec<AssignerTask>,
    result: Option<String>
) -> (Option<ResultPayload>, Vec<Sender>) {
    tracing::debug!("Result is: {:?}", result);
    let mut winners: Vec<Sender> = vec![];
    let mut result_payload: Option<ResultPayload> = None;
    tasks.iter().for_each(|task| {
        let task_result = get_task_identifier(task);
        if task_result == result && !task_result.is_none() {
            winners.push(task.worker.clone());
            result_payload = Some(task.result.clone());
        }
    });

    (result_payload, winners)
}

// Return values from a task identifier, in case of
// failure happened, return None
fn get_task_identifier(
    task: &AssignerTask
) -> Option<String> {
    match get_result_identifier(task.result.output.clone()) {
        Ok(Some(val)) => {
            Some(val)
        }
        Ok(None) => {
            None
        }
        Err(err) => {
            tracing::error!("Failed to parse identifier, error: {}", err);
            None
        }
    }
}

async fn reward(
    assign_event_id: String,
    workers: &Vec<Sender>,
    payload: ResultPayload,
    classifier_result_collection: &Collection<ClassifierResult>,
    collection: &Collection<FinishedJobs>,
    job_type: u16,
) -> bool {
    tracing::debug!(
        "All the results for {} are consistent, sending",
        assign_event_id
    );
    let raw_data_id = payload.header.raw_data_id.clone();
    let result_identifier = get_result_identifier(payload.output.clone());
    match result_identifier {
        Err(_) =>{
            tracing::error!("Failed to get the identifier from the result: {}", payload.output);
            return false;
        },
        Ok(None)=>{
            tracing::error!("Get identifier as None from the result: {}", payload.output);
            return false;
        },
        Ok(Some(tag_id)) =>{
            // Save classifier result to db
            let classifier_result = ClassifierResult {
                _id: raw_data_id,
                kv_key: payload.kv_key.clone(),
                tag_id
            };
            match classifier_result_collection.insert_one(classifier_result, None).await {
                Ok(_) => {
                    tracing::info!("Write classifier result to db: {}", payload.kv_key.clone());
                }
                Err(err) => {
                    tracing::error!("Cannot write classifier result to db: {}", err);
                    FAILED_JOBS.inc();
                    return false;
                }
            }
        }
    }

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
            tracing::error!("Cannot write finished job to db: {}", err);
            FAILED_JOBS.inc();
            false
        }
    }

}

fn get_result_payload(ev: &RelayEvent) -> Result<ResultPayload> {
    tracing::debug!("Content is: {}", (ev.event.content.as_str()));
    let res: ResultPayload = serde_json::from_str(ev.event.content.as_str())?;
    Ok(res)
}

 /**
     * For now, only use the first item in array to compare results
     * result example: [{"tag_id":2867,"distance":0.5045340279460676}]
     * return example: 2867
*/
pub fn get_result_identifier(output:String) -> Result<Option<String>> {
    tracing::debug!("output: {}", output);
    let result_arr: Vec<ClassifierJobOutput> = from_str(output.as_str())?;
    match result_arr.get(0) {
        None => {
            tracing::debug!("Returning none as identifier");
            Ok(None)
        }
        Some(answer) => {
            tracing::debug!("Returning identifier: {}", answer.tag_id.to_string());
            Ok(Some(answer.tag_id.to_string()))
        }
    }
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
        sender: hex::encode(event.sender.to_bytes()), tags: event.tags,
        content: event.content,
    };

    redis_con.set(key.clone(), redis_event)?;
    Ok(())
}

// This method helps us to save tasks to redis
fn save_task_to_redis(redis_con: &mut Connection, id: String, tasks: Vec<AssignerTask>) -> Result<()> {
    redis_con.set(id, serde_json::json!(tasks).to_string())?;

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
