use ccr_app_core::client_config::claude::{activate_ccr, deactivate_ccr};
use ccr_app_core::client_config::codex::{activate_codex_ccr, deactivate_codex_ccr};
use ccr_app_core::client_config::hermes::{activate_hermes_ccr, deactivate_hermes_ccr};
use ccr_app_core::client_config::openclaw::{activate_openclaw_ccr, deactivate_openclaw_ccr};
use ccr_app_core::client_config::opencode::{activate_opencode_ccr, deactivate_opencode_ccr};
use ccr_app_core::status::{
    ccr_server_executable_from, check_health, fetch_runtime_status_snapshot, read_status_snapshot,
    start_server, stop_server, AdditiveClientSnapshot, InjectionSnapshot, RoutePoolConfigSnapshot,
    RuntimeStatusSnapshot, ServerSnapshot, StatusSnapshot,
};
use ccr_app_core::{
    metrics::{RouteMetricSummary, TtftMetricSummary},
    runtime_status::RoutePoolStatusResponse,
};
use eframe::egui;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

const ROUTE_POOL_STATUS_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
enum OperationTarget {
    Server,
    Claude,
    Codex,
    OpenCode,
    OpenClaw,
    Hermes,
}

#[derive(Debug, Clone)]
enum StatusTaskResult {
    Snapshot {
        snapshot: StatusSnapshot,
        health_status: Option<String>,
    },
    Runtime(RuntimeStatusSnapshot),
    Health(String),
    Operation {
        target: OperationTarget,
        message: String,
        snapshot: StatusSnapshot,
    },
}

pub struct StatusTab {
    snapshot: Option<StatusSnapshot>,
    snapshot_refreshing: bool,
    task_tx: Sender<StatusTaskResult>,
    task_rx: Receiver<StatusTaskResult>,
    health_status: String,
    health_checking: bool,
    claude_activation_status: String,
    claude_activation_checking: bool,
    codex_activation_status: String,
    codex_activation_checking: bool,
    opencode_status: String,
    opencode_checking: bool,
    openclaw_status: String,
    openclaw_checking: bool,
    hermes_status: String,
    hermes_checking: bool,
    server_operation_status: String,
    server_operation_checking: bool,
    route_pool_status: Option<Result<RoutePoolStatusResponse, String>>,
    runtime_metrics_summary: Option<Result<Vec<RouteMetricSummary>, String>>,
    ttft_metrics_summary: Option<Result<Vec<TtftMetricSummary>, String>>,
    route_pool_last_refresh: Option<Instant>,
    route_pool_refreshing: bool,
    status_last_refresh: Option<Instant>,
}

impl StatusTab {
    pub fn new() -> Self {
        let (task_tx, task_rx) = mpsc::channel();
        let mut tab = Self {
            snapshot: None,
            snapshot_refreshing: false,
            task_tx,
            task_rx,
            health_status: String::new(),
            health_checking: false,
            claude_activation_status: String::new(),
            claude_activation_checking: false,
            codex_activation_status: String::new(),
            codex_activation_checking: false,
            opencode_status: String::new(),
            opencode_checking: false,
            openclaw_status: String::new(),
            openclaw_checking: false,
            hermes_status: String::new(),
            hermes_checking: false,
            server_operation_status: String::new(),
            server_operation_checking: false,
            route_pool_status: None,
            runtime_metrics_summary: None,
            ttft_metrics_summary: None,
            route_pool_last_refresh: None,
            route_pool_refreshing: false,
            status_last_refresh: None,
        };
        tab.refresh_snapshot();
        tab
    }

    pub fn start_server_on_launch(&mut self) {
        if self.server_operation_checking {
            return;
        }
        self.server_operation_checking = true;
        let exe_path = ccr_server_executable();
        self.spawn_task(move || {
            let snapshot = read_status_snapshot();
            let message = if !snapshot.server_auto_start {
                String::new()
            } else if matches!(snapshot.server, ServerSnapshot::Running { .. }) {
                String::new()
            } else {
                match exe_path {
                    Some(exe_path) => match start_server(&exe_path) {
                        Ok(operation) => operation.ui_message(),
                        Err(e) => format!("✗ Failed to start server: {}", e),
                    },
                    None => "✗ Cannot find ccr-server executable".to_string(),
                }
            };
            StatusTaskResult::Operation {
                target: OperationTarget::Server,
                message,
                snapshot: read_status_snapshot(),
            }
        });
    }

