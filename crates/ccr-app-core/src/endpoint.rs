use crate::official_provider::{SecretResolutionError, resolve_provider_api_key};
use crate::provider_kind::{
    EndpointTestMode, provider_api_kind_defaults, resolve_provider_api_kind,
};
use ccr_types::{Config, Provider, ProviderApiKind, RoutePoolCandidate, RoutePoolConfig};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EndpointStatus {
    Available,
    HttpError(u16),
    MissingCredential,
    CredentialUnsupported,
    Timeout,
    NetworkError,
    InvalidUrl,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointTestResult {
    pub provider_name: String,
    pub endpoint: String,
    pub status: EndpointStatus,
    pub latency_ms: Option<u64>,
    pub stream_available: Option<bool>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointTestRequest {
    pub headers: Vec<(String, String)>,
    pub body: serde_json::Value,
    pub kind: ProviderApiKind,
    pub mode: EndpointTestMode,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct EndpointTestState {
    pub config: Config,
    pub selected_provider: usize,
    pub new_endpoint: String,
    pub test_results: Vec<EndpointTestResult>,
    pub testing: bool,
    pub status: String,
    pub confirm_apply_route_pool: bool,
}

#[allow(clippy::derivable_impls)]
impl Default for EndpointTestState {
    fn default() -> Self {
        Self {
            config: Config::default(),
            selected_provider: 0,
            new_endpoint: String::new(),
            test_results: Vec::new(),
            testing: false,
            status: String::new(),
            confirm_apply_route_pool: false,
        }
    }
}

impl EndpointTestState {
    pub fn sync_config(&mut self, config: Config) {
        self.config = config;
        if self.selected_provider >= self.config.providers.len() {
            self.selected_provider = 0;
        }
    }

    pub fn selected_provider(&self) -> Option<&Provider> {
        self.config.providers.get(self.selected_provider)
    }

    pub fn add_candidate(&mut self, endpoint: &str) -> Result<(), String> {
        let endpoint = endpoint.trim();
        if reqwest::Url::parse(endpoint).is_err() {
            return Err("invalid endpoint URL".to_string());
        }
        let provider = self
            .config
            .providers
            .get_mut(self.selected_provider)
            .ok_or_else(|| "provider not found".to_string())?;
        if !provider.endpoint_candidates.iter().any(|e| e == endpoint) {
            provider.endpoint_candidates.push(endpoint.to_string());
        }
        Ok(())
    }

    pub fn remove_candidate(&mut self, index: usize) -> bool {
        let Some(provider) = self.config.providers.get_mut(self.selected_provider) else {
            return false;
        };
        if index >= provider.endpoint_candidates.len() {
            return false;
        }
        provider.endpoint_candidates.remove(index);
        true
    }

    pub fn run_selected(&mut self) {
        self.testing = true;
        self.test_results.clear();
        if let Some(provider) = self.selected_provider() {
            self.test_results = sort_results(
                provider_endpoints(provider)
                    .iter()
                    .map(|endpoint| test_endpoint(provider, endpoint))
                    .collect(),
            );
        }
        self.testing = false;
    }

    pub fn run_all(&mut self) {
        self.testing = true;
        self.test_results.clear();
        for provider in &self.config.providers {
            self.test_results.extend(
                provider_endpoints(provider)
                    .iter()
                    .map(|endpoint| test_endpoint(provider, endpoint)),
            );
        }
        self.test_results = sort_results(std::mem::take(&mut self.test_results));
        self.testing = false;
    }

    pub fn request_apply_to_route_pool(&mut self) {
        self.confirm_apply_route_pool = true;
    }

    pub fn cancel_apply_to_route_pool(&mut self) {
        self.confirm_apply_route_pool = false;
    }

    pub fn apply_to_route_pool(&mut self) -> Result<(), String> {
        let provider = self
            .selected_provider()
            .ok_or_else(|| "selected provider not found".to_string())?;
        let model = provider
            .models
            .first()
            .cloned()
            .ok_or_else(|| "selected provider has no model".to_string())?;
        let route = format!("{},{}", provider.name, model);
        let provider_name = provider.name.clone();
        let available = self.test_results.iter().any(|result| {
            result.provider_name == provider_name
                && matches!(result.status, EndpointStatus::Available)
        });

        let pool = self
            .config
            .route_pool
            .get_or_insert_with(|| RoutePoolConfig {
                enabled: false,
                failure_threshold: 3,
                ban_seconds: 3600,
                candidates: Vec::new(),
            });
        if let Some(existing) = pool.candidates.iter_mut().find(|p| p.route == route) {
            existing.enabled = available;
            existing.priority = 1;
        } else {
            pool.candidates.push(RoutePoolCandidate {
                route,
                enabled: available,
                priority: 1,
            });
        }
        normalize_priorities(pool);
        self.confirm_apply_route_pool = false;
        Ok(())
    }
}

pub fn provider_endpoints(provider: &Provider) -> Vec<String> {
    if provider.endpoint_candidates.is_empty() {
        vec![provider.api_base_url.clone()]
    } else {
        provider.endpoint_candidates.clone()
    }
}

pub fn sort_results(mut results: Vec<EndpointTestResult>) -> Vec<EndpointTestResult> {
    results.sort_by(|a, b| {
        let a_ok = matches!(a.status, EndpointStatus::Available);
        let b_ok = matches!(b.status, EndpointStatus::Available);
        b_ok.cmp(&a_ok)
            .then_with(|| {
                a.latency_ms
                    .unwrap_or(u64::MAX)
                    .cmp(&b.latency_ms.unwrap_or(u64::MAX))
            })
            .then_with(|| a.endpoint.cmp(&b.endpoint))
    });
    results
}

pub fn test_endpoint(provider: &Provider, endpoint: &str) -> EndpointTestResult {
    let provider_name = provider.name.clone();
    if reqwest::Url::parse(endpoint).is_err() {
        log_endpoint_event(
            "invalid_url",
            &[
                ("provider", provider_name.clone()),
                ("endpoint", endpoint.to_string()),
                ("status", "invalid_url".to_string()),
            ],
        );
        return EndpointTestResult {
            provider_name,
            endpoint: endpoint.to_string(),
            status: EndpointStatus::InvalidUrl,
            latency_ms: None,
            stream_available: None,
            error: Some("Invalid URL".to_string()),
        };
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let start = Instant::now();
    let test_request = match endpoint_test_request_resolved(provider) {
        Ok(request) => request,
        Err(error) => {
            let (status, status_text) = endpoint_status_for_secret_error(&error);
            let error_text = error.to_string();
            log_endpoint_event(
                "credential_unavailable",
                &[
                    ("provider", provider_name.clone()),
                    ("endpoint", endpoint.to_string()),
                    ("status", status_text.to_string()),
                    ("error", error_text.clone()),
                ],
            );
            return EndpointTestResult {
                provider_name,
                endpoint: endpoint.to_string(),
                status,
                latency_ms: None,
                stream_available: None,
                error: Some(error_text),
            };
        }
    };
    log_endpoint_start(provider, endpoint, &test_request);
    let result = client
        .post(endpoint)
        .headers(headers(&test_request))
        .json(&test_request.body)
        .send();
    let elapsed = start.elapsed().as_millis() as u64;

    match result {
        Ok(response) if response.status().is_success() => {
            let http_status = response.status().as_u16();
            let response_text = response.text().unwrap_or_default();
            let response_summary = crate::logging::response_log_summary(&response_text);
            let stream_available = check_stream_endpoint(&client, provider, endpoint);
            log_endpoint_event(
                "result",
                &endpoint_result_fields(
                    provider,
                    endpoint,
                    &test_request,
                    "available",
                    Some(http_status),
                    Some(elapsed),
                    Some(stream_available),
                    None,
                    Some(&response_summary),
                ),
            );
            EndpointTestResult {
                stream_available: Some(stream_available),
                ..available_endpoint_result(provider, endpoint, elapsed)
            }
        }
        Ok(response) => {
            let http_status = response.status().as_u16();
            let response_text = response.text().unwrap_or_default();
            let response_summary = crate::logging::response_log_summary(&response_text);
            log_endpoint_event(
                "result",
                &endpoint_result_fields(
                    provider,
                    endpoint,
                    &test_request,
                    "http_error",
                    Some(http_status),
                    Some(elapsed),
                    None,
                    None,
                    Some(&response_summary),
                ),
            );
            http_error_endpoint_result(provider, endpoint, http_status, elapsed, &response_summary)
        }
        Err(error) => {
            let status = if error.is_timeout() {
                EndpointStatus::Timeout
            } else {
                EndpointStatus::NetworkError
            };
            let status_text = match status {
                EndpointStatus::Timeout => "timeout",
                EndpointStatus::NetworkError => "network_error",
                _ => "request_error",
            };
            let error_text = error.to_string();
            log_endpoint_event(
                "result",
                &endpoint_result_fields(
                    provider,
                    endpoint,
                    &test_request,
                    status_text,
                    None,
                    None,
                    None,
                    Some(&error_text),
                    None,
                ),
            );
            request_error_endpoint_result(provider, endpoint, status, error_text)
        }
    }
}

fn check_stream_endpoint(client: &Client, provider: &Provider, endpoint: &str) -> bool {
    let Ok(mut test_request) = endpoint_test_request_resolved(provider) else {
        return false;
    };
    match resolve_provider_api_kind(provider).kind {
        ccr_types::ProviderApiKind::OpenAiResponses => {
            test_request.body["stream"] = serde_json::json!(true);
        }
        _ => {
            test_request.body["stream"] = serde_json::json!(true);
        }
    }

    let start = Instant::now();
    let result = client
        .post(endpoint)
        .headers(headers(&test_request))
        .json(&test_request.body)
        .send();
    let elapsed = start.elapsed().as_millis() as u64;

    match result {
        Ok(response) => {
            let http_status = response.status().as_u16();
            let response_text = response.text().unwrap_or_default();
            let response_summary = crate::logging::response_log_summary(&response_text);
            let stream_available = response_indicates_stream(http_status, &response_text);
            log_endpoint_event(
                "stream_result",
                &endpoint_result_fields(
                    provider,
                    endpoint,
                    &test_request,
                    if stream_available {
                        "available"
                    } else {
                        "unavailable"
                    },
                    Some(http_status),
                    Some(elapsed),
                    Some(stream_available),
                    None,
                    Some(&response_summary),
                ),
            );
            stream_available
        }
        Err(error) => {
            let error_text = error.to_string();
            log_endpoint_event(
                "stream_result",
                &endpoint_result_fields(
                    provider,
                    endpoint,
                    &test_request,
                    if error.is_timeout() {
                        "timeout"
                    } else {
                        "network_error"
                    },
                    None,
                    Some(elapsed),
                    Some(false),
                    Some(&error_text),
                    None,
                ),
            );
            false
        }
    }
}

pub fn endpoint_test_request(provider: &Provider) -> EndpointTestRequest {
    let api_key = resolve_provider_api_key(provider).unwrap_or_else(|_| provider.api_key.clone());
    endpoint_test_request_with_api_key(provider, &api_key)
}

pub fn endpoint_test_request_resolved(
    provider: &Provider,
) -> Result<EndpointTestRequest, SecretResolutionError> {
    let api_key = resolve_provider_api_key(provider)?;
    Ok(endpoint_test_request_with_api_key(provider, &api_key))
}

fn endpoint_test_request_with_api_key(provider: &Provider, api_key: &str) -> EndpointTestRequest {
    let kind = resolve_provider_api_kind(provider).kind;
    let defaults = provider_api_kind_defaults(kind, provider.models.first().map(String::as_str));
    let model = endpoint_test_model(provider, kind);

    match defaults.endpoint_test_mode {
        EndpointTestMode::OpenAiResponses => EndpointTestRequest {
            headers: vec![
                ("authorization".to_string(), format!("Bearer {api_key}")),
                ("content-type".to_string(), "application/json".to_string()),
            ],
            body: serde_json::json!({
                "model": model.clone(),
                "input": "test",
                "max_output_tokens": 1,
                "stream": false
            }),
            kind,
            mode: defaults.endpoint_test_mode,
            model,
        },
        EndpointTestMode::AnthropicMessages => EndpointTestRequest {
            headers: vec![
                ("x-api-key".to_string(), api_key.to_string()),
                ("anthropic-version".to_string(), "2023-06-01".to_string()),
                ("content-type".to_string(), "application/json".to_string()),
            ],
            body: serde_json::json!({
                "model": model.clone(),
                "max_tokens": 1,
                "messages": [{"role": "user", "content": "test"}],
                "stream": false
            }),
            kind,
            mode: defaults.endpoint_test_mode,
            model,
        },
        EndpointTestMode::OpenAiChat | EndpointTestMode::BasicPost => EndpointTestRequest {
            headers: vec![
                ("authorization".to_string(), format!("Bearer {api_key}")),
                ("content-type".to_string(), "application/json".to_string()),
            ],
            body: serde_json::json!({
                "model": model.clone(),
                "max_tokens": 1,
                "messages": [{"role": "user", "content": "test"}],
                "stream": false
            }),
            kind,
            mode: defaults.endpoint_test_mode,
            model,
        },
    }
}

fn endpoint_status_for_secret_error(
    error: &SecretResolutionError,
) -> (EndpointStatus, &'static str) {
    match error {
        SecretResolutionError::Missing { .. } => {
            (EndpointStatus::MissingCredential, "missing_credential")
        }
        SecretResolutionError::UnsupportedSafeRead { .. } => (
            EndpointStatus::CredentialUnsupported,
            "credential_unsupported",
        ),
    }
}

fn endpoint_test_model(provider: &Provider, kind: ProviderApiKind) -> String {
    provider
        .models
        .iter()
        .find(|model| !model.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| default_endpoint_test_model(kind).to_string())
}

fn default_endpoint_test_model(kind: ProviderApiKind) -> &'static str {
    match kind {
        ProviderApiKind::AnthropicMessages | ProviderApiKind::AnthropicCompatible => {
            "claude-sonnet-4-20250514"
        }
        ProviderApiKind::OpenAiResponses
        | ProviderApiKind::OpenAiChat
        | ProviderApiKind::OpenRouter
        | ProviderApiKind::Groq
        | ProviderApiKind::Vercel
        | ProviderApiKind::Custom => "gpt-4o-mini",
        ProviderApiKind::DeepSeek => "deepseek-chat",
    }
}

fn log_endpoint_start(provider: &Provider, endpoint: &str, request: &EndpointTestRequest) {
    log_endpoint_event(
        "start",
        &[
            ("provider", provider.name.clone()),
            ("endpoint", endpoint.to_string()),
            ("kind", format!("{:?}", request.kind)),
            ("mode", format!("{:?}", request.mode)),
            ("model", request.model.clone()),
            ("headers", crate::logging::redact_headers(&request.headers)),
            ("body", request.body.to_string()),
        ],
    );
}

#[allow(clippy::too_many_arguments)]
fn endpoint_result_fields(
    provider: &Provider,
    endpoint: &str,
    request: &EndpointTestRequest,
    status: &str,
    http_status: Option<u16>,
    latency_ms: Option<u64>,
    stream_available: Option<bool>,
    error: Option<&str>,
    response: Option<&str>,
) -> Vec<(&'static str, String)> {
    let mut fields = vec![
        ("provider", provider.name.clone()),
        ("endpoint", endpoint.to_string()),
        ("kind", format!("{:?}", request.kind)),
        ("mode", format!("{:?}", request.mode)),
        ("model", request.model.clone()),
        ("status", status.to_string()),
    ];
    if let Some(http_status) = http_status {
        fields.push(("http_status", http_status.to_string()));
    }
    if let Some(latency_ms) = latency_ms {
        fields.push(("latency_ms", latency_ms.to_string()));
    }
    if let Some(stream_available) = stream_available {
        fields.push(("stream_available", stream_available.to_string()));
    }
    if let Some(error) = error {
        fields.push(("error", crate::logging::response_log_summary(error)));
    }
    if let Some(response) = response {
        fields.push(("response", response.to_string()));
    }
    fields
}

fn http_error_summary(status: u16, response_summary: &str) -> String {
    if response_summary.is_empty() {
        format!("HTTP {status}")
    } else {
        format!(
            "HTTP {status}: {}",
            crate::logging::ui_error_summary(response_summary)
        )
    }
}

fn available_endpoint_result(
    provider: &Provider,
    endpoint: &str,
    elapsed: u64,
) -> EndpointTestResult {
    EndpointTestResult {
        provider_name: provider.name.clone(),
        endpoint: endpoint.to_string(),
        status: EndpointStatus::Available,
        latency_ms: Some(elapsed),
        stream_available: None,
        error: None,
    }
}

fn http_error_endpoint_result(
    provider: &Provider,
    endpoint: &str,
    http_status: u16,
    elapsed: u64,
    response_summary: &str,
) -> EndpointTestResult {
    EndpointTestResult {
        provider_name: provider.name.clone(),
        endpoint: endpoint.to_string(),
        status: EndpointStatus::HttpError(http_status),
        latency_ms: Some(elapsed),
        stream_available: None,
        error: Some(http_error_summary(http_status, response_summary)),
    }
}

fn request_error_endpoint_result(
    provider: &Provider,
    endpoint: &str,
    status: EndpointStatus,
    error: String,
) -> EndpointTestResult {
    EndpointTestResult {
        provider_name: provider.name.clone(),
        endpoint: endpoint.to_string(),
        status,
        latency_ms: None,
        stream_available: None,
        error: Some(error),
    }
}

fn response_indicates_stream(http_status: u16, response_text: &str) -> bool {
    (200..300).contains(&http_status)
        && (response_text.contains("data:") || response_text.contains("\"type\""))
}

fn log_endpoint_event(event: &str, fields: &[(&str, String)]) {
    crate::logging::append_app_log("endpoint-test", event, fields);
}

fn headers(test_request: &EndpointTestRequest) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    for (name, value) in &test_request.headers {
        if let (Ok(name), Ok(value)) = (
            reqwest::header::HeaderName::from_bytes(name.as_bytes()),
            reqwest::header::HeaderValue::from_str(value),
        ) {
            headers.insert(name, value);
        }
    }
    headers
}

fn normalize_priorities(pool: &mut RoutePoolConfig) {
    pool.candidates.sort_by_key(|p| p.priority);
    for (index, candidate) in pool.candidates.iter_mut().enumerate() {
        candidate.priority = index as u32 + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccr_types::{ProviderApiKind, ProviderApiKindSource};

    fn provider() -> Provider {
        Provider {
            name: "p".into(),
            api_kind: None,
            api_kind_source: ProviderApiKindSource::Inferred,
            api_base_url: "https://api.example.com/v1/messages".into(),
            api_key: "k".into(),
            models: vec!["m".into()],
            endpoint_candidates: vec![],
            transformer: Default::default(),
        }
    }

    #[test]
    fn provider_endpoints_falls_back_to_base_url() {
        assert_eq!(
            provider_endpoints(&provider()),
            vec!["https://api.example.com/v1/messages"]
        );
    }

    #[test]
    fn provider_endpoints_uses_candidates() {
        let mut provider = provider();
        provider.endpoint_candidates = vec!["https://a".into(), "https://b".into()];
        assert_eq!(
            provider_endpoints(&provider),
            vec!["https://a", "https://b"]
        );
    }

    #[test]
    fn sort_results_success_before_failure_and_by_latency() {
        let results = sort_results(vec![
            EndpointTestResult {
                provider_name: "p".into(),
                endpoint: "slow".into(),
                status: EndpointStatus::Available,
                latency_ms: Some(100),
                stream_available: Some(true),
                error: None,
            },
            EndpointTestResult {
                provider_name: "p".into(),
                endpoint: "fail".into(),
                status: EndpointStatus::NetworkError,
                latency_ms: None,
                stream_available: None,
                error: Some("x".into()),
            },
            EndpointTestResult {
                provider_name: "p".into(),
                endpoint: "fast".into(),
                status: EndpointStatus::Available,
                latency_ms: Some(10),
                stream_available: Some(false),
                error: None,
            },
        ]);
        assert_eq!(results[0].endpoint, "fast");
        assert_eq!(results[1].endpoint, "slow");
        assert_eq!(results[2].endpoint, "fail");
    }

    #[test]
    fn add_candidate_rejects_invalid_url() {
        let mut state = EndpointTestState::default();
        state.config.providers.push(provider());
        assert_eq!(
            state.add_candidate("not a url").unwrap_err(),
            "invalid endpoint URL"
        );
    }

    #[test]
    fn sync_config_resets_invalid_selected_provider() {
        let mut state = EndpointTestState {
            selected_provider: 4,
            ..Default::default()
        };
        state.sync_config(Config {
            providers: vec![provider()],
            ..Default::default()
        });
        assert_eq!(state.selected_provider, 0);
        assert_eq!(state.selected_provider().unwrap().name, "p");
    }

    #[test]
    fn selected_provider_returns_none_for_empty_config() {
        let state = EndpointTestState::default();
        assert!(state.selected_provider().is_none());
    }

    #[test]
    fn add_candidate_deduplicates() {
        let mut state = EndpointTestState::default();
        state.config.providers.push(provider());
        state.add_candidate("https://one.example.com").unwrap();
        state.add_candidate("https://one.example.com").unwrap();
        assert_eq!(
            state.config.providers[0].endpoint_candidates,
            vec!["https://one.example.com"]
        );
    }

    #[test]
    fn add_candidate_requires_selected_provider() {
        let mut state = EndpointTestState::default();
        assert_eq!(
            state.add_candidate("https://one.example.com").unwrap_err(),
            "provider not found"
        );
    }

    #[test]
    fn remove_candidate_removes_target() {
        let mut state = EndpointTestState::default();
        let mut provider = provider();
        provider.endpoint_candidates = vec!["https://a".into(), "https://b".into()];
        state.config.providers.push(provider);
        assert!(state.remove_candidate(0));
        assert_eq!(
            state.config.providers[0].endpoint_candidates,
            vec!["https://b"]
        );
    }

    #[test]
    fn remove_candidate_returns_false_for_missing_provider_or_index() {
        let mut state = EndpointTestState::default();
        assert!(!state.remove_candidate(0));

        state.config.providers.push(provider());
        assert!(!state.remove_candidate(0));
    }

    #[test]
    fn run_selected_records_invalid_endpoint_without_network() {
        let mut state = EndpointTestState::default();
        let mut provider = provider();
        provider.api_base_url = "not a url".into();
        state.config.providers.push(provider);

        state.run_selected();

        assert!(!state.testing);
        assert_eq!(state.test_results.len(), 1);
        assert_eq!(state.test_results[0].status, EndpointStatus::InvalidUrl);
    }

    #[test]
    fn run_all_records_invalid_endpoints_without_network() {
        let mut state = EndpointTestState::default();
        let mut first = provider();
        first.api_base_url = "bad one".into();
        let mut second = provider();
        second.name = "q".into();
        second.api_base_url = "bad two".into();
        state.config.providers = vec![first, second];

        state.run_all();

        assert!(!state.testing);
        assert_eq!(state.test_results.len(), 2);
        assert!(
            state
                .test_results
                .iter()
                .all(|result| result.status == EndpointStatus::InvalidUrl)
        );
    }

    #[test]
    fn request_and_cancel_apply_to_route_pool_updates_confirmation_state() {
        let mut state = EndpointTestState::default();
        state.request_apply_to_route_pool();
        assert!(state.confirm_apply_route_pool);
        state.cancel_apply_to_route_pool();
        assert!(!state.confirm_apply_route_pool);
    }

    #[test]
    fn apply_to_route_pool_requires_selected_provider() {
        let mut state = EndpointTestState::default();
        assert_eq!(
            state.apply_to_route_pool().unwrap_err(),
            "selected provider not found"
        );
    }

    #[test]
    fn apply_to_route_pool_requires_model() {
        let mut state = EndpointTestState::default();
        let mut provider = provider();
        provider.models.clear();
        state.config.providers.push(provider);
        assert_eq!(
            state.apply_to_route_pool().unwrap_err(),
            "selected provider has no model"
        );
    }

    #[test]
    fn apply_to_route_pool_writes_route_after_confirmation() {
        let mut state = EndpointTestState::default();
        state.config.providers.push(provider());
        state.test_results.push(EndpointTestResult {
            provider_name: "p".into(),
            endpoint: "https://api.example.com".into(),
            status: EndpointStatus::Available,
            latency_ms: Some(1),
            stream_available: Some(true),
            error: None,
        });
        state.request_apply_to_route_pool();
        assert!(state.confirm_apply_route_pool);
        state.apply_to_route_pool().unwrap();
        assert!(!state.confirm_apply_route_pool);
        assert_eq!(state.config.route_pool.unwrap().candidates[0].route, "p,m");
    }

    #[test]
    fn apply_to_route_pool_updates_existing_route_and_normalizes_priority() {
        let mut state = EndpointTestState::default();
        state.config.providers.push(provider());
        state.config.route_pool = Some(RoutePoolConfig {
            enabled: true,
            failure_threshold: 3,
            ban_seconds: 3600,
            candidates: vec![
                RoutePoolCandidate {
                    route: "other,m".into(),
                    enabled: true,
                    priority: 5,
                },
                RoutePoolCandidate {
                    route: "p,m".into(),
                    enabled: true,
                    priority: 9,
                },
            ],
        });

        state.apply_to_route_pool().unwrap();

        let pool = state.config.route_pool.unwrap();
        assert_eq!(pool.candidates[0].route, "p,m");
        assert!(!pool.candidates[0].enabled);
        assert_eq!(pool.candidates[0].priority, 1);
        assert_eq!(pool.candidates[1].priority, 2);
    }

    #[test]
    fn test_endpoint_rejects_invalid_url_without_network() {
        let result = test_endpoint(&provider(), "not a url");

        assert_eq!(result.status, EndpointStatus::InvalidUrl);
        assert_eq!(result.error.as_deref(), Some("Invalid URL"));
    }

    #[test]
    fn test_endpoint_reports_unsupported_official_credential_without_network() {
        let mut provider = provider();
        provider.api_kind = Some(ProviderApiKind::OpenAiChat);
        provider.api_kind_source = ProviderApiKindSource::Explicit;
        provider.api_key = crate::official_provider::GITHUB_COPILOT_OFFICIAL_SECRET.to_string();

        let result = test_endpoint(&provider, "https://api.githubcopilot.com/chat/completions");

        assert_eq!(result.status, EndpointStatus::CredentialUnsupported);
        assert!(
            result
                .error
                .as_deref()
                .is_some_and(|error| error.contains("credential is unsupported"))
        );
    }

    #[test]
    fn http_error_summary_truncates_long_response_for_ui() {
        let summary = http_error_summary(500, &"x".repeat(400));

        assert!(summary.starts_with("HTTP 500: "));
        assert!(summary.len() < 330);
        assert!(summary.ends_with("..."));
    }

    #[test]
    fn endpoint_result_fields_include_optional_diagnostics() {
        let provider = provider();
        let request = endpoint_test_request(&provider);
        let fields = endpoint_result_fields(
            &provider,
            "https://api.example.com/v1/messages",
            &request,
            "http_error",
            Some(429),
            Some(12),
            Some(false),
            Some("rate limited"),
            Some("{\"error\":\"rate limited\"}"),
        );

        assert!(fields.contains(&("http_status", "429".to_string())));
        assert!(fields.contains(&("latency_ms", "12".to_string())));
        assert!(fields.contains(&("stream_available", "false".to_string())));
        assert!(
            fields
                .iter()
                .any(|(key, value)| *key == "response" && value.contains("rate limited"))
        );
    }

    #[test]
    fn endpoint_result_builders_preserve_diagnostics() {
        let provider = provider();
        let available = available_endpoint_result(&provider, "https://e", 7);
        assert_eq!(available.status, EndpointStatus::Available);
        assert_eq!(available.latency_ms, Some(7));
        assert_eq!(available.error, None);

        let http_error =
            http_error_endpoint_result(&provider, "https://e", 400, 9, r#"{"error":"bad model"}"#);
        assert_eq!(http_error.status, EndpointStatus::HttpError(400));
        assert_eq!(http_error.latency_ms, Some(9));
        assert!(http_error.error.unwrap().contains("bad model"));

        let request_error = request_error_endpoint_result(
            &provider,
            "https://e",
            EndpointStatus::Timeout,
            "deadline".to_string(),
        );
        assert_eq!(request_error.status, EndpointStatus::Timeout);
        assert_eq!(request_error.latency_ms, None);
        assert_eq!(request_error.error.as_deref(), Some("deadline"));
    }

    #[test]
    fn response_indicates_stream_requires_success_and_stream_markers() {
        assert!(response_indicates_stream(200, "data: {}\n\n"));
        assert!(response_indicates_stream(
            200,
            r#"{"type":"response.created"}"#
        ));
        assert!(!response_indicates_stream(500, "data: {}\n\n"));
        assert!(!response_indicates_stream(200, r#"{"id":"plain"}"#));
    }

    #[test]
    fn endpoint_request_uses_openai_chat_payload() {
        let mut provider = provider();
        provider.api_kind = Some(ProviderApiKind::OpenAiChat);
        provider.api_kind_source = ProviderApiKindSource::Explicit;

        let request = endpoint_test_request(&provider);

        assert_eq!(request.body["messages"][0]["content"], "test");
        assert_eq!(request.body["max_tokens"], 1);
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name == "authorization" && value == "Bearer k")
        );
        assert_eq!(request.kind, ProviderApiKind::OpenAiChat);
        assert_eq!(request.mode, EndpointTestMode::OpenAiChat);
        assert_eq!(request.model, "m");
    }

    #[test]
    fn endpoint_request_uses_openai_responses_payload() {
        let mut provider = provider();
        provider.api_kind = Some(ProviderApiKind::OpenAiResponses);
        provider.api_kind_source = ProviderApiKindSource::Explicit;

        let request = endpoint_test_request(&provider);

        assert_eq!(request.body["input"], "test");
        assert_eq!(request.body["max_output_tokens"], 1);
        assert!(request.body.get("messages").is_none());
        assert_eq!(request.kind, ProviderApiKind::OpenAiResponses);
        assert_eq!(request.mode, EndpointTestMode::OpenAiResponses);
    }

    #[test]
    fn endpoint_request_uses_anthropic_headers() {
        let mut provider = provider();
        provider.api_kind = Some(ProviderApiKind::AnthropicMessages);
        provider.api_kind_source = ProviderApiKindSource::Explicit;

        let request = endpoint_test_request(&provider);

        assert_eq!(request.body["messages"][0]["content"], "test");
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name == "x-api-key" && value == "k")
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name == "anthropic-version" && value == "2023-06-01")
        );
        assert_eq!(request.kind, ProviderApiKind::AnthropicMessages);
        assert_eq!(request.mode, EndpointTestMode::AnthropicMessages);
    }

    #[test]
    fn endpoint_request_does_not_fallback_to_test_model() {
        let mut provider = provider();
        provider.models.clear();
        provider.api_kind = Some(ProviderApiKind::AnthropicMessages);
        provider.api_kind_source = ProviderApiKindSource::Explicit;

        let request = endpoint_test_request(&provider);

        assert_eq!(request.model, "claude-sonnet-4-20250514");
        assert_eq!(request.body["model"], "claude-sonnet-4-20250514");
        assert_ne!(request.model, "test");
    }

    #[test]
    fn endpoint_request_uses_kind_specific_default_model() {
        let mut provider = provider();
        provider.models.clear();
        provider.api_kind = Some(ProviderApiKind::DeepSeek);
        provider.api_kind_source = ProviderApiKindSource::Explicit;

        let request = endpoint_test_request(&provider);

        assert_eq!(request.model, "deepseek-chat");
        assert_eq!(request.body["model"], "deepseek-chat");
    }
}
