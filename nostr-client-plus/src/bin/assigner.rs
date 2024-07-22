use linked_hash_map::LinkedHashMap;
use nostr_client_plus::client::Client;
use nostr_client_plus::event::UnsignedEvent;
use nostr_client_plus::job_protocol::Kind;
use nostr_client_plus::request::{Filter, Request};
use nostr_client_plus::utils::get_timestamp;
use nostr_crypto::eoa_signer::EoaSigner;
use nostr_crypto::sender_signer::SenderSigner;
use nostr_plus_common::relay_event::RelayEvent;
use nostr_plus_common::relay_message::RelayMessage;
use nostr_plus_common::sender::Sender;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tracing_subscriber::FmtSubscriber;

mod utils;
use utils::get_private_key_from_name;

type WorkersBook = LinkedHashMap<Sender, ()>;
type JobQueue = VecDeque<RelayEvent>;

type PubStruct = (Vec<Sender>, RelayEvent); // Struct for passing jobs to assign

// ToDo: we do not need so many, every few seconds new ones will show up.
const MAX_WORKERS: usize = 10_000;
const WORKERS_PER_JOB: usize = 3;

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

    // Get a silly private key based on a string identifying the service.
    let private_key = get_private_key_from_name("Assigner").unwrap();

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
        let mut avail_workers = WorkersBook::new();
        // Queue of jobs that cannot be assigned immediately
        let mut job_queue = VecDeque::new();

        loop {
            tokio::select! {
                Some(msg) = relay_channel.recv() => {
                    handle_event(msg, &mut avail_workers, &mut job_queue, &pub_tx).await;
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
}

// If Some() is returned, then the event needs to be re-scheduled
async fn handle_event(
    msg: RelayMessage,
    book: &mut WorkersBook,
    queue: &mut JobQueue,
    pub_tx: &mpsc::Sender<PubStruct>,
) {
    match msg {
        RelayMessage::Event(ev) => {
            match ev.event.kind {
                Kind::ALIVE => {
                    let worker_id = ev.event.sender.clone();
                    book.insert(worker_id, ());
                    tracing::debug!("HB, now {}", book.len());
                    // Check if there is a queue to empty
                    while !queue.is_empty() && book.len() >= WORKERS_PER_JOB {
                        tracing::debug!("Emptying queue with {} el", queue.len());
                        let ev = queue.pop_front().unwrap();
                        let workers = get_workers(book);
                        pub_tx
                            .send((workers, ev))
                            .await
                            .expect("pub channel broken");
                    }
                    // Keep the number of workers below a certain threshold
                    if book.len() > MAX_WORKERS {
                        // Remove LRU
                        tracing::debug!("Trimming workers book ({} el)", book.len());
                        book.pop_front().unwrap();
                    }
                }
                Kind::NEW_JOB => {
                    tracing::debug!("Got a job to assign");
                    if book.len() < WORKERS_PER_JOB {
                        tracing::debug!("Not enough workers: {}", book.len());
                        // not enough workers, queue
                        queue.push_back(ev);
                    } else {
                        let workers = get_workers(book);
                        pub_tx
                            .send((workers, ev))
                            .await
                            .expect("pub channel broken");
                    }
                }
                _ => {}
            }
        }
        _ => {
            tracing::warn!("non-event message");
        }
    }
}

#[inline]
fn get_workers(book: &mut WorkersBook) -> Vec<Sender> {
    let mut workers = Vec::with_capacity(WORKERS_PER_JOB);
    for _ in 0..WORKERS_PER_JOB {
        let (w, _) = book.pop_back().expect("There should be enough workers");
        workers.push(w);
    }
    workers
}
