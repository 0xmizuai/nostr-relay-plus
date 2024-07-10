use std::net::SocketAddr;

use anyhow::Result;

use axum::routing::get;
use axum::Router;
use tower_http::cors::CorsLayer;

use nostr_relay::{ws_wrapper, GlobalState};

#[tokio::main]
async fn main() -> Result<()> {
    // Basic command line parsing
    let args: Vec<String> = std::env::args().collect();
    let mut is_local = true;
    if args.len() > 1 && args[1] == "--remote" {
        is_local = false;
    }

    let mut builder = env_logger::Builder::from_default_env();
    builder.format_timestamp(None);
    builder.filter_level(log::LevelFilter::Debug);
    builder.try_init()?;

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
        .route("/", get(ws_wrapper))
        .layer(cors)
        .with_state(global_state);

    let port: u16 = std::env::var("PORT")
        .unwrap_or("3033".into())
        .parse()
        .expect("failed to convert to number");
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap();
    log::info!("Listening on port 3033");
    axum::serve(
        listener, 
        app.into_make_service_with_connect_info::<SocketAddr>()
    ).await.unwrap();

    Ok(())
}
