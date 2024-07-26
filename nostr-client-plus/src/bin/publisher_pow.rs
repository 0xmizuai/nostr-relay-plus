use nostr_client_plus::client::Client;
use nostr_client_plus::crypto::CryptoHash;
use nostr_client_plus::event::UnsignedEvent;
use nostr_client_plus::job_protocol::{JobType, Kind, NewJobPayload, PayloadHeader};
use nostr_client_plus::utils::get_timestamp;
use nostr_crypto::eoa_signer::EoaSigner;
use nostr_crypto::sender_signer::SenderSigner;

mod utils;
use rand::random;
use utils::get_private_key_from_name;

#[tokio::main]
async fn main() {
    // Define needed env variables
    let relay_url = std::env::var("RELAY_URL").unwrap_or("ws://127.0.0.1:3033".to_string());

    // Command line parsing
    let args: Vec<String> = std::env::args().collect();
    let limit_publish: i64 = match args.len() {
        1 => 10_000_i64,
        2 => args[1].parse().expect("Invalid number"),
        _ => {
            eprintln!("Too many arguments");
            return;
        }
    };

    // Get a silly private key based on a string identifying the service.
    let private_key = get_private_key_from_name("Publisher").unwrap();

    // Create client
    let signer = EoaSigner::from_bytes(&private_key);
    let mut client = Client::new(SenderSigner::Eoa(signer));
    client.connect(relay_url.as_str()).await.unwrap();

    let timestamp_now = get_timestamp();

    // Get type job id and name
    let job_type = JobType::PoW.job_type();
    let job_type_str = JobType::PoW.as_ref();

    for _ in 0..limit_publish {
        let header = PayloadHeader {
            job_type,
            raw_data_id: CryptoHash::new(random()),
            time: timestamp_now,
        };
        let payload = NewJobPayload {
            header,
            kv_key: "pow".to_string(),
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
    println!("Done");
}