    fn refresh_snapshot(&mut self) {
        if self.snapshot_refreshing {
            return;
        }
        self.snapshot_refreshing = true;
        self.spawn_task(|| StatusTaskResult::Snapshot {
            snapshot: read_status_snapshot(),
            health_status: None,
        });
    }

    fn refresh_status(&mut self) {
        if self.snapshot_refreshing {
            return;
        }
        self.snapshot_refreshing = true;
        self.route_pool_last_refresh = None;
        self.spawn_task(|| {
            let snapshot = read_status_snapshot();
            let health_status = snapshot
                .server
                .is_running()
                .then(|| check_health(snapshot.server.port()).ui_message());
            StatusTaskResult::Snapshot {
                snapshot,
                health_status,
            }
        });
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.poll_task_results(ui.ctx());

        ui.horizontal(|ui| {
            ui.heading("Status");
            if ui
                .add_enabled(
                    !self.snapshot_refreshing,
                    egui::Button::new("Refresh Status"),
                )
                .clicked()
            {
                self.refresh_status();
            }
        });

        if self.snapshot_refreshing {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Refreshing status...");
            });
        } else if let Some(last_refresh) = self.status_last_refresh {
            ui.label(format!(
                "Last status refresh: {:.0}s ago",
                last_refresh.elapsed().as_secs()
            ));
        }

        if self.snapshot.is_none() {
            ui.add_space(8.0);
            ui.label("Status snapshot is loading.");
            self.request_repaint_if_busy(ui.ctx());
            return;
        }

        ui.add_space(8.0);
        self.show_server(ui);
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        self.show_routing(ui);
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        ui.heading("Client Injection");
        self.show_claude(ui);
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        self.show_codex(ui);
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        self.show_opencode(ui);
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        self.show_openclaw(ui);
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(8.0);
        self.show_hermes(ui);
        self.request_repaint_if_busy(ui.ctx());
    }

    fn snapshot(&self) -> &StatusSnapshot {
        self.snapshot
            .as_ref()
            .expect("status snapshot checked before rendering sections")
    }

    fn spawn_task<F>(&self, task: F)
    where
        F: FnOnce() -> StatusTaskResult + Send + 'static,
    {
        let tx = self.task_tx.clone();
        thread::spawn(move || {
            let _ = tx.send(task());
        });
    }

    fn poll_task_results(&mut self, ctx: &egui::Context) {
        while let Ok(result) = self.task_rx.try_recv() {
            match result {
                StatusTaskResult::Snapshot {
                    snapshot,
                    health_status,
                } => {
                    self.snapshot = Some(snapshot);
                    self.snapshot_refreshing = false;
                    self.status_last_refresh = Some(Instant::now());
                    if let Some(health_status) = health_status {
                        self.health_status = health_status;
                        self.health_checking = false;
                    }
                }
                StatusTaskResult::Runtime(snapshot) => {
                    self.route_pool_status = Some(snapshot.route_pool_status);
                    self.runtime_metrics_summary = Some(snapshot.runtime_metrics_summary);
                    self.ttft_metrics_summary = Some(snapshot.ttft_metrics_summary);
                    self.route_pool_refreshing = false;
                }
                StatusTaskResult::Health(message) => {
                    self.health_status = message;
                    self.health_checking = false;
                }
                StatusTaskResult::Operation {
                    target,
                    message,
                    snapshot,
                } => {
                    self.snapshot = Some(snapshot);
                    self.status_last_refresh = Some(Instant::now());
                    match target {
                        OperationTarget::Server => {
                            self.server_operation_status = message;
                            self.server_operation_checking = false;
                            self.route_pool_last_refresh = None;
                        }
                        OperationTarget::Claude => {
                            self.claude_activation_status = message;
                            self.claude_activation_checking = false;
                        }
                        OperationTarget::Codex => {
                            self.codex_activation_status = message;
                            self.codex_activation_checking = false;
                        }
                        OperationTarget::OpenCode => {
                            self.opencode_status = message;
                            self.opencode_checking = false;
                        }
                        OperationTarget::OpenClaw => {
                            self.openclaw_status = message;
                            self.openclaw_checking = false;
                        }
                        OperationTarget::Hermes => {
                            self.hermes_status = message;
                            self.hermes_checking = false;
                        }
                    }
                }
            }
            ctx.request_repaint();
        }
    }

    fn request_repaint_if_busy(&self, ctx: &egui::Context) {
        if self.snapshot_refreshing
            || self.health_checking
            || self.claude_activation_checking
            || self.codex_activation_checking
            || self.opencode_checking
            || self.openclaw_checking
            || self.hermes_checking
            || self.server_operation_checking
            || self.route_pool_refreshing
        {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    fn show_server(&mut self, ui: &mut egui::Ui) {
        ui.heading("Server");

        let server = self.snapshot().server.clone();
        match &server {
            ServerSnapshot::Running { pid, port } => {
                ui.label(format!("Process: Running (PID {})", pid));
                ui.label(format!("Address: http://127.0.0.1:{}", port));
            }
            ServerSnapshot::Stopped { .. } => {
                ui.label("Process: Stopped");
            }
            ServerSnapshot::StalePid { pid, .. } => {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!("Process: Stopped (stale PID {})", pid),
                );
            }
        }

        if server.is_running() {
            if self.health_status.is_empty() {
                ui.label("HTTP Health: Not checked");
            } else {
                ui.label(format!("HTTP Health: {}", self.health_status));
            }
        } else {
            ui.label("HTTP Health: Unavailable while server is stopped");
        }

        ui.add_space(4.0);

        let server_running = server.is_running();
        let port = server.port();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !server_running && !self.server_operation_checking,
                    egui::Button::new("Start Server"),
                )
                .clicked()
            {
                self.start_server();
            }

            if ui
                .add_enabled(
                    server_running && !self.server_operation_checking,
                    egui::Button::new("Stop Server"),
                )
                .clicked()
            {
                self.stop_server();
            }

            if ui
                .add_enabled(
                    server_running && !self.health_checking,
                    egui::Button::new("Check Health"),
                )
                .clicked()
            {
                self.check_health(port);
            }
        });

        if self.server_operation_checking {
            ui.spinner();
        }
        if self.health_checking {
            ui.spinner();
        }

        if !self.server_operation_status.is_empty() {
            ui.label(format!("Last operation: {}", self.server_operation_status));
        }
    }

    fn show_routing(&mut self, ui: &mut egui::Ui) {
        ui.heading("Routing");

        match self.snapshot().route_pool.clone() {
            RoutePoolConfigSnapshot::Enabled {
                active_routes,
                failure_threshold,
                ban_seconds,
            } => {
                ui.label(format!(
                    "Route Pool: Enabled ({active_routes} active routes, {failure_threshold} failures, {ban_seconds}s ban)"
                ));
            }
            RoutePoolConfigSnapshot::EnabledEmpty {
                configured_routes,
                active_routes,
            } => {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!(
                        "Route Pool: Enabled but empty ({configured_routes} configured, {active_routes} active)"
                    ),
                );
                ui.label("Runtime: Not applicable until Route Pool has active routes");
                self.route_pool_status = None;
                self.route_pool_last_refresh = None;
                return;
            }
            RoutePoolConfigSnapshot::Disabled { configured_routes } => {
                ui.label(format!(
                    "Route Pool: Disabled ({configured_routes} configured routes)"
                ));
                ui.label("Runtime: Not applicable while Route Pool is disabled");
                self.route_pool_status = None;
                self.route_pool_last_refresh = None;
                return;
            }
            RoutePoolConfigSnapshot::NotConfigured => {
                ui.label("Route Pool: Not configured");
                ui.label("Runtime: Not applicable");
                self.route_pool_status = None;
                self.route_pool_last_refresh = None;
                return;
            }
        }

        let server = self.snapshot().server.clone();
        if server.is_running() {
            let api_key = self.snapshot().api_key.clone();
            self.refresh_route_pool_status_if_due(server.port(), api_key.as_deref());
            if self.route_pool_refreshing {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Refreshing route runtime...");
                });
            }
            match &self.route_pool_status {
                Some(Ok(status)) => show_route_pool_runtime(ui, status),
                Some(Err(error)) => {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("Route Pool runtime unavailable: {error}"),
                    );
                }
                None => {
                    ui.label("Route Pool runtime: Loading.");
                }
            }
            match &self.runtime_metrics_summary {
                Some(Ok(summary)) => show_runtime_metrics_summary(ui, summary),
                Some(Err(error)) => {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("Runtime metrics unavailable: {error}"),
                    );
                }
                None => {
                    ui.label("Real traffic metrics: Loading.");
                }
            }
            match &self.ttft_metrics_summary {
                Some(Ok(summary)) => show_ttft_metrics_summary(ui, summary),
                Some(Err(error)) => {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("TTFT metrics unavailable: {error}"),
                    );
                }
                None => {
                    ui.label("TTFT metrics from real client traffic: Loading.");
                }
            }
        } else {
            self.route_pool_status = None;
            self.runtime_metrics_summary = None;
            self.ttft_metrics_summary = None;
            self.route_pool_last_refresh = None;
            ui.label("Route Pool runtime: Unavailable while server is stopped.");
        }
    }

    fn refresh_route_pool_status_if_due(&mut self, port: u16, api_key: Option<&str>) {
        let now = Instant::now();
        let should_refresh = self
            .route_pool_last_refresh
            .map(|last| now.duration_since(last) >= ROUTE_POOL_STATUS_REFRESH_INTERVAL)
            .unwrap_or(true);
        if !should_refresh || self.route_pool_refreshing {
            return;
        }

        let api_key = normalized_api_key(api_key).map(str::to_string);
        self.route_pool_refreshing = true;
        self.route_pool_last_refresh = Some(now);
        self.spawn_task(move || {
            StatusTaskResult::Runtime(fetch_runtime_status_snapshot(port, api_key.as_deref()))
        });
    }

    fn show_claude(&mut self, ui: &mut egui::Ui) {
        ui.heading("Claude Config Switch");

        let snapshot = self.snapshot().clone();
        show_injection_snapshot(ui, &snapshot.claude);

        ui.add_space(4.0);

        let server_running = snapshot.server.is_running();
        let activated = snapshot.claude.is_activated();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    server_running && !activated && !self.claude_activation_checking,
                    egui::Button::new("Activate CCR for Claude Code"),
                )
                .clicked()
            {
                self.activate_claude_config();
            }

            if ui
                .add_enabled(
                    activated && !self.claude_activation_checking,
                    egui::Button::new("Deactivate CCR for Claude Code"),
                )
                .clicked()
            {
                self.deactivate_claude_config();
            }
        });

        if !server_running && !activated {
            ui.label("Start the server before activating CCR for Claude Code.");
        }

        if self.claude_activation_checking {
            ui.spinner();
        }

        show_status_message(ui, &self.claude_activation_status);
    }

    fn show_codex(&mut self, ui: &mut egui::Ui) {
        ui.heading("Codex Config Switch");

        let snapshot = self.snapshot().clone();
        show_injection_snapshot(ui, &snapshot.codex);

        ui.add_space(4.0);

        let server_running = snapshot.server.is_running();
        let activated = snapshot.codex.is_activated();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    server_running && !activated && !self.codex_activation_checking,
                    egui::Button::new("Activate CCR for Codex"),
                )
                .clicked()
            {
                self.activate_codex_config();
            }

            if ui
                .add_enabled(
                    activated && !self.codex_activation_checking,
                    egui::Button::new("Deactivate CCR for Codex"),
                )
                .clicked()
            {
                self.deactivate_codex_config();
            }
        });

        if !server_running && !activated {
            ui.label("Start the server before activating CCR for Codex.");
        }

        if self.codex_activation_checking {
            ui.spinner();
        }

        show_status_message(ui, &self.codex_activation_status);
    }

    fn show_opencode(&mut self, ui: &mut egui::Ui) {
        ui.heading("OpenCode Config");
        let snapshot = self.snapshot().clone();
        show_additive_client_snapshot(ui, &snapshot.opencode);

        let server_running = snapshot.server.is_running();
        let provider_present = snapshot.opencode.provider_present();
        let current = snapshot.opencode.is_current();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    server_running && !current && !self.opencode_checking,
                    egui::Button::new("Add CCR provider to OpenCode"),
                )
                .clicked()
            {
                self.activate_opencode_config();
            }
            if ui
                .add_enabled(
                    provider_present && !self.opencode_checking,
                    egui::Button::new("Remove CCR provider from OpenCode"),
                )
                .clicked()
            {
                self.deactivate_opencode_config();
            }
        });

        if !server_running && !provider_present {
            ui.label("Start the server before adding CCR provider to OpenCode.");
        }
        if self.opencode_checking {
            ui.spinner();
        }
        show_status_message(ui, &self.opencode_status);
    }

    fn show_openclaw(&mut self, ui: &mut egui::Ui) {
        ui.heading("OpenClaw Config");
        let snapshot = self.snapshot().clone();
        show_additive_client_snapshot(ui, &snapshot.openclaw);

        let server_running = snapshot.server.is_running();
        let provider_present = snapshot.openclaw.provider_present();
        let current = snapshot.openclaw.is_current();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    server_running && !current && !self.openclaw_checking,
                    egui::Button::new("Add CCR provider to OpenClaw"),
                )
                .clicked()
            {
                self.activate_openclaw_config();
            }
            if ui
                .add_enabled(
                    provider_present && !self.openclaw_checking,
                    egui::Button::new("Remove CCR provider from OpenClaw"),
                )
                .clicked()
            {
                self.deactivate_openclaw_config();
            }
        });

        if !server_running && !provider_present {
            ui.label("Start the server before adding CCR provider to OpenClaw.");
        }
        if self.openclaw_checking {
            ui.spinner();
        }
        show_status_message(ui, &self.openclaw_status);
    }

    fn show_hermes(&mut self, ui: &mut egui::Ui) {
        ui.heading("Hermes Config");
        let snapshot = self.snapshot().clone();
        show_additive_client_snapshot(ui, &snapshot.hermes);

        let server_running = snapshot.server.is_running();
        let provider_present = snapshot.hermes.provider_present();
        let current = snapshot.hermes.is_current();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    server_running && !current && !self.hermes_checking,
                    egui::Button::new("Add CCR provider to Hermes"),
                )
                .clicked()
            {
                self.activate_hermes_config();
            }
            if ui
                .add_enabled(
                    provider_present && !self.hermes_checking,
                    egui::Button::new("Remove CCR provider from Hermes"),
                )
                .clicked()
            {
                self.deactivate_hermes_config();
            }
        });

        if !server_running && !provider_present {
            ui.label("Start the server before adding CCR provider to Hermes.");
        }
        if self.hermes_checking {
            ui.spinner();
        }
        show_status_message(ui, &self.hermes_status);
    }
}

