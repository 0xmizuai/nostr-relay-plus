use std::collections::HashMap;
use std::sync::Arc;
use serde_json::json;
use tokio::sync::Mutex;
use nostr_client_plus::client::Client;
use nostr_client_plus::event::UnsignedEvent;
use nostr_client_plus::request::{Filter, Request};
use nostr_crypto::eoa_signer::EoaSigner;
use nostr_crypto::sender_signer::SenderSigner;
use nostr_plus_common::relay_message::RelayMessage;

#[tokio::main]
async fn main() {
    // Create client
    let signer = EoaSigner::from_bytes(&[7; 32]);
    let mut client = Client::new(SenderSigner::Eoa(signer));
    let mut relay_channel = client.connect_with_channel("ws://127.0.0.1:3033").await.unwrap();

    let mut job_assigned = false;
    let mut client = Arc::new(Mutex::new(client));
    let mut client_clone = client.clone();

    // Create JobPost (kind == 6_000)
    let event = UnsignedEvent::new(client.lock().await.sender(), 0, 6_000, vec![], "job_1".to_string());
    let event_id = event.id();
    if client.lock().await.publish(event).await.is_err() {
        eprintln!("Cannot publish job");
        return;
    }

    // Start relay listener
    let listener_handle = tokio::spawn(async move {
        while let Some(msg) = relay_channel.recv().await {
            match msg {
                RelayMessage::Event(ev) => {
                    // Some clever logic to assign the job
                    if job_assigned {
                        println!("Too late, job is gone");
                    } else {
                        // Create JobAssigned (kind == 6_002)
                        let event = UnsignedEvent::new(
                            client_clone.lock().await.sender(),
                            0,
                            6_002,
                            vec![
                                vec!["#e".to_string(), hex::encode(event_id)],
                                vec!["#p".to_string(), hex::encode(ev.event.sender.to_bytes())],
                            ],
                            "job 1 assigned".to_string(),
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

    // Job is published, register for "booking" events (kind == 6_001)
    let filter = Filter {
        kinds: vec![6_001],
        tags: HashMap::from([
            ("e".to_string(), json!(hex::encode(event_id))),
        ]),
        ..Default::default()
    };
    let subscription_id = "ae4788ade947b42bb8b0d89c9fb3c129c10be87043c32190a96daa9e822a9bf6";
    let req = Request::new(subscription_id.to_string(), vec![filter]);
    client.lock().await.subscribe(req).await.unwrap();

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