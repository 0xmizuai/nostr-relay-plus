mod server;
pub mod global;

use axum::{
    extract::{ws::WebSocketUpgrade, State},
    response::IntoResponse,
};
use std::net::SocketAddr;

//allows to extract the IP of connecting user
use axum::extract::connect_info::ConnectInfo;

use crate::{
    ws::server::handle_websocket_connection,
    GlobalState
};

pub async fn ws_wrapper(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(global_state): State<GlobalState>,
) -> impl IntoResponse {
    tracing::debug!("{addr} connected.");
    ws.on_upgrade(move |socket| handle_websocket_connection(socket, addr, global_state))
}
