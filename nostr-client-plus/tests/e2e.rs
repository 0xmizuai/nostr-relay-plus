use nostr_client_plus::client::Client;
use nostr_client_plus::event::UnsignedEvent;
use nostr_client_plus::request::{Filter, Request};
use nostr_crypto::schnorr_signer::SchnorrSigner;
use nostr_crypto::sender_signer::SenderSigner;
use std::collections::HashMap;
use std::time::Duration;
use nostr_crypto::eoa_signer::EoaSigner;

#[tokio::test]
async fn e2e() {
    // Generate signing keys
    let schnorr_signer = SchnorrSigner::from_bytes(&[2; 32]).unwrap();
    let eoa_signer = EoaSigner::from_bytes(&[3; 32]);

    // Create clients (one signer <-> one client)
    let mut client = Client::new(SenderSigner::Schnorr(schnorr_signer));
    client.connect("ws://127.0.0.1:3033").await.unwrap();
    let mut client_2 = Client::new(SenderSigner::Eoa(eoa_signer));
    client_2.connect("ws://127.0.0.1:3033").await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Prepare subscription request and send
    let filter = Filter {
        ids: vec![],
        authors: vec![],
        kinds: vec![1],
        since: None,
        until: None,
        limit: None,
        tags: HashMap::new(),
    };
    let req = Request::new(
        "ae4788ade947b42bb8b0d89c9fbcc129c10be87043c32190a96daa9e822a9bf6".to_string(),
        vec![filter],
    );
    client.subscribe(req).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Prepare event for Schnorr client and send
    let event = UnsignedEvent::new(client.sender(), 0, 1, vec![], "Hello Rust".to_string());
    let event_id = event.id();
    match client.publish(event).await {
        Ok(_) => println!("Published OK: event {}", hex::encode(event_id)),
        Err(err) => {
            println!("{}", err);
            assert!(false);
        }
    }
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Prepare event for Eoa client and send
    let event = UnsignedEvent::new(client_2.sender(), 0, 1, vec![], "Hello Rust".to_string());
    let event_id = event.id();
    match client_2.publish(event).await {
        Ok(_) => println!("Published OK: event {}", hex::encode(event_id)),
        Err(err) => {
            println!("{}", err);
            assert!(false);
        }
    }
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(true);
}
