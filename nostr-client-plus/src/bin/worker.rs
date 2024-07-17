/*
  This is a dummy implementation of a worker. Not necessarily fully functional
*/
use nostr_client_plus::{
    client::Client,
    event::UnsignedEvent,
    job_protocol::Kind,
    request::{Filter, Request},
    utils::get_timestamp,
};
use nostr_crypto::eoa_signer::EoaSigner;
use nostr_crypto::sender_signer::SenderSigner;
use nostr_plus_common::relay_message::RelayMessage;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::select;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};

#[tokio::main]
async fn main() {
    // Parse cmdline
    let args: Vec<String> = std::env::args().collect();
    let worker_id: u8;
    if args.len() > 1 {
        worker_id = (&args[1]).parse().expect("Cannot convert to u8");
    } else {
        eprintln!("Missing worker id");
        return;
    }

    // Create client
    let signer = EoaSigner::from_bytes(&[worker_id; 32]);
    let mut client = Client::new(SenderSigner::Eoa(signer));
    let mut relay_channel = client
        .connect_with_channel("ws://127.0.0.1:3033")
        .await
        .unwrap();

    let client = Arc::new(Mutex::new(client));
    let client_id = hex::encode(client.lock().await.sender().to_bytes());
    let client_clone = client.clone();

    // Prepare heartbeat
    let mut heartbeat = interval(Duration::from_secs(20));

    // Start event handler
    let event_handler = tokio::spawn(async move {
        loop {
            select! {
                Some(msg) = relay_channel.recv() => {
                    match msg {
                        RelayMessage::Event(ev) => {
                            println!("Working on: {}", ev.event.content);
                        }
                        _ => eprintln!("Ignoring non-event message"),
                    }
                }
                _ = heartbeat.tick() => {
                    let event = UnsignedEvent::new(
                        client_clone.lock().await.sender(),
                        get_timestamp(),
                        Kind::ALIVE,
                        vec![],
                        String::default(),
                    );
                    client_clone.lock().await.publish(event).await.unwrap();
                    println!("Heartbeat sent");
                }
            }
        }
    });
    println!("Client {} listening", client_id);

    // subscription id used for Jobs Assign
    let sub_id_assign = "be4788ade0000000000000000000000000000000000000000000000000000000";

    // Subscribe to Assign
    let filter = Filter {
        kinds: vec![Kind::ASSIGN],
        since: Some(get_timestamp()),
        tags: HashMap::from([("#p".to_string(), json!([client_id]))]),
        ..Default::default()
    };
    let req = Request::new(sub_id_assign.to_string(), vec![filter]);
    client.lock().await.subscribe(req).await.unwrap();
    tracing::info!("Subscribed to NewJob and Alive");

    event_handler.await.unwrap();
}
