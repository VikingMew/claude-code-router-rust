use ccr_app_core::provider_kind::{
    EndpointTestMode, provider_api_kind_defaults, resolve_provider_api_kind,
};
use ccr_transformer::TransformerRegistry;
use ccr_types::{Config, Provider, RoutePoolCandidate};
use serde_json::Value;

/// Returns true if the request is authorized to access the config API.
/// - If APIKEY is set: require matching Bearer token
/// - Otherwise: only allow loopback addresses
pub fn check_auth(api_key: Option<&str>, auth_header: Option<&str>, peer_ip: &str) -> bool {
    if let Some(key) = api_key {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboundProtocol {
    AnthropicMessages,
    OpenAiResponses,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamRequest {
    pub route: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
    pub stream: bool,
    pub model: String,
}

/// Rewrite the Responses payload model from CCR route format to upstream model name.
pub fn prepare_responses_body(body: Value, model_route: &str) -> Value {
    let upstream_model =
        route_model_override(model_route).unwrap_or_else(|| ccr_router::model_name(model_route));
    prepare_responses_body_for_mode(body, upstream_model, EndpointTestMode::OpenAiResponses)
        .expect("OpenAI Responses mode should not require input conversion")
}

pub fn build_upstream_request(
    inbound: InboundProtocol,
    route: &str,
    provider: &Provider,
    body: Value,
    transformers: &TransformerRegistry,
) -> Result<UpstreamRequest, String> {
    let mode = provider_api_kind_defaults(resolve_provider_api_kind(provider).kind, None)
        .endpoint_test_mode;
    let upstream_model = upstream_model_name(&body, route, provider)?;
    let mut upstream_body = match inbound {
        InboundProtocol::AnthropicMessages => prepare_messages_body(body, &upstream_model),
        InboundProtocol::OpenAiResponses => {
            prepare_responses_body_for_mode(body, &upstream_model, mode)?
        }
    };

    let transformer_names = transformer_names(provider);
    let transformer_refs: Vec<&str> = transformer_names.iter().map(String::as_str).collect();
    upstream_body = transformers.apply_chain(&transformer_refs, upstream_body);
    let stream = upstream_body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    Ok(UpstreamRequest {
        route: route.to_string(),
        url: provider.api_base_url.clone(),
        headers: upstream_headers(provider),
        body: upstream_body,
        stream,
        model: upstream_model,
    })
}

pub fn upstream_headers(provider: &Provider) -> Vec<(String, String)> {
    let mode = provider_api_kind_defaults(resolve_provider_api_kind(provider).kind, None)
        .endpoint_test_mode;
    let mut headers = vec![("content-type".to_string(), "application/json".to_string())];
    match mode {
        EndpointTestMode::AnthropicMessages => {
            headers.push(("x-api-key".to_string(), provider.api_key.clone()));
            headers.push(("anthropic-version".to_string(), "2023-06-01".to_string()));
        }
        EndpointTestMode::OpenAiChat
        | EndpointTestMode::OpenAiResponses
        | EndpointTestMode::BasicPost => {
            headers.push((
                "authorization".to_string(),
                format!("Bearer {}", provider.api_key),
            ));
        }
    }
    headers
}

fn prepare_messages_body(mut body: Value, upstream_model: &str) -> Value {
    body["model"] = Value::String(upstream_model.to_string());
    body
}

fn prepare_responses_body_for_mode(
    mut body: Value,
    upstream_model: &str,
    mode: EndpointTestMode,
) -> Result<Value, String> {
    body["model"] = Value::String(upstream_model.to_string());
    if mode == EndpointTestMode::OpenAiResponses {
        return Ok(body);
    }

    let messages = responses_input_to_messages(&body)?;
    let mut converted = serde_json::Map::new();
    converted.insert(
        "model".to_string(),
        Value::String(upstream_model.to_string()),
    );
    converted.insert("messages".to_string(), messages);
    if let Some(stream) = body.get("stream") {
        converted.insert("stream".to_string(), stream.clone());
    }
    if let Some(max_tokens) = body
        .get("max_output_tokens")
        .or_else(|| body.get("max_tokens"))
    {
        converted.insert("max_tokens".to_string(), max_tokens.clone());
    }
    Ok(Value::Object(converted))
}

pub fn route_model_override(route: &str) -> Option<&str> {
    let (_, model) = route.split_once(',')?;
    let model = model.trim();
    if model.is_empty() { None } else { Some(model) }
}

pub fn upstream_model_name(
    body: &Value,
    route: &str,
    provider: &Provider,
) -> Result<String, String> {
    if let Some(model) = route_model_override(route) {
        return Ok(model.to_string());
    }
    if let Some(model) = body.get("model").and_then(|value| value.as_str()) {
        let model = model.trim();
        if !model.is_empty() {
            return Ok(model.to_string());
        }
    }
    match provider.models.as_slice() {
        [model] if !model.trim().is_empty() => Ok(model.clone()),
        [] => Err(format!(
            "missing upstream model for provider-only route '{}'",
            route
        )),
        _ => Err(format!(
            "ambiguous upstream model for provider-only route '{}'",
            route
        )),
    }
}

fn responses_input_to_messages(body: &Value) -> Result<Value, String> {
    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        return Ok(Value::Array(messages.clone()));
    }
    match body.get("input") {
        Some(Value::String(input)) => Ok(serde_json::json!([
            {"role": "user", "content": input}
        ])),
        Some(Value::Array(items)) => Ok(Value::Array(items.clone())),
        Some(_) => Err("Unsupported Responses input shape".to_string()),
        None => Err("Responses request is missing input/messages".to_string()),
    }
}

fn transformer_names(provider: &Provider) -> Vec<String> {
    provider
        .transformer
        .use_transformers
        .iter()
        .filter_map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .or_else(|| value.as_array()?.first()?.as_str().map(str::to_string))
        })
        .collect()
}

