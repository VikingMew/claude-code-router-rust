mod handlers;
mod runtime;

use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
use ccr_agent::{Agent, ImageAgent};
use ccr_app_core::logging::LogQuery;
use ccr_app_core::metrics::RuntimeMetricsStore;
use ccr_config::{ReloadableConfig, default_config_path, load_config, save_config};
use ccr_preset::{delete_preset, list_presets, load_preset};
use ccr_server::{
    InboundProtocol, UpstreamRequest, check_auth, provider_names, redact_config, route_for_display,
    route_pool_ban_seconds, route_pool_candidates, route_pool_enabled,
    route_pool_failure_threshold,
};
use ccr_sse::{SseParser, SseRewriter, invoke_continuation};
use ccr_transformer::TransformerRegistry;
use ccr_types::{Config, MessagesRequest};
use handlers::{
    get_runtime_metric_attempts, get_runtime_metric_diagnostics, get_runtime_metric_requests,
    get_runtime_metric_summary, get_runtime_metric_ttft_summary,
};
use runtime::metrics::{record_pending_attempt_ttft, stream_response_with_ttft};
use runtime::responses_stream::stream_anthropic_as_responses_with_ttft;
use runtime::route_pool::{
    ROUTE_POOL_EVENT_HISTORY_LIMIT, RoutePoolEvent, RoutePoolRouteState, RoutePoolRuntime,
    clear_route_pool_ban_state, record_route_pool_event, reset_route_pool_route_state,
    route_pool_routes_for_log, send_with_route_pool,
};
use serde::Deserialize;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tracing::{debug, error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub struct AppState {
    reloadable_config: Arc<ReloadableConfig>,
    transformers: Arc<TransformerRegistry>,
    client: reqwest::Client,
    agents: Vec<Arc<dyn Agent>>,
    route_pool_state: Arc<Mutex<HashMap<String, RoutePoolRouteState>>>,
    route_pool_events: Arc<Mutex<VecDeque<RoutePoolEvent>>>,
    metrics: Arc<Mutex<RuntimeMetricsStore>>,
}

#[derive(Debug, Clone, Deserialize)]
struct RoutePoolRouteActionRequest {
    route: String,
}
impl AppState {
    /// Reload configuration from disk
    pub async fn reload_config(&self) -> anyhow::Result<()> {
        self.reloadable_config.reload().await
    }

    /// Get current config
    pub async fn get_config(&self) -> Config {
        self.reloadable_config.get().await
    }
}

async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "ok"}))
}

pub(crate) fn auth_check(req: &HttpRequest, config: &Config) -> bool {
    let peer = req
        .peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_default();
    let auth = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());
    check_auth(config.api_key.as_deref(), auth, &peer)
}

async fn get_config(req: HttpRequest, state: web::Data<Arc<AppState>>) -> HttpResponse {
    let config = state.get_config().await;
    if !auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }
    HttpResponse::Ok().json(redact_config(&config))
}

async fn put_config(
    req: HttpRequest,
    body: web::Json<serde_json::Value>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let config = state.get_config().await;
    if !auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }

    let new_config: Config = match serde_json::from_value(body.into_inner()) {
        Ok(c) => c,
        Err(e) => return HttpResponse::BadRequest().body(e.to_string()),
    };
    if let Err(e) = save_config(&new_config, state.reloadable_config.path()) {
        return HttpResponse::InternalServerError().body(e.to_string());
    }

    // Reload config to apply changes
    if let Err(e) = state.reload_config().await {
        return HttpResponse::InternalServerError().body(format!("Failed to reload config: {}", e));
    }

    HttpResponse::Ok().json(serde_json::json!({"ok": true}))
}

async fn get_transformers(req: HttpRequest, state: web::Data<Arc<AppState>>) -> HttpResponse {
    let config = state.get_config().await;
    if !auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }
    let names = state.transformers.names();
    HttpResponse::Ok().json(names)
}

async fn get_logs(req: HttpRequest, state: web::Data<Arc<AppState>>) -> HttpResponse {
    let config = state.get_config().await;
    if !auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }
    let log_path = ccr_app_core::logging::app_log_path();
    match std::fs::read_to_string(&log_path) {
        Ok(content) => HttpResponse::Ok().content_type("text/plain").body(content),
        Err(_) => HttpResponse::Ok().body(""),
    }
}

async fn query_logs(
    req: HttpRequest,
    query: web::Query<LogQuery>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let config = state.get_config().await;
    if !auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }
    let log_path = ccr_app_core::logging::app_log_path();
    match ccr_app_core::logging::query_app_log(&log_path, &query.into_inner()) {
        Ok(events) => HttpResponse::Ok().json(events),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

async fn delete_logs(req: HttpRequest, state: web::Data<Arc<AppState>>) -> HttpResponse {
    let config = state.get_config().await;
    if !auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }
    let log_path = ccr_app_core::logging::app_log_path();
    std::fs::write(&log_path, "").ok();
    HttpResponse::Ok().json(serde_json::json!({"ok": true}))
}