impl Default for StatusTab {
    fn default() -> Self {
        Self::new()
    }
}

fn show_injection_snapshot(ui: &mut egui::Ui, snapshot: &InjectionSnapshot) {
    match snapshot {
        InjectionSnapshot::ActiveAndCurrent {
            backup_path,
            backup_time,
        } => {
            ui.colored_label(egui::Color32::GREEN, "✓ Using CCR router");

            if let Some(time) = backup_time {
                ui.label(format!("Backup: {} ({})", backup_path, time));
            } else {
                ui.label(format!("Backup: {}", backup_path));
            }
        }
        InjectionSnapshot::ActiveButDrifted {
            backup_path,
            backup_time,
        } => {
            ui.colored_label(
                egui::Color32::YELLOW,
                "⚠ Backup exists, but current config no longer points to CCR",
            );
            if let Some(time) = backup_time {
                ui.label(format!("Backup: {} ({})", backup_path, time));
            } else {
                ui.label(format!("Backup: {}", backup_path));
            }
        }
        InjectionSnapshot::InjectedNoBackup => {
            ui.colored_label(
                egui::Color32::YELLOW,
                "⚠ Current config points to CCR, but no backup was found",
            );
        }
        InjectionSnapshot::Inactive => {
            ui.label("Using original configuration");
        }
    }
}

