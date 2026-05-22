use actix_web::web;
use ccr_app_core::metrics::{
    AttemptOutcome, ClientRequestMetric, RequestOutcome, RuntimeMetricsStore, UpstreamAttemptMetric,
};
use ccr_server::InboundProtocol;
use futures_util::StreamExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

static NEXT_REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone)]
pub(crate) struct RequestMetricContext {
    pub(crate) request_id: String,
    pub(crate) started_epoch_secs: u64,
    pub(crate) started_at: Instant,
    pub(crate) inbound: InboundProtocol,
    pub(crate) requested_model: String,
}

pub(crate) fn epoch_secs(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

pub(crate) fn next_request_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let sequence = NEXT_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("req-{now}-{sequence}")
}

pub(crate) fn stream_response_with_ttft(
    response: reqwest::Response,
    metrics: Arc<Mutex<RuntimeMetricsStore>>,
    mut pending_attempt_metric: Option<UpstreamAttemptMetric>,
    attempt_started_at: Option<Instant>,
) -> futures_util::stream::BoxStream<'static, Result<web::Bytes, actix_web::Error>> {
    response
        .bytes_stream()
        .map(move |chunk| {
            if chunk.is_ok() {
                record_pending_attempt_ttft(
                    &metrics,
                    pending_attempt_metric.take(),
                    attempt_started_at,
                    "stream_first_chunk",
                );
            }
            chunk.map_err(actix_web::error::ErrorBadGateway)
        })
        .boxed()
}

pub(crate) fn record_pending_attempt_ttft(
    metrics: &Arc<Mutex<RuntimeMetricsStore>>,
    pending_attempt_metric: Option<UpstreamAttemptMetric>,
    attempt_started_at: Option<Instant>,
    source: &str,
) {
    if let (Some(mut metric), Some(started_at)) = (pending_attempt_metric, attempt_started_at) {
        metric.ttft_ms = Some(started_at.elapsed().as_millis() as u64);
        metric.ttft_source = Some(source.to_string());
        record_prepared_attempt_metric(metrics, metric);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn record_attempt_metric(
    metrics: &Arc<Mutex<RuntimeMetricsStore>>,
    request_id: &str,
    inbound: InboundProtocol,
    route: &str,
    provider: &str,
    endpoint: &str,
    model: &str,
    http_status: Option<u16>,
    error_class: Option<&str>,
    latency_ms: Option<u64>,
    retry_attempt_index: usize,
    outcome: AttemptOutcome,
) {
    let metric = upstream_attempt_metric(
        request_id,
        inbound,
        route,
        provider,
        endpoint,
        model,
        http_status,
        error_class,
        latency_ms,
        retry_attempt_index,
        outcome,
    );
    record_prepared_attempt_metric(metrics, metric);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn upstream_attempt_metric(
    request_id: &str,
    inbound: InboundProtocol,
    route: &str,
    provider: &str,
    endpoint: &str,
    model: &str,
    http_status: Option<u16>,
    error_class: Option<&str>,
    latency_ms: Option<u64>,
    retry_attempt_index: usize,
    outcome: AttemptOutcome,
) -> UpstreamAttemptMetric {
    UpstreamAttemptMetric {
        request_id: request_id.to_string(),
        timestamp_epoch_secs: epoch_secs(SystemTime::now()),
        inbound: format!("{:?}", inbound),
        route: route.to_string(),
        provider: provider.to_string(),
        endpoint: endpoint.to_string(),
        model: model.to_string(),
        http_status,
        error_class: error_class.map(str::to_string),
        latency_ms,
        ttft_ms: None,
        ttft_source: None,
        retry_attempt_index,
        outcome,
    }
}

pub(crate) fn record_prepared_attempt_metric(
    metrics: &Arc<Mutex<RuntimeMetricsStore>>,
    metric: UpstreamAttemptMetric,
) {
    if let Ok(mut metrics) = metrics.lock() {
        metrics.record(metric);
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn record_request_metric(
    metrics: &Arc<Mutex<RuntimeMetricsStore>>,
    context: &RequestMetricContext,
    selected_route: &str,
    final_provider: &str,
    final_model: &str,
    http_status: Option<u16>,
    error_class: Option<&str>,
    attempt_count: usize,
    outcome: RequestOutcome,
) {
    let metric = ClientRequestMetric {
        request_id: context.request_id.clone(),
        started_epoch_secs: context.started_epoch_secs,
        finished_epoch_secs: epoch_secs(SystemTime::now()),
        inbound: format!("{:?}", context.inbound),
        requested_model: context.requested_model.clone(),
        selected_route: selected_route.to_string(),
        final_provider: final_provider.to_string(),
        final_model: final_model.to_string(),
        http_status,
        error_class: error_class.map(str::to_string),
        total_latency_ms: context.started_at.elapsed().as_millis() as u64,
        attempt_count,
        outcome,
    };
    if let Ok(mut metrics) = metrics.lock() {
        metrics.record_request(metric);
    }
}

pub(crate) fn status_error_class(status: u16) -> &'static str {
    match status {
        401 | 403 => "auth",
        429 => "rate_limit",
        500..=599 => "server",
        400..=499 => "client_request",
        _ => "http_error",
    }
}
