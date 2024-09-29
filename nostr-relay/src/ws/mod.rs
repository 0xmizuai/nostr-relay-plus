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
use lazy_static::lazy_static;

lazy_static! {
    pub static ref ALLOWED_DOMAINS: Vec<String> = load_domains();
}

/// Parse ALLOWED_DOMAINS from env (if any) and create a list of allowed domains by
/// splitting at `,` (then leading and trailing whitespaces are trimmed).
/// e.g. "some.domain, another.domain, domain.com"
fn load_domains() -> Vec<String> {
    // If missing, we default to an empty string. So empty string is considered missing
    // from the caller point of view.
    let env_value = std::env::var("ALLOWED_DOMAINS").unwrap_or_default();
    let env_value = env_value.trim();

    // For empty strings, we want empty vectors
    if env_value.is_empty() {
        return vec![];
    }

    env_value.split(',').map(|el| el.trim().to_string()).collect()
}

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
                if !is_allowed(origin, &*ALLOWED_DOMAINS) {
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

fn is_allowed(origin: &str, allowed_domains: &Vec<String>) -> bool {
    if allowed_domains.iter().any(|s| s == "any") {
        return true;
    }
    let domain = origin.trim_start_matches("https://");
    allowed_domains.iter().any(|suffix| domain.ends_with(suffix))
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::ALLOWED_DOMAINS;
    use crate::ws::{is_allowed, load_domains};

    #[test]
    #[serial]
    fn test_load_domains() {
        std::env::set_var("ALLOWED_DOMAINS", "mizu.global, any, another.com");
        let expected = ["mizu.global", "any", "another.com"];
        let result = load_domains();
        assert_eq!(result.len(), expected.len()); // needed because we use `zip`
        let is_equal: bool = load_domains()
            .iter()
            .map(String::as_str)
            .zip(expected.iter())
            .all(|(res, exp)| res == *exp);
        assert!(is_equal);

        std::env::remove_var("ALLOWED_DOMAINS"); // We need to clean-up this var anyway, even if not testing it below
        let result = load_domains();
        assert!(result.is_empty());
    }

    #[test]
    fn test_is_allowed() {
        // Check that if "any" appears anywhere, than it's always true
        let allowed_domains = vec!["mizu.global".to_string(), "any".to_string(), "another.com".to_string()];
        assert_eq!(is_allowed("https://my_domain.com", &allowed_domains), true);

        let allowed_domains = vec!["mizu.global".to_string(), "another.com".to_string()];
        assert_eq!(is_allowed("https://some.mizu.global", &allowed_domains), true);
        assert_eq!(is_allowed("https://mizu.global", &allowed_domains), true);
        assert_eq!(is_allowed("https://my_domain.com", &allowed_domains), false);

        // Pass an empty vector and show there are restrictions.
        let allowed_domains = vec![];
        assert_eq!(is_allowed("https://my_domain.com", &allowed_domains), false);
    }
}