fn show_additive_client_snapshot(ui: &mut egui::Ui, snapshot: &AdditiveClientSnapshot) {
    match snapshot {
        AdditiveClientSnapshot::ProviderCurrent { path } => {
            ui.colored_label(egui::Color32::GREEN, "✓ CCR provider is configured");
            ui.label(format!("Config path: {path}"));
        }
        AdditiveClientSnapshot::ProviderDrifted { path } => {
            ui.colored_label(
                egui::Color32::YELLOW,
                "⚠ CCR provider exists, but endpoint does not match current server port",
            );
            ui.label(format!("Config path: {path}"));
        }
        AdditiveClientSnapshot::Missing { path } => {
            ui.label("CCR provider is not configured");
            ui.label(format!("Config path: {path}"));
        }
    }
}

fn show_status_message(ui: &mut egui::Ui, message: &str) {
    if message.is_empty() {
        return;
    }

    if message.starts_with("✓") {
        ui.colored_label(egui::Color32::GREEN, message);
    } else {
        ui.colored_label(egui::Color32::RED, message);
    }
}

fn normalized_api_key(api_key: Option<&str>) -> Option<&str> {
    api_key.map(str::trim).filter(|key| !key.is_empty())
}

fn show_route_pool_runtime(ui: &mut egui::Ui, status: &RoutePoolStatusResponse) {
    ui.label(format!(
        "Route Pool runtime: enabled={}, policy {} failures, {}s ban",
        status.enabled, status.failure_threshold, status.ban_seconds
    ));
    if status.routes.is_empty() {
        ui.label("No runtime failures recorded.");
        return;
    }

    let mut routes = status.routes.iter().collect::<Vec<_>>();
    routes.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (route, state) in routes {
        ui.horizontal_wrapped(|ui| {
            let banned = state.banned_until_epoch_secs.is_some();
            if banned {
                ui.colored_label(egui::Color32::YELLOW, "Banned");
            } else {
                ui.label("Ready");
            }
            ui.label(route);
            ui.label(format!("Failures: {}", state.consecutive_failures));
            if let Some(until) = state.banned_until_epoch_secs {
                ui.label(format!("Until: {until}"));
            }
            if let Some(last_success) = state.last_success_epoch_secs {
                ui.label(format!("Last success: {last_success}"));
            }
            if let Some(last_failure) = state.last_failure_epoch_secs {
                ui.label(format!("Last failure: {last_failure}"));
            }
            if let Some(error) = &state.last_error {
                ui.label(format!("Error: {error}"));
            }
        });
    }
}

