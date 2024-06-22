use std::collections::HashMap;
use std::sync::Arc;
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

    let mut client = Arc::new(Mutex::new(client));
    let mut client_clone = client.clone();

    // Start relay listener
    let listener_handle = tokio::spawn(async move {
        while let Some(msg) = relay_channel.recv().await {
            match msg {
                RelayMessage::Event(ev) => {
                    // Let's compete for this job
                    println!("Trying to get job: {}", ev.event.content);
                    // Prepare JobBookingAttempt (kind == 6_001)
                    let event = UnsignedEvent::new(
                        client_clone.lock().await.sender(),
                        0,
                        6_001,
                        vec![vec!["#e".to_string(), hex::encode(ev.event.id)]],
                        "trying to get this job".to_string(),
                    );
                    client_clone.lock().await.publish(event).await.unwrap();
                }
                _ => println!("Non-Event message: ignored"),
            }
        }
    });

    // Register for jobs available (kind == 6_000)
    let filter = Filter {
        kinds: vec![6_000],
        tags: HashMap::new(),
        ..Default::default()
    };
    let subscription_id = "ae4788ade947b42bb8b0d89c9fb3c129c10be87043c32190a96daa9e822a9b00";
    let req = Request::new(subscription_id.to_string(), vec![filter]);
    client.lock().await.subscribe(req).await.unwrap();


    listener_handle.await.unwrap();
}