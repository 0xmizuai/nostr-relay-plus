use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use linked_hash_map::LinkedHashMap;
use nostr_client_plus::__private::config::{load_config, AssignerConfig};
use nostr_client_plus::__private::errors::Unrecoverable;
use nostr_client_plus::__private::metrics::get_metrics_app;
use nostr_client_plus::client::Client;
use nostr_client_plus::event::UnsignedEvent;
use nostr_client_plus::job_protocol::{JobType, Kind};
use nostr_client_plus::request::{Filter, Request};
use nostr_crypto::eoa_signer::EoaSigner;
use nostr_crypto::sender_signer::SenderSigner;
use nostr_plus_common::binary_protocol::BinaryMessage;
use nostr_plus_common::relay_event::RelayEvent;
use nostr_plus_common::relay_message::RelayMessage;
use nostr_plus_common::sender::Sender;
use prometheus::{IntCounter, IntGauge, Registry};
use serde_json::json;
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tokio::time::{interval, sleep};
use tokio_tungstenite::tungstenite::Message;
use tracing_subscriber::FmtSubscriber;

mod utils;
use crate::utils::get_single_tag_entry;

type WorkersBook = LinkedHashMap<Sender, Instant>;
type AssignedSenders = HashMap<Sender, Instant>;
type NumWorkers = usize;
type JobQueue = VecDeque<(RelayEvent, NumWorkers)>;

type PubStruct = (Vec<Sender>, RelayEvent); // Struct for passing jobs to assign

// ToDo: we do not need so many, every few seconds new ones will show up.
const MAX_WORKERS: usize = 10_000;
const ALIVE_INTERVAL: Duration = Duration::from_secs(120); // how often we say we are alive
const COOL_DOWN_PERIOD: Duration = Duration::from_secs(30);
const STALE_TIME: Duration = Duration::from_secs(300);

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::default();
    pub static ref ASSIGNED_JOBS: IntCounter =
        IntCounter::new("assigned_jobs_total", "Total Assigned Jobs")
            .expect("Failed to create assigned_jobs");
    pub static ref CACHED_JOBS: IntGauge =
        IntGauge::new("cached_jobs", "Cached Jobs").expect("Failed to create cached_jobs");
    pub static ref WORKERS_ONLINE: IntGauge =
        IntGauge::new("workers_online", "Workers Online").expect("Failed to create workers_online");
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    // Define needed env variables
    dotenv::dotenv().ok();
    let relay_url = std::env::var("RELAY_URL").unwrap_or("ws://127.0.0.1:3033".to_string());
    let raw_private_key = std::env::var("ASSIGNER_PRIVATE_KEY").expect("Missing private key");
    let min_hb = std::env::var("MIN_HB_VERSION").expect("Missing min HB version");

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
    let config: AssignerConfig = load_config(config_file_path, "assigner")?.try_into()?;

    // Check whitelist is not empty and then create a set out of it
    if config.whitelist.is_empty() {
        return Err(anyhow!("whitelist is empty"));
    }
    let whitelist: BTreeSet<Sender> = config
        .whitelist
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

    // Private key
    let private_key = hex::decode(raw_private_key).unwrap().try_into().unwrap();

    // Create nostr-relay client
    let signer = EoaSigner::from_bytes(&private_key);
    let mut client = Client::new(SenderSigner::Eoa(signer));
    let mut relay_channel = client
        .connect_with_channel(relay_url.as_str())
        .await
        .unwrap();
    let client = Arc::new(Mutex::new(client));

    let client_eh = client.clone();

    /*
     * Start event handler
     */
    let event_handler = tokio::spawn(async move {
        tracing::info!("Assigner event handler started");

        // Publishing channel
        let (pub_tx, mut pub_rx) = mpsc::channel(100);

        // Data structure to keep track of most recent workers
        let avail_workers = WorkersBook::new();
        // Keep track of last time senders were assigned a job
        let mut assigned_senders = Arc::new(Mutex::new(AssignedSenders::new()));
        // Queue of jobs that cannot be assigned immediately
        let job_queue = VecDeque::new();

        let mut context = Context {
            book: avail_workers,
            queue: job_queue,
            whitelist,
            assigned_senders: Arc::clone(&assigned_senders),
        };

        // Prepare alive interval timer
        let mut alive_interval_timer = interval(ALIVE_INTERVAL);

        loop {
            tokio::select! {
                Some(msg) = relay_channel.recv() => {
                    tracing::debug!("Select from client ws relay");
                    if let Err(err) = handle_event(msg, &mut context, &pub_tx).await {
                        tracing::error!("{err}");
                        if err.downcast_ref::<Unrecoverable>().is_some() {
                            break;
                        }
                    }
                }
                Some((workers, ev)) = pub_rx.recv() => {
                    tracing::debug!(r#"Sending "{}""#, ev.event.content);
                    let mut tags: Vec<Vec<String>> = Vec::with_capacity(workers.len());
                    tags.push(vec!["e".to_string(), hex::encode(ev.event.id)]);
                    for w in &workers {
                        tags.push(vec!["p".to_string(), hex::encode(w.to_bytes())]);
                    }
                    let timestamp = chrono::Utc::now().timestamp() as u64;
                    let event = UnsignedEvent::new(
                        client_eh.lock().await.sender(),
                        timestamp,
                        Kind::ASSIGN,
                        tags,
                        ev.event.content,
                    );

                    // Try to publish
                    let event_clone = event.clone(); // clone in case we need the retry task;
                    let client_clone = client_eh.clone();
                    if client_eh.lock().await.publish(event).await.is_err() {
                        // Spawn a task to retry a few times in case ACK did not arrive on time
                        // or client reconnecting.
                        let mut assigned_senders_clone = Arc::clone(&assigned_senders);
                        let _ = tokio::spawn(async move {
                            let mut counter = 0;
                            while counter < 5 {
                                match client_clone.lock().await.publish(event_clone.clone()).await {
                                    Ok(_) => break,
                                    Err(_) => {
                                        tracing::warn!("Publishing {} failed, attempt {}\nRetrying", hex::encode(event_clone.id()), counter);
                                        counter += 1;
                                        sleep(Duration::from_secs(2)).await;
                                    },
                                }
                            }
                            if counter < 5 {
                                // Record when senders have been given jobs
                                let current_instant = Instant::now();
                                for w in &workers {
                                    let mut assigned_senders = (&mut assigned_senders_clone).lock().await;
                                    assigned_senders.insert(w.clone(), current_instant);
                                }
                                ASSIGNED_JOBS.inc();
                            } else {
                                tracing::error!("Could not assign event {}", hex::encode(event_clone.id()));
                            }
                        });
                    } else {
                        // Record when senders have been given jobs
                        let current_instant = Instant::now();
                        for w in &workers {
                            let mut assigned_senders = (&mut assigned_senders).lock().await;
                            assigned_senders.insert(w.clone(), current_instant);
                        }
                        ASSIGNED_JOBS.inc();
                    }
                }
                _ = alive_interval_timer.tick() => {
                    tracing::info!("I am still alive");
                    // Request info about current subscriptions
                    let query = BinaryMessage::QuerySubscription;
                    if client_eh.lock().await.send_binary(Message::Binary(query.to_bytes())).await.is_err() {
                        tracing::error!("Cannot query subscriptino status");
                    }
                }
                else => {
                    tracing::warn!("Unexpected select event");
                }
            }
        }
    });

    // subscription id used for NewJob and Alive
    let sub_id = "ce4788ade0000000000000000000000000000000000000000000000000000000";
    let current_time = chrono::Utc::now().timestamp() as u64;

    // Subscribe to NewJob and Alive/Heartbeats
    let filter_job = Filter {
        kinds: vec![Kind::NEW_JOB],
        since: Some(current_time),
        ..Default::default()
    };
    let filter_hb = Filter {
        kinds: vec![Kind::ALIVE],
        since: Some(current_time),
        tags: HashMap::from([("#v".to_string(), json!([min_hb.to_string()]))]),
        ..Default::default()
    };
    let req = Request::new(sub_id.to_string(), vec![filter_job, filter_hb]);
    client
        .lock()
        .await
        .subscribe(req, Some(0))
        .await
        .expect("Cannot subscribe");
    tracing::info!("Subscribed to NewJob and Heartbeats");

    tokio::select! {
        _ = event_handler => tracing::warn!("event handler task has stopped"),
        _ = metrics_handle => tracing::warn!("metrics task has stopped"),
    }
    tracing::info!("Shutting down (almost) gracefully");
    Ok(())
}

