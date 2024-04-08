use std::net::SocketAddr;

use axum::extract::ws::{self, Message, WebSocket};
use futures::{SinkExt, StreamExt};
use nostr_surreal_db::{message::{events::Event, filter::Filter, notice::Notice}, DB};
use tokio::{select, sync::{broadcast, mpsc, oneshot}};

use crate::{local::LocalState, util::wrap_error_message};
use crate::message::{IncomingMessage, OutgoingMessage};
use crate::GlobalState;
use crate::util::{wrap_ws_message, unwrap_ws_message};

pub async fn handle_websocket_connection(
    socket: WebSocket, 
    who: SocketAddr,
    global_state: GlobalState,
) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // 1. spawn the local connection state 
    let (outgoing_sender, mut outgoing_receiver) = mpsc::channel(100);    
    let mut local_state = LocalState::new(
        who, outgoing_sender,
        global_state.clone()
    );

    // 2. send the initial auth request 
    local_state.start_auhentication().await;

    // 4. start the main event loop
    loop {
        select! {
            Some(Ok(ws_message)) = ws_receiver.next() => {
                let maybe_incoming = unwrap_ws_message(ws_message, who);
                if let Some(incoming) = maybe_incoming {
                    let result = local_state.handle_incoming_message(incoming).await;
                    println!("{:?}", result);
                    if result.is_err() {
                        let closing_notice = wrap_error_message("something", &result.err().unwrap());
                        let _ = ws_sender.send(closing_notice).await;
                        ws_sender.close().await.unwrap();
                    }
                }
            },
            Ok(event) = local_state.global_state.global_events_pub_receiver.recv() => {
                let result = local_state.handle_global_incoming_events(event).await;
                if result.is_err() {
                    let closing_notice = wrap_error_message("something", &result.err().unwrap());
                    let _ = ws_sender.send(closing_notice).await;
                    ws_sender.close().await.unwrap();
                }
            },
            Some(outgoing) = outgoing_receiver.recv() => {
                let msg = wrap_ws_message(outgoing);
                let _ = ws_sender.send(msg).await;
            }
        }
    }
}
