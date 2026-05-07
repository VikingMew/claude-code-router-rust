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
pub struct UpstreamAttemptMetric {
    pub timestamp_epoch_secs: u64,
    pub inbound: String,
    pub route: String,
    pub provider: String,
    pub endpoint: String,
    pub model: String,
    pub http_status: Option<u16>,
    pub error_class: Option<String>,
    pub latency_ms: Option<u64>,
    pub retry_attempt_index: usize,
    pub outcome: AttemptOutcome,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeMetricsStore {
    attempts: Vec<UpstreamAttemptMetric>,
    max_attempts: usize,
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

impl RuntimeMetricsStore {
    pub fn new(max_attempts: usize) -> Self {
        Self {
            attempts: Vec::new(),
            max_attempts,
        }
    }

    pub fn load(max_attempts: usize) -> Self {
        let mut store = Self::new(max_attempts);
        if let Ok(attempts) = read_attempt_metrics_from_path(&runtime_metrics_path()) {
            for attempt in attempts {
                store.record_in_memory(attempt);
            }
        }
        store
    }

    pub fn record(&mut self, metric: UpstreamAttemptMetric) {
        append_attempt_metric(&metric);
        self.record_in_memory(metric);
    }

    fn record_in_memory(&mut self, metric: UpstreamAttemptMetric) {
        self.attempts.push(metric);
        if self.max_attempts > 0 && self.attempts.len() > self.max_attempts {
            let overflow = self.attempts.len() - self.max_attempts;
            self.attempts.drain(0..overflow);
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
}

pub fn runtime_metrics_path() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude-code-router")
        .join("runtime-metrics.jsonl")
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
            average_latency_ms: if self.latency_count == 0 {
                None
            } else {
                Some(self.latency_sum / self.latency_count)
            },
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
            retry_attempt_index: 0,
            outcome,
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
}