async fn get_presets(req: HttpRequest, state: web::Data<Arc<AppState>>) -> HttpResponse {
    let config = state.get_config().await;
    if !auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }
    let presets = list_presets().unwrap_or_default();
    HttpResponse::Ok().json(presets)
}

async fn get_route_pool_status(req: HttpRequest, state: web::Data<Arc<AppState>>) -> HttpResponse {
    let config = state.get_config().await;
    if !auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }

    let snapshot = state
        .route_pool_state
        .lock()
        .map(|state| state.clone())
        .unwrap_or_default();
    HttpResponse::Ok().json(serde_json::json!({
        "enabled": route_pool_enabled(&config),
        "failureThreshold": route_pool_failure_threshold(&config),
        "banSeconds": route_pool_ban_seconds(&config),
        "routes": snapshot
    }))
}

async fn get_route_pool_events(
    req: HttpRequest,
    query: web::Query<HashMap<String, String>>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let config = state.get_config().await;
    if !auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }

    let limit = query
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(50)
        .min(ROUTE_POOL_EVENT_HISTORY_LIMIT);
    let events = state
        .route_pool_events
        .lock()
        .map(|events| {
            events
                .iter()
                .rev()
                .take(limit)
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    HttpResponse::Ok().json(events)
}

async fn clear_route_pool_ban(
    req: HttpRequest,
    body: web::Json<RoutePoolRouteActionRequest>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let config = state.get_config().await;
    if !auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }
    let route = body.route.trim();
    if !configured_route_exists(&config, route) {
        return HttpResponse::NotFound().json(serde_json::json!({
            "ok": false,
            "route": route,
            "status": "not_configured",
            "message": "Route is not an enabled Route Pool candidate"
        }));
    }

    let (changed, state_snapshot) = clear_route_pool_ban_state(&state.route_pool_state, route);
    record_route_pool_event(
        &state.route_pool_events,
        "route_pool_candidate_ban_cleared",
        route,
        if changed {
            "user_clear_ban"
        } else {
            "not_banned"
        },
        state_snapshot
            .as_ref()
            .map(|state| state.consecutive_failures),
        state_snapshot
            .as_ref()
            .and_then(|state| state.banned_until_epoch_secs),
    );
    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "route": route,
        "status": if changed { "cleared" } else { "not_banned" },
        "changed": changed,
        "state": state_snapshot
    }))
}

async fn reset_route_pool_route(
    req: HttpRequest,
    body: web::Json<RoutePoolRouteActionRequest>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let config = state.get_config().await;
    if !auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }
    let route = body.route.trim();
    if !configured_route_exists(&config, route) {
        return HttpResponse::NotFound().json(serde_json::json!({
            "ok": false,
            "route": route,
            "status": "not_configured",
            "message": "Route is not an enabled Route Pool candidate"
        }));
    }

    let changed = reset_route_pool_route_state(&state.route_pool_state, route);
    record_route_pool_event(
        &state.route_pool_events,
        "route_pool_candidate_reset",
        route,
        "user_reset_route_state",
        Some(0),
        None,
    );
    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "route": route,
        "status": if changed { "reset" } else { "already_empty" },
        "changed": changed,
        "state": RoutePoolRouteState::default()
    }))
}

fn configured_route_exists(config: &Config, route: &str) -> bool {
    route_pool_candidates(config, &[])
        .iter()
        .any(|candidate| candidate == route)
}

async fn get_preset(
    req: HttpRequest,
    path: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let config = state.get_config().await;
    if !auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }
    let name = path.into_inner();
    let preset_dir = ccr_preset::presets_dir().join(&name);
    match load_preset(preset_dir.to_str().unwrap_or("")) {
        Ok(m) => HttpResponse::Ok().json(m),
        Err(_) => HttpResponse::NotFound().finish(),
    }
}

async fn delete_preset_handler(
    req: HttpRequest,
    path: web::Path<String>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let config = state.get_config().await;
    if !auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }
    let name = path.into_inner();
    match delete_preset(&name) {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({"ok": true})),
        Err(e) => HttpResponse::NotFound().body(e.to_string()),
    }
}

async fn count_tokens_handler(
    req: HttpRequest,
    body: web::Bytes,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let config = state.get_config().await;
    if !auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }
    let msg_req: MessagesRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return HttpResponse::BadRequest().body(e.to_string()),
    };
    let n = ccr_router::count_tokens_async(&msg_req, &config.router.tokenizer_backend).await;
    HttpResponse::Ok().json(serde_json::json!({"input_tokens": n}))
}