fn show_runtime_metrics_summary(ui: &mut egui::Ui, summary: &[RouteMetricSummary]) {
    ui.label("Runtime metrics from real client traffic:");
    if summary.is_empty() {
        ui.label("No real upstream attempts recorded.");
        return;
    }
    let mut rows = summary.iter().collect::<Vec<_>>();
    rows.sort_by(|left, right| left.route.cmp(&right.route));
    for item in rows {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{} ({})", item.route, item.provider));
            ui.label(format!("Attempts: {}", item.attempts));
            ui.label(format!("Success: {}", item.successes));
            ui.label(format!("Failures: {}", item.failures));
            if let Some(latency) = item.average_latency_ms {
                ui.label(format!("Avg: {latency}ms"));
            }
            if let Some(status) = item.last_http_status {
                ui.label(format!("Last HTTP: {status}"));
            }
            if let Some(error_class) = &item.last_error_class {
                ui.label(format!("Last error: {error_class}"));
            }
        });
    }
}

fn show_ttft_metrics_summary(ui: &mut egui::Ui, summary: &[TtftMetricSummary]) {
    ui.label("TTFT from real client traffic (5m window):");
    if summary.is_empty() {
        ui.label("No real TTFT samples recorded.");
        return;
    }
    let mut rows = summary.iter().collect::<Vec<_>>();
    rows.sort_by(|left, right| left.route.cmp(&right.route));
    for item in rows {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "{} / {} ({})",
                item.route, item.model, item.provider
            ));
            ui.label(format!("Samples: {}", item.samples));
            if let Some(avg) = item.average_ttft_ms {
                ui.label(format!("Avg: {avg}ms"));
            }
            if let Some(p50) = item.p50_ttft_ms {
                ui.label(format!("P50: {p50}ms"));
            }
            if let Some(p90) = item.p90_ttft_ms {
                ui.label(format!("P90: {p90}ms"));
            }
            if let Some(p95) = item.p95_ttft_ms {
                ui.label(format!("P95: {p95}ms"));
            }
            if let Some(latest) = item.latest_ttft_ms {
                ui.label(format!("Latest: {latest}ms"));
            }
        });
    }
}

