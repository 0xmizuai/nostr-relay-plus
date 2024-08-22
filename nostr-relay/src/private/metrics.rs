use common_private::metrics::metrics_handler_body;
use lazy_static::lazy_static;
use prometheus::{opts, IntCounterVec, IntGauge, Registry};
use std::sync::Arc;

lazy_static! {
    pub static ref REGISTRY: Arc<Registry> = Arc::new(Registry::default());
    pub static ref WS_CONNECTIONS: IntGauge =
        IntGauge::new("ws_connections", "Total number of current WS connections")
            .expect("Failed to create WS_CONNECTIONS");
    pub static ref RX_EVENT_COUNTER: IntCounterVec = IntCounterVec::new(
        opts!("recv_event_count", "Count of received events"),
        &["event_id"]
    )
    .expect("failed to create RX_EVENT_COUNTER");
    pub static ref TX_EVENT_COUNTER: IntCounterVec = IntCounterVec::new(
        opts!("sent_event_count", "Count of sent events"),
        &["event_id"]
    )
    .expect("failed to create TX_EVENT_COUNTER");
}

pub async fn metrics_handler() -> String {
    let metric_families = REGISTRY.gather();
    metrics_handler_body(&metric_families).await
}

#[inline]
pub fn track_rx_event(event_id: u16) {
    match RX_EVENT_COUNTER.get_metric_with_label_values(&[u16_to_str(event_id)]) {
        Ok(metrics) => metrics.inc(),
        Err(err) => tracing::error!("track_rx_event: {err}"),
    }
}

#[inline]
pub fn track_tx_event(event_id: u16) {
    match TX_EVENT_COUNTER.get_metric_with_label_values(&[u16_to_str(event_id)]) {
        Ok(metrics) => metrics.inc(),
        Err(err) => tracing::error!("track_tx_event: {err}"),
    }
}

fn u16_to_str(val: u16) -> &'static str {
    match val {
        6_000 => "6000",
        6_001 => "6001",
        6_002 => "6002",
        6_003 => "6003",
        6_004 => "6004",
        6_005 => "6005",
        6_006 => "6006",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::u16_to_str;

    #[test]
    fn test_u16_to_str() {
        assert_eq!(u16_to_str(6000), "6000");
        assert_eq!(u16_to_str(6006), "6006");
        assert_eq!(u16_to_str(6007), "other");
        assert_eq!(u16_to_str(0), "other");
    }
}
