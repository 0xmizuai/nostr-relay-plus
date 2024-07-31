use std::collections::HashMap;
use std::sync::Arc;
use serde_json::json;
use tokio::sync::Mutex;
use tracing_subscriber::FmtSubscriber;

use nostr_client_plus::client::Client;
use nostr_client_plus::event::UnsignedEvent;
use nostr_client_plus::request::{Filter, Request};
use nostr_crypto::eoa_signer::EoaSigner;
use nostr_crypto::sender_signer::SenderSigner;
use nostr_plus_common::relay_message::RelayMessage;

#[path = "../utils.rs"]
mod utils;
use utils::{get_mins_before_now_timestamp, get_single_tag_entry};

type PublisherEv = (String, Vec<String>);

#[tokio::main]
async fn main() {
    // Logger setup
    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("setting default subscriber failed");

    // Create client
    let signer = EoaSigner::from_bytes(&[7; 32]);
    let mut client = Client::new(SenderSigner::Eoa(signer));
    let mut relay_channel = client.connect_with_channel("ws://127.0.0.1:3033").await.unwrap();
    let client = Arc::new(Mutex::new(client));
    let client_clone = client.clone();

    let mut job_assigned = false;

    /*
     * Start Publisher task
     */
    let subscription_id = "ae4788ade947b42bb8b0d89c9fb3c129c10be87043c32190a96daa9e822a9bf6"; // Subscription_id used for accepting JobBooking events
    let (pub_tx, mut pub_rx) = tokio::sync::mpsc::channel::<PublisherEv>(1);
    let _publisher_handle = tokio::spawn(async move {
        tracing::info!("Publisher task started");

        while let Some((content, _winners)) = pub_rx.recv().await {
            let timestamp = get_mins_before_now_timestamp(0);

            // Create JobPost (kind == 6_000)
            let event = UnsignedEvent::new(
                client.lock().await.sender(),
                timestamp,
                6_000,
                vec![],
                content,
            );
            let event_id = event.id(); // id of original job post, keep it for following events
            if client.lock().await.publish(event).await.is_err() {
                eprintln!("Cannot publish job");
                return;
            }

            // Job is published, register for "booking" events (kind == 6_001)
            let filter = Filter {
                kinds: vec![6_001],
                tags: HashMap::from([
                    ("#e".to_string(), json!([hex::encode(event_id)])),
                ]),
                ..Default::default()
            };
            let req = Request::new(subscription_id.to_string(), vec![filter]);
            client.lock().await.subscribe(req).await.unwrap();
        }
        tracing::error!("Publisher error while reading from channel: task has exited");
    });

    // Start relay listener
    let listener_handle = tokio::spawn(async move {
        println!("Publisher ready to listen");
        while let Some(msg) = relay_channel.recv().await {
            match msg {
                RelayMessage::Event(ev) => {
                    // Here: some clever logic to assign the job
                    // This is just first-come, first-served
                    if job_assigned {
                        println!("Too late, job is gone: unsubscribing");
                        // Cancel subscription
                        client_clone.lock().await.close_subscription(subscription_id.to_string()).await.unwrap();
                    } else {
                        let event_publisher_id = hex::encode(ev.event.sender.to_bytes());
                        let job_id = match get_single_tag_entry('e', &ev.event.tags) {
                            Err(err) => {
                                tracing::error!("{err}");
                                continue
                            }
                            Ok(None) => {
                                tracing::error!("e tag not found in JobBooking");
                                continue
                            }
                            Ok(Some(val)) => val,
                        };
                        println!("Going to assign job to {}", event_publisher_id);
                        // Create JobAssigned (kind == 6_002)
                        let event = UnsignedEvent::new(
                            client_clone.lock().await.sender(),
                            get_mins_before_now_timestamp(0),
                            6_002,
                            vec![
                                vec!["e".to_string(), hex::encode(job_id)],
                                vec!["p".to_string(), event_publisher_id],
                            ],
                            "Job 1 assigned".to_string(),
                        );
                        match client_clone.lock().await.publish(event).await {
                            Ok(_) => {
                                job_assigned = true;
                            }
                            Err(err) => {
                                println!("CANNOT PUBLISH EVENT: {}", err);
                            }
                        }
                    }
                }
                _ => println!("Non-Event message: ignored"),
            }
        }
    });

    // Publish Job
    pub_tx.send(("Job 1".to_string(), vec![])).await.unwrap();

    listener_handle.await.unwrap();
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use nostr_plus_common::wire::FilterOnWire;

    #[test]
    fn xxx() {
        let filter_1 = FilterOnWire {
            ids: vec![],
            authors: vec![],
            kinds: vec![1],
            since: None,
            until: None,
            limit: None,
            tags: HashMap::new(),
        };

        let filter_2 = FilterOnWire {
            kinds: vec![1],
            ..Default::default()
        };

        assert_eq!(filter_1, filter_2)
    }
}