use nostr_client_plus::client::Client;
use nostr_client_plus::event::UnsignedEvent;
use nostr_client_plus::request::{Filter, Request};
use nostr_crypto::schnorr_signer::SchnorrSigner;
use nostr_crypto::sender_signer::SenderSigner;
use std::collections::HashMap;
use std::time::Duration;

#[tokio::test]
async fn e2e() {
    // Generate signing key
    let signer = SchnorrSigner::from_bytes(&[2; 32]).unwrap();

    let mut client = Client::new(SenderSigner::Schnorr(signer));
    client.connect("ws://127.0.0.1:3033").await.unwrap();
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

    // Prepare event and send
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
    assert!(true);
}
