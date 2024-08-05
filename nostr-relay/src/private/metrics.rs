use common_private::metrics::metrics_handler_body;
use lazy_static::lazy_static;
use prometheus::{IntGauge, Registry};
use std::sync::Arc;

lazy_static! {
    pub static ref REGISTRY: Arc<Registry> = Arc::new(Registry::default());
    pub static ref WS_CONNECTIONS: IntGauge =
        IntGauge::new("ws_connections", "Total number of current WS connections")
            .expect("Failed to create WS_CONNECTIONS");
}

pub async fn metrics_handler() -> String {
    let metric_families = REGISTRY.gather();
    metrics_handler_body(&metric_families).await
}
