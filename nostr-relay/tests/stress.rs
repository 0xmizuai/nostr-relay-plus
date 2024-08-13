use nostr_client_plus::client::Client;
use nostr_client_plus::event::UnsignedEvent;
use nostr_client_plus::job_protocol::Kind;
use nostr_crypto::eoa_signer::EoaSigner;
use nostr_crypto::sender_signer::SenderSigner;
use nostr_plus_common::relay_message::RelayMessage;
use std::time::Duration;

#[tokio::test]
#[ignore]
async fn stress_hb() {
    const NUM_WORKERS: u8 = 255;
    const NUM_HEARTBEATS: usize = 100;
    const TIMEOUT: Duration = Duration::from_secs(5);
    const HB_PAUSE: Duration = Duration::from_secs(1);

    let mut handles_vec = Vec::new();
    for idx in 0..NUM_WORKERS {
        let handle = tokio::spawn(async move {
            let mut ok_counter: u64 = 0; // count OK occurences (ACKs for HB events)
            let signer = EoaSigner::from_bytes(&[idx; 32]);
            let mut client = Client::new(SenderSigner::Eoa(signer));
            let mut relay_channel = client
                .connect_with_channel("ws://127.0.0.1:3033")
                .await
                .unwrap();

            for _ in 0..NUM_HEARTBEATS {
                let hb = UnsignedEvent::new(
                    client.sender(),
                    chrono::Utc::now().timestamp() as u64,
                    Kind::ALIVE,
                    vec![vec!["v".to_string(), "1".to_string()]],
                    String::default(),
                );
                client.publish(hb).await.unwrap();
                tokio::time::sleep(HB_PAUSE).await;
            }

            loop {
                match tokio::time::timeout(TIMEOUT, relay_channel.recv()).await {
                    Ok(Some(val)) => match val {
                        RelayMessage::Ok(_) => ok_counter += 1,
                        _ => {}
                    },
                    Ok(None) => {
                        eprintln!("None in task {idx}");
                    }
                    Err(_) => {
                        eprintln!("Timeout in task {idx}. If counter is {NUM_HEARTBEATS}, then it's normal");
                        break;
                    }
                }
            }
            ok_counter
        });
        handles_vec.push(handle);
    }

    let mut tot_counter: u64 = 0;
    for (idx, handle) in handles_vec.into_iter().enumerate() {
        let (res,) = tokio::join!(handle);
        match res {
            Ok(counter) => {
                println!("OK count = {} for task {}", counter, idx);
                tot_counter += counter;
            }
            Err(_) => {
                eprintln!("Error for task {idx}");
            }
        }
    }
    assert_eq!(NUM_HEARTBEATS as u64 * NUM_WORKERS as u64, tot_counter);
    println!("total ACKs received = {}", tot_counter);
}
