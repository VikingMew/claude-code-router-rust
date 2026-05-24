use ccr_app_core::logging::{
    app_log_path, append_app_log, log_event_matches_query, parse_app_log_line, LogQuery,
};
use eframe::egui;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(1);
const LOG_CHUNK_BYTES: u64 = 256 * 1024;
const MAX_AUTO_REFRESH_LINES: usize = 2_000;
const MAX_FILTERED_LINES: usize = 500;
const MAX_DISPLAY_LINE_CHARS: usize = 4_000;

pub struct LogsTab {
    visible_lines: Vec<String>,
    visible: bool,
    last_refresh: Option<Instant>,
    last_snapshot: Option<LogFileSnapshot>,
    loaded_start: u64,
    has_more_older: bool,
    status: String,
    target_filter: String,
    event_filter: String,
    provider_filter: String,
    route_filter: String,
}

impl LogsTab {
    pub fn new() -> Self {
        Self {
            visible_lines: Vec::new(),
            visible: false,
            last_refresh: None,
            last_snapshot: None,
            loaded_start: 0,
            has_more_older: false,
            status: String::new(),
            target_filter: String::new(),
            event_filter: String::new(),
            provider_filter: String::new(),
            route_filter: String::new(),
        }
    }

    pub fn set_visible(&mut self, visible: bool) {
        if visible && !self.visible {
            append_app_log("ui", "logs_tab_focused", &[]);
            self.refresh_now();
        }
        self.visible = visible;
    }

    fn load_latest_from_path(path: &Path) -> LogLoadResult {
        let snapshot = LogFileSnapshot::from_path(path);
        let Some(snapshot) = snapshot else {
            return LogLoadResult::empty();
        };
        if snapshot.len == 0 {
            return LogLoadResult {
                lines: Vec::new(),
                loaded_start: 0,
                has_more_older: false,
                snapshot: Some(snapshot),
            };
        }

        match read_log_chunk(
            path,
            snapshot.len.saturating_sub(LOG_CHUNK_BYTES),
            snapshot.len,
        ) {
            Ok(chunk) => LogLoadResult {
                lines: chunk.lines,
                loaded_start: chunk.start,
                has_more_older: chunk.start > 0,
                snapshot: Some(snapshot),
            },
            Err(error) => {
                append_app_log("ui", "logs_load_failed", &[("error", error.to_string())]);
                LogLoadResult {
                    snapshot: Some(snapshot),
                    ..LogLoadResult::empty()
                }
            }
        }
    }

    fn load_older_from_path(path: &Path, loaded_start: u64) -> std::io::Result<LogChunk> {
        let start = loaded_start.saturating_sub(LOG_CHUNK_BYTES);
        read_log_chunk(path, start, loaded_start)
    }

    fn refresh_now(&mut self) {
        let result = Self::load_latest_from_path(&app_log_path());
        self.visible_lines = result.lines;
        self.last_snapshot = result.snapshot;
        self.loaded_start = result.loaded_start;
        self.has_more_older = result.has_more_older;
        self.status = self.status_text();
        self.last_refresh = Some(Instant::now());
    }

    fn refresh_if_due(&mut self, now: Instant) {
        if !should_auto_refresh(self.visible, self.last_refresh, now, AUTO_REFRESH_INTERVAL) {
            return;
        }

        let path = app_log_path();
        let snapshot = LogFileSnapshot::from_path(&path);
        if snapshot != self.last_snapshot {
            match (self.last_snapshot, snapshot) {
                (Some(previous), Some(current)) if current.len > previous.len => {
                    self.prepend_appended_lines(&path, previous.len, current.len);
                    self.last_snapshot = Some(current);
                    self.status = self.status_text();
                }
                _ => self.refresh_now(),
            }
        }
        self.last_refresh = Some(now);
    }

