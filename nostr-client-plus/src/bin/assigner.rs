use anyhow::{anyhow, Result};
use linked_hash_map::LinkedHashMap;
use nostr_client_plus::__private::config::{load_config, Config};
use nostr_client_plus::client::Client;
use nostr_client_plus::event::UnsignedEvent;
use nostr_client_plus::job_protocol::{JobType, Kind};
use nostr_client_plus::request::{Filter, Request};
use nostr_client_plus::utils::get_timestamp;
use nostr_crypto::eoa_signer::EoaSigner;
use nostr_crypto::sender_signer::SenderSigner;
use nostr_plus_common::relay_event::RelayEvent;
use nostr_plus_common::relay_message::RelayMessage;
use nostr_plus_common::sender::Sender;
use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tracing_subscriber::FmtSubscriber;

mod utils;
use crate::utils::get_single_tag_entry;

type WorkersBook = LinkedHashMap<Sender, ()>;
type NumWorkers = usize;
type JobQueue = VecDeque<(RelayEvent, NumWorkers)>;

type PubStruct = (Vec<Sender>, RelayEvent); // Struct for passing jobs to assign

// ToDo: we do not need so many, every few seconds new ones will show up.
const MAX_WORKERS: usize = 10_000;

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
    let raw_private_key = std::env::var("ASSIGNER_PRIVATE_KEY").unwrap();

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
    let config: Config = load_config(config_file_path, "assigner")?.try_into()?;

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
        // ToDo: We don't use the value, so a HashSet maintaining insertion order, should suffice
        let avail_workers = WorkersBook::new();
        // Queue of jobs that cannot be assigned immediately
        let job_queue = VecDeque::new();

        let mut context = Context {
            book: avail_workers,
            queue: job_queue,
            whitelist,
        };

        loop {
            tokio::select! {
                Some(msg) = relay_channel.recv() => {
                    if let Err(err) = handle_event(msg, &mut context, &pub_tx).await {
                        tracing::error!("{err}");
                    }
                }
                Some((workers, ev)) = pub_rx.recv() => {
                    tracing::debug!(r#"Sending "{}""#, ev.event.content);
                    let mut tags: Vec<Vec<String>> = Vec::with_capacity(workers.len());
                    tags.push(vec!["e".to_string(), hex::encode(ev.event.id)]);
                    for w in workers {
                        tags.push(vec!["p".to_string(), hex::encode(w.to_bytes())]);
                    }
                    let timestamp = get_timestamp();
                    let event = UnsignedEvent::new(
                        client_eh.lock().await.sender(),
                        timestamp,
                        Kind::ASSIGN,
                        tags,
                        ev.event.content,
                    );
                    client_eh.lock().await.publish(event).await.expect("Failed to publish");
                }
                else => {
                    tracing::warn!("Unexpected select event");
                }
            }
        }
    });

    // subscription id used for NewJob and Alive
    let sub_id = "ce4788ade0000000000000000000000000000000000000000000000000000000";

    // Subscribe to NewJob and Alive
    let filter = Filter {
        kinds: vec![Kind::NEW_JOB, Kind::ALIVE],
        since: Some(get_timestamp()),
        ..Default::default()
    };
    let req = Request::new(sub_id.to_string(), vec![filter]);
    client.lock().await.subscribe(req).await.unwrap();
    tracing::info!("Subscribed to NewJob and Alive");

    event_handler.await.unwrap();
    tracing::info!("Shutting down gracefully");
    Ok(())
}

// If Some() is returned, then the event needs to be re-scheduled
async fn handle_event(
    msg: RelayMessage,
    ctx: &mut Context,
    pub_tx: &mpsc::Sender<PubStruct>,
) -> Result<()> {
    match msg {
        RelayMessage::Event(ev) => {
            match ev.event.kind {
                Kind::ALIVE => {
                    let worker_id = ev.event.sender.clone();
                    ctx.book.insert(worker_id, ());
                    tracing::debug!("HB, now {}", ctx.book.len());
                    // Check if there is a queue to empty
                    while !ctx.queue.is_empty() {
                        tracing::debug!("Emptying queue with {} el", ctx.queue.len());
                        // peek first and check how many workers are needed
                        if let Some((_, num_workers)) = ctx.queue.front() {
                            if num_workers > &ctx.book.len() {
                                tracing::debug!("Not enough workers for this event");
                                break;
                            }
                        } else {
                            tracing::error!("Queue should not be empty");
                        }
                        let (ev, num_workers) = ctx.queue.pop_front().unwrap();
                        let workers = get_workers(&mut ctx.book, num_workers);
                        pub_tx
                            .send((workers, ev))
                            .await
                            .expect("pub channel broken");
                    }
                    // Keep the number of workers below a certain threshold
                    if ctx.book.len() > MAX_WORKERS {
                        // Remove LRU
                        tracing::debug!("Trimming workers book ({} el)", ctx.book.len());
                        ctx.book.pop_front();
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
                    } else {
                        let workers = get_workers(&mut ctx.book, num_workers);
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
        _ => {
            tracing::warn!("non-event message");
            Ok(())
        }
    }
}

#[inline]
fn get_workers(book: &mut WorkersBook, num_workers: NumWorkers) -> Vec<Sender> {
    tracing::debug!(
        "get_workers: getting {} workers out of {}",
        num_workers,
        book.len()
    );
    let mut workers = Vec::with_capacity(num_workers);
    for _ in 0..num_workers {
        let (w, _) = book.pop_back().expect("There should be enough workers");
        workers.push(w);
    }
    workers
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
}
