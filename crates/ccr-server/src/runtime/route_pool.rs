use crate::runtime::metrics::{
    RequestMetricContext, epoch_secs, next_request_id, record_attempt_metric,
    record_prepared_attempt_metric, record_request_metric, status_error_class,
    upstream_attempt_metric,
};
use crate::{log_upstream_event, send_upstream_request};
use ccr_app_core::metrics::{
    AttemptOutcome, RequestOutcome, RuntimeMetricsStore, UpstreamAttemptMetric,
};
use ccr_router::find_provider;
use ccr_server::{
    InboundProtocol, build_upstream_request, provider_names, provider_not_found_message,
    route_for_display, route_pool_ban_seconds, route_pool_candidates, route_pool_enabled,
    route_pool_failure_threshold,
};
use ccr_transformer::TransformerRegistry;
use ccr_types::Config;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};
use tracing::{info, warn};

pub(crate) const ROUTE_POOL_EVENT_HISTORY_LIMIT: usize = 200;

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct RoutePoolRouteState {
    pub(crate) consecutive_failures: u32,
    pub(crate) banned_until_epoch_secs: Option<u64>,
    pub(crate) last_error: Option<String>,
    pub(crate) last_failure_epoch_secs: Option<u64>,
    pub(crate) last_success_epoch_secs: Option<u64>,
}