    fn prepend_appended_lines(&mut self, path: &Path, start: u64, end: u64) {
        match read_exact_log_range(path, start, end) {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                let mut lines = display_lines_from_text(&text, false);
                lines.reverse();
                if !lines.is_empty() {
                    self.visible_lines.splice(0..0, lines);
                    self.trim_auto_refresh_lines();
                }
            }
            Err(error) => {
                append_app_log(
                    "ui",
                    "logs_incremental_refresh_failed",
                    &[("error", error.to_string())],
                );
            }
        }
    }

    fn load_older(&mut self) {
        if !self.has_more_older {
            return;
        }
        match Self::load_older_from_path(&app_log_path(), self.loaded_start) {
            Ok(chunk) => {
                self.visible_lines.extend(chunk.lines);
                self.loaded_start = chunk.start;
                self.has_more_older = chunk.start > 0;
                self.status = self.status_text();
            }
            Err(error) => {
                self.status = format!("Failed to load older logs: {error}");
                append_app_log(
                    "ui",
                    "logs_load_older_failed",
                    &[("error", error.to_string())],
                );
            }
        }
    }

    fn trim_auto_refresh_lines(&mut self) {
        if self.visible_lines.len() > MAX_AUTO_REFRESH_LINES {
            self.visible_lines.truncate(MAX_AUTO_REFRESH_LINES);
            self.has_more_older = true;
        }
    }

    fn status_text(&self) -> String {
        let suffix = if self.has_more_older {
            "Load older to continue."
        } else {
            "All loaded."
        };
        format!(
            "Showing latest {} loaded lines, newest first. {suffix}",
            self.visible_lines.len()
        )
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.set_visible(true);
        self.refresh_if_due(Instant::now());

        ui.horizontal(|ui| {
            ui.heading("Logs");
            if ui.button("Clear").clicked() {
                let path = app_log_path();
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                std::fs::write(&path, "").ok();
                append_app_log("ui", "logs_clear_clicked", &[]);
                self.refresh_now();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Target");
            ui.text_edit_singleline(&mut self.target_filter);
            ui.label("Event");
            ui.text_edit_singleline(&mut self.event_filter);
            ui.label("Provider");
            ui.text_edit_singleline(&mut self.provider_filter);
            ui.label("Route");
            ui.text_edit_singleline(&mut self.route_filter);
        });
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
        ui.separator();
        let filtered = self.filtered_lines();
        egui::ScrollArea::vertical().show(ui, |ui| {
            if filtered.is_empty() {
                ui.label("No log entries match the current filters.");
            } else {
                for line in &filtered {
                    ui.monospace(line);
                }
            }
            if self.has_more_older && ui.button("Load older").clicked() {
                self.load_older();
            }
        });
        ui.ctx().request_repaint_after(AUTO_REFRESH_INTERVAL);
    }

    fn filtered_lines(&self) -> Vec<String> {
        if self.target_filter.trim().is_empty()
            && self.event_filter.trim().is_empty()
            && self.provider_filter.trim().is_empty()
            && self.route_filter.trim().is_empty()
        {
            return self.visible_lines.clone();
        }
        let query = LogQuery {
            target: non_empty_filter(&self.target_filter),
            event: non_empty_filter(&self.event_filter),
            provider: non_empty_filter(&self.provider_filter),
            route: non_empty_filter(&self.route_filter),
            limit: Some(MAX_FILTERED_LINES),
        };
        self.visible_lines
            .iter()
            .filter_map(|line| parse_app_log_line(line))
            .filter(|event| log_event_matches_query(event, &query))
            .map(|event| event.raw)
            .take(MAX_FILTERED_LINES)
            .collect()
    }
}

