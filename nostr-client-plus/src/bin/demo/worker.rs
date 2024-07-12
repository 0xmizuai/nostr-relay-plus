use rand::Rng;
use std::collections::HashMap;
use std::env;
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
    let args: Vec<String> = env::args().collect();
    let mut salt: u8 = 1;
    if args.len() > 1 {
        salt = (&args[1]).parse().expect("Cannot convert to u8");
    } else {
        println!("No worker id provided: id = 1 assumed");
    }

    // Get 2 random subscription id
    let mut rng = rand::thread_rng();
    let mut random_bytes: [u8; 32] = rng.gen();
    let sub_id_6000 = hex::encode(random_bytes);
    println!("Subscription id for 6000 = {}", sub_id_6000);
    random_bytes[31] = random_bytes[31].wrapping_add(1);
    let sub_id_6002 = hex::encode(random_bytes);
    println!("Subscription id for 6002 = {}", sub_id_6002);

    // Create client
    let signer = EoaSigner::from_bytes(&[salt; 32]);
    let mut client = Client::new(SenderSigner::Eoa(signer));
    let mut relay_channel = client.connect_with_channel("ws://127.0.0.1:3033").await.unwrap();

    let client = Arc::new(Mutex::new(client));
    let client_clone = client.clone();
    let sub_id_6000_clone = sub_id_6000.clone();

    // Start relay listener
    let listener_handle = tokio::spawn(async move {
        let client_id = hex::encode(client_clone.lock().await.sender().to_bytes());
        println!("Client {} ready to listen", client_id);
        while let Some(msg) = relay_channel.recv().await {
            match msg {
                RelayMessage::Event(ev) => {
                    if ev.subscription_id == sub_id_6000_clone {
                        // Let's compete for this job
                        println!("Trying to get job: {}", ev.event.content);
                        // Prepare JobBookingAttempt (kind == 6_001)
                        let event = UnsignedEvent::new(
                            client_clone.lock().await.sender(),
                            12345,
                            6_001,
                            vec![vec!["e".to_string(), hex::encode(ev.event.id)]],
                            "trying to get this job".to_string(),
                        );
                        client_clone.lock().await.publish(event).await.unwrap();
                    } else { // It must be 6002
                        println!("YAY!");
                        println!("{}", serde_json::to_string(&ev).unwrap());
                    }
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
    let req = Request::new(sub_id_6000.to_string(), vec![filter]);
    println!("REQ subscription (kind = 6000)");
    client.lock().await.subscribe(req).await.unwrap();

    // Register for jobs assigned to us
    let client_id = hex::encode(client.lock().await.sender().to_bytes());
    let filter = Filter {
        kinds: vec![6_002],
        tags: HashMap::from([
            ("#p".to_string(), json!([client_id])),
        ]),
        ..Default::default()
    };
    let req = Request::new(sub_id_6002.to_string(), vec![filter]);
    println!("REQ subscription (kind = 6002): checking jobs for {}", client_id);
    client.lock().await.subscribe(req).await.unwrap();
    println!("All subscribed");

    listener_handle.await.unwrap();
}