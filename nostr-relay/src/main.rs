use std::net::SocketAddr;

use anyhow::Result;

use axum::routing::get;
use axum::Router;
use nostr_plus_common::logging::init_tracing;
use nostr_relay::__private::metrics::{
    metrics_handler, REGISTRY, RX_EVENT_COUNTER, TX_EVENT_COUNTER, WS_CONNECTIONS,
};
use nostr_relay::{ws_wrapper, GlobalState, ALLOWED_DOMAINS};
use tower_http::cors::CorsLayer;

#[tokio::main]
async fn main() -> Result<()> {
    // Check needed env variables
    if (*ALLOWED_DOMAINS).is_empty() {
        // This variable is filled with content from env::ALLOWED_DOMAINS. If it's empty
        // it means the env variable was either missing or empty.
        eprintln!("ALLOWED_DOMAINS env variable not provided");
        return Ok(());
    }

    // Basic command line parsing
    let args: Vec<String> = std::env::args().collect();
    let mut is_local = true;
    if args.len() > 1 && args[1] == "--remote" {
        is_local = false;
    }

    // Logger setup
    init_tracing();

    // Initialize metrics
    register_metrics();

    let cors = CorsLayer::very_permissive();
    // .allow_methods([Method::GET, Method::POST])
    // .allow_origin(Any);

    let global_state = if is_local {
        GlobalState::new_with_local_db().await?
    } else {
        GlobalState::new_with_remote_db().await?
    };
    let app = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/metrics", get(metrics_handler))
        .route("/", get(ws_wrapper))
        .layer(cors)
        .with_state(global_state);

    let port: u16 = std::env::var("PORT")
        .unwrap_or("3033".into())
        .parse()
        .expect("failed to convert to number");
    let listener = tokio::net::TcpListener::bind(format!(":::{port}"))
        .await
        .unwrap();
    log::info!("Listening on port 3033");
    axum::serve(
        listener, 
        app.into_make_service_with_connect_info::<SocketAddr>()
    ).await.unwrap();

    Ok(())
}

fn register_metrics() {
    REGISTRY
        .register(Box::new(WS_CONNECTIONS.clone()))
        .expect("Cannot register WS_CONNECTIONS");
    REGISTRY
        .register(Box::new(RX_EVENT_COUNTER.clone()))
        .expect("Cannot register RX_EVENT_COUNTER");
    REGISTRY
        .register(Box::new(TX_EVENT_COUNTER.clone()))
        .expect("Cannot register TX_EVENT_COUNTER");
}
