pub mod global;
mod server;

use axum::{
    extract::{ws::WebSocketUpgrade, State},
    response::IntoResponse,
};
use std::net::SocketAddr;

//allows to extract the IP of connecting user
use crate::{ws::server::handle_websocket_connection, GlobalState};
use axum::extract::connect_info::ConnectInfo;
use axum::http::{HeaderMap, StatusCode};

pub async fn ws_wrapper(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    State(global_state): State<GlobalState>,
) -> impl IntoResponse {
    // ToDo: this is a naive protection against malicious/unwanted connections.
    //  In the future it will be handled differently
    let is_secure = headers
        .get("x-forwarded-proto")
        .map(|proto| proto == "https")
        .unwrap_or(false);
    // If it's not a secure connection, it's OK, we assume it's on the private network
    if is_secure {
        match headers.get("Origin").and_then(|hdr| hdr.to_str().ok()) {
            Some(origin) => {
                if !is_allowed(origin) {
                    tracing::warn!("Origin {origin} not allowed");
                    return StatusCode::FORBIDDEN.into_response();
                }
            }
            None => {
                tracing::warn!("Connection request without Origin");
                return StatusCode::BAD_REQUEST.into_response();
            }
        }
    }

    tracing::debug!("{addr} connected.");
    ws.on_upgrade(move |socket| handle_websocket_connection(socket, addr, global_state))
}

fn is_allowed(origin: &str) -> bool {
    let domain = origin.trim_start_matches("https://");
    domain.ends_with(".mizu.global") || domain == "mizu.global"
}
