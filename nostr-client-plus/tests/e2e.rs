use alloy_primitives::address;
use nostr_client_plus::client::Client;
use nostr_client_plus::event::Event;
use nostr_client_plus::request::{Filter, Request};
use nostr_surreal_db::message::sender::Sender;
use std::time::Duration;

#[tokio::test]
async fn e2e() {
    let mut client = Client::new();
    client.connect("ws://127.0.0.1:3033").await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Prepare subscription request and send
    let filter = Filter {
        ids: None,
        authors: None,
        kinds: Some(vec![1]),
        e: None,
        p: None,
        since: None,
        until: None,
        limit: None,
    };
    let req = Request::new(
        Some("ae4788ade947b42bb8b0d89c9fbcc129c10be87043c32190a96daa9e822a9bf6"),
        vec![filter],
    );
    client.subscribe(req).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Prepare event and send
    let event = Event::new(
        Sender::EoaAddress(address!("4838B106FCe9647Bdf1E7877BF73cE8B0BAD5f98")),
        0,
        1,
        vec![],
        "Hello Rust".to_string(),
    );
    client.publish(event).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(true);
}
