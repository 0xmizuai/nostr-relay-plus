/*
  This is a dummy implementation of a worker. Not necessarily fully functional
*/
use anyhow::{anyhow, Result};
use nostr_client_plus::job_protocol::{NewJobPayload, ResultPayload};
use nostr_client_plus::{
    client::Client,
    event::UnsignedEvent,
    job_protocol::Kind,
    request::{Filter, Request},
};
use nostr_crypto::eoa_signer::EoaSigner;
use nostr_crypto::sender_signer::SenderSigner;
use nostr_plus_common::relay_event::RelayEvent;
use nostr_plus_common::relay_message::RelayMessage;
use nostr_plus_common::sender::Sender;
use nostr_plus_common::wire::EventOnWire;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::select;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};

mod utils;
use crate::utils::get_single_tag_entry;

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
                        RelayMessage::Event(ev) => match ev.event.kind {
                            Kind::ASSIGN => {
                                println!("Working on: {}", ev.event.content);
                                let sender = client_clone.lock().await.sender();
                                match process_assign(ev, sender) {
                                    Ok(ev) => {
                                        if client_clone.lock().await.publish(ev).await.is_err() {
                                            eprintln!("Cannot publish");
                                        }
                                    }
                                    Err(err) => eprintln!("{err}"),
                                }
                            }
                            _ => eprintln!("Expecting just Assign events"),
                        },
                        RelayMessage::Ok(ok_msg) => {
                            if ok_msg.accepted {
                                println!("Event {} accepted", hex::encode(ok_msg.event_id))
                            } else {
                                println!(r#"Event {} rejected: "{}""#, hex::encode(ok_msg.event_id), ok_msg.message);
                            }
                        }
                        _ => eprintln!("Ignoring non-event message"),
                    }
                }
                _ = heartbeat.tick() => {
                    let event = UnsignedEvent::new(
                        client_clone.lock().await.sender(),
                        chrono::Utc::now().timestamp() as u64,
                        Kind::ALIVE,
                        vec![
                            vec!["v".to_string(), "3".to_string()],
                        ],
                        String::default(),
                    );
                    client_clone.lock().await.publish(event).await.unwrap();
                    println!("Heartbeat sent");
                }
            }
        }
    });
    println!("Worker {} listening", client_id);

    // subscription id used for Jobs Assign
    let sub_id_assign = "be4788ade0000000000000000000000000000000000000000000000000000000";

    // Subscribe to Assign
    let filter = Filter {
        kinds: vec![Kind::ASSIGN],
        since: Some(chrono::Utc::now().timestamp() as u64),
        tags: HashMap::from([("#p".to_string(), json!([client_id]))]),
        ..Default::default()
    };
    let req = Request::new(sub_id_assign.to_string(), vec![filter]);
    client.lock().await.subscribe(req, None).await.unwrap();
    tracing::info!("Subscribed to NewJob and Alive");

    event_handler.await.unwrap();
}

fn process_assign(ev: RelayEvent, sender: Sender) -> Result<UnsignedEvent> {
    let job_id = get_single_tag_entry('e', &ev.event.tags)?.ok_or(anyhow!("Missing e tag"))?;
    let payload = get_new_job_payload(&ev)?;
    // Let's pretend we did something with the payload, and
    // now we submit the result
    get_result_event(ev.event, payload, sender, job_id)
}

fn get_new_job_payload(ev: &RelayEvent) -> Result<NewJobPayload> {
    let res: NewJobPayload = serde_json::from_str(ev.event.content.as_str())?;
    Ok(res)
}

fn get_result_event(
    assign_event: EventOnWire,
    payload: NewJobPayload,
    sender: Sender,
    job_id: String,
) -> Result<UnsignedEvent> {
    let fake_result = format!("Some result for {}", payload.header.raw_data_id.to_string());
    let result = ResultPayload {
        header: payload.header,
        output: fake_result,
        version: "v0.0.1".to_string(),
        kv_key: "fake_kv".to_string(),
    };
    let content = match serde_json::to_string(&result) {
        Ok(val) => val,
        Err(err) => {
            tracing::error!("Cannot serialize payload");
            return Err(anyhow!(err));
        }
    };
    let ev = UnsignedEvent::new(
        sender,
        chrono::Utc::now().timestamp() as u64,
        Kind::RESULT,
        vec![vec!["e".to_string(), job_id]],
        content,
    );
    Ok(ev)
}
