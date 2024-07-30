use axum::extract::State;
use axum::routing::get;
use axum::Router;
use k256::sha2::digest::typenum::Le;
use prometheus::Registry;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::Level;

pub async fn get_metrics_app(metrics_registry: Arc<Registry>, port: u16) -> (Router, TcpListener) {
    let router = Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(metrics_registry);

    let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
        .await
        .expect("Cannot bind metrics server");
    (router, listener)
}

async fn metrics_handler(State(registry): State<Arc<Registry>>) -> String {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();

    let mut buffer = Vec::new();
    if let Err(e) = encoder.encode(&registry.gather(), &mut buffer) {
        tracing::error!("could not encode custom metrics: {}", e);
    };
    let mut res = String::from_utf8(buffer.clone()).unwrap_or_else(|e| {
        tracing::error!("custom metrics could not be from_utf8'd: {}", e);
        String::default()
    });
    buffer.clear();
    res
}
