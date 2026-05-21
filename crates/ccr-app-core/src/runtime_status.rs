use crate::metrics::{RouteMetricSummary, RuntimeMetricsDiagnostics, TtftMetricSummary};
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

const STATUS_API_TIMEOUT: Duration = Duration::from_millis(800);
const TTFT_SUMMARY_WINDOW_SECONDS: u64 = 300;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RoutePoolStatusResponse {
    pub enabled: bool,
    #[serde(rename = "failureThreshold")]
    pub failure_threshold: u32,
    #[serde(rename = "banSeconds")]
    pub ban_seconds: u64,
    pub routes: HashMap<String, RoutePoolRouteStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RoutePoolRouteStatus {
    pub consecutive_failures: u32,
    pub banned_until_epoch_secs: Option<u64>,
    pub last_error: Option<String>,
    pub last_failure_epoch_secs: Option<u64>,
    pub last_success_epoch_secs: Option<u64>,
}

pub fn fetch_route_pool_status(
    port: u16,
    api_key: Option<&str>,
) -> Result<RoutePoolStatusResponse, String> {
    fetch_json(
        &format!("http://127.0.0.1:{port}/api/route-pool/status"),
        api_key,
    )
}

pub fn fetch_runtime_metrics_summary(
    port: u16,
    api_key: Option<&str>,
) -> Result<Vec<RouteMetricSummary>, String> {
    fetch_json(
        &format!("http://127.0.0.1:{port}/api/runtime-metrics/summary"),
        api_key,
    )
}

pub fn fetch_ttft_metrics_summary(
    port: u16,
    api_key: Option<&str>,
) -> Result<Vec<TtftMetricSummary>, String> {
    fetch_json(
        &format!(
            "http://127.0.0.1:{port}/api/runtime-metrics/ttft-summary?window={TTFT_SUMMARY_WINDOW_SECONDS}"
        ),
        api_key,
    )
}

pub fn fetch_runtime_metrics_diagnostics(
    port: u16,
    api_key: Option<&str>,
) -> Result<RuntimeMetricsDiagnostics, String> {
    fetch_json(
        &format!("http://127.0.0.1:{port}/api/runtime-metrics/diagnostics"),
        api_key,
    )
}

fn fetch_json<T>(url: &str, api_key: Option<&str>) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let client = reqwest::blocking::Client::builder()
        .timeout(STATUS_API_TIMEOUT)
        .build()
        .map_err(|error| error.to_string())?;
    let mut request = client.get(url);
    if let Some(api_key) = api_key {
        request = request.bearer_auth(api_key);
    }
    let response = request.send().map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        if response.status().as_u16() == 401 {
            return Err(
                "unauthorized. The UI config API key does not match the running server."
                    .to_string(),
            );
        }
        return Err(format!("HTTP {}", response.status()));
    }
    response.json().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    fn serve_once(response_status: &str, body: &str) -> (u16, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let port = listener.local_addr().expect("local addr").port();
        let body = body.to_string();
        let status = response_status.to_string();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let request = read_request(&mut stream);
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
            request
        });
        (port, handle)
    }

    fn read_request(stream: &mut TcpStream) -> String {
        let mut buffer = [0; 2048];
        let size = stream.read(&mut buffer).expect("read request");
        String::from_utf8_lossy(&buffer[..size]).into_owned()
    }

    #[test]
    fn route_pool_status_fetch_parses_response_and_sends_auth() {
        let body = r#"{
            "enabled": true,
            "failureThreshold": 2,
            "banSeconds": 30,
            "routes": {
                "anthropic,claude": {
                    "consecutive_failures": 1,
                    "banned_until_epoch_secs": null,
                    "last_error": "timeout",
                    "last_failure_epoch_secs": 10,
                    "last_success_epoch_secs": 20
                }
            }
        }"#;
        let (port, handle) = serve_once("200 OK", body);

        let status =
            fetch_route_pool_status(port, Some("secret-token")).expect("route pool status");
        let request = handle.join().expect("request capture");

        assert!(request.starts_with("GET /api/route-pool/status HTTP/1.1"));
        assert!(
            request
                .to_ascii_lowercase()
                .contains("authorization: bearer secret-token")
        );
        assert!(status.enabled);
        assert_eq!(status.failure_threshold, 2);
        assert_eq!(status.ban_seconds, 30);
        assert_eq!(
            status
                .routes
                .get("anthropic,claude")
                .expect("route status")
                .last_error
                .as_deref(),
            Some("timeout")
        );
    }

    #[test]
    fn runtime_metric_summary_fetch_uses_core_metric_type() {
        let body = r#"[{
            "route": "anthropic,claude",
            "provider": "anthropic",
            "attempts": 3,
            "successes": 2,
            "failures": 1,
            "average_latency_ms": 123,
            "last_http_status": 429,
            "last_error_class": "http_429"
        }]"#;
        let (port, handle) = serve_once("200 OK", body);

        let summary = fetch_runtime_metrics_summary(port, None).expect("metrics summary");
        let request = handle.join().expect("request capture");

        assert!(request.starts_with("GET /api/runtime-metrics/summary HTTP/1.1"));
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].attempts, 3);
        assert_eq!(summary[0].average_latency_ms, Some(123));
    }

    #[test]
    fn ttft_metric_summary_fetch_uses_five_minute_window() {
        let body = r#"[{
            "route": "anthropic,claude",
            "provider": "anthropic",
            "model": "claude-sonnet",
            "window_seconds": 300,
            "samples": 4,
            "average_ttft_ms": 100,
            "p50_ttft_ms": 90,
            "p90_ttft_ms": 120,
            "p95_ttft_ms": 130,
            "latest_ttft_ms": 80,
            "latest_request_id": "request-1"
        }]"#;
        let (port, handle) = serve_once("200 OK", body);

        let summary = fetch_ttft_metrics_summary(port, None).expect("ttft summary");
        let request = handle.join().expect("request capture");

        assert!(request.starts_with("GET /api/runtime-metrics/ttft-summary?window=300 HTTP/1.1"));
        assert_eq!(summary[0].window_seconds, 300);
        assert_eq!(summary[0].latest_request_id.as_deref(), Some("request-1"));
    }

    #[test]
    fn runtime_metrics_diagnostics_fetch_uses_core_metric_type() {
        let body = r#"{
            "attempts": {
                "path": "/tmp/runtime-metrics.jsonl",
                "read_lines": 3,
                "successful_lines": 2,
                "malformed_lines": 1,
                "recent_error_summary": "line 2: expected value",
                "file_size_bytes": 120,
                "max_file_size_bytes": 5242880,
                "retention_applied": false,
                "retained_lines": 2,
                "retention_error_summary": null
            },
            "requests": {
                "path": "/tmp/runtime-request-metrics.jsonl",
                "read_lines": 2,
                "successful_lines": 2,
                "malformed_lines": 0,
                "recent_error_summary": null,
                "file_size_bytes": 80,
                "max_file_size_bytes": 5242880,
                "retention_applied": true,
                "retained_lines": 1,
                "retention_error_summary": null
            }
        }"#;
        let (port, handle) = serve_once("200 OK", body);

        let diagnostics =
            fetch_runtime_metrics_diagnostics(port, None).expect("metrics diagnostics");
        let request = handle.join().expect("request capture");

        assert!(request.starts_with("GET /api/runtime-metrics/diagnostics HTTP/1.1"));
        assert_eq!(diagnostics.attempts.malformed_lines, 1);
        assert!(diagnostics.requests.retention_applied);
        assert_eq!(diagnostics.requests.retained_lines, 1);
    }

    #[test]
    fn unauthorized_response_maps_to_ui_api_key_message() {
        let (port, handle) = serve_once("401 Unauthorized", "");

        let error = fetch_route_pool_status(port, None).expect_err("unauthorized");
        let _ = handle.join().expect("request capture");

        assert_eq!(
            error,
            "unauthorized. The UI config API key does not match the running server."
        );
    }

    #[test]
    fn http_error_response_maps_to_status_warning() {
        let (port, handle) = serve_once("503 Service Unavailable", "");

        let error = fetch_runtime_metrics_summary(port, None).expect_err("http error");
        let _ = handle.join().expect("request capture");

        assert_eq!(error, "HTTP 503 Service Unavailable");
    }

    #[test]
    fn invalid_json_response_maps_to_warning_text() {
        let (port, handle) = serve_once("200 OK", "not-json");

        let error = fetch_ttft_metrics_summary(port, None).expect_err("json error");
        let _ = handle.join().expect("request capture");

        assert!(!error.is_empty());
    }
}
