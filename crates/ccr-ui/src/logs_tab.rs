use ccr_app_core::logging::{app_log_path, append_app_log};
use eframe::egui;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

const AUTO_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

pub struct LogsTab {
    content: String,
    visible: bool,
    last_refresh: Option<Instant>,
    last_snapshot: Option<LogFileSnapshot>,
}

impl LogsTab {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            visible: false,
            last_refresh: None,
            last_snapshot: None,
        }
    }

    pub fn set_visible(&mut self, visible: bool) {
        if visible && !self.visible {
            append_app_log("ui", "logs_tab_focused", &[]);
            self.refresh_now();
        }
        self.visible = visible;
    }

    fn load_from_path(path: &Path) -> (String, Option<LogFileSnapshot>) {
        let snapshot = LogFileSnapshot::from_path(path);
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => {
                append_app_log("ui", "logs_load_failed", &[("error", error.to_string())]);
                String::new()
            }
        };
        (content, snapshot)
    }

    fn refresh_now(&mut self) {
        let (content, snapshot) = Self::load_from_path(&app_log_path());
        self.content = content;
        self.last_snapshot = snapshot;
        self.last_refresh = Some(Instant::now());
    }

    fn refresh_if_due(&mut self, now: Instant) {
        if !should_auto_refresh(self.visible, self.last_refresh, now, AUTO_REFRESH_INTERVAL) {
            return;
        }

        let path = app_log_path();
        let snapshot = LogFileSnapshot::from_path(&path);
        if snapshot != self.last_snapshot {
            let (content, snapshot) = Self::load_from_path(&path);
            self.content = content;
            self.last_snapshot = snapshot;
        }
        self.last_refresh = Some(now);
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.set_visible(true);
        self.refresh_if_due(Instant::now());

        ui.horizontal(|ui| {
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
        ui.separator();
        let mut display = self.content.as_str();
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut display)
                    .desired_width(f32::INFINITY)
                    .font(egui::TextStyle::Monospace),
            );
        });
        ui.ctx().request_repaint_after(AUTO_REFRESH_INTERVAL);
    }
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

        assert_eq!(tab.content, "");
        assert!(!tab.visible);
        assert!(tab.last_refresh.is_none());
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

        let (content, snapshot) = LogsTab::load_from_path(&missing);

        assert_eq!(content, "");
        assert_eq!(snapshot, None);
        std::fs::remove_dir_all(temp).ok();
    }

    #[test]
    fn load_existing_log_returns_snapshot() {
        let temp = temp_test_dir("existing");
        let path = temp.join("app.log");
        std::fs::write(&path, "这是一个什么项目？").unwrap();

        let (content, snapshot) = LogsTab::load_from_path(&path);

        assert_eq!(content, "这是一个什么项目？");
        assert!(snapshot.unwrap().len > 4);
        std::fs::remove_dir_all(temp).ok();
    }
}
