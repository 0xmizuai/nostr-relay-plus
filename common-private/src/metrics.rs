use axum::extract::State;
use prometheus::Registry;
use std::sync::Arc;

pub async fn metrics_handler(State(registry): State<Arc<Registry>>) -> String {
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
