use mongodb::bson::from_document;
use mongodb::{Client as DbClient, Collection};
use nostr_client_plus::client::Client;
use nostr_client_plus::db::{left_anti_join, RawDataEntry};
use nostr_client_plus::event::UnsignedEvent;
use nostr_client_plus::job_protocol::{JobType, Kind, NewJobPayload, PayloadHeader};
use nostr_client_plus::utils::get_timestamp;
use nostr_crypto::eoa_signer::EoaSigner;
use nostr_crypto::sender_signer::SenderSigner;
use nostr_plus_common::relay_message::RelayMessage;
use std::time::Duration;
use tokio::time::{interval, Instant};

const TIMEOUT: Duration = Duration::from_secs(10);

#[tokio::main]
async fn main() {
    // Define needed env variables
    dotenv::dotenv().ok();
    let db_url = std::env::var("MONGO_URL").expect("MONGO_URL is not set");
    let relay_url = std::env::var("RELAY_URL").unwrap_or("ws://127.0.0.1:3033".to_string());
    let raw_private_key = std::env::var("PUBLISHER_PRIVATE_KEY").unwrap();

    // Command line parsing
    let args: Vec<String> = std::env::args().collect();
    let limit_publish: i64 = match args.len() {
        1 => 1000_i64,
        2 => args[1].parse().expect("Invalid number"),
        _ => {
            eprintln!("Too many arguments");
            return;
        }
    };

    // Configure DB from args
    let db = DbClient::with_uri_str(db_url)
        .await
        .expect("Cannot connect to db")
        .database("test-preprocessor");
    let collection: Collection<RawDataEntry> = db.collection("raw_data");

    // Private key
    let private_key = hex::decode(raw_private_key).unwrap().try_into().unwrap();

    // Create client
    let signer = EoaSigner::from_bytes(&private_key);
    let mut client = Client::new(SenderSigner::Eoa(signer));
    let mut relay_channel = client
        .connect_with_channel(relay_url.as_str())
        .await
        .unwrap();

    // start a listener just for OK messages
    let mut timeout_timer = interval(TIMEOUT);
    let mut last_activity = Instant::now();
    let listener_handle = tokio::spawn(async move {
        println!("OK Listener started");
        loop {
            tokio::select! {
                Some(RelayMessage::Ok(ok_msg)) = relay_channel.recv() => {
                    last_activity = Instant::now();
                    if ok_msg.accepted {
                        println!("Job published: {}", hex::encode(ok_msg.event_id));
                    } else {
                        println!(r#"Job {} rejected: "{}""#, hex::encode(ok_msg.event_id), ok_msg.message);
                    }
                }
                _ = timeout_timer.tick() => {
                    let now = Instant::now();
                    if now.duration_since(last_activity) >= TIMEOUT {
                        println!("No more OK messages within the last {}s, closing", TIMEOUT.as_secs());
                        break;
                    }
                }
                else => {
                    println!("unhandled event");
                }
            }
        }
    });

    let timestamp_now = get_timestamp();

    let entries = left_anti_join(&collection, "finished_jobs", limit_publish)
        .await
        .unwrap();

    // Get type job id and name
    let job_type = JobType::Classification.job_type();
    let job_type_str = JobType::Classification.as_ref();

    for entry in entries {
        let entry: RawDataEntry = from_document(entry).unwrap();
        println!("Entry {}", entry._id.to_string());
        let header = PayloadHeader {
            job_type,
            raw_data_id: entry._id,
            time: timestamp_now,
        };
        let payload = NewJobPayload {
            header,
            kv_key: entry.r2_key,
            config: None,
        };
        let event = UnsignedEvent::new(
            client.sender(),
            timestamp_now,
            Kind::NEW_JOB,
            vec![vec!["t".to_string(), job_type_str.to_string()]],
            serde_json::to_string(&payload).expect("Payload serialization failed"),
        );
        if client.publish(event).await.is_err() {
            eprintln!("Cannot publish job");
        }
    }

    listener_handle.await.unwrap();
    println!("Done");
}