pub fn route_pool_candidates(config: &Config, tried_routes: &[String]) -> Vec<String> {
    let Some(pool) = &config.route_pool else {
        return Vec::new();
    };
    if !pool.enabled {
        return Vec::new();
    }

    let mut candidates = pool.candidates.clone();
    normalize_route_pool_order(&mut candidates);
    let mut routes = Vec::new();
    for candidate in candidates {
        let route = candidate.route.trim();
        if !candidate.enabled || route.is_empty() {
            continue;
        }
        if tried_routes.iter().any(|tried| tried == route) || routes.iter().any(|r| r == route) {
            continue;
        }
        routes.push(route.to_string());
    }
    routes
}

pub fn normalize_route_pool_order(candidates: &mut [RoutePoolCandidate]) {
    candidates.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.route.cmp(&b.route))
    });
}

pub fn route_pool_enabled(config: &Config) -> bool {
    config.route_pool.as_ref().is_some_and(|pool| pool.enabled)
}

pub fn route_pool_failure_threshold(config: &Config) -> u32 {
    config
        .route_pool
        .as_ref()
        .map(|pool| pool.failure_threshold.max(1))
        .unwrap_or(3)
}

pub fn route_pool_ban_seconds(config: &Config) -> u64 {
    config
        .route_pool
        .as_ref()
        .map(|pool| pool.ban_seconds.max(1))
        .unwrap_or(3600)
}

pub fn provider_for_route<'a>(route: &str, config: &'a Config) -> Option<(&'a Provider, String)> {
    let provider = ccr_router::find_provider(route, config)?;
    Some((provider, ccr_router::model_name(route).to_string()))
}

pub fn route_for_display(route: &str) -> String {
    if route.trim().is_empty() {
        "<empty>".to_string()
    } else {
        route.to_string()
    }
}