async fn messages(
    _req: HttpRequest,
    body: web::Bytes,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let mut msg_req: MessagesRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return HttpResponse::BadRequest().body(e.to_string()),
    };

    let config = state.get_config().await;
    log_server_event(
        "messages_received",
        &[
            ("inbound_model", msg_req.model.clone()),
            ("route_pool", route_pool_routes_for_log(&config)),
            ("providers", provider_names(&config)),
            ("body_bytes", body.len().to_string()),
        ],
    );

    // Check if any agent should handle this request
    let mut agent_handling: Option<Arc<dyn Agent>> = None;
    for agent in &state.agents {
        if agent.detect(&msg_req, &config).is_some() {
            // Agent will modify request and inject tools
            agent.modify_request(&mut msg_req, &config);
            agent_handling = Some(agent.clone());
            info!(agent = agent.name(), "Agent handling request");
            break;
        }
    }

    let (model_str, reason) = if route_pool_enabled(&config) {
        ("<route-pool>".to_string(), "routePool")
    } else {
        (String::new(), "routePoolNotConfigured")
    };
    info!(model = %model_str, reason = %reason, "Model selected");
    log_server_event(
        "model_selected",
        &[
            ("route", route_for_display(&model_str)),
            ("reason", reason.to_string()),
            ("inbound_model", msg_req.model.clone()),
        ],
    );

    let config_clone = config.clone();
    let body_json: serde_json::Value = serde_json::to_value(&msg_req).unwrap();

    let mut upstream_res = match send_with_route_pool(
        RoutePoolRuntime {
            client: &state.client,
            config: &config,
            transformers: &state.transformers,
            route_pool_state: &state.route_pool_state,
            route_pool_events: &state.route_pool_events,
            metrics: &state.metrics,
        },
        InboundProtocol::AnthropicMessages,
        &model_str,
        body_json,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => return HttpResponse::BadGateway().body(e),
    };

    let status = actix_web::http::StatusCode::from_u16(upstream_res.response.status().as_u16())
        .unwrap_or(actix_web::http::StatusCode::OK);

    if upstream_res.stream {
        // Check if we need tool interception
        if let Some(agent) = agent_handling {
            // Use tool interception pipeline
            record_pending_attempt_ttft(
                &state.metrics,
                upstream_res.pending_attempt_metric.take(),
                upstream_res.attempt_started_at,
                "stream_interception_response_headers",
            );
            match process_with_tool_interception(
                upstream_res.response,
                agent,
                msg_req,
                config_clone,
            )
            .await
            {
                Ok(stream) => {
                    return HttpResponse::build(status)
                        .content_type("text/event-stream")
                        .insert_header(("cache-control", "no-cache"))
                        .insert_header(("x-accel-buffering", "no"))
                        .streaming(stream);
                }
                Err(e) => {
                    error!("Tool interception error: {}", e);
                    return HttpResponse::InternalServerError().body(e.to_string());
                }
            }
        }

        // Normal passthrough streaming
        let stream = stream_response_with_ttft(
            upstream_res.response,
            state.metrics.clone(),
            upstream_res.pending_attempt_metric.take(),
            upstream_res.attempt_started_at,
        );
        HttpResponse::build(status)
            .content_type("text/event-stream")
            .insert_header(("cache-control", "no-cache"))
            .insert_header(("x-accel-buffering", "no"))
            .streaming(stream)
    } else {
        match upstream_res.response.bytes().await {
            Ok(b) => HttpResponse::build(status)
                .content_type("application/json")
                .body(b),
            Err(e) => HttpResponse::BadGateway().body(e.to_string()),
        }
    }
}

