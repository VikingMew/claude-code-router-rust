use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttemptOutcome {
    Success,
    HttpError,
    NetworkError,
    BuildError,
    ProviderNotFound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RequestOutcome {
    Success,
    HttpError,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpstreamAttemptMetric {
    #[serde(default)]
    pub request_id: String,
    pub timestamp_epoch_secs: u64,
    pub inbound: String,
    pub route: String,
    pub provider: String,
    pub endpoint: String,
    pub model: String,
    pub http_status: Option<u16>,
    pub error_class: Option<String>,
    pub latency_ms: Option<u64>,
    #[serde(default)]
    pub ttft_ms: Option<u64>,
    #[serde(default)]
    pub ttft_source: Option<String>,
    pub retry_attempt_index: usize,
    pub outcome: AttemptOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientRequestMetric {
    pub request_id: String,
    pub started_epoch_secs: u64,
    pub finished_epoch_secs: u64,
    pub inbound: String,
    pub requested_model: String,
    pub selected_route: String,
    pub final_provider: String,
    pub final_model: String,
    pub http_status: Option<u16>,
    pub error_class: Option<String>,
    pub total_latency_ms: u64,
    pub attempt_count: usize,
    pub outcome: RequestOutcome,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeMetricsStore {
    attempts: Vec<UpstreamAttemptMetric>,
    requests: Vec<ClientRequestMetric>,
    max_attempts: usize,
    max_requests: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteMetricSummary {
    pub route: String,
    pub provider: String,
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
    pub average_latency_ms: Option<u64>,
    pub last_http_status: Option<u16>,
    pub last_error_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TtftMetricSummary {
    pub route: String,
    pub provider: String,
    pub model: String,
    pub window_seconds: u64,
    pub samples: u64,
    pub average_ttft_ms: Option<u64>,
    pub p50_ttft_ms: Option<u64>,
    pub p90_ttft_ms: Option<u64>,
    pub p95_ttft_ms: Option<u64>,
    pub latest_ttft_ms: Option<u64>,
    pub latest_request_id: Option<String>,
}

impl RuntimeMetricsStore {
    pub fn new(max_attempts: usize) -> Self {
        Self {
            attempts: Vec::new(),
            requests: Vec::new(),
            max_attempts,
            max_requests: max_attempts,
        }
    }

    pub fn load(max_attempts: usize) -> Self {
        let mut store = Self::new(max_attempts);
        if let Ok(attempts) = read_attempt_metrics_from_path(&runtime_metrics_path()) {
            for attempt in attempts {
                store.record_in_memory(attempt);
            }
        }
        if let Ok(requests) = read_request_metrics_from_path(&runtime_request_metrics_path()) {
            for request in requests {
                store.record_request_in_memory(request);
            }
        }
        store
    }

    pub fn record(&mut self, metric: UpstreamAttemptMetric) {
        append_attempt_metric(&metric);
        self.record_in_memory(metric);
    }

    pub fn record_request(&mut self, metric: ClientRequestMetric) {
        append_request_metric(&metric);
        self.record_request_in_memory(metric);
    }

    fn record_in_memory(&mut self, metric: UpstreamAttemptMetric) {
        self.attempts.push(metric);
        if self.max_attempts > 0 && self.attempts.len() > self.max_attempts {
            let overflow = self.attempts.len() - self.max_attempts;
            self.attempts.drain(0..overflow);
        }
    }

    fn record_request_in_memory(&mut self, metric: ClientRequestMetric) {
        self.requests.push(metric);
        if self.max_requests > 0 && self.requests.len() > self.max_requests {
            let overflow = self.requests.len() - self.max_requests;
            self.requests.drain(0..overflow);
        }
    }

    pub fn recent_attempts(&self, limit: usize) -> Vec<UpstreamAttemptMetric> {
        self.attempts
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    pub fn recent_requests(&self, limit: usize) -> Vec<ClientRequestMetric> {
        self.requests
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    pub fn summary(&self) -> Vec<RouteMetricSummary> {
        let mut groups: BTreeMap<(String, String), SummaryBuilder> = BTreeMap::new();
        for attempt in &self.attempts {
            let key = (attempt.route.clone(), attempt.provider.clone());
            groups.entry(key).or_default().record(attempt);
        }
        groups
            .into_iter()
            .map(|((route, provider), builder)| builder.build(route, provider))
            .collect()
    }

    pub fn ttft_summary(&self, window_seconds: u64, now_epoch_secs: u64) -> Vec<TtftMetricSummary> {
        let cutoff = now_epoch_secs.saturating_sub(window_seconds);
        let mut groups: BTreeMap<(String, String, String), TtftSummaryBuilder> = BTreeMap::new();
        for attempt in &self.attempts {
            if attempt.timestamp_epoch_secs < cutoff {
                continue;
            }
            let Some(ttft_ms) = attempt.ttft_ms else {
                continue;
            };
            let key = (
                attempt.route.clone(),
                attempt.provider.clone(),
                attempt.model.clone(),
            );
            groups.entry(key).or_default().record(attempt, ttft_ms);
        }
        groups
            .into_iter()
            .map(|((route, provider, model), builder)| {
                builder.build(route, provider, model, window_seconds)
            })
            .collect()
    }
}

pub fn runtime_metrics_path() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude-code-router")
        .join("runtime-metrics.jsonl")
}

pub fn runtime_request_metrics_path() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude-code-router")
        .join("runtime-request-metrics.jsonl")
}

pub fn append_attempt_metric(metric: &UpstreamAttemptMetric) {
    #[cfg(not(test))]
    {
        let _ = append_attempt_metric_to_path(&runtime_metrics_path(), metric);
    }
    #[cfg(test)]
    {
        let _ = metric;
    }
}

pub fn append_request_metric(metric: &ClientRequestMetric) {
    #[cfg(not(test))]
    {
        let _ = append_request_metric_to_path(&runtime_request_metrics_path(), metric);
    }
    #[cfg(test)]
    {
        let _ = metric;
    }
}

pub fn append_attempt_metric_to_path(
    path: &Path,
    metric: &UpstreamAttemptMetric,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let line = serde_json::to_string(metric)?;
    writeln!(file, "{line}")?;
    Ok(())
}

pub fn read_attempt_metrics_from_path(path: &Path) -> std::io::Result<Vec<UpstreamAttemptMetric>> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    Ok(content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect())
}

pub fn append_request_metric_to_path(
    path: &Path,
    metric: &ClientRequestMetric,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let line = serde_json::to_string(metric)?;
    writeln!(file, "{line}")?;
    Ok(())
}

pub fn read_request_metrics_from_path(path: &Path) -> std::io::Result<Vec<ClientRequestMetric>> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    Ok(content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect())
}

#[derive(Debug, Default)]
struct SummaryBuilder {
    attempts: u64,
    successes: u64,
    failures: u64,
    latency_sum: u64,
    latency_count: u64,
    last_http_status: Option<u16>,
    last_error_class: Option<String>,
}

#[derive(Debug, Default)]
struct TtftSummaryBuilder {
    samples: Vec<u64>,
    latest_epoch_secs: u64,
    latest_ttft_ms: Option<u64>,
    latest_request_id: Option<String>,
}

impl TtftSummaryBuilder {
    fn record(&mut self, attempt: &UpstreamAttemptMetric, ttft_ms: u64) {
        self.samples.push(ttft_ms);
        if attempt.timestamp_epoch_secs >= self.latest_epoch_secs {
            self.latest_epoch_secs = attempt.timestamp_epoch_secs;
            self.latest_ttft_ms = Some(ttft_ms);
            self.latest_request_id = Some(attempt.request_id.clone());
        }
    }

    fn build(
        mut self,
        route: String,
        provider: String,
        model: String,
        window_seconds: u64,
    ) -> TtftMetricSummary {
        self.samples.sort_unstable();
        let sample_count = self.samples.len() as u64;
        let average_ttft_ms = if self.samples.is_empty() {
            None
        } else {
            Some(self.samples.iter().sum::<u64>() / sample_count)
        };
        TtftMetricSummary {
            route,
            provider,
            model,
            window_seconds,
            samples: sample_count,
            average_ttft_ms,
            p50_ttft_ms: percentile(&self.samples, 50),
            p90_ttft_ms: percentile(&self.samples, 90),
            p95_ttft_ms: percentile(&self.samples, 95),
            latest_ttft_ms: self.latest_ttft_ms,
            latest_request_id: self.latest_request_id,
        }
    }
}

fn percentile(sorted_samples: &[u64], percentile: u64) -> Option<u64> {
    if sorted_samples.is_empty() {
        return None;
    }
    let rank = ((sorted_samples.len() as u64)
        .saturating_mul(percentile)
        .saturating_add(99)
        / 100)
        .saturating_sub(1);
    sorted_samples.get(rank as usize).copied()
}

impl SummaryBuilder {
    fn record(&mut self, attempt: &UpstreamAttemptMetric) {
        self.attempts += 1;
        match attempt.outcome {
            AttemptOutcome::Success => self.successes += 1,
            _ => self.failures += 1,
        }
        if let Some(latency_ms) = attempt.latency_ms {
            self.latency_sum = self.latency_sum.saturating_add(latency_ms);
            self.latency_count += 1;
        }
        self.last_http_status = attempt.http_status;
        self.last_error_class = attempt.error_class.clone();
    }

    fn build(self, route: String, provider: String) -> RouteMetricSummary {
        RouteMetricSummary {
            route,
            provider,
            attempts: self.attempts,
            successes: self.successes,
            failures: self.failures,
            average_latency_ms: self.latency_sum.checked_div(self.latency_count),
            last_http_status: self.last_http_status,
            last_error_class: self.last_error_class,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metric(
        route: &str,
        outcome: AttemptOutcome,
        latency_ms: Option<u64>,
    ) -> UpstreamAttemptMetric {
        UpstreamAttemptMetric {
            request_id: "request-1".into(),
            timestamp_epoch_secs: 1,
            inbound: "AnthropicMessages".into(),
            route: route.into(),
            provider: route.split(',').next().unwrap_or(route).into(),
            endpoint: "https://example.com".into(),
            model: "m".into(),
            http_status: match outcome {
                AttemptOutcome::Success => Some(200),
                AttemptOutcome::HttpError => Some(429),
                _ => None,
            },
            error_class: match outcome {
                AttemptOutcome::Success => None,
                _ => Some("error".into()),
            },
            latency_ms,
            ttft_ms: latency_ms,
            ttft_source: latency_ms.map(|_| "test".into()),
            retry_attempt_index: 0,
            outcome,
        }
    }

    fn request_metric(request_id: &str, attempt_count: usize) -> ClientRequestMetric {
        ClientRequestMetric {
            request_id: request_id.into(),
            started_epoch_secs: 1,
            finished_epoch_secs: 2,
            inbound: "AnthropicMessages".into(),
            requested_model: "claude".into(),
            selected_route: "openai,a".into(),
            final_provider: "openai".into(),
            final_model: "a".into(),
            http_status: Some(200),
            error_class: None,
            total_latency_ms: 12,
            attempt_count,
            outcome: RequestOutcome::Success,
        }
    }

    #[test]
    fn records_and_limits_recent_attempts() {
        let mut store = RuntimeMetricsStore::new(2);

        store.record(metric("a,m", AttemptOutcome::Success, Some(10)));
        store.record(metric("b,m", AttemptOutcome::HttpError, Some(20)));
        store.record(metric("c,m", AttemptOutcome::NetworkError, None));

        let attempts = store.recent_attempts(10);
        assert_eq!(attempts.len(), 2);
        assert_eq!(attempts[0].route, "b,m");
        assert_eq!(attempts[1].route, "c,m");
    }

    #[test]
    fn summarizes_by_route_and_provider() {
        let mut store = RuntimeMetricsStore::new(10);

        store.record(metric("openai,a", AttemptOutcome::Success, Some(10)));
        store.record(metric("openai,a", AttemptOutcome::HttpError, Some(30)));

        let summary = store.summary();
        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].attempts, 2);
        assert_eq!(summary[0].successes, 1);
        assert_eq!(summary[0].failures, 1);
        assert_eq!(summary[0].average_latency_ms, Some(20));
    }

    #[test]
    fn attempt_metrics_round_trip_and_skip_malformed_lines() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("metrics.jsonl");
        let first = metric("openai,a", AttemptOutcome::Success, Some(10));
        let second = metric("openai,a", AttemptOutcome::HttpError, Some(20));

        append_attempt_metric_to_path(&path, &first).unwrap();
        std::fs::write(
            &path,
            format!(
                "{}\nnot-json\n{}\n",
                serde_json::to_string(&first).unwrap(),
                serde_json::to_string(&second).unwrap()
            ),
        )
        .unwrap();

        let metrics = read_attempt_metrics_from_path(&path).unwrap();

        assert_eq!(metrics, vec![first, second]);
    }

    #[test]
    fn records_and_limits_recent_requests() {
        let mut store = RuntimeMetricsStore::new(2);

        store.record_request(request_metric("request-1", 1));
        store.record_request(request_metric("request-2", 2));
        store.record_request(request_metric("request-3", 3));

        let requests = store.recent_requests(10);
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].request_id, "request-2");
        assert_eq!(requests[1].request_id, "request-3");
    }

    #[test]
    fn request_metrics_round_trip_and_skip_malformed_lines() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("requests.jsonl");
        let first = request_metric("request-1", 1);
        let second = request_metric("request-2", 2);

        append_request_metric_to_path(&path, &first).unwrap();
        std::fs::write(
            &path,
            format!(
                "{}\nnot-json\n{}\n",
                serde_json::to_string(&first).unwrap(),
                serde_json::to_string(&second).unwrap()
            ),
        )
        .unwrap();

        let metrics = read_request_metrics_from_path(&path).unwrap();

        assert_eq!(metrics, vec![first, second]);
    }

    #[test]
    fn ttft_summary_filters_window_and_calculates_percentiles() {
        let mut store = RuntimeMetricsStore::new(10);
        let mut old = metric("openai,a", AttemptOutcome::Success, Some(50));
        old.timestamp_epoch_secs = 1;
        old.request_id = "old".into();
        let mut first = metric("openai,a", AttemptOutcome::Success, Some(100));
        first.timestamp_epoch_secs = 100;
        first.request_id = "first".into();
        first.ttft_ms = Some(100);
        let mut second = metric("openai,a", AttemptOutcome::Success, Some(200));
        second.timestamp_epoch_secs = 101;
        second.request_id = "second".into();
        second.ttft_ms = Some(200);

        store.record(old);
        store.record(first);
        store.record(second);

        let summary = store.ttft_summary(10, 105);

        assert_eq!(summary.len(), 1);
        assert_eq!(summary[0].samples, 2);
        assert_eq!(summary[0].average_ttft_ms, Some(150));
        assert_eq!(summary[0].p50_ttft_ms, Some(100));
        assert_eq!(summary[0].p90_ttft_ms, Some(200));
        assert_eq!(summary[0].p95_ttft_ms, Some(200));
        assert_eq!(summary[0].latest_ttft_ms, Some(200));
        assert_eq!(summary[0].latest_request_id.as_deref(), Some("second"));
    }
}