impl StatusTab {
    fn check_health(&mut self, port: u16) {
        if self.health_checking {
            return;
        }
        self.health_checking = true;
        self.health_status.clear();
        self.spawn_task(move || StatusTaskResult::Health(check_health(port).ui_message()));
    }

    fn activate_claude_config(&mut self) {
        if self.claude_activation_checking {
            return;
        }
        self.claude_activation_checking = true;
        self.claude_activation_status.clear();
        self.spawn_task(|| {
            let message = match activate_ccr() {
                Ok(_) => "✓ Activated! Claude Code is now using CCR router".to_string(),
                Err(e) => format!("✗ Activation failed: {}", e),
            };
            StatusTaskResult::Operation {
                target: OperationTarget::Claude,
                message,
                snapshot: read_status_snapshot(),
            }
        });
    }

    fn deactivate_claude_config(&mut self) {
        if self.claude_activation_checking {
            return;
        }
        self.claude_activation_checking = true;
        self.claude_activation_status.clear();
        self.spawn_task(|| {
            let message = match deactivate_ccr() {
                Ok(_) => {
                    "✓ Deactivated! Claude Code is now using original configuration".to_string()
                }
                Err(e) => format!("✗ Deactivation failed: {}", e),
            };
            StatusTaskResult::Operation {
                target: OperationTarget::Claude,
                message,
                snapshot: read_status_snapshot(),
            }
        });
    }

