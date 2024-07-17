use std::collections::HashMap;
use nostr_client_plus::client::Client;
use nostr_client_plus::event::UnsignedEvent;
use nostr_client_plus::request::{Filter, Request};
use nostr_crypto::eoa_signer::EoaSigner;
use nostr_crypto::schnorr_signer::SchnorrSigner;
use nostr_crypto::sender_signer::SenderSigner;
use std::time::Duration;
use serde_json::json;
use nostr_plus_common::types::Timestamp;

#[tokio::test]
async fn e2e() {
    // Generate signing keys
    let schnorr_signer = SchnorrSigner::from_bytes(&[2; 32]).unwrap();
    let eoa_signer = EoaSigner::from_bytes(&[3; 32]);

    // Create clients (one signer <-> one client)
    let mut client = Client::new(SenderSigner::Schnorr(schnorr_signer));
    client.connect("ws://127.0.0.1:3033").await.unwrap();
    let mut client_2 = Client::new(SenderSigner::Eoa(eoa_signer));
    let mut eoa_rcv = client_2
        .connect_with_channel("ws://127.0.0.1:3033")
        .await
        .unwrap();
    // Start a handler feeding from the receiver, otherwise it will fill-up
    tokio::spawn(async move {
        while let Some(msg) = eoa_rcv.recv().await {
            match serde_json::to_string(&msg) {
                Ok(msg) => println!("Handled: {}", msg),
                Err(err) => println!("{err}"),
            }
        }
    });
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Prepare subscription request and send
    let filter = Filter {
        kinds: vec![1],
        tags: HashMap::from([
            ("#e".to_string(), json!([hex::encode([1, 2, 3])])),
        ]),
        ..Default::default()
    };
    let req = Request::new(
        "ae4788ade947b42bb8b0d89c9fbcc129c10be87043c32190a96daa9e822a9bf6".to_string(),
        vec![filter],
    );
    client_2.subscribe(req).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Prepare event for Schnorr client and send
    let event = UnsignedEvent::new(
        client.sender(),
        Timestamp::default(),
        1,
        vec![vec![
            "e".to_string(),
            "5c83da77af1dec6d7289834998ad7aafbd9e2191396d75ec3cc27f5a77226f36".to_string(),
        ]],
        "Hello Rust".to_string(),
    );
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
    let event = UnsignedEvent::new(
        client_2.sender(),
        Timestamp::default(),
        1,
        vec![],
        "Hello Rust".to_string()
    );
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
