use std::net::SocketAddr;

use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use tokio::{select, sync::mpsc};
use tokio::time::{Duration, Instant, interval, sleep};

use crate::{local::LocalState, util::wrap_error_message};
use crate::GlobalState;
use crate::message::IncomingMessage;
use crate::util::{wrap_ws_message, unwrap_ws_message};

const PING_INTERVAL: Duration = Duration::from_secs(300);
const PONG_TIMEOUT: Duration = Duration::from_secs(20);

pub async fn handle_websocket_connection(
    socket: WebSocket, 
    who: SocketAddr,
    global_state: GlobalState,
) {
    tracing::info!("New WS connection from {}", who);
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // 1. spawn the local connection state 
    let (outgoing_sender, mut outgoing_receiver) = mpsc::unbounded_channel();
    let mut local_state = LocalState::new(
        who, outgoing_sender,
        global_state.clone()
    );

    // 2. send the initial auth request 
    local_state.start_auhentication().await;

    // 3. Prepare Ping variables used for Ping and Pong timeout logic:
    let mut ping_interval_timer = interval(PING_INTERVAL);
    let mut pong_timeout_timer = Box::pin(sleep(PONG_TIMEOUT));
    let mut last_activity = Instant::now(); // last time we saw something on WS
    let mut waiting_for_pong = false;


    // 4. start the main event loop
    loop {
        select! {
            msg = ws_receiver.next() => match msg {
                Some(Ok(Message::Text(ws_message))) => {
                    // Reset activity
                    last_activity = Instant::now();

                    if let Ok(incoming) = serde_json::from_str(&ws_message) {
                        let result = local_state.handle_incoming_message(incoming).await;
                        // ToDo: the following block does not distinguish between critical and
                        //  non-critical errors. Closing the websocket connection is too drastic,
                        //  replace with tracing, for the time being
                        if result.is_err() {
                            // let closing_notice = wrap_error_message("ws something", &result.err().unwrap());
                            // let _ = ws_sender.send(closing_notice).await;
                            // ws_sender.close().await.unwrap();
                            tracing::error!("{}", result.unwrap_err());
                        }
                    } else {
                        tracing::error!("Cannot deserialize message from {}", who);
                    }
                }
                Some(Ok(Message::Close(c))) => {
                    if let Some(cf) = c {
                        tracing::debug!(
                            ">>> {} sent close with code {} and reason `{}`",
                            who, cf.code, cf.reason
                        );
                    } else {
                        tracing::debug!(">>> {} sent close", who);
                    }
                    tracing::debug!(">>> Closing our side, too");
                    break;
                }
                Some(Ok(Message::Pong(_))) => {
                    // Reset activity
                    last_activity = Instant::now();

                    waiting_for_pong = false;
                }
                Some(Ok(Message::Ping(_))) => {
                    // Reset activity
                    last_activity = Instant::now();
                }
                Some(Ok(Message::Binary(msg))) => {
                    // Reset activity
                    last_activity = Instant::now();
                    tracing::info!("Binary message received from {}", who);

                    match local_state.handle_binary_message(msg.as_ref()) {
                        Ok(msg) => {
                            if ws_sender.send(Message::Binary(msg)).await.is_err() {
                                tracing::error!("Cannot send binary msg");
                            } else {
                                tracing::info!("Binary reply to {}", who);
                            }
                        }
                        Err(err) => tracing::warn!("{}", err),
                    }
                },
                Some(Err(e)) => tracing::error!("While receiving from websocket: {}", e),
                None => {
                    tracing::error!("Error on websocket: closing our side");
                    break;
                }
            },
            // Handle Pong timeouts
            _ = Pin::new(&mut pong_timeout_timer), if waiting_for_pong => {
                    tracing::error!("Pong timeout exceeded for address {}", who);
                    break;
            }
            // Send Ping if connection not active for more than PING_INTERVAL
            _ = ping_interval_timer.tick() => {
                let now = Instant::now();
                tracing::debug!("{} Check if inactive", who);
                if now.duration_since(last_activity) >= PING_INTERVAL {
                    tracing::debug!("Sending Ping to {}", who);
                    if let Err(err) = ws_sender.send(Message::Ping(Vec::new())).await {
                        tracing::error!("Unable to send ping: {}", err);
                        break;
                    }
                    waiting_for_pong = true;
                    pong_timeout_timer = Box::pin(sleep(PONG_TIMEOUT)); // Restart the pong timeout
                    last_activity = now;
                } else {
                    tracing::debug!("No need to send ping");
                }
            }
            maybe_event = local_state.global_state.global_events_pub_receiver.recv() => {
                match maybe_event {
                    Ok(event) => {
                        let result = local_state.handle_global_incoming_events(event).await;
                        // ToDo: this handling is wrong because any error with broadcasting, including
                        //  unwanted events, results in the termination of the main websocket.
                        // if result.is_err() {
                        //     let closing_notice = wrap_error_message("glob something", &result.err().unwrap());
                        //     let _ = ws_sender.send(closing_notice).await;
                        //     ws_sender.close().await.unwrap();
                        // }
                    }
                    Err(err) => tracing::error!("global event receiver: {}", err),
                }
            },
            Some(outgoing) = outgoing_receiver.recv() => {
                let msg = wrap_ws_message(outgoing);
                let _ = ws_sender.send(msg).await;
                tracing::debug!("Replied to {}", local_state.client_ip_addr);
            }
            else => {
                tracing::warn!("Unhandled case in select statement");
            }
        }
    }
    tracing::warn!("Stopped serving connection for {}", who);
}