    fn activate_codex_config(&mut self) {
        if self.codex_activation_checking {
            return;
        }
        self.codex_activation_checking = true;
        self.codex_activation_status.clear();
        self.spawn_task(|| {
            let message = match activate_codex_ccr() {
                Ok(_) => "✓ Activated! Codex is now using CCR router".to_string(),
                Err(e) => format!("✗ Activation failed: {}", e),
            };
            StatusTaskResult::Operation {
                target: OperationTarget::Codex,
                message,
                snapshot: read_status_snapshot(),
            }
        });
    }

    fn deactivate_codex_config(&mut self) {
        if self.codex_activation_checking {
            return;
        }
        self.codex_activation_checking = true;
        self.codex_activation_status.clear();
        self.spawn_task(|| {
            let message = match deactivate_codex_ccr() {
                Ok(_) => "✓ Deactivated! Codex is now using original configuration".to_string(),
                Err(e) => format!("✗ Deactivation failed: {}", e),
            };
            StatusTaskResult::Operation {
                target: OperationTarget::Codex,
                message,
                snapshot: read_status_snapshot(),
            }
        });
    }

    fn activate_opencode_config(&mut self) {
        if self.opencode_checking {
            return;
        }
        self.opencode_checking = true;
        self.opencode_status.clear();
        self.spawn_task(|| {
            let message = match activate_opencode_ccr() {
                Ok(_) => "✓ Added CCR provider to OpenCode config".to_string(),
                Err(e) => format!("✗ OpenCode update failed: {}", e),
            };
            StatusTaskResult::Operation {
                target: OperationTarget::OpenCode,
                message,
                snapshot: read_status_snapshot(),
            }
        });
    }

    fn deactivate_opencode_config(&mut self) {
        if self.opencode_checking {
            return;
        }
        self.opencode_checking = true;
        self.opencode_status.clear();
        self.spawn_task(|| {
            let message = match deactivate_opencode_ccr() {
                Ok(_) => "✓ Removed CCR provider from OpenCode config".to_string(),
                Err(e) => format!("✗ OpenCode update failed: {}", e),
            };
            StatusTaskResult::Operation {
                target: OperationTarget::OpenCode,
                message,
                snapshot: read_status_snapshot(),
            }
        });
    }

    fn activate_openclaw_config(&mut self) {
        if self.openclaw_checking {
            return;
        }
        self.openclaw_checking = true;
        self.openclaw_status.clear();
        self.spawn_task(|| {
            let message = match activate_openclaw_ccr() {
                Ok(_) => "✓ Added CCR provider to OpenClaw config".to_string(),
                Err(e) => format!("✗ OpenClaw update failed: {}", e),
            };
            StatusTaskResult::Operation {
                target: OperationTarget::OpenClaw,
                message,
                snapshot: read_status_snapshot(),
            }
        });
    }

