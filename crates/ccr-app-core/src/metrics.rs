use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub const DEFAULT_MAX_METRICS_FILE_BYTES: u64 = 5 * 1024 * 1024;

static ATTEMPT_METRICS_FILE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static REQUEST_METRICS_FILE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

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
    diagnostics: RuntimeMetricsDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeMetricsDiagnostics {
    pub attempts: MetricsFileDiagnostics,
    pub requests: MetricsFileDiagnostics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricsFileDiagnostics {
    pub path: String,
    pub read_lines: u64,
    pub successful_lines: u64,
    pub malformed_lines: u64,
    pub recent_error_summary: Option<String>,
    pub file_size_bytes: Option<u64>,
    pub max_file_size_bytes: u64,
    pub retention_applied: bool,
    pub retained_lines: u64,
    pub retention_error_summary: Option<String>,
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
            diagnostics: RuntimeMetricsDiagnostics::for_paths(
                &runtime_metrics_path(),
                &runtime_request_metrics_path(),
            ),
        }
    }

    pub fn load(max_attempts: usize) -> Self {
        let mut store = Self::new(max_attempts);
        let attempts = read_attempt_metrics_with_diagnostics_from_path(&runtime_metrics_path());
        store.diagnostics.attempts = attempts.diagnostics;
        for attempt in attempts.records {
            store.record_in_memory(attempt);
        }
        let requests =
            read_request_metrics_with_diagnostics_from_path(&runtime_request_metrics_path());
        store.diagnostics.requests = requests.diagnostics;
        for request in requests.records {
            store.record_request_in_memory(request);
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

    pub fn diagnostics(&self) -> RuntimeMetricsDiagnostics {
        self.diagnostics.clone()
    }
}

impl Default for RuntimeMetricsDiagnostics {
    fn default() -> Self {
        Self::for_paths(&runtime_metrics_path(), &runtime_request_metrics_path())
    }
}

impl RuntimeMetricsDiagnostics {
    fn for_paths(attempts_path: &Path, requests_path: &Path) -> Self {
        Self {
            attempts: MetricsFileDiagnostics::new(attempts_path),
            requests: MetricsFileDiagnostics::new(requests_path),
        }
    }
}

impl MetricsFileDiagnostics {
    fn new(path: &Path) -> Self {
        Self {
            path: path.display().to_string(),
            read_lines: 0,
            successful_lines: 0,
            malformed_lines: 0,
            recent_error_summary: None,
            file_size_bytes: None,
            max_file_size_bytes: DEFAULT_MAX_METRICS_FILE_BYTES,
            retention_applied: false,
            retained_lines: 0,
            retention_error_summary: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricsReadResult<T> {
    pub records: Vec<T>,
    pub diagnostics: MetricsFileDiagnostics,
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
        let path = runtime_metrics_path();
        let append_result = with_attempt_metrics_file_lock(|| {
            append_attempt_metric_line_to_path(&path, metric).and_then(|_| {
                metrics_file_exceeds_retention_limit(&path, DEFAULT_MAX_METRICS_FILE_BYTES)
            })
        });
        if append_result.unwrap_or(false) {
            let _ = std::thread::Builder::new()
                .name("ccr-attempt-metrics-retention".into())
                .spawn(move || {
                    let _ = with_attempt_metrics_file_lock(|| {
                        compact_attempt_metrics_if_needed(&path, DEFAULT_MAX_METRICS_FILE_BYTES)
                    });
                });
        }
    }
    #[cfg(test)]
    {
        let _ = metric;
    }
}

pub fn append_request_metric(metric: &ClientRequestMetric) {
    #[cfg(not(test))]
    {
        let path = runtime_request_metrics_path();
        let append_result = with_request_metrics_file_lock(|| {
            append_request_metric_line_to_path(&path, metric).and_then(|_| {
                metrics_file_exceeds_retention_limit(&path, DEFAULT_MAX_METRICS_FILE_BYTES)
            })
        });
        if append_result.unwrap_or(false) {
            let _ = std::thread::Builder::new()
                .name("ccr-request-metrics-retention".into())
                .spawn(move || {
                    let _ = with_request_metrics_file_lock(|| {
                        compact_request_metrics_if_needed(&path, DEFAULT_MAX_METRICS_FILE_BYTES)
                    });
                });
        }
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
    append_attempt_metric_to_path_with_retention(path, metric, DEFAULT_MAX_METRICS_FILE_BYTES)
}

pub fn append_attempt_metric_to_path_with_retention(
    path: &Path,
    metric: &UpstreamAttemptMetric,
    max_file_size_bytes: u64,
) -> std::io::Result<()> {
    with_attempt_metrics_file_lock(|| {
        append_attempt_metric_line_to_path(path, metric)?;
        let _ = compact_attempt_metrics_if_needed(path, max_file_size_bytes);
        Ok(())
    })
}

fn append_attempt_metric_line_to_path(
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
    let result = read_attempt_metrics_with_diagnostics_from_path(path);
    Ok(result.records)
}

pub fn read_attempt_metrics_with_diagnostics_from_path(
    path: &Path,
) -> MetricsReadResult<UpstreamAttemptMetric> {
    read_metrics_with_diagnostics_from_path(path, DEFAULT_MAX_METRICS_FILE_BYTES, |line| {
        serde_json::from_str::<UpstreamAttemptMetric>(line)
    })
}

pub fn read_attempt_metrics_with_retention_from_path(
    path: &Path,
    max_file_size_bytes: u64,
) -> MetricsReadResult<UpstreamAttemptMetric> {
    read_metrics_with_diagnostics_from_path(path, max_file_size_bytes, |line| {
        serde_json::from_str::<UpstreamAttemptMetric>(line)
    })
}

fn compact_attempt_metrics_if_needed(path: &Path, max_file_size_bytes: u64) -> std::io::Result<()> {
    compact_metrics_if_needed(path, max_file_size_bytes, |line| {
        serde_json::from_str::<UpstreamAttemptMetric>(line)
    })
}

fn with_attempt_metrics_file_lock<T>(operation: impl FnOnce() -> T) -> T {
    let _guard = ATTEMPT_METRICS_FILE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    operation()
}

fn read_metrics_with_diagnostics_from_path<T, F>(
    path: &Path,
    max_file_size_bytes: u64,
    parse: F,
) -> MetricsReadResult<T>
where
    T: Serialize,
    F: Fn(&str) -> serde_json::Result<T>,
{
    let mut diagnostics = MetricsFileDiagnostics::new(path);
    diagnostics.max_file_size_bytes = max_file_size_bytes;
    diagnostics.file_size_bytes = std::fs::metadata(path).ok().map(|metadata| metadata.len());
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return MetricsReadResult {
                records: Vec::new(),
                diagnostics,
            };
        }
        Err(error) => {
            diagnostics.recent_error_summary = Some(error.to_string());
            return MetricsReadResult {
                records: Vec::new(),
                diagnostics,
            };
        }
    };
    let mut records = Vec::new();
    for (index, line) in content.lines().enumerate() {
        diagnostics.read_lines += 1;
        match parse(line) {
            Ok(record) => {
                diagnostics.successful_lines += 1;
                records.push(record);
            }
            Err(error) => {
                diagnostics.malformed_lines += 1;
                diagnostics.recent_error_summary = Some(format!("line {}: {}", index + 1, error));
            }
        }
    }
    diagnostics.retained_lines = diagnostics.successful_lines;
    if diagnostics
        .file_size_bytes
        .is_some_and(|size| size > max_file_size_bytes)
    {
        diagnostics.retention_applied = true;
        match rewrite_records_within_size(path, &records, max_file_size_bytes) {
            Ok(retained_lines) => {
                diagnostics.retained_lines = retained_lines;
                diagnostics.file_size_bytes =
                    std::fs::metadata(path).ok().map(|metadata| metadata.len());
            }
            Err(error) => {
                diagnostics.retention_error_summary = Some(error.to_string());
            }
        }
    }
    MetricsReadResult {
        records,
        diagnostics,
    }
}

fn compact_metrics_if_needed<T, F>(
    path: &Path,
    max_file_size_bytes: u64,
    parse: F,
) -> std::io::Result<()>
where
    T: Serialize,
    F: Fn(&str) -> serde_json::Result<T>,
{
    let size = match std::fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if size <= max_file_size_bytes {
        return Ok(());
    }
    let _ = read_metrics_with_diagnostics_from_path(path, max_file_size_bytes, parse);
    Ok(())
}

#[cfg(not(test))]
fn metrics_file_exceeds_retention_limit(
    path: &Path,
    max_file_size_bytes: u64,
) -> std::io::Result<bool> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.len() > max_file_size_bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn rewrite_records_within_size<T: Serialize>(
    path: &Path,
    records: &[T],
    max_file_size_bytes: u64,
) -> std::io::Result<u64> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut lines = Vec::new();
    let mut bytes = 0_u64;
    for record in records.iter().rev() {
        let line = serde_json::to_string(record)?;
        let line_bytes = line.len() as u64 + 1;
        if !lines.is_empty() && bytes.saturating_add(line_bytes) > max_file_size_bytes {
            break;
        }
        if lines.is_empty() && line_bytes > max_file_size_bytes {
            lines.push(line);
            break;
        }
        bytes += line_bytes;
        lines.push(line);
    }
    lines.reverse();
    let mut content = lines.join("\n");
    if !content.is_empty() {
        content.push('\n');
    }
    std::fs::write(path, content)?;
    Ok(lines.len() as u64)
}

pub fn append_request_metric_to_path(
    path: &Path,
    metric: &ClientRequestMetric,
) -> std::io::Result<()> {
    append_request_metric_to_path_with_retention(path, metric, DEFAULT_MAX_METRICS_FILE_BYTES)
}

pub fn append_request_metric_to_path_with_retention(
    path: &Path,
    metric: &ClientRequestMetric,
    max_file_size_bytes: u64,
) -> std::io::Result<()> {
    with_request_metrics_file_lock(|| {
        append_request_metric_line_to_path(path, metric)?;
        let _ = compact_request_metrics_if_needed(path, max_file_size_bytes);
        Ok(())
    })
}

fn append_request_metric_line_to_path(
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
    let result = read_request_metrics_with_diagnostics_from_path(path);
    Ok(result.records)
}

pub fn read_request_metrics_with_diagnostics_from_path(
    path: &Path,
) -> MetricsReadResult<ClientRequestMetric> {
    read_metrics_with_diagnostics_from_path(path, DEFAULT_MAX_METRICS_FILE_BYTES, |line| {
        serde_json::from_str::<ClientRequestMetric>(line)
    })
}

pub fn read_request_metrics_with_retention_from_path(
    path: &Path,
    max_file_size_bytes: u64,
) -> MetricsReadResult<ClientRequestMetric> {
    read_metrics_with_diagnostics_from_path(path, max_file_size_bytes, |line| {
        serde_json::from_str::<ClientRequestMetric>(line)
    })
}

fn compact_request_metrics_if_needed(path: &Path, max_file_size_bytes: u64) -> std::io::Result<()> {
    compact_metrics_if_needed(path, max_file_size_bytes, |line| {
        serde_json::from_str::<ClientRequestMetric>(line)
    })
}

fn with_request_metrics_file_lock<T>(operation: impl FnOnce() -> T) -> T {
    let _guard = REQUEST_METRICS_FILE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    operation()
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
    fn attempt_metrics_diagnostics_count_malformed_lines() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("metrics.jsonl");
        let first = metric("openai,a", AttemptOutcome::Success, Some(10));
        let second = metric("openai,b", AttemptOutcome::HttpError, Some(20));
        std::fs::write(
            &path,
            format!(
                "{}\nnot-json\n{}\n",
                serde_json::to_string(&first).unwrap(),
                serde_json::to_string(&second).unwrap()
            ),
        )
        .unwrap();

        let result = read_attempt_metrics_with_diagnostics_from_path(&path);

        assert_eq!(result.records, vec![first, second]);
        assert_eq!(result.diagnostics.read_lines, 3);
        assert_eq!(result.diagnostics.successful_lines, 2);
        assert_eq!(result.diagnostics.malformed_lines, 1);
        assert!(
            result
                .diagnostics
                .recent_error_summary
                .as_deref()
                .unwrap_or_default()
                .contains("line 2")
        );
    }

    #[test]
    fn attempt_metrics_retention_keeps_recent_valid_records() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("metrics.jsonl");
        let old = metric("old,m", AttemptOutcome::Success, Some(10));
        let recent = metric("recent,m", AttemptOutcome::Success, Some(20));
        let newest = metric("newest,m", AttemptOutcome::HttpError, Some(30));
        std::fs::write(
            &path,
            format!(
                "{}\nnot-json\n{}\n{}\n",
                serde_json::to_string(&old).unwrap(),
                serde_json::to_string(&recent).unwrap(),
                serde_json::to_string(&newest).unwrap()
            ),
        )
        .unwrap();
        let max_bytes = serde_json::to_string(&recent).unwrap().len() as u64
            + serde_json::to_string(&newest).unwrap().len() as u64
            + 2;

        let result = read_attempt_metrics_with_retention_from_path(&path, max_bytes);
        let after_retention = read_attempt_metrics_from_path(&path).unwrap();

        assert_eq!(result.records, vec![old, recent.clone(), newest.clone()]);
        assert_eq!(result.diagnostics.malformed_lines, 1);
        assert!(result.diagnostics.retention_applied);
        assert_eq!(result.diagnostics.retained_lines, 2);
        assert_eq!(after_retention, vec![recent, newest]);
    }

    #[test]
    fn attempt_metrics_append_helper_applies_retention() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("metrics.jsonl");
        let old = metric("old,m", AttemptOutcome::Success, Some(10));
        let recent = metric("recent,m", AttemptOutcome::Success, Some(20));
        let newest = metric("newest,m", AttemptOutcome::Success, Some(30));
        append_attempt_metric_to_path(&path, &old).unwrap();
        append_attempt_metric_to_path(&path, &recent).unwrap();
        let max_bytes = serde_json::to_string(&recent).unwrap().len() as u64
            + serde_json::to_string(&newest).unwrap().len() as u64
            + 2;

        append_attempt_metric_to_path_with_retention(&path, &newest, max_bytes).unwrap();
        let after_retention = read_attempt_metrics_from_path(&path).unwrap();

        assert_eq!(after_retention, vec![recent, newest]);
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
    fn request_metrics_diagnostics_count_malformed_lines() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("requests.jsonl");
        let first = request_metric("request-1", 1);
        let second = request_metric("request-2", 2);
        std::fs::write(
            &path,
            format!(
                "{}\nnot-json\n{}\n",
                serde_json::to_string(&first).unwrap(),
                serde_json::to_string(&second).unwrap()
            ),
        )
        .unwrap();

        let result = read_request_metrics_with_diagnostics_from_path(&path);

        assert_eq!(result.records, vec![first, second]);
        assert_eq!(result.diagnostics.read_lines, 3);
        assert_eq!(result.diagnostics.successful_lines, 2);
        assert_eq!(result.diagnostics.malformed_lines, 1);
    }

    #[test]
    fn request_metrics_retention_keeps_recent_valid_records() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("requests.jsonl");
        let old = request_metric("request-1", 1);
        let recent = request_metric("request-2", 2);
        let newest = request_metric("request-3", 3);
        std::fs::write(
            &path,
            format!(
                "{}\nnot-json\n{}\n{}\n",
                serde_json::to_string(&old).unwrap(),
                serde_json::to_string(&recent).unwrap(),
                serde_json::to_string(&newest).unwrap()
            ),
        )
        .unwrap();
        let max_bytes = serde_json::to_string(&recent).unwrap().len() as u64
            + serde_json::to_string(&newest).unwrap().len() as u64
            + 2;

        let result = read_request_metrics_with_retention_from_path(&path, max_bytes);
        let after_retention = read_request_metrics_from_path(&path).unwrap();

        assert_eq!(result.records, vec![old, recent.clone(), newest.clone()]);
        assert_eq!(result.diagnostics.malformed_lines, 1);
        assert!(result.diagnostics.retention_applied);
        assert_eq!(result.diagnostics.retained_lines, 2);
        assert_eq!(after_retention, vec![recent, newest]);
    }

    #[test]
    fn request_metrics_append_helper_applies_retention() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("requests.jsonl");
        let old = request_metric("request-1", 1);
        let recent = request_metric("request-2", 2);
        let newest = request_metric("request-3", 3);
        append_request_metric_to_path(&path, &old).unwrap();
        append_request_metric_to_path(&path, &recent).unwrap();
        let max_bytes = serde_json::to_string(&recent).unwrap().len() as u64
            + serde_json::to_string(&newest).unwrap().len() as u64
            + 2;

        append_request_metric_to_path_with_retention(&path, &newest, max_bytes).unwrap();
        let after_retention = read_request_metrics_from_path(&path).unwrap();

        assert_eq!(after_retention, vec![recent, newest]);
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