pub fn provider_names(config: &Config) -> String {
    if config.providers.is_empty() {
        return "<none>".to_string();
    }
    config
        .providers
        .iter()
        .map(|provider| provider.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn provider_not_found_message(route: &str, config: &Config) -> String {
    format!(
        "No provider found for route '{}'. Available providers: {}",
        route_for_display(route),
        provider_names(config)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccr_types::{Config, Provider, ProviderApiKind, ProviderApiKindSource, TransformerConfig};

    #[test]
    fn auth_with_api_key_correct() {
        assert!(check_auth(Some("secret"), Some("Bearer secret"), "1.2.3.4"));
    }

    #[test]
    fn auth_with_api_key_wrong() {
        assert!(!check_auth(
            Some("secret"),
            Some("Bearer wrong"),
            "127.0.0.1"
        ));
    }

    #[test]
    fn auth_no_key_loopback_allowed() {
        assert!(check_auth(None, None, "127.0.0.1"));
        assert!(check_auth(None, None, "::1"));
    }

    #[test]
    fn auth_no_key_remote_denied() {
        assert!(!check_auth(None, None, "1.2.3.4"));
    }

    #[test]
    fn redact_masks_api_keys() {
        let config = Config {
            providers: vec![Provider {
                name: "openai".into(),
                api_kind: None,
                api_kind_source: ccr_types::ProviderApiKindSource::Inferred,
                api_base_url: "https://api.openai.com".into(),
                api_key: "sk-real-key".into(),
                models: vec![],
                endpoint_candidates: vec![],
                transformer: Default::default(),
            }],
            ..Default::default()
        };
        let val = redact_config(&config);
        assert_eq!(val["Providers"][0]["api_key"], "***");
    }

    #[test]
    fn prepare_responses_body_strips_provider_prefix() {
        let body = serde_json::json!({
            "model": "openai,gpt-5-codex",
            "input": "hello"
        });
        let prepared = prepare_responses_body(body, "openai,gpt-5-codex");
        assert_eq!(prepared["model"], "gpt-5-codex");
        assert_eq!(prepared["input"], "hello");
    }

    #[test]
    fn route_model_override_only_returns_explicit_model() {
        assert_eq!(route_model_override("openai,gpt-5"), Some("gpt-5"));
        assert_eq!(route_model_override("zenmux"), None);
        assert_eq!(route_model_override("zenmux,"), None);
    }

    #[test]
    fn provider_not_found_message_explains_empty_route() {
        let config = Config {
            providers: vec![provider_with_kind(
                ProviderApiKind::AnthropicMessages,
                vec![],
            )],
            ..Default::default()
        };

        let message = provider_not_found_message("", &config);

        assert!(message.contains("'<empty>'"));
        assert!(message.contains("Available providers: p"));
    }

    #[test]
    fn provider_names_handles_empty_config() {
        assert_eq!(provider_names(&Config::default()), "<none>");
    }

    fn provider_with_kind(kind: ProviderApiKind, transformer: Vec<serde_json::Value>) -> Provider {
        Provider {
            name: "p".into(),
            api_kind: Some(kind),
            api_kind_source: ProviderApiKindSource::Explicit,
            api_base_url: "https://api.example.com".into(),
            api_key: "sk-key".into(),
            models: vec!["m".into()],
            endpoint_candidates: vec![],
            transformer: TransformerConfig {
                use_transformers: transformer,
                model_overrides: Default::default(),
            },
        }
    }

    #[test]
    fn upstream_headers_use_anthropic_headers() {
        let provider = provider_with_kind(ProviderApiKind::AnthropicMessages, vec![]);
        let headers = upstream_headers(&provider);

        assert!(
            headers
                .iter()
                .any(|(name, value)| name == "x-api-key" && value == "sk-key")
        );
        assert!(
            headers
                .iter()
                .any(|(name, value)| name == "anthropic-version" && value == "2023-06-01")
        );
        assert!(!headers.iter().any(|(name, _)| name == "authorization"));
    }

    #[test]
    fn upstream_headers_use_bearer_for_openai_like() {
        let provider = provider_with_kind(ProviderApiKind::OpenAiChat, vec![]);
        let headers = upstream_headers(&provider);

        assert!(
            headers
                .iter()
                .any(|(name, value)| name == "authorization" && value == "Bearer sk-key")
        );
    }

    #[test]
    fn upstream_builder_keeps_responses_body_for_responses_provider() {
        let provider = provider_with_kind(ProviderApiKind::OpenAiResponses, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::OpenAiResponses,
            "p,gpt-5-codex",
            &provider,
            serde_json::json!({"model": "p,gpt-5-codex", "input": "hello", "max_output_tokens": 2}),
            &registry,
        )
        .unwrap();

        assert_eq!(request.body["model"], "gpt-5-codex");
        assert_eq!(request.body["input"], "hello");
        assert_eq!(request.body["max_output_tokens"], 2);
        assert!(request.body.get("messages").is_none());
    }

    #[test]
    fn upstream_builder_preserves_inbound_model_for_provider_only_route() {
        let provider = provider_with_kind(ProviderApiKind::AnthropicMessages, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::AnthropicMessages,
            "p",
            &provider,
            serde_json::json!({
                "model": "claude-sonnet-4-20250514",
                "messages": [{"role": "user", "content": "hello"}]
            }),
            &registry,
        )
        .unwrap();

        assert_eq!(request.model, "claude-sonnet-4-20250514");
        assert_eq!(request.body["model"], "claude-sonnet-4-20250514");
    }

    #[test]
    fn upstream_builder_uses_route_model_when_explicit() {
        let provider = provider_with_kind(ProviderApiKind::AnthropicMessages, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::AnthropicMessages,
            "p,forced-model",
            &provider,
            serde_json::json!({
                "model": "inbound-model",
                "messages": [{"role": "user", "content": "hello"}]
            }),
            &registry,
        )
        .unwrap();

        assert_eq!(request.model, "forced-model");
        assert_eq!(request.body["model"], "forced-model");
    }

    #[test]
    fn upstream_builder_uses_unique_provider_model_when_inbound_missing() {
        let provider = provider_with_kind(ProviderApiKind::AnthropicMessages, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::AnthropicMessages,
            "p",
            &provider,
            serde_json::json!({
                "messages": [{"role": "user", "content": "hello"}]
            }),
            &registry,
        )
        .unwrap();

        assert_eq!(request.model, "m");
        assert_eq!(request.body["model"], "m");
    }

    #[test]
    fn upstream_builder_rejects_provider_only_without_model() {
        let mut provider = provider_with_kind(ProviderApiKind::AnthropicMessages, vec![]);
        provider.models.clear();
        let registry = TransformerRegistry::new();
        let err = build_upstream_request(
            InboundProtocol::AnthropicMessages,
            "p",
            &provider,
            serde_json::json!({
                "messages": [{"role": "user", "content": "hello"}]
            }),
            &registry,
        )
        .unwrap_err();

        assert!(err.contains("missing upstream model"));
    }

    #[test]
    fn upstream_builder_converts_responses_input_for_anthropic_provider() {
        let provider = provider_with_kind(ProviderApiKind::AnthropicMessages, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::OpenAiResponses,
            "p,claude-sonnet-4",
            &provider,
            serde_json::json!({"model": "p,claude-sonnet-4", "input": "hello", "max_output_tokens": 2}),
            &registry,
        )
        .unwrap();

        assert_eq!(request.body["model"], "claude-sonnet-4");
        assert_eq!(request.body["messages"][0]["role"], "user");
        assert_eq!(request.body["messages"][0]["content"], "hello");
        assert_eq!(request.body["max_tokens"], 2);
        assert!(request.headers.iter().any(|(name, _)| name == "x-api-key"));
    }

    #[test]
    fn upstream_builder_applies_openai_transformer_for_messages() {
        let provider = provider_with_kind(
            ProviderApiKind::OpenAiChat,
            vec![serde_json::json!("openai")],
        );
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::AnthropicMessages,
            "p,gpt-4o",
            &provider,
            serde_json::json!({
                "model": "p,gpt-4o",
                "system": "be brief",
                "messages": [{"role": "user", "content": "hello"}]
            }),
            &registry,
        )
        .unwrap();

        assert_eq!(request.body["model"], "gpt-4o");
        assert!(request.body.get("system").is_none());
        assert_eq!(request.body["messages"][0]["role"], "system");
        assert_eq!(request.body["messages"][0]["content"], "be brief");
    }

    #[test]
    fn upstream_builder_rejects_unsupported_responses_input() {
        let provider = provider_with_kind(ProviderApiKind::OpenAiChat, vec![]);
        let registry = TransformerRegistry::new();
        let err = build_upstream_request(
            InboundProtocol::OpenAiResponses,
            "p,gpt-4o",
            &provider,
            serde_json::json!({"input": {"bad": true}}),
            &registry,
        )
        .unwrap_err();

        assert!(err.contains("Unsupported Responses input"));
    }

    #[test]
    fn route_pool_candidates_sort_skip_disabled_and_deduplicate() {
        let mut config = Config::default();
        config.route_pool = Some(ccr_types::RoutePoolConfig {
            enabled: true,
            failure_threshold: 3,
            ban_seconds: 3600,
            candidates: vec![
                ccr_types::RoutePoolCandidate {
                    route: "b".into(),
                    enabled: true,
                    priority: 2,
                },
                ccr_types::RoutePoolCandidate {
                    route: "a".into(),
                    enabled: true,
                    priority: 1,
                },
                ccr_types::RoutePoolCandidate {
                    route: "c".into(),
                    enabled: false,
                    priority: 0,
                },
                ccr_types::RoutePoolCandidate {
                    route: "a".into(),
                    enabled: true,
                    priority: 3,
                },
            ],
        });

        assert_eq!(route_pool_candidates(&config, &[]), vec!["a", "b"]);
    }

    #[test]
    fn route_pool_defaults_are_clamped() {
        let mut config = Config::default();
        config.route_pool = Some(ccr_types::RoutePoolConfig {
            enabled: true,
            failure_threshold: 0,
            ban_seconds: 0,
            candidates: vec![],
        });

        assert_eq!(route_pool_failure_threshold(&config), 1);
        assert_eq!(route_pool_ban_seconds(&config), 1);
    }
}