impl RoutePoolRouteState {
    fn is_banned(&self, now: SystemTime) -> bool {
        self.banned_until_epoch_secs
            .is_some_and(|until| until > epoch_secs(now))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RoutePoolEvent {
    pub(crate) timestamp_epoch_secs: u64,
    pub(crate) event_type: String,
    pub(crate) route: String,
    pub(crate) reason: String,
    pub(crate) consecutive_failures: Option<u32>,
    pub(crate) banned_until_epoch_secs: Option<u64>,
}

pub(crate) struct UpstreamAttemptResponse {
    pub(crate) response: reqwest::Response,
    pub(crate) stream: bool,
    pub(crate) mode: ccr_app_core::provider_kind::EndpointTestMode,
    pub(crate) pending_attempt_metric: Option<UpstreamAttemptMetric>,
    pub(crate) attempt_started_at: Option<Instant>,
}

pub(crate) async fn send_with_route_pool(
    client: &reqwest::Client,
    config: &Config,
    transformers: &TransformerRegistry,
    route_pool_state: &Arc<Mutex<HashMap<String, RoutePoolRouteState>>>,
    route_pool_events: &Arc<Mutex<VecDeque<RoutePoolEvent>>>,
    metrics: &Arc<Mutex<RuntimeMetricsStore>>,
    inbound: InboundProtocol,
    requested_model: &str,
    body_json: serde_json::Value,
) -> Result<UpstreamAttemptResponse, String> {
    let request_context = RequestMetricContext {
        request_id: next_request_id(),
        started_epoch_secs: epoch_secs(SystemTime::now()),
        started_at: Instant::now(),
        inbound,
        requested_model: requested_model.to_string(),
    };
    let pool_enabled = route_pool_enabled(config);
    let configured_pool_routes = route_pool_candidates(config, &[]);
    if !pool_enabled || configured_pool_routes.is_empty() {
        record_request_metric(
            metrics,
            &request_context,
            "",
            "",
            "",
            None,
            Some("route_pool"),
            0,
            RequestOutcome::Failed,
        );
        return Err("Route Pool is not configured or has no enabled routes".to_string());
    }

    let mut attempts = configured_pool_routes.clone();
    let mut last_error = None;
    let mut attempted_count = 0usize;

    let now = SystemTime::now();
    attempts.retain(|route| {
        let banned = route_pool_state
            .lock()
            .ok()
            .and_then(|state| state.get(route).cloned())
            .is_some_and(|state| state.is_banned(now));
        if banned {
            record_route_pool_event(
                route_pool_events,
                "route_pool_candidate_skipped",
                route,
                "banned",
                None,
                route_pool_state
                    .lock()
                    .ok()
                    .and_then(|state| state.get(route).cloned())
                    .and_then(|state| state.banned_until_epoch_secs),
            );
        }
        !banned
    });

    for (attempt_index, route) in attempts.into_iter().enumerate() {
        attempted_count += 1;
        let Some(provider) = find_provider(&route, config) else {
            let error = provider_not_found_message(&route, config);
            record_route_pool_failure(route_pool_state, route_pool_events, config, &route, &error);
            record_attempt_metric(
                metrics,
                request_context.request_id.as_str(),
                inbound,
                &route,
                "",
                "",
                "",
                None,
                Some("provider_not_found"),
                None,
                attempt_index,
                AttemptOutcome::ProviderNotFound,
            );
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
                record_route_pool_failure(
                    route_pool_state,
                    route_pool_events,
                    config,
                    &route,
                    &error,
                );
                record_attempt_metric(
                    metrics,
                    request_context.request_id.as_str(),
                    inbound,
                    &route,
                    provider.name.as_str(),
                    provider.api_base_url.as_str(),
                    "",
                    None,
                    Some("build_error"),
                    None,
                    attempt_index,
                    AttemptOutcome::BuildError,
                );
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
                        route_pool_events,
                        config,
                        &route,
                        last_error.as_deref().unwrap_or("upstream failed"),
                    );
                    record_attempt_metric(
                        metrics,
                        request_context.request_id.as_str(),
                        inbound,
                        &route,
                        provider.name.as_str(),
                        upstream.url.as_str(),
                        upstream.model.as_str(),
                        Some(status_u16),
                        Some(status_error_class(status_u16)),
                        Some(latency_ms),
                        attempt_index,
                        AttemptOutcome::HttpError,
                    );
                    continue;
                }
                if response.status().is_success() {
                    record_route_pool_success(route_pool_state, route_pool_events, &route);
                }
                let mut attempt_metric = upstream_attempt_metric(
                    request_context.request_id.as_str(),
                    inbound,
                    &route,
                    provider.name.as_str(),
                    upstream.url.as_str(),
                    upstream.model.as_str(),
                    Some(response.status().as_u16()),
                    if response.status().is_success() {
                        None
                    } else {
                        Some(status_error_class(response.status().as_u16()))
                    },
                    Some(latency_ms),
                    attempt_index,
                    if response.status().is_success() {
                        AttemptOutcome::Success
                    } else {
                        AttemptOutcome::HttpError
                    },
                );
                if !upstream.stream {
                    attempt_metric.ttft_ms = Some(latency_ms);
                    attempt_metric.ttft_source = Some("non_stream_response".to_string());
                    record_prepared_attempt_metric(metrics, attempt_metric.clone());
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
                record_request_metric(
                    metrics,
                    &request_context,
                    &route,
                    provider.name.as_str(),
                    upstream.model.as_str(),
                    Some(response.status().as_u16()),
                    if response.status().is_success() {
                        None
                    } else {
                        Some(status_error_class(response.status().as_u16()))
                    },
                    attempted_count,
                    if response.status().is_success() {
                        RequestOutcome::Success
                    } else {
                        RequestOutcome::HttpError
                    },
                );
                return Ok(UpstreamAttemptResponse {
                    response,
                    stream: upstream.stream,
                    mode: upstream.mode,
                    pending_attempt_metric: if upstream.stream {
                        Some(attempt_metric)
                    } else {
                        None
                    },
                    attempt_started_at: if upstream.stream { Some(start) } else { None },
                });
            }
            Err(error) => {
                let latency_ms = start.elapsed().as_millis() as u64;
                warn!(route = %route, error = %error, "Upstream request failed");
                record_route_pool_failure(
                    route_pool_state,
                    route_pool_events,
                    config,
                    &route,
                    &error.to_string(),
                );
                record_attempt_metric(
                    metrics,
                    request_context.request_id.as_str(),
                    inbound,
                    &route,
                    provider.name.as_str(),
                    upstream.url.as_str(),
                    upstream.model.as_str(),
                    None,
                    Some("network"),
                    Some(latency_ms),
                    attempt_index,
                    AttemptOutcome::NetworkError,
                );
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
    record_request_metric(
        metrics,
        &request_context,
        "<route-pool>",
        "",
        "",
        None,
        Some("route_pool_exhausted"),
        attempted_count,
        RequestOutcome::Failed,
    );
    record_route_pool_event(
        route_pool_events,
        "route_pool_exhausted",
        "<route-pool>",
        &error,
        None,
        None,
    );
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

pub(crate) fn last_route_pool_error(
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

pub(crate) fn route_pool_routes_for_log(config: &Config) -> String {
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

pub(crate) fn should_try_next_status(status: reqwest::StatusCode, _pool_enabled: bool) -> bool {
    status.is_server_error()
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status == reqwest::StatusCode::UNAUTHORIZED
        || status == reqwest::StatusCode::FORBIDDEN
}

fn record_route_pool_success(
    states: &Arc<Mutex<HashMap<String, RoutePoolRouteState>>>,
    events: &Arc<Mutex<VecDeque<RoutePoolEvent>>>,
    route: &str,
) {
    let now = epoch_secs(SystemTime::now());
    if apply_route_pool_success(states, route, now) {
        record_route_pool_event(
            events,
            "route_pool_candidate_recovered",
            route,
            "success",
            Some(0),
            None,
        );
    }
}

pub(crate) fn apply_route_pool_success(
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
    events: &Arc<Mutex<VecDeque<RoutePoolEvent>>>,
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

    record_route_pool_event(
        events,
        "route_pool_candidate_failed",
        route,
        error,
        Some(failures),
        None,
    );
    if let Some(until) = banned_until {
        record_route_pool_event(
            events,
            "route_pool_candidate_banned",
            route,
            error,
            Some(failures),
            Some(until),
        );
    }
}

pub(crate) fn apply_route_pool_failure(
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

pub(crate) fn record_route_pool_event(
    events: &Arc<Mutex<VecDeque<RoutePoolEvent>>>,
    event_type: &str,
    route: &str,
    reason: &str,
    consecutive_failures: Option<u32>,
    banned_until_epoch_secs: Option<u64>,
) {
    let route = route_for_display(route);
    let reason = ccr_app_core::logging::ui_error_summary(reason);
    let mut fields = vec![("route", route.clone()), ("error", reason.clone())];
    if let Some(consecutive_failures) = consecutive_failures {
        fields.push(("consecutive_failures", consecutive_failures.to_string()));
    }
    if let Some(banned_until_epoch_secs) = banned_until_epoch_secs {
        fields.push((
            "banned_until_epoch_secs",
            banned_until_epoch_secs.to_string(),
        ));
    }
    ccr_app_core::logging::append_app_log("route-pool", event_type, &fields);

    if let Ok(mut events) = events.lock() {
        if events.len() >= ROUTE_POOL_EVENT_HISTORY_LIMIT {
            events.pop_front();
        }
        events.push_back(RoutePoolEvent {
            timestamp_epoch_secs: epoch_secs(SystemTime::now()),
            event_type: event_type.to_string(),
            route,
            reason,
            consecutive_failures,
            banned_until_epoch_secs,
        });
    }
}

pub(crate) fn clear_route_pool_ban_state(
    states: &Arc<Mutex<HashMap<String, RoutePoolRouteState>>>,
    route: &str,
) -> (bool, Option<RoutePoolRouteState>) {
    let Ok(mut states) = states.lock() else {
        return (false, None);
    };
    let state = states.entry(route.to_string()).or_default();
    let changed = state.banned_until_epoch_secs.take().is_some();
    (changed, Some(state.clone()))
}

pub(crate) fn reset_route_pool_route_state(
    states: &Arc<Mutex<HashMap<String, RoutePoolRouteState>>>,
    route: &str,
) -> bool {
    let Ok(mut states) = states.lock() else {
        return false;
    };
    states.remove(route).is_some()
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
    fn retry_statuses_include_rate_limit_auth_and_server_errors() {
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
    fn bans_after_threshold_failures() {
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
    fn success_clears_failure_and_ban_state() {
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
    fn last_error_uses_last_configured_candidate_with_error() {
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

    #[test]
    fn event_history_is_bounded_and_recent() {
        let events = Arc::new(Mutex::new(VecDeque::new()));

        for index in 0..(ROUTE_POOL_EVENT_HISTORY_LIMIT + 5) {
            record_route_pool_event(
                &events,
                "route_pool_candidate_failed",
                "p",
                &format!("error-{index}"),
                Some(index as u32),
                None,
            );
        }

        let events = events.lock().unwrap();
        assert_eq!(events.len(), ROUTE_POOL_EVENT_HISTORY_LIMIT);
        assert_eq!(events.front().unwrap().reason, "error-5");
        assert_eq!(
            events.back().unwrap().event_type,
            "route_pool_candidate_failed"
        );
    }

    #[test]
    fn clear_ban_state_keeps_failures_and_clears_only_ban() {
        let states = Arc::new(Mutex::new(HashMap::from([(
            "p".to_string(),
            RoutePoolRouteState {
                consecutive_failures: 3,
                banned_until_epoch_secs: Some(1000),
                last_error: Some("failed".to_string()),
                last_failure_epoch_secs: Some(10),
                last_success_epoch_secs: None,
            },
        )])));

        let (changed, state) = clear_route_pool_ban_state(&states, "p");

        assert!(changed);
        let state = state.unwrap();
        assert_eq!(state.consecutive_failures, 3);
        assert_eq!(state.banned_until_epoch_secs, None);
        assert_eq!(state.last_error.as_deref(), Some("failed"));
    }

    #[test]
    fn reset_route_state_removes_runtime_state() {
        let states = Arc::new(Mutex::new(HashMap::from([(
            "p".to_string(),
            RoutePoolRouteState {
                consecutive_failures: 3,
                banned_until_epoch_secs: Some(1000),
                last_error: Some("failed".to_string()),
                last_failure_epoch_secs: Some(10),
                last_success_epoch_secs: Some(20),
            },
        )])));

        assert!(reset_route_pool_route_state(&states, "p"));
        assert!(!states.lock().unwrap().contains_key("p"));
        assert!(!reset_route_pool_route_state(&states, "p"));
    }
}