fn non_empty_filter(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LogLoadResult {
    lines: Vec<String>,
    loaded_start: u64,
    has_more_older: bool,
    snapshot: Option<LogFileSnapshot>,
}

impl LogLoadResult {
    fn empty() -> Self {
        Self {
            lines: Vec::new(),
            loaded_start: 0,
            has_more_older: false,
            snapshot: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LogChunk {
    lines: Vec<String>,
    start: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LogFileSnapshot {
    modified: Option<SystemTime>,
    len: u64,
}

impl LogFileSnapshot {
    fn from_path(path: &Path) -> Option<Self> {
        let metadata = std::fs::metadata(path).ok()?;
        Some(Self {
            modified: metadata.modified().ok(),
            len: metadata.len(),
        })
    }
}

fn read_log_chunk(path: &Path, requested_start: u64, end: u64) -> std::io::Result<LogChunk> {
    let bytes = read_exact_log_range(path, requested_start, end)?;
    let (text, start) = trim_partial_start(bytes, requested_start);
    let mut lines = display_lines_from_text(&text, true);
    lines.reverse();
    Ok(LogChunk { lines, start })
}

fn read_exact_log_range(path: &Path, start: u64, end: u64) -> std::io::Result<Vec<u8>> {
    if end <= start {
        return Ok(Vec::new());
    }
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = vec![0; (end - start) as usize];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn trim_partial_start(bytes: Vec<u8>, requested_start: u64) -> (String, u64) {
    if requested_start == 0 {
        return (String::from_utf8_lossy(&bytes).into_owned(), 0);
    }
    let Some(index) = bytes.iter().position(|byte| *byte == b'\n') else {
        return (String::new(), requested_start + bytes.len() as u64);
    };
    let start = requested_start + index as u64 + 1;
    (
        String::from_utf8_lossy(&bytes[index + 1..]).into_owned(),
        start,
    )
}

fn display_lines_from_text(text: &str, drop_empty_tail: bool) -> Vec<String> {
    let mut lines = text.lines().map(truncate_display_line).collect::<Vec<_>>();
    if !drop_empty_tail && text.ends_with('\n') {
        lines.retain(|line| !line.is_empty());
    }
    lines
}

fn truncate_display_line(line: &str) -> String {
    if line.chars().count() <= MAX_DISPLAY_LINE_CHARS {
        return line.to_string();
    }
    let mut truncated = line
        .chars()
        .take(MAX_DISPLAY_LINE_CHARS)
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

fn should_auto_refresh(
    visible: bool,
    last_refresh: Option<Instant>,
    now: Instant,
    interval: Duration,
) -> bool {
    visible && last_refresh.map(|last| now.duration_since(last) >= interval) != Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_test_dir(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("ccr-ui-logs-tab-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&path).ok();
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn logs_tab_starts_without_loading_file() {
        let tab = LogsTab::new();

        assert!(tab.visible_lines.is_empty());
        assert!(!tab.visible);
        assert!(tab.last_refresh.is_none());
        assert!(tab.filtered_lines().is_empty());
    }

    #[test]
    fn should_refresh_when_visible_and_never_refreshed() {
        let now = Instant::now();

        assert!(should_auto_refresh(true, None, now, AUTO_REFRESH_INTERVAL));
    }

    #[test]
    fn should_not_refresh_when_hidden() {
        let now = Instant::now();

        assert!(!should_auto_refresh(
            false,
            None,
            now,
            AUTO_REFRESH_INTERVAL
        ));
    }

    #[test]
    fn should_wait_for_refresh_interval() {
        let now = Instant::now();

        assert!(!should_auto_refresh(
            true,
            Some(now),
            now + Duration::from_millis(500),
            AUTO_REFRESH_INTERVAL,
        ));
        assert!(should_auto_refresh(
            true,
            Some(now),
            now + AUTO_REFRESH_INTERVAL,
            AUTO_REFRESH_INTERVAL,
        ));
    }

    #[test]
    fn load_missing_log_returns_empty_content() {
        let temp = temp_test_dir("missing");
        let missing = temp.join("missing.log");

        let result = LogsTab::load_latest_from_path(&missing);

        assert!(result.lines.is_empty());
        assert_eq!(result.snapshot, None);
        std::fs::remove_dir_all(temp).ok();
    }

    #[test]
    fn load_existing_log_returns_latest_lines_newest_first() {
        let temp = temp_test_dir("existing");
        let path = temp.join("app.log");
        std::fs::write(&path, "old\n这是一个什么项目？\nnew\n").unwrap();

        let result = LogsTab::load_latest_from_path(&path);

        assert_eq!(result.lines, vec!["new", "这是一个什么项目？", "old"]);
        assert!(result.snapshot.unwrap().len > 4);
        std::fs::remove_dir_all(temp).ok();
    }

    #[test]
    fn filtered_content_filters_by_target_and_event() {
        let mut tab = LogsTab::new();
        tab.visible_lines = vec![
            "2026-05-07T12:00:01+08:00 [upstream] event=\"result\"\n",
            "2026-05-07T12:00:00+08:00 [server] event=\"started\"",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        tab.target_filter = "upstream".to_string();
        tab.event_filter = "result".to_string();

        let filtered = tab.filtered_lines().join("\n");

        assert!(filtered.contains("[upstream]"));
        assert!(!filtered.contains("[server]"));
    }

    #[test]
    fn filtered_content_filters_by_provider_and_route() {
        let mut tab = LogsTab::new();
        tab.visible_lines = vec![
            "2026-05-07T12:00:02+08:00 [upstream] event=\"result\" provider=\"openai\" route=\"primary,gpt-4o\"",
            "2026-05-07T12:00:01+08:00 [upstream] event=\"result\" provider=\"anthropic\" route=\"primary,sonnet\"",
            "2026-05-07T12:00:00+08:00 [upstream] event=\"result\" provider=\"openai\" route=\"fallback,gpt-4o-mini\"",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        tab.provider_filter = "openai".to_string();
        tab.route_filter = "primary,gpt-4o".to_string();

        let filtered = tab.filtered_lines();

        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].contains("provider=\"openai\""));
        assert!(filtered[0].contains("route=\"primary,gpt-4o\""));
    }

    #[test]
    fn read_log_chunk_drops_partial_start_and_reverses_lines() {
        let temp = temp_test_dir("chunk");
        let path = temp.join("app.log");
        std::fs::write(&path, "first\nsecond\nthird\n").unwrap();

        let chunk = read_log_chunk(&path, 3, 18).unwrap();

        assert_eq!(chunk.lines, vec!["third", "second"]);
        assert_eq!(chunk.start, 6);
        std::fs::remove_dir_all(temp).ok();
    }

    #[test]
    fn load_older_appends_older_lines_after_latest_lines() {
        let temp = temp_test_dir("older");
        let path = temp.join("app.log");
        std::fs::write(&path, "oldest\nmiddle\nnewest\n").unwrap();
        let mut tab = LogsTab::new();
        tab.visible_lines = vec!["newest".to_string()];
        tab.loaded_start = "oldest\nmiddle\n".len() as u64;
        tab.has_more_older = true;

        let chunk = LogsTab::load_older_from_path(&path, tab.loaded_start).unwrap();
        tab.visible_lines.extend(chunk.lines);

        assert_eq!(tab.visible_lines, vec!["newest", "middle", "oldest"]);
        std::fs::remove_dir_all(temp).ok();
    }

    #[test]
    fn prepend_appended_lines_inserts_newest_lines_at_top() {
        let temp = temp_test_dir("append");
        let path = temp.join("app.log");
        std::fs::write(&path, "old\n").unwrap();
        let old_len = std::fs::metadata(&path).unwrap().len();
        std::fs::write(&path, "old\nnew1\nnew2\n").unwrap();
        let new_len = std::fs::metadata(&path).unwrap().len();
        let mut tab = LogsTab::new();
        tab.visible_lines = vec!["old".to_string()];

        tab.prepend_appended_lines(&path, old_len, new_len);

        assert_eq!(tab.visible_lines, vec!["new2", "new1", "old"]);
        std::fs::remove_dir_all(temp).ok();
    }

    #[test]
    fn truncate_display_line_limits_long_lines() {
        let line = "a".repeat(MAX_DISPLAY_LINE_CHARS + 100);

        let truncated = truncate_display_line(&line);

        assert!(truncated.ends_with("..."));
        assert_eq!(truncated.chars().count(), MAX_DISPLAY_LINE_CHARS + 3);
    }
}