async fn handle_event(
    msg: RelayMessage,
    ctx: &mut Context,
    pub_tx: &mpsc::Sender<PubStruct>,
) -> Result<()> {
    match msg {
        RelayMessage::Event(ev) => {
            match ev.event.kind {
                Kind::ALIVE => {
                    tracing::debug!("Heartbeat received: {:?}", ev.event.tags);
                    let worker_id = ev.event.sender.clone();
                    if is_too_early(&ev.event.sender, ctx).await {
                        tracing::info!("HB from {} too early", hex::encode(worker_id.to_bytes()));
                        return Ok(());
                    }
                    if let None = ctx.book.insert(worker_id, Instant::now()) {
                        WORKERS_ONLINE.inc();
                    }
                    tracing::debug!("HB, now {}", ctx.book.len());
                    // Check if there is a queue to empty
                    while !ctx.queue.is_empty() {
                        tracing::debug!("Emptying queue with {} el", ctx.queue.len());
                        let num_workers = match check_avail_workers(ctx) {
                            None => {
                                tracing::debug!("Not enough workers for this event");
                                break;
                            }
                            Some(num_w) => num_w,
                        };
                        let workers = match get_workers(&mut ctx.book, num_workers) {
                            None => break,
                            Some(res) => res,
                        };
                        let (ev, _) = ctx.queue.pop_front().unwrap();
                        CACHED_JOBS.dec();
                        pub_tx
                            .send((workers, ev))
                            .await
                            .expect("pub channel broken");
                    }
                    // Keep the number of workers below a certain threshold
                    if ctx.book.len() > MAX_WORKERS {
                        // Remove LRU
                        tracing::debug!("Trimming workers book ({} el)", ctx.book.len());
                        if ctx.book.pop_front().is_some() {
                            WORKERS_ONLINE.dec();
                        }
                    }
                    Ok(())
                }
                Kind::NEW_JOB => {
                    tracing::debug!("Got a job to assign");
                    let num_workers = validate_job(&ev, &ctx)?;
                    if ctx.book.len() < num_workers {
                        tracing::debug!("Not enough workers: {}", ctx.book.len());
                        // not enough workers, queue
                        ctx.queue.push_back((ev, num_workers));
                        CACHED_JOBS.inc();
                    } else {
                        let workers = match get_workers(&mut ctx.book, num_workers) {
                            None => {
                                // not enough workers, queue
                                ctx.queue.push_back((ev, num_workers));
                                CACHED_JOBS.inc();
                                return Ok(());
                            }
                            Some(res) => res,
                        };
                        pub_tx
                            .send((workers, ev))
                            .await
                            .expect("pub channel broken");
                    }
                    Ok(())
                }
                _ => Ok(()),
            }
        }
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
            Ok(())
        }
        RelayMessage::Disconnected => {
            tracing::error!("Client got disconnected, we are shutting down");
            Err(anyhow!(Unrecoverable))
        }
        RelayMessage::EOSE => {
            tracing::debug!("EOSE message");
            Ok(())
        }
        RelayMessage::Closed => {
            tracing::warn!("Closed message");
            Ok(())
        } // ToDo: maybe do something
        RelayMessage::Notice => {
            tracing::debug!("Notice message");
            Ok(())
        }
        RelayMessage::Auth(_) => {
            tracing::debug!("Auth message");
            Ok(())
        }
        RelayMessage::Binary(msg) => {
            handle_binary(&msg[..])?;
            Ok(())
        }
    }
}

