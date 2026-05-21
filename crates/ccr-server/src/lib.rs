pub mod protocol;
pub mod route_pool_config;

use ccr_types::Config;
use serde_json::Value;

pub use protocol::body_mapping::{
    InboundProtocol, UpstreamRequest, build_upstream_request, prepare_responses_body,
    route_model_override, upstream_headers, upstream_model_name,
};
pub use route_pool_config::{
    provider_for_route, provider_names, provider_not_found_message, route_for_display,
    route_pool_ban_seconds, route_pool_candidates, route_pool_enabled,
    route_pool_failure_threshold,
};

/// Returns true if the request is authorized to access the config API.
/// - If APIKEY is set: require matching Bearer token
/// - Otherwise: only allow loopback addresses
pub fn check_auth(api_key: Option<&str>, auth_header: Option<&str>, peer_ip: &str) -> bool {
    if let Some(key) = api_key.map(str::trim).filter(|key| !key.is_empty()) {
        let expected = format!("Bearer {key}");
        return auth_header.map(|v| v == expected).unwrap_or(false);
    }
    peer_ip == "127.0.0.1" || peer_ip == "::1"
}

/// Redact api_key fields in config JSON for safe display.
pub fn redact_config(config: &Config) -> Value {
    let mut val = serde_json::to_value(config).unwrap();
    if let Some(providers) = val.get_mut("Providers").and_then(|p| p.as_array_mut()) {
        for p in providers.iter_mut() {
            if let Some(k) = p.get_mut("api_key") {
                *k = Value::String("***".into());
            }
        }
    }
    val
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_allows_loopback_without_key() {
        assert!(check_auth(None, None, "127.0.0.1"));
        assert!(check_auth(None, None, "::1"));
        assert!(!check_auth(None, None, "192.168.1.10"));
    }

    #[test]
    fn auth_requires_bearer_when_key_configured() {
        assert!(check_auth(
            Some("secret"),
            Some("Bearer secret"),
            "192.168.1.10"
        ));
        assert!(!check_auth(
            Some("secret"),
            Some("Bearer wrong"),
            "127.0.0.1"
        ));
        assert!(!check_auth(Some("secret"), None, "127.0.0.1"));
    }
}
