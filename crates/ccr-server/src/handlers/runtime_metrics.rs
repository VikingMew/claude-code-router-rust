use actix_web::{HttpRequest, HttpResponse, web};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use crate::runtime::metrics::epoch_secs;
use crate::{AppState, auth_check};

pub async fn get_runtime_metric_attempts(
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
        .unwrap_or(100);
    let attempts = state
        .metrics
        .lock()
        .map(|metrics| metrics.recent_attempts(limit))
        .unwrap_or_default();
    HttpResponse::Ok().json(attempts)
}

pub async fn get_runtime_metric_requests(
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
        .unwrap_or(100);
    let requests = state
        .metrics
        .lock()
        .map(|metrics| metrics.recent_requests(limit))
        .unwrap_or_default();
    HttpResponse::Ok().json(requests)
}

pub async fn get_runtime_metric_summary(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let config = state.get_config().await;
    if !auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }
    let summary = state
        .metrics
        .lock()
        .map(|metrics| metrics.summary())
        .unwrap_or_default();
    HttpResponse::Ok().json(summary)
}

pub async fn get_runtime_metric_ttft_summary(
    req: HttpRequest,
    query: web::Query<HashMap<String, String>>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let config = state.get_config().await;
    if !auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }
    let window_seconds = query
        .get("window")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(300)
        .max(1);
    let now = epoch_secs(SystemTime::now());
    let summary = state
        .metrics
        .lock()
        .map(|metrics| metrics.ttft_summary(window_seconds, now))
        .unwrap_or_default();
    HttpResponse::Ok().json(summary)
}

pub async fn get_runtime_metric_diagnostics(
    req: HttpRequest,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let config = state.get_config().await;
    if !auth_check(&req, &config) {
        return HttpResponse::Unauthorized().finish();
    }
    let diagnostics = state
        .metrics
        .lock()
        .map(|metrics| metrics.diagnostics())
        .unwrap_or_default();
    HttpResponse::Ok().json(diagnostics)
}
