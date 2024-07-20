use mongodb::bson::from_document;
use mongodb::{Client as DbClient, Collection};
use nostr_client_plus::client::Client;
use nostr_client_plus::db::{left_anti_join, RawDataEntry};
use nostr_client_plus::event::UnsignedEvent;
use nostr_client_plus::job_protocol::{Kind, NewJobPayload, PayloadHeader};
use nostr_client_plus::utils::get_timestamp;
use nostr_crypto::eoa_signer::EoaSigner;
use nostr_crypto::sender_signer::SenderSigner;

mod utils;
use utils::get_private_key_from_name;

#[tokio::main]
async fn main() {
    // Command line parsing
    let args: Vec<String> = std::env::args().collect();
    let relay_url = match args.len() {
        1 => String::from("ws://127.0.0.1:3033"),
        2 => args[1].clone(),
        _ => {
            eprintln!("Too many arguments");
            return;
        }
    };

    // Configure DB from env
    let db_url = std::env::var("MONGO_URL").expect("MONGO_URL is not set");
    let db = DbClient::with_uri_str(db_url)
        .await
        .expect("Cannot connect to db")
        .database("test-preprocessor");
    let collection: Collection<RawDataEntry> = db.collection("raw_data");

    // Get a silly private key based on a string identifying the service.
    let private_key = get_private_key_from_name("Publisher").unwrap();

    // Create client
    let signer = EoaSigner::from_bytes(&private_key);
    let mut client = Client::new(SenderSigner::Eoa(signer));
    client.connect(relay_url.as_str()).await.unwrap();

    let timestamp_now = get_timestamp();

    let entries = left_anti_join(&collection, "finished_jobs", 3)
        .await
        .unwrap();
    for entry in entries {
        let entry: RawDataEntry = from_document(entry).unwrap();
        println!("Entry {}", entry._id.to_string());
        let header = PayloadHeader {
            job_type: 0,
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
            vec![], // ToDo: there should be a `t` tag
            serde_json::to_string(&payload).expect("Payload serialization failed"),
        );
        if client.publish(event).await.is_err() {
            eprintln!("Cannot publish job");
        }
    }
    println!("Done");
}
