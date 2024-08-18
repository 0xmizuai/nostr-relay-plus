use std::net::SocketAddr;

use anyhow::Result;

use axum::routing::get;
use axum::Router;
use tower_http::cors::CorsLayer;
use nostr_plus_common::logging::init_tracing;
use nostr_relay::{ws_wrapper, GlobalState};
use nostr_relay::__private::metrics::{metrics_handler, REGISTRY, WS_CONNECTIONS};

#[tokio::main]
async fn main() -> Result<()> {
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
        .expect("Cannot register ws_connections");
}