async fn responses(body: web::Bytes, state: web::Data<Arc<AppState>>) -> HttpResponse {
    let body_json: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return HttpResponse::BadRequest().body(e.to_string()),
    };

    let config = state.get_config().await;
    let requested_model = body_json
        .get("model")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    log_server_event(
        "responses_received",
        &[
            (
                "requested_model",
                requested_model
                    .clone()
                    .unwrap_or_else(|| "<missing>".to_string()),
            ),
            ("route_pool", route_pool_routes_for_log(&config)),
            ("providers", provider_names(&config)),
            ("body_bytes", body.len().to_string()),
        ],
    );

    let model_str = requested_model.unwrap_or_else(|| "<route-pool>".to_string());

    info!(model = %model_str, "Codex responses model selected");
    log_server_event(
        "responses_model_selected",
        &[("route", route_for_display(&model_str))],
    );

    let mut upstream_res = match send_with_route_pool(
        RoutePoolRuntime {
            client: &state.client,
            config: &config,
            transformers: &state.transformers,
            route_pool_state: &state.route_pool_state,
            route_pool_events: &state.route_pool_events,
            metrics: &state.metrics,
        },
        InboundProtocol::OpenAiResponses,
        &model_str,
        body_json,
    )
    .await
    {
        Ok(r) => r,
        Err(e) => return HttpResponse::BadGateway().body(e),
    };

    let status = actix_web::http::StatusCode::from_u16(upstream_res.response.status().as_u16())
        .unwrap_or(actix_web::http::StatusCode::OK);

    if upstream_res.stream {
        let stream = if matches!(
            upstream_res.mode,
            ccr_app_core::provider_kind::EndpointTestMode::AnthropicMessages
                | ccr_app_core::provider_kind::EndpointTestMode::BasicPost
        ) {
            stream_anthropic_as_responses_with_ttft(
                upstream_res.response,
                state.metrics.clone(),
                upstream_res.pending_attempt_metric.take(),
                upstream_res.attempt_started_at,
            )
        } else {
            stream_response_with_ttft(
                upstream_res.response,
                state.metrics.clone(),
                upstream_res.pending_attempt_metric.take(),
                upstream_res.attempt_started_at,
            )
        };
        HttpResponse::build(status)
            .content_type("text/event-stream")
            .insert_header(("cache-control", "no-cache"))
            .insert_header(("x-accel-buffering", "no"))
            .streaming(stream)
    } else {
        match upstream_res.response.bytes().await {
            Ok(b) => HttpResponse::build(status)
                .content_type("application/json")
                .body(b),
            Err(e) => HttpResponse::BadGateway().body(e.to_string()),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn log_upstream_event(
    event: &str,
    inbound: InboundProtocol,
    route: &str,
    provider: &str,
    endpoint: &str,
    model: &str,
    http_status: Option<u16>,
    latency_ms: Option<u64>,
    error: Option<&str>,
    response: Option<&str>,
    headers: &[(String, String)],
    body: Option<&serde_json::Value>,
    extra_fields: &[(&str, String)],
) {
    let mut fields = vec![
        ("inbound", format!("{:?}", inbound)),
        ("route", route_for_display(route)),
        ("provider", provider.to_string()),
        ("endpoint", endpoint.to_string()),
        ("model", model.to_string()),
    ];
    if let Some(status) = http_status {
        fields.push(("http_status", status.to_string()));
    }
    if let Some(latency_ms) = latency_ms {
        fields.push(("latency_ms", latency_ms.to_string()));
    }
    if let Some(error) = error {
        fields.push(("error", ccr_app_core::logging::response_log_summary(error)));
    }
    if let Some(response) = response {
        fields.push(("response", response.to_string()));
    }
    if !headers.is_empty() {
        fields.push(("headers", ccr_app_core::logging::redact_headers(headers)));
    }
    if let Some(body) = body {
        fields.push(("body", body.to_string()));
    }
    fields.extend(
        extra_fields
            .iter()
            .map(|(key, value)| (*key, value.clone())),
    );
    ccr_app_core::logging::append_app_log("upstream", event, &fields);
}

fn log_server_event(event: &str, fields: &[(&str, String)]) {
    ccr_app_core::logging::append_app_log("server", event, fields);
}

async fn send_upstream_request(
    client: &reqwest::Client,
    upstream: &UpstreamRequest,
) -> Result<reqwest::Response, reqwest::Error> {
    let mut request = client.post(&upstream.url);
    for (name, value) in &upstream.headers {
        request = request.header(name.as_str(), value.as_str());
    }
    request.json(&upstream.body).send().await
}

async fn process_with_tool_interception(
    upstream_res: reqwest::Response,
    agent: Arc<dyn Agent>,
    original_req: MessagesRequest,
    config: Config,
) -> Result<impl futures_util::Stream<Item = Result<actix_web::web::Bytes, actix_web::Error>>, String>
{
    // Collect full response text
    let text = upstream_res
        .text()
        .await
        .map_err(|e| format!("Failed to read upstream response: {}", e))?;

    // Parse SSE events
    let mut parser = SseParser::new();
    let events = parser.feed(&text);

    // Process events through rewriter
    let mut rewriter = SseRewriter::new();
    let mut output_events = Vec::new();
    let mut assistant_messages = Vec::new();
    let mut tool_messages = Vec::new();

    for event in events {
        let (forward, tool_call) = rewriter.feed(event);
        output_events.extend(forward);

        if let Some(call) = tool_call {
            debug!(tool = %call.name, id = %call.id, "Tool call intercepted");

            // Record assistant message
            assistant_messages.push(serde_json::json!({
                "type": "tool_use",
                "id": call.id,
                "name": call.name,
                "input": call.input
            }));

            // Execute tool
            match agent
                .execute_tool(&call.name, call.input, &original_req, &config)
                .await
            {
                Some(result) => {
                    debug!(tool = %call.name, "Tool execution successful");
                    tool_messages.push(serde_json::json!({
                        "tool_use_id": call.id,
                        "type": "tool_result",
                        "content": result
                    }));
                }
                None => {
                    error!(tool = %call.name, "Tool execution failed");
                    tool_messages.push(serde_json::json!({
                        "tool_use_id": call.id,
                        "type": "tool_result",
                        "content": "Tool execution failed",
                        "is_error": true
                    }));
                }
            }
        }
    }

    // If we have tool calls, invoke continuation
    if !tool_messages.is_empty() {
        debug!(
            count = tool_messages.len(),
            "Invoking continuation with tool results"
        );
        match invoke_continuation(&original_req, assistant_messages, tool_messages, &config).await {
            Ok(continuation_events) => {
                debug!(
                    count = continuation_events.len(),
                    "Continuation returned events"
                );
                output_events.extend(continuation_events);
            }
            Err(e) => {
                error!("Continuation failed: {}", e);
                // Add error event
                output_events.push(ccr_sse::SseEvent {
                    event: Some("error".to_string()),
                    data: format!(r#"{{"error":"Continuation failed: {}"}}"#, e),
                });
            }
        }
    }

    // Serialize events to SSE format
    let sse_text = output_events
        .iter()
        .map(|e| e.serialize())
        .collect::<String>();

    // Create a stream that yields the SSE text
    let stream = futures_util::stream::once(async move {
        Ok::<_, actix_web::Error>(actix_web::web::Bytes::from(sse_text))
    });

    Ok(stream)
}

fn build_http_client(config: &Config) -> reqwest::Client {
    let mut client_builder = reqwest::Client::builder().timeout(Duration::from_secs(300));

    // Configure proxy if present
    if let Some(proxy_config) = &config.proxy {
        if let Some(http_proxy) = &proxy_config.http {
            match reqwest::Proxy::http(http_proxy) {
                Ok(proxy) => {
                    info!(proxy = %http_proxy, "Configured HTTP proxy");
                    client_builder = client_builder.proxy(proxy);
                }
                Err(e) => {
                    warn!(proxy = %http_proxy, error = %e, "Failed to configure HTTP proxy");
                }
            }
        }

        if let Some(https_proxy) = &proxy_config.https {
            match reqwest::Proxy::https(https_proxy) {
                Ok(proxy) => {
                    info!(proxy = %https_proxy, "Configured HTTPS proxy");
                    client_builder = client_builder.proxy(proxy);
                }
                Err(e) => {
                    warn!(proxy = %https_proxy, error = %e, "Failed to configure HTTPS proxy");
                }
            }
        }

        if let Some(socks5_proxy) = &proxy_config.socks5 {
            match reqwest::Proxy::all(socks5_proxy) {
                Ok(proxy) => {
                    info!(proxy = %socks5_proxy, "Configured SOCKS5 proxy");
                    client_builder = client_builder.proxy(proxy);
                }
                Err(e) => {
                    warn!(proxy = %socks5_proxy, error = %e, "Failed to configure SOCKS5 proxy");
                }
            }
        }
    }

    client_builder.build().unwrap_or_else(|e| {
        warn!(error = %e, "Failed to build HTTP client, using default");
        reqwest::Client::new()
    })
}

fn init_tracing(log_level: &str) {
    use tracing_appender::rolling;
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::fmt;

    // Get log directory
    let log_dir = dirs_next::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".claude-code-router")
        .join("logs");

    // Create log directory if it doesn't exist
    std::fs::create_dir_all(&log_dir).ok();

    // File appender (daily rotation)
    let file_appender = rolling::daily(log_dir, "ccr-server.log");
    let env_filter = std::env::var("RUST_LOG")
        .ok()
        .and_then(|value| EnvFilter::try_new(value).ok())
        .unwrap_or_else(|| {
            EnvFilter::try_new(format!(
                "warn,ccr_server={level},ccr_agent={level},ccr_router={level},ccr_transformer={level},ccr_sse={level},ccr_app_core={level},ccr_config={level},ccr_preset={level}",
                level = normalize_log_level(log_level)
            ))
            .unwrap_or_else(|_| EnvFilter::new("warn,ccr_server=info"))
        });

    // Build subscriber with both console and file output
    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_writer(std::io::stdout)
                .with_ansi(true)
                .with_target(false),
        )
        .with(
            fmt::layer()
                .with_writer(file_appender)
                .with_ansi(false)
                .with_target(true),
        )
        .init();
}

fn normalize_log_level(log_level: &str) -> &'static str {
    match log_level.trim().to_ascii_lowercase().as_str() {
        "trace" => "trace",
        "debug" => "debug",
        "warn" | "warning" => "warn",
        "error" => "error",
        "info" => "info",
        _ => "info",
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let config_path = default_config_path();
    let config = load_config(&config_path).unwrap_or_else(|e| {
        eprintln!("Could not load config: {e}");
        Config::default()
    });
    init_tracing(&config.app_settings.log_level);

    let port = config.port.unwrap_or(3456);
    let host = config
        .host
        .clone()
        .unwrap_or_else(|| "127.0.0.1".to_string());

    info!(providers = config.providers.len(), "Loaded config");
    info!(host = %host, port = port, "Starting server");
    log_server_event(
        "server_started",
        &[
            ("host", host.clone()),
            ("port", port.to_string()),
            ("route_pool", route_pool_routes_for_log(&config)),
            ("providers", provider_names(&config)),
            (
                "app_log_path",
                ccr_app_core::logging::app_log_path().display().to_string(),
            ),
        ],
    );

    // Build HTTP client with proxy support
    let client = build_http_client(&config);

    // Create reloadable config
    let reloadable_config = Arc::new(ReloadableConfig::new(config, config_path));

    // Initialize agents
    let agents: Vec<Arc<dyn Agent>> = vec![Arc::new(ImageAgent)];
    info!(count = agents.len(), "Registered agents");

    let state = Arc::new(AppState {
        reloadable_config,
        transformers: Arc::new(TransformerRegistry::new()),
        client,
        agents,
        route_pool_state: Arc::new(Mutex::new(HashMap::new())),
        route_pool_events: Arc::new(Mutex::new(VecDeque::new())),
        metrics: Arc::new(Mutex::new(RuntimeMetricsStore::load(1000))),
    });
    let data = web::Data::new(state);

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .route("/health", web::get().to(health))
            .route("/v1/messages", web::post().to(messages))
            .route("/v1/responses", web::post().to(responses))
            .route(
                "/v1/messages/count_tokens",
                web::post().to(count_tokens_handler),
            )
            .route("/api/v1/messages", web::post().to(messages))
            .route("/api/config", web::get().to(get_config))
            .route("/api/config", web::put().to(put_config))
            .route("/api/transformers", web::get().to(get_transformers))
            .route("/api/logs", web::get().to(get_logs))
            .route("/api/logs/query", web::get().to(query_logs))
            .route("/api/logs", web::delete().to(delete_logs))
            .route(
                "/api/route-pool/status",
                web::get().to(get_route_pool_status),
            )
            .route(
                "/api/route-pool/events",
                web::get().to(get_route_pool_events),
            )
            .route(
                "/api/route-pool/clear-ban",
                web::post().to(clear_route_pool_ban),
            )
            .route(
                "/api/route-pool/reset-route",
                web::post().to(reset_route_pool_route),
            )
            .route(
                "/api/runtime-metrics/attempts",
                web::get().to(get_runtime_metric_attempts),
            )
            .route(
                "/api/runtime-metrics/requests",
                web::get().to(get_runtime_metric_requests),
            )
            .route(
                "/api/runtime-metrics/summary",
                web::get().to(get_runtime_metric_summary),
            )
            .route(
                "/api/runtime-metrics/ttft-summary",
                web::get().to(get_runtime_metric_ttft_summary),
            )
            .route(
                "/api/runtime-metrics/diagnostics",
                web::get().to(get_runtime_metric_diagnostics),
            )
            .route("/api/presets", web::get().to(get_presets))
            .route("/api/presets/{name}", web::get().to(get_preset))
            .route(
                "/api/presets/{name}",
                web::delete().to(delete_preset_handler),
            )
            .route("/api/admin/reload", web::post().to(handlers::reload_config))
    })
    .bind((host.as_str(), port))?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::body::to_bytes;
    use ccr_types::{Message, RoutePoolCandidate, RoutePoolConfig, TokenizerBackend};
    use std::path::PathBuf;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn spawn_tokenizer_server(token_count: usize) -> (String, Arc<AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind tokenizer test server");
        let addr = listener.local_addr().expect("tokenizer test server addr");
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_for_thread = calls.clone();

        tokio::spawn(async move {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            calls_for_thread.fetch_add(1, Ordering::SeqCst);

            let mut buf = [0; 1024];
            let _ = stream.read(&mut buf).await;

            let body = format!(r#"{{"token_count":{}}}"#, token_count);
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes()).await;
        });

        (format!("http://{addr}/tokenize"), calls)
    }

    fn test_state(config: Config) -> Arc<AppState> {
        Arc::new(AppState {
            reloadable_config: Arc::new(ReloadableConfig::new(
                config,
                PathBuf::from("/tmp/ccr-server-test-config.json"),
            )),
            transformers: Arc::new(TransformerRegistry::new()),
            client: reqwest::Client::new(),
            agents: vec![],
            route_pool_state: Arc::new(Mutex::new(HashMap::new())),
            route_pool_events: Arc::new(Mutex::new(VecDeque::new())),
            metrics: Arc::new(Mutex::new(RuntimeMetricsStore::load(1000))),
        })
    }

    fn pool_config() -> Config {
        Config {
            route_pool: Some(RoutePoolConfig {
                enabled: true,
                failure_threshold: 3,
                ban_seconds: 3600,
                candidates: vec![RoutePoolCandidate {
                    route: "p".to_string(),
                    enabled: true,
                    priority: 1,
                }],
            }),
            ..Default::default()
        }
    }

    #[actix_web::test]
    async fn count_tokens_handler_uses_async_api_tokenizer_fallback() {
        let mut config = Config {
            api_key: Some("test-key".into()),
            ..Default::default()
        };
        config.router.tokenizer_backend = TokenizerBackend::Api {
            endpoint: "http://127.0.0.1:9/tokenize".into(),
        };
        let state = web::Data::new(test_state(config));
        let msg_req = MessagesRequest {
            model: "claude-3-5-sonnet".into(),
            messages: vec![Message {
                role: "user".into(),
                content: serde_json::json!("server handler async api fallback unique"),
            }],
            system: None,
            tools: None,
            max_tokens: Some(100),
            stream: false,
            thinking: None,
        };
        let expected = ccr_router::count_tokens(&msg_req, &TokenizerBackend::Tiktoken);
        let req = actix_web::test::TestRequest::post()
            .insert_header(("authorization", "Bearer test-key"))
            .to_http_request();
        let body = web::Bytes::from(serde_json::to_vec(&msg_req).unwrap());

        let resp = count_tokens_handler(req, body, state).await;

        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
        let body = to_bytes(resp.into_body()).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["input_tokens"].as_u64(), Some(expected as u64));
    }

    #[actix_web::test]
    async fn runtime_metrics_diagnostics_handler_serializes_diagnostics() {
        let mut config = Config {
            api_key: Some("test-key".into()),
            ..Default::default()
        };
        config.route_pool = Some(ccr_types::RoutePoolConfig {
            enabled: true,
            candidates: vec![],
            failure_threshold: 1,
            ban_seconds: 1,
        });
        let state = web::Data::new(test_state(config));
        let req = actix_web::test::TestRequest::get()
            .insert_header(("authorization", "Bearer test-key"))
            .to_http_request();

        let resp = get_runtime_metric_diagnostics(req, state).await;

        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
        let body = actix_web::body::to_bytes(resp.into_body()).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["attempts"]["malformed_lines"], 0);
        assert_eq!(value["requests"]["malformed_lines"], 0);
        assert!(
            value["attempts"]["path"]
                .as_str()
                .unwrap_or_default()
                .ends_with("runtime-metrics.jsonl")
        );
        assert!(
            value["requests"]["path"]
                .as_str()
                .unwrap_or_default()
                .ends_with("runtime-request-metrics.jsonl")
        );
    }

    #[actix_web::test]
    async fn count_tokens_handler_uses_async_api_tokenizer_success() {
        let (endpoint, calls) = spawn_tokenizer_server(77).await;
        let mut config = Config {
            api_key: Some("test-key".into()),
            ..Default::default()
        };
        config.router.tokenizer_backend = TokenizerBackend::Api { endpoint };
        let state = web::Data::new(test_state(config));
        let msg_req = MessagesRequest {
            model: "claude-3-5-sonnet".into(),
            messages: vec![Message {
                role: "user".into(),
                content: serde_json::json!("server handler async api success unique"),
            }],
            system: None,
            tools: None,
            max_tokens: Some(100),
            stream: false,
            thinking: None,
        };
        let req = actix_web::test::TestRequest::post()
            .insert_header(("authorization", "Bearer test-key"))
            .to_http_request();
        let body = web::Bytes::from(serde_json::to_vec(&msg_req).unwrap());

        let resp = count_tokens_handler(req, body, state).await;

        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
        let body = to_bytes(resp.into_body()).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["input_tokens"].as_u64(), Some(77));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    #[actix_web::test]
    async fn route_pool_action_requires_auth() {
        let mut config = pool_config();
        config.api_key = Some("secret".to_string());
        let state = web::Data::new(test_state(config));
        let req = actix_web::test::TestRequest::post().to_http_request();
        let body = web::Json(RoutePoolRouteActionRequest {
            route: "p".to_string(),
        });

        let resp = clear_route_pool_ban(req, body, state).await;

        assert_eq!(resp.status(), actix_web::http::StatusCode::UNAUTHORIZED);
    }

    #[actix_web::test]
    async fn route_pool_clear_ban_reports_not_configured_not_banned_and_cleared() {
        let mut config = pool_config();
        config.api_key = Some("secret".to_string());
        let state = web::Data::new(test_state(config.clone()));
        let authed_req = || {
            actix_web::test::TestRequest::post()
                .insert_header(("authorization", "Bearer secret"))
                .to_http_request()
        };

        let missing = clear_route_pool_ban(
            authed_req(),
            web::Json(RoutePoolRouteActionRequest {
                route: "missing".to_string(),
            }),
            state.clone(),
        )
        .await;
        assert_eq!(missing.status(), actix_web::http::StatusCode::NOT_FOUND);

        let ready = clear_route_pool_ban(
            authed_req(),
            web::Json(RoutePoolRouteActionRequest {
                route: "p".to_string(),
            }),
            state.clone(),
        )
        .await;
        assert_eq!(ready.status(), actix_web::http::StatusCode::OK);
        let ready_body = to_bytes(ready.into_body()).await.unwrap();
        let ready_value: serde_json::Value = serde_json::from_slice(&ready_body).unwrap();
        assert_eq!(ready_value["status"], "not_banned");
        assert_eq!(ready_value["changed"], false);

        state.route_pool_state.lock().unwrap().insert(
            "p".to_string(),
            RoutePoolRouteState {
                consecutive_failures: 3,
                banned_until_epoch_secs: Some(3702),
                last_error: Some("failed".to_string()),
                last_failure_epoch_secs: Some(102),
                last_success_epoch_secs: None,
            },
        );

        let cleared = clear_route_pool_ban(
            authed_req(),
            web::Json(RoutePoolRouteActionRequest {
                route: "p".to_string(),
            }),
            state.clone(),
        )
        .await;
        assert_eq!(cleared.status(), actix_web::http::StatusCode::OK);
        let cleared_body = to_bytes(cleared.into_body()).await.unwrap();
        let cleared_value: serde_json::Value = serde_json::from_slice(&cleared_body).unwrap();
        assert_eq!(cleared_value["status"], "cleared");
        assert_eq!(cleared_value["changed"], true);
        assert_eq!(
            state
                .route_pool_state
                .lock()
                .unwrap()
                .get("p")
                .unwrap()
                .banned_until_epoch_secs,
            None
        );
    }

    #[actix_web::test]
    async fn route_pool_reset_route_reports_clear_semantics_and_event_query_is_authed() {
        let mut config = pool_config();
        config.api_key = Some("secret".to_string());
        let state = web::Data::new(test_state(config));
        state.route_pool_state.lock().unwrap().insert(
            "p".to_string(),
            RoutePoolRouteState {
                consecutive_failures: 2,
                last_error: Some("failed".to_string()),
                ..Default::default()
            },
        );
        let authed_req = || {
            actix_web::test::TestRequest::post()
                .insert_header(("authorization", "Bearer secret"))
                .to_http_request()
        };

        let unauth_events = get_route_pool_events(
            actix_web::test::TestRequest::get().to_http_request(),
            web::Query(HashMap::new()),
            state.clone(),
        )
        .await;
        assert_eq!(
            unauth_events.status(),
            actix_web::http::StatusCode::UNAUTHORIZED
        );

        let missing = reset_route_pool_route(
            authed_req(),
            web::Json(RoutePoolRouteActionRequest {
                route: "missing".to_string(),
            }),
            state.clone(),
        )
        .await;
        assert_eq!(missing.status(), actix_web::http::StatusCode::NOT_FOUND);

        let reset = reset_route_pool_route(
            authed_req(),
            web::Json(RoutePoolRouteActionRequest {
                route: "p".to_string(),
            }),
            state.clone(),
        )
        .await;
        assert_eq!(reset.status(), actix_web::http::StatusCode::OK);
        let reset_body = to_bytes(reset.into_body()).await.unwrap();
        let reset_value: serde_json::Value = serde_json::from_slice(&reset_body).unwrap();
        assert_eq!(reset_value["status"], "reset");
        assert!(!state.route_pool_state.lock().unwrap().contains_key("p"));

        let already_empty = reset_route_pool_route(
            authed_req(),
            web::Json(RoutePoolRouteActionRequest {
                route: "p".to_string(),
            }),
            state.clone(),
        )
        .await;
        assert_eq!(already_empty.status(), actix_web::http::StatusCode::OK);
        let already_empty_body = to_bytes(already_empty.into_body()).await.unwrap();
        let already_empty_value: serde_json::Value =
            serde_json::from_slice(&already_empty_body).unwrap();
        assert_eq!(already_empty_value["status"], "already_empty");
        assert_eq!(already_empty_value["changed"], false);

        let events_req = actix_web::test::TestRequest::get()
            .insert_header(("authorization", "Bearer secret"))
            .to_http_request();
        let events = get_route_pool_events(
            events_req,
            web::Query(HashMap::from([("limit".to_string(), "10".to_string())])),
            state,
        )
        .await;
        assert_eq!(events.status(), actix_web::http::StatusCode::OK);
        let events_body = to_bytes(events.into_body()).await.unwrap();
        let events_value: serde_json::Value = serde_json::from_slice(&events_body).unwrap();
        assert_eq!(events_value[0]["event_type"], "route_pool_candidate_reset");
    }
}
