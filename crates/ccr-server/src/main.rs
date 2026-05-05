mod handlers;

use actix_web::{App, HttpRequest, HttpResponse, HttpServer, web};
use ccr_agent::{Agent, ImageAgent};
use ccr_config::{ReloadableConfig, default_config_path, load_config, save_config};
use ccr_preset::{delete_preset, list_presets, load_preset};
use ccr_router::find_provider;
use ccr_server::{
    InboundProtocol, UpstreamRequest, build_upstream_request, check_auth, provider_names,
    provider_not_found_message, redact_config, route_for_display, route_pool_ban_seconds,
    route_pool_candidates, route_pool_enabled, route_pool_failure_threshold,
};
use ccr_sse::{SseParser, SseRewriter, invoke_continuation};
use ccr_transformer::TransformerRegistry;
use ccr_types::{Config, MessagesRequest};
use futures_util::StreamExt;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tracing::{debug, error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub struct AppState {
    reloadable_config: Arc<ReloadableConfig>,
    transformers: Arc<TransformerRegistry>,
    client: reqwest::Client,
    agents: Vec<Arc<dyn Agent>>,
    route_pool_state: Arc<Mutex<HashMap<String, RoutePoolRouteState>>>,
}

#[derive(Debug, Clone, Default, Serialize)]
struct RoutePoolRouteState {
    consecutive_failures: u32,
    banned_until_epoch_secs: Option<u64>,
    last_error: Option<String>,
    last_failure_epoch_secs: Option<u64>,
    last_success_epoch_secs: Option<u64>,
}

impl RoutePoolRouteState {
    fn is_banned(&self, now: SystemTime) -> bool {
        self.banned_until_epoch_secs
            .is_some_and(|until| until > epoch_secs(now))
    }
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

fn auth_check(req: &HttpRequest, config: &Config) -> bool {
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
    let n = ccr_router::count_tokens(&msg_req, &config.router.tokenizer_backend);
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

    let upstream_res = match send_with_route_pool(
        &state.client,
        &config,
        &state.transformers,
        &state.route_pool_state,
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
        let stream = upstream_res
            .response
            .bytes_stream()
            .map(|chunk| chunk.map_err(|e| actix_web::error::ErrorBadGateway(e)));
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

    let upstream_res = match send_with_route_pool(
        &state.client,
        &config,
        &state.transformers,
        &state.route_pool_state,
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
        let stream = upstream_res
            .response
            .bytes_stream()
            .map(|chunk| chunk.map_err(|e| actix_web::error::ErrorBadGateway(e)));
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

struct UpstreamAttemptResponse {
    response: reqwest::Response,
    stream: bool,
}

async fn send_with_route_pool(
    client: &reqwest::Client,
    config: &Config,
    transformers: &TransformerRegistry,
    route_pool_state: &Arc<Mutex<HashMap<String, RoutePoolRouteState>>>,
    inbound: InboundProtocol,
    _primary_route: &str,
    body_json: serde_json::Value,
) -> Result<UpstreamAttemptResponse, String> {
    let pool_enabled = route_pool_enabled(config);
    let configured_pool_routes = route_pool_candidates(config, &[]);
    if !pool_enabled || configured_pool_routes.is_empty() {
        return Err("Route Pool is not configured or has no enabled routes".to_string());
    }

    let mut attempts = configured_pool_routes.clone();
    let mut last_error = None;

    let now = SystemTime::now();
    attempts.retain(|route| {
        let banned = route_pool_state
            .lock()
            .ok()
            .and_then(|state| state.get(route).cloned())
            .is_some_and(|state| state.is_banned(now));
        if banned {
            log_route_pool_event("route_pool_candidate_skipped", route, "banned", None);
        }
        !banned
    });

    for route in attempts {
        let Some(provider) = find_provider(&route, config) else {
            let error = provider_not_found_message(&route, config);
            record_route_pool_failure(route_pool_state, config, &route, &error);
            log_upstream_event(
                "provider_not_found",
                inbound,
                &route,
                "",
                "",
                "",
                None,
                None,
                Some(&error),
                None,
                &[],
                Some(&body_json),
                &[("available_providers", provider_names(config))],
            );
            last_error = Some(error);
            continue;
        };
        let upstream = match build_upstream_request(
            inbound,
            &route,
            provider,
            body_json.clone(),
            transformers,
        ) {
            Ok(upstream) => upstream,
            Err(error) => {
                log_upstream_event(
                    "build_failed",
                    inbound,
                    &route,
                    provider.name.as_str(),
                    provider.api_base_url.as_str(),
                    "",
                    None,
                    None,
                    Some(&error),
                    None,
                    &[],
                    None,
                    &[],
                );
                record_route_pool_failure(route_pool_state, config, &route, &error);
                last_error = Some(error.clone());
                continue;
            }
        };
        info!(route = %route, url = %upstream.url, "Sending upstream request");
        log_upstream_event(
            "start",
            inbound,
            &route,
            provider.name.as_str(),
            upstream.url.as_str(),
            upstream.model.as_str(),
            None,
            None,
            None,
            None,
            &upstream.headers,
            Some(&upstream.body),
            &[],
        );

        let start = Instant::now();
        match send_upstream_request(client, &upstream).await {
            Ok(response) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                if should_try_next_status(response.status(), pool_enabled) {
                    let status = response.status();
                    let status_u16 = status.as_u16();
                    let response_text = response.text().await.unwrap_or_default();
                    let response_summary =
                        ccr_app_core::logging::response_log_summary(&response_text);
                    warn!(route = %route, status = %status, "Upstream returned retryable status");
                    log_upstream_event(
                        "retryable_status",
                        inbound,
                        &route,
                        provider.name.as_str(),
                        upstream.url.as_str(),
                        upstream.model.as_str(),
                        Some(status_u16),
                        Some(latency_ms),
                        None,
                        Some(&response_summary),
                        &upstream.headers,
                        Some(&upstream.body),
                        &[],
                    );
                    last_error = Some(format!(
                        "Upstream {route} returned {status}: {}",
                        ccr_app_core::logging::ui_error_summary(&response_summary)
                    ));
                    record_route_pool_failure(
                        route_pool_state,
                        config,
                        &route,
                        last_error.as_deref().unwrap_or("upstream failed"),
                    );
                    continue;
                }
                if response.status().is_success() {
                    record_route_pool_success(route_pool_state, &route);
                }
                log_upstream_event(
                    "result",
                    inbound,
                    &route,
                    provider.name.as_str(),
                    upstream.url.as_str(),
                    upstream.model.as_str(),
                    Some(response.status().as_u16()),
                    Some(latency_ms),
                    None,
                    None,
                    &upstream.headers,
                    None,
                    &[],
                );
                return Ok(UpstreamAttemptResponse {
                    response,
                    stream: upstream.stream,
                });
            }
            Err(error) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                warn!(route = %route, error = %error, "Upstream request failed");
                record_route_pool_failure(route_pool_state, config, &route, &error.to_string());
                log_upstream_event(
                    "request_failed",
                    inbound,
                    &route,
                    provider.name.as_str(),
                    upstream.url.as_str(),
                    upstream.model.as_str(),
                    None,
                    Some(latency_ms),
                    Some(&error.to_string()),
                    None,
                    &upstream.headers,
                    Some(&upstream.body),
                    &[],
                );
                last_error = Some(error.to_string());
            }
        }
    }

    let error = last_error
        .or_else(|| last_route_pool_error(route_pool_state, &configured_pool_routes))
        .unwrap_or_else(|| "No upstream routes attempted".to_string());
    log_route_pool_event("route_pool_exhausted", "<route-pool>", &error, None);
    log_upstream_event(
        "final_failure",
        inbound,
        "<route-pool>",
        "",
        "",
        "",
        None,
        None,
        Some(&error),
        None,
        &[],
        Some(&body_json),
        &[("available_providers", provider_names(config))],
    );
    Err(error)
}

fn last_route_pool_error(
    states: &Arc<Mutex<HashMap<String, RoutePoolRouteState>>>,
    routes: &[String],
) -> Option<String> {
    let states = states.lock().ok()?;
    routes
        .iter()
        .rev()
        .filter_map(|route| {
            states
                .get(route)
                .and_then(|state| state.last_error.clone())
                .map(|error| format!("Upstream {route} unavailable: {error}"))
        })
        .next()
}

fn route_pool_routes_for_log(config: &Config) -> String {
    let routes = route_pool_candidates(config, &[]);
    if routes.is_empty() {
        return "<empty>".to_string();
    }
    routes
        .into_iter()
        .map(|route| route_for_display(&route))
        .collect::<Vec<_>>()
        .join(",")
}

fn should_try_next_status(status: reqwest::StatusCode, _pool_enabled: bool) -> bool {
    status.is_server_error()
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status == reqwest::StatusCode::UNAUTHORIZED
        || status == reqwest::StatusCode::FORBIDDEN
}

fn record_route_pool_success(
    states: &Arc<Mutex<HashMap<String, RoutePoolRouteState>>>,
    route: &str,
) {
    let now = epoch_secs(SystemTime::now());
    if apply_route_pool_success(states, route, now) {
        log_route_pool_event("route_pool_candidate_recovered", route, "success", None);
    }
}

fn apply_route_pool_success(
    states: &Arc<Mutex<HashMap<String, RoutePoolRouteState>>>,
    route: &str,
    now: u64,
) -> bool {
    if let Ok(mut states) = states.lock() {
        let state = states.entry(route.to_string()).or_default();
        let was_banned = state.banned_until_epoch_secs.take().is_some();
        state.consecutive_failures = 0;
        state.last_success_epoch_secs = Some(now);
        state.last_error = None;
        return was_banned;
    }
    false
}

fn record_route_pool_failure(
    states: &Arc<Mutex<HashMap<String, RoutePoolRouteState>>>,
    config: &Config,
    route: &str,
    error: &str,
) {
    let threshold = route_pool_failure_threshold(config);
    let ban_seconds = route_pool_ban_seconds(config);
    let now = SystemTime::now();
    let now_secs = epoch_secs(now);
    let (failures, banned_until) =
        apply_route_pool_failure(states, route, error, threshold, ban_seconds, now_secs);

    log_route_pool_event(
        "route_pool_candidate_failed",
        route,
        error,
        Some(("consecutive_failures", failures.to_string())),
    );
    if let Some(until) = banned_until {
        log_route_pool_event(
            "route_pool_candidate_banned",
            route,
            error,
            Some(("banned_until_epoch_secs", until.to_string())),
        );
    }
}

fn apply_route_pool_failure(
    states: &Arc<Mutex<HashMap<String, RoutePoolRouteState>>>,
    route: &str,
    error: &str,
    threshold: u32,
    ban_seconds: u64,
    now_secs: u64,
) -> (u32, Option<u64>) {
    let mut banned_until = None;
    let mut failures = 0;

    if let Ok(mut states) = states.lock() {
        let state = states.entry(route.to_string()).or_default();
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        state.last_error = Some(ccr_app_core::logging::ui_error_summary(error));
        state.last_failure_epoch_secs = Some(now_secs);
        failures = state.consecutive_failures;
        if state.consecutive_failures >= threshold {
            let until = now_secs.saturating_add(ban_seconds);
            state.banned_until_epoch_secs = Some(until);
            banned_until = Some(until);
        }
    }

    (failures, banned_until)
}

fn log_route_pool_event(
    event: &str,
    route: &str,
    error: &str,
    extra: Option<(&'static str, String)>,
) {
    let mut fields = vec![
        ("route", route_for_display(route)),
        ("error", ccr_app_core::logging::ui_error_summary(error)),
    ];
    if let Some(extra) = extra {
        fields.push(extra);
    }
    ccr_app_core::logging::append_app_log("route-pool", event, &fields);
}

fn epoch_secs(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
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
            .route("/api/logs", web::delete().to(delete_logs))
            .route(
                "/api/route-pool/status",
                web::get().to(get_route_pool_status),
            )
            .route(
                "/api/provider-pool/status",
                web::get().to(get_route_pool_status),
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
    use ccr_types::{RoutePoolCandidate, RoutePoolConfig};

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

    #[test]
    fn route_pool_retry_statuses_include_rate_limit_auth_and_server_errors() {
        assert!(should_try_next_status(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            true
        ));
        assert!(should_try_next_status(
            reqwest::StatusCode::UNAUTHORIZED,
            true
        ));
        assert!(should_try_next_status(reqwest::StatusCode::FORBIDDEN, true));
        assert!(should_try_next_status(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            true
        ));
        assert!(!should_try_next_status(
            reqwest::StatusCode::BAD_REQUEST,
            true
        ));
    }

    #[test]
    fn route_pool_bans_after_threshold_failures() {
        let config = pool_config();
        let states = Arc::new(Mutex::new(HashMap::new()));
        let threshold = route_pool_failure_threshold(&config);
        let ban_seconds = route_pool_ban_seconds(&config);

        apply_route_pool_failure(&states, "p", "first", threshold, ban_seconds, 100);
        apply_route_pool_failure(&states, "p", "second", threshold, ban_seconds, 101);
        apply_route_pool_failure(&states, "p", "third", threshold, ban_seconds, 102);

        let state = states.lock().unwrap().get("p").cloned().unwrap();
        assert_eq!(state.consecutive_failures, 3);
        assert_eq!(state.banned_until_epoch_secs, Some(3702));
        assert_eq!(state.last_error.as_deref(), Some("third"));
    }

    #[test]
    fn route_pool_success_clears_failure_and_ban_state() {
        let config = pool_config();
        let states = Arc::new(Mutex::new(HashMap::new()));
        let threshold = route_pool_failure_threshold(&config);
        let ban_seconds = route_pool_ban_seconds(&config);
        apply_route_pool_failure(&states, "p", "first", threshold, ban_seconds, 100);
        apply_route_pool_failure(&states, "p", "second", threshold, ban_seconds, 101);
        apply_route_pool_failure(&states, "p", "third", threshold, ban_seconds, 102);

        assert!(apply_route_pool_success(&states, "p", 200));

        let state = states.lock().unwrap().get("p").cloned().unwrap();
        assert_eq!(state.consecutive_failures, 0);
        assert_eq!(state.banned_until_epoch_secs, None);
        assert_eq!(state.last_error, None);
        assert!(state.last_success_epoch_secs.is_some());
    }

    #[test]
    fn route_pool_last_error_uses_last_configured_candidate_with_error() {
        let states = Arc::new(Mutex::new(HashMap::from([
            (
                "a".to_string(),
                RoutePoolRouteState {
                    last_error: Some("first error".to_string()),
                    ..Default::default()
                },
            ),
            (
                "b".to_string(),
                RoutePoolRouteState {
                    last_error: Some("second error".to_string()),
                    ..Default::default()
                },
            ),
        ])));

        let error = last_route_pool_error(&states, &["a".to_string(), "b".to_string()]);

        assert_eq!(
            error.as_deref(),
            Some("Upstream b unavailable: second error")
        );
    }
}
