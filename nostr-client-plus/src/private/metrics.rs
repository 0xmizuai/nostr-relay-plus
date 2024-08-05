use axum::extract::State;
use axum::routing::get;
use axum::Router;
use common_private::metrics::metrics_handler_body;
use prometheus::Registry;
use std::sync::Arc;
use tokio::net::TcpListener;

pub async fn get_metrics_app(
    metrics_registry: Arc<Registry>,
    socker_addr: &str,
) -> (Router, TcpListener) {
    let router = Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(metrics_registry);

    let listener = TcpListener::bind(socker_addr)
        .await
        .expect("Cannot bind metrics server");
    (router, listener)
}

pub async fn metrics_handler(State(registry): State<Arc<Registry>>) -> String {
    let metric_family = registry.gather();
    metrics_handler_body(&metric_family).await
}