#[inline]
fn get_workers(book: &mut WorkersBook, num_workers: NumWorkers) -> Option<Vec<Sender>> {
    tracing::debug!(
        "get_workers: getting {} workers out of {}",
        num_workers,
        book.len()
    );

    let current_instant = Instant::now();

    // This is an approximation to avoid storing the Instants for every valid worker.
    // So, all the workers reinserted in the queue wil have this value.
    // It is initialized with current_instant, but it won't be used, or used when
    // updated with a better instant
    let mut last_valid_instant = current_instant;

    let mut workers = Vec::with_capacity(num_workers);
    for _ in 0..num_workers {
        let (w, inst) = book.pop_back()?;
        // check if entry is stale
        if current_instant.duration_since(inst) >= STALE_TIME {
            // This entry and all the other ones still in the linked hashmap are stale.
            // Ignore this one, drain the linked hashmap, put back the ones saved in vector
            // but in the reverse order.
            book.clear();
            for w in workers.into_iter().rev() {
                book.insert(w, last_valid_instant);
            }
            tracing::warn!("Stale workers, trimmed");
            return None;
        }
        last_valid_instant = inst;
        workers.push(w);
    }
    WORKERS_ONLINE.sub(num_workers as i64);
    Some(workers)
}

fn validate_job(ev: &RelayEvent, ctx: &Context) -> Result<NumWorkers> {
    // Is the sender authorized?
    if !ctx.whitelist.contains(&ev.event.sender) {
        return Err(anyhow!("Sender not whitelisted"));
    }
    // Does the message belong to the sender?
    ev.event.verify()?;

    let job_type_name =
        get_single_tag_entry('t', &ev.event.tags)?.ok_or(anyhow!("Missing t tag"))?;
    let job_type: JobType = job_type_name.parse()?;
    Ok(job_type.workers())
}

struct Context {
    book: WorkersBook,
    queue: JobQueue,
    whitelist: BTreeSet<Sender>,
    assigned_senders: Arc<Mutex<AssignedSenders>>,
}

fn register_metrics() {
    REGISTRY
        .register(Box::new(ASSIGNED_JOBS.clone()))
        .expect("Cannot register assigned_jobs");
    REGISTRY
        .register(Box::new(CACHED_JOBS.clone()))
        .expect("Cannot register cached_jobs");
    REGISTRY
        .register(Box::new(WORKERS_ONLINE.clone()))
        .expect("Cannot register workers_online");
}

async fn is_too_early(sender: &Sender, ctx: &mut Context) -> bool {
    let assigned_senders = ctx.assigned_senders.lock().await;
    if let Some(last_assigned) = assigned_senders.get(sender) {
        let current_time = Instant::now();
        if current_time.duration_since(*last_assigned) >= COOL_DOWN_PERIOD {
            false
        } else {
            true
        }
    } else {
        false
    }
}

// Check how many workers are needed and return the number
// if there is a sufficient number. None otherwise.
fn check_avail_workers(ctx: &mut Context) -> Option<NumWorkers> {
    if let Some((_, num_workers)) = ctx.queue.front() {
        if num_workers > &ctx.book.len() {
            return None;
        }
        Some(*num_workers)
    } else {
        None
    }
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
