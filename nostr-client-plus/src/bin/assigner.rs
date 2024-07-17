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
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tracing_subscriber::FmtSubscriber;

type WorkersBook = LinkedHashMap<Sender, ()>;
type PubStruct = (Vec<Sender>, RelayEvent); // Struct for passing jobs to assign

// ToDo: we do not need so many, every few seconds new ones will show up.
//  For now, just report, but then make it a hard limit and start evicting.
const MAX_WORKERS: usize = 1_000_000;
const WORKERS_PER_JOB: usize = 3;

#[tokio::main]
async fn main() {
    // Logger setup
    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    // Create nostr-relay client
    let signer = EoaSigner::from_bytes(&[7; 32]); // ToDo: better private key
    let mut client = Client::new(SenderSigner::Eoa(signer));
    let mut relay_channel = client
        .connect_with_channel("ws://127.0.0.1:3033")
        .await
        .unwrap();
    let client = Arc::new(Mutex::new(client));

    let client_eh = client.clone();

    /*
     * Start event handler
     */
    let event_handler = tokio::spawn(async move {
        tracing::info!("Assigner event handler started");

        // Rescheduling channel: contains event that could not be processed immediately
        let (resch_tx, mut resch_rx) = mpsc::channel(100);

        // Publishing channel
        let (pub_tx, mut pub_rx) = mpsc::channel(100);

        // Data structure to keep track of most recent workers
        // ToDo: We don't use the value, so a HashSet maintaining insertion order, should suffice
        let mut avail_workers = WorkersBook::new();

        loop {
            tokio::select! {
                Some(msg) = relay_channel.recv() => {
                    if let Some(ev) = handle_event(msg, &mut avail_workers, &pub_tx).await {
                        if resch_tx.send(ev).await.is_err() {
                            tracing::error!("error while rescheduling message");
                        }
                    }
                }
                Some(msg) = resch_rx.recv() => {
                    tracing::debug!("Got rescheduled event");
                    if let Some(ev) = handle_event(RelayMessage::Event(msg), &mut avail_workers, &pub_tx).await {
                        if resch_tx.send(ev).await.is_err() {
                            tracing::error!("error while rescheduling message");
                        }
                    }
                }
                Some((workers, ev)) = pub_rx.recv() => {
                    tracing::debug!(r#"Sending "{}""#, ev.event.content);
                    let mut tags: Vec<Vec<String>> = Vec::with_capacity(workers.len());
                    for w in workers {
                        tags.push(vec!["p".to_string(), hex::encode(w.to_bytes())])
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
    pub_tx: &mpsc::Sender<PubStruct>,
) -> Option<RelayEvent> {
    match msg {
        RelayMessage::Event(ev) => {
            match ev.event.kind {
                Kind::ALIVE => {
                    let worker_id = ev.event.sender.clone();
                    book.insert(worker_id, ());
                    tracing::debug!("available workers = {}", book.len());
                    // ToDo: if above threshold, trim
                    if book.len() > MAX_WORKERS {
                        tracing::warn!("Too many registered workers: {}", book.len());
                    }
                }
                Kind::NEW_JOB => {
                    if book.len() < WORKERS_PER_JOB {
                        // not enough workers, reschedule
                        return Some(ev);
                    }
                    // Assign same job to N workers
                    let mut workers = Vec::with_capacity(WORKERS_PER_JOB);
                    for _ in 0..WORKERS_PER_JOB {
                        let (w, _) = book.pop_back().expect("There should be at enough workers");
                        workers.push(w);
                    }
                    pub_tx.send((workers, ev)).await.expect("");
                }
                _ => {}
            }
            None
        }
        _ => {
            tracing::warn!("non-event message");
            None
        }
    }
}
