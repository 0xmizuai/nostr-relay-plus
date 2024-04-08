use std::net::SocketAddr;

use axum::extract::ws::Message;
use nostr_surreal_db::message::notice::Notice;

use crate::message::IncomingMessage;

pub fn wrap_error_message(socket: &str, error: &anyhow::Error) -> Message {
    let error_notice = Notice::error(
        socket.to_string(), 
        &format!("{:?}", error)
    );

    wrap_ws_message(error_notice)
}

pub fn wrap_ws_message(outgoing: Notice) -> Message {
    Message::Text(outgoing.to_string())
}

pub fn unwrap_ws_message(ws_message: Message, who: SocketAddr) -> Option<IncomingMessage> {
    match ws_message {
        Message::Text(text) => {
            println!("Raw Text {text}");
            Some(serde_json::from_str(&text).unwrap())
        },
        Message::Binary(d) => {
            tracing::trace!(">>> {} sent {} bytes: {:?}", who, d.len(), d);
            None
        },
        Message::Close(c) => {
            if let Some(cf) = c {
                tracing::trace!(
                    ">>> {} sent close with code {} and reason `{}`",
                    who, cf.code, cf.reason
                );
            }

            None
        },
        Message::Pong(v) => {
            tracing::trace!(">>> {who} sent pong with {v:?}");

            None
        }
        // You should never need to manually handle Message::Ping, as axum's websocket library
        // will do so for you automagically by replying with Pong and copying the v according to
        // spec. But if you need the contents of the pings you can see them here.
        Message::Ping(v) => {
            tracing::trace!(">>> {who} sent ping with {v:?}");

            None
        }
    }
}