    fn deactivate_openclaw_config(&mut self) {
        if self.openclaw_checking {
            return;
        }
        self.openclaw_checking = true;
        self.openclaw_status.clear();
        self.spawn_task(|| {
            let message = match deactivate_openclaw_ccr() {
                Ok(_) => "✓ Removed CCR provider from OpenClaw config".to_string(),
                Err(e) => format!("✗ OpenClaw update failed: {}", e),
            };
            StatusTaskResult::Operation {
                target: OperationTarget::OpenClaw,
                message,
                snapshot: read_status_snapshot(),
            }
        });
    }

    fn activate_hermes_config(&mut self) {
        if self.hermes_checking {
            return;
        }
        self.hermes_checking = true;
        self.hermes_status.clear();
        self.spawn_task(|| {
            let message = match activate_hermes_ccr() {
                Ok(_) => "✓ Added CCR provider to Hermes config".to_string(),
                Err(e) => format!("✗ Hermes update failed: {}", e),
            };
            StatusTaskResult::Operation {
                target: OperationTarget::Hermes,
                message,
                snapshot: read_status_snapshot(),
            }
        });
    }

    fn deactivate_hermes_config(&mut self) {
        if self.hermes_checking {
            return;
        }
        self.hermes_checking = true;
        self.hermes_status.clear();
        self.spawn_task(|| {
            let message = match deactivate_hermes_ccr() {
                Ok(_) => "✓ Removed CCR provider from Hermes config".to_string(),
                Err(e) => format!("✗ Hermes update failed: {}", e),
            };
            StatusTaskResult::Operation {
                target: OperationTarget::Hermes,
                message,
                snapshot: read_status_snapshot(),
            }
        });
    }

    fn start_server(&mut self) {
        if self.server_operation_checking {
            return;
        }
        self.server_operation_checking = true;
        self.server_operation_status.clear();
        let exe_path = ccr_server_executable();
        self.spawn_task(move || {
            let message = match exe_path {
                Some(exe_path) => match start_server(&exe_path) {
                    Ok(operation) => operation.ui_message(),
                    Err(e) => format!("✗ Failed to start server: {}", e),
                },
                None => "✗ Cannot find ccr-server executable".to_string(),
            };
            StatusTaskResult::Operation {
                target: OperationTarget::Server,
                message,
                snapshot: read_status_snapshot(),
            }
        });
    }

    fn stop_server(&mut self) {
        if self.server_operation_checking {
            return;
        }
        self.server_operation_checking = true;
        self.server_operation_status.clear();
        self.spawn_task(|| {
            let message = match stop_server() {
                Ok(operation) => operation.ui_message(),
                Err(e) => format!("✗ Failed to stop server: {}", e),
            };
            StatusTaskResult::Operation {
                target: OperationTarget::Server,
                message,
                snapshot: read_status_snapshot(),
            }
        });
    }
}

fn ccr_server_executable() -> Option<std::path::PathBuf> {
    let current_exe = std::env::current_exe().ok()?;
    ccr_server_executable_from(&current_exe)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_snapshot_running_reports_running_and_port() {
        let snapshot = ServerSnapshot::Running {
            pid: 123,
            port: 4567,
        };

        assert!(snapshot.is_running());
        assert_eq!(snapshot.port(), 4567);
    }

    #[test]
    fn server_snapshot_stopped_disables_running() {
        let snapshot = ServerSnapshot::Stopped { port: 3456 };

        assert!(!snapshot.is_running());
        assert_eq!(snapshot.port(), 3456);
    }

    #[test]
    fn server_snapshot_stale_pid_is_not_running() {
        let snapshot = ServerSnapshot::StalePid {
            pid: 123,
            port: 3456,
        };

        assert!(!snapshot.is_running());
        assert_eq!(snapshot.port(), 3456);
    }

    #[test]
    fn injection_snapshot_reports_activation_state() {
        assert!(!InjectionSnapshot::Inactive.is_activated());
        assert!(InjectionSnapshot::ActiveAndCurrent {
            backup_path: "/tmp/config.backup".to_string(),
            backup_time: None,
        }
        .is_activated());
        assert!(InjectionSnapshot::InjectedNoBackup.has_current_injection());
    }

    #[test]
    fn normalized_api_key_ignores_blank_values() {
        assert_eq!(normalized_api_key(None), None);
        assert_eq!(normalized_api_key(Some("")), None);
        assert_eq!(normalized_api_key(Some("  ")), None);
        assert_eq!(normalized_api_key(Some(" secret ")), Some("secret"));
    }
}
