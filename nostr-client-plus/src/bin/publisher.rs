use anyhow::Result;
use mongodb::bson::from_document;
use mongodb::{Client as DbClient, Collection};
use nostr_client_plus::client::Client;
use nostr_client_plus::crypto::CryptoHash;
use nostr_client_plus::db::{left_anti_join, RawDataEntry};
use nostr_client_plus::event::UnsignedEvent;
use nostr_client_plus::job_protocol::{JobType, Kind, NewJobPayload, PayloadHeader};
use nostr_crypto::eoa_signer::EoaSigner;
use nostr_crypto::sender_signer::SenderSigner;
use nostr_plus_common::relay_message::RelayMessage;
use rand::random;
use std::convert::TryInto;
use std::time::Duration;
use tokio::time::{interval, Instant};

mod utils;
use crate::utils::get_queued_jobs;

const TIMEOUT: Duration = Duration::from_secs(10);
const LOW_VAL_JOBS: usize = 5_000;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("{}", err);
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    // Define needed env variables
    dotenv::dotenv().ok();
    let db_url = std::env::var("MONGO_URL").expect("MONGO_URL is not set");
    let relay_url = std::env::var("RELAY_URL").unwrap_or("ws://127.0.0.1:3033".to_string());
    let raw_private_key =
        std::env::var("PUBLISHER_PRIVATE_KEY").expect("Missing PUBLISHER_PRIVATE_KEY");
    let metrics_server = std::env::var("PROMETHEUS_URL").expect("Missing PROMETHEUS_URL");
    let low_val_jobs = std::env::var("JOBS_THRESHOLD").unwrap_or(LOW_VAL_JOBS.to_string());
    let low_val_jobs: usize = low_val_jobs.parse()?;
    // Percentage (0-100) of Classifications jobs. Remainder is PoW jobs.
    let classification_job_percentage = match std::env::var("CLASSIFICATION_PERCENT")
        .unwrap_or("100".to_string())
        .parse::<u8>()
    {
        Ok(val) => {
            // check range
            match val {
                0..=100 => val,
                _ => panic!("CLASSIFICATION_PERCENT must be in range [0, 100]"),
            }
        }
        Err(err) => {
            panic!("Failed to parse CLASSIFICATION_PERCENT to u8: {err}");
        }
    };
    println!(
        "{}% of all jobs published should be classification job",
        classification_job_percentage
    );

    // Command line parsing
    let args: Vec<String> = std::env::args().collect();
    let limit_publish: i64 = match args.len() {
        1 => 1000_i64,
        2 => args[1].parse().expect("Invalid number"),
        _ => {
            eprintln!("Too many arguments");
            return Ok(());
        }
    };

    // Check if jobs are low and, if not, exit
    let queued_jobs = get_queued_jobs(metrics_server.as_str(), "cached_jobs").await?;
    if queued_jobs > low_val_jobs {
        eprintln!("Enough jobs in the queue");
        return Ok(());
    }

    // Configure DB from args
    let db = DbClient::with_uri_str(db_url)
        .await
        .expect("Cannot connect to db")
        .database("test-preprocessor");
    let collection: Collection<RawDataEntry> = db.collection("raw_data");

    // Private key
    let private_key = hex::decode(raw_private_key)?.try_into().unwrap();

    // Create client
    let signer = EoaSigner::from_bytes(&private_key);
    let mut client = Client::new(SenderSigner::Eoa(signer));
    let mut relay_channel = client.connect_with_channel(relay_url.as_str()).await?;

    // start a listener just for OK messages
    let mut timeout_timer = interval(TIMEOUT);
    let mut last_activity = Instant::now();
    let listener_handle = tokio::spawn(async move {
        println!("OK Listener started");
        let mut ack_received: u32 = 0;
        loop {
            tokio::select! {
                Some(RelayMessage::Ok(ok_msg)) = relay_channel.recv() => {
                    last_activity = Instant::now();
                    ack_received += 1;
                    if ok_msg.accepted {
                        println!("Job published: {}", hex::encode(ok_msg.event_id));
                    } else {
                        eprintln!(r#"Job {} rejected: "{}""#, hex::encode(ok_msg.event_id), ok_msg.message);
                    }
                }
                _ = timeout_timer.tick() => {
                    let now = Instant::now();
                    if now.duration_since(last_activity) >= TIMEOUT {
                        println!("No more OK messages within the last {}s, closing", TIMEOUT.as_secs());
                        println!("{} ACK received in total", ack_received);
                        break;
                    }
                }
                else => {
                    println!("unhandled event");
                }
            }
        }
    });

    let timestamp_now = chrono::Utc::now().timestamp() as u64;

    // Let's figure out number of classifications and pow to send out
    let classification_count = (limit_publish * classification_job_percentage as i64) / 100_i64;
    println!(
        "Plan to publish {} classification jobs",
        classification_count
    );

    // Now let's fetch all classification entries needed.
    // ToDo: probably if we have fewer classification jobs and fill the rest with PoW is not
    //  what we want. Review it later.
    let entries = left_anti_join(&collection, "finished_jobs", limit_publish).await?;
    println!(
        "Actually fetched {} classifications to publish: PoW will fill the rest",
        entries.len()
    );
    let pow_entries = limit_publish - entries.len() as i64;

    /*
     * Publish classification jobs
     */
    let classification_type = JobType::Classification.job_type();
    let classification_type_str = JobType::Classification.as_ref();
    let mut jobs_sent = 0;
    for entry in entries {
        let entry: RawDataEntry = from_document(entry)?;
        let header = PayloadHeader {
            job_type: classification_type,
            raw_data_id: entry._id,
            time: timestamp_now,
        };
        let payload = NewJobPayload {
            header,
            kv_key: entry.r2_key,
            config: None,
            validator: "default".to_string(),
            classifier: "default".to_string(),
        };
        let event = UnsignedEvent::new(
            client.sender(),
            timestamp_now,
            Kind::NEW_JOB,
            vec![vec!["t".to_string(), classification_type_str.to_string()]],
            serde_json::to_string(&payload).expect("Payload serialization failed"),
        );
        if client.publish(event).await.is_err() {
            eprintln!("Cannot publish job");
        } else {
            jobs_sent += 1;
        }
    }

    /*
     * Publish PoW jobs
     */
    let pow_type = JobType::PoW.job_type();
    let pow_type_str = JobType::PoW.as_ref();
    for _ in 0..pow_entries {
        let header = PayloadHeader {
            job_type: pow_type,
            raw_data_id: CryptoHash::new(random()),
            time: timestamp_now,
        };
        let payload = NewJobPayload {
            header,
            kv_key: "pow".to_string(),
            config: None,
            validator: "default".to_string(),
            classifier: "default".to_string(),
        };
        let event = UnsignedEvent::new(
            client.sender(),
            timestamp_now,
            Kind::NEW_JOB,
            vec![vec!["t".to_string(), pow_type_str.to_string()]],
            serde_json::to_string(&payload).expect("Payload serialization failed"),
        );
        if client.publish(event).await.is_err() {
            eprintln!("Cannot publish job");
        } else {
            jobs_sent += 1;
        }
    }

    listener_handle.await?;
    println!("Done: {} jobs sent", jobs_sent);
    Ok(())
}
