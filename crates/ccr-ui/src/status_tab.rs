use ccr_app_core::client_config::claude::{
    activate_ccr, claude_injection_snapshot, deactivate_ccr,
};
use ccr_app_core::client_config::codex::{
    activate_codex_ccr, codex_injection_snapshot, deactivate_codex_ccr,
};
use ccr_app_core::client_config::openclaw::{
    activate_openclaw_ccr, deactivate_openclaw_ccr, openclaw_config_path, openclaw_provider_exists,
    openclaw_provider_present,
};
use ccr_app_core::client_config::opencode::{
    activate_opencode_ccr, deactivate_opencode_ccr, opencode_config_path, opencode_provider_exists,
    opencode_provider_present,
};
use ccr_app_core::settings::route_pool_config;
use ccr_app_core::status::{
    ccr_server_executable_from, check_health,
    read_server_snapshot as read_server_snapshot_from_core, start_server, stop_server,
    AdditiveClientSnapshot, InjectionSnapshot, ServerSnapshot,
};
use ccr_config::{default_config_path, load_config};
use eframe::egui;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const ROUTE_POOL_STATUS_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
struct StatusSnapshot {
    server: ServerSnapshot,
    claude: InjectionSnapshot,
    codex: InjectionSnapshot,
    opencode: AdditiveClientSnapshot,
    openclaw: AdditiveClientSnapshot,
}

#[derive(Debug, Clone, Deserialize)]
struct RoutePoolStatusResponse {
    enabled: bool,
    #[serde(rename = "failureThreshold")]
    failure_threshold: u32,
    #[serde(rename = "banSeconds")]
    ban_seconds: u64,
    routes: HashMap<String, RoutePoolRouteStatus>,
}

#[derive(Debug, Clone, Deserialize)]
struct RoutePoolRouteStatus {
    consecutive_failures: u32,
    banned_until_epoch_secs: Option<u64>,
    last_error: Option<String>,
    last_failure_epoch_secs: Option<u64>,
    last_success_epoch_secs: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct RuntimeMetricSummary {
    route: String,
    provider: String,
    attempts: u64,
    successes: u64,
    failures: u64,
    average_latency_ms: Option<u64>,
    last_http_status: Option<u16>,
    last_error_class: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TtftMetricSummary {
    route: String,
    provider: String,
    model: String,
    samples: u64,
    average_ttft_ms: Option<u64>,
    p50_ttft_ms: Option<u64>,
    p90_ttft_ms: Option<u64>,
    p95_ttft_ms: Option<u64>,
    latest_ttft_ms: Option<u64>,
}

pub struct StatusTab {
    snapshot: StatusSnapshot,
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
    server_operation_status: String,
    server_operation_checking: bool,
    route_pool_status: Option<Result<RoutePoolStatusResponse, String>>,
    runtime_metrics_summary: Option<Result<Vec<RuntimeMetricSummary>, String>>,
    ttft_metrics_summary: Option<Result<Vec<TtftMetricSummary>, String>>,
    route_pool_last_refresh: Option<Instant>,
}

impl StatusTab {
    pub fn new() -> Self {
        let mut tab = Self {
            snapshot: read_status_snapshot(),
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
            server_operation_status: String::new(),
            server_operation_checking: false,
            route_pool_status: None,
            runtime_metrics_summary: None,
            ttft_metrics_summary: None,
            route_pool_last_refresh: None,
        };
        tab.refresh_snapshot();
        tab
    }

    pub fn start_server_on_launch(&mut self) {
        let config = load_config(&default_config_path()).unwrap_or_default();
        if !config.app_settings.server_auto_start {
            return;
        }
        self.refresh_snapshot();
        if matches!(self.snapshot.server, ServerSnapshot::Running { .. }) {
            return;
        }

        self.start_server();
    }

    fn refresh_snapshot(&mut self) {
        self.snapshot = read_status_snapshot();
    }

    fn refresh_status(&mut self) {
        self.refresh_snapshot();
        self.route_pool_last_refresh = None;
        let port = self.snapshot.server.port();
        self.check_health(port);
        self.refresh_snapshot();
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.refresh_snapshot();

        ui.horizontal(|ui| {
            ui.heading("Status");
            if ui.button("Refresh Status").clicked() {
                self.refresh_status();
            }
        });

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
    }

    fn show_server(&mut self, ui: &mut egui::Ui) {
        ui.heading("Server");

        match &self.snapshot.server {
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

        if self.snapshot.server.is_running() {
            if self.health_status.is_empty() {
                ui.label("HTTP Health: Not checked");
            } else {
                ui.label(format!("HTTP Health: {}", self.health_status));
            }
        } else {
            ui.label("HTTP Health: Unavailable while server is stopped");
        }

        ui.add_space(4.0);

        let server_running = self.snapshot.server.is_running();
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
                self.check_health(self.snapshot.server.port());
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
        let config = load_config(&default_config_path()).unwrap_or_default();

        match route_pool_config(&config) {
            Some(pool)
                if pool.enabled && pool.candidates.iter().any(|candidate| candidate.enabled) =>
            {
                let enabled = pool
                    .candidates
                    .iter()
                    .filter(|candidate| candidate.enabled)
                    .count();
                ui.label(format!(
                    "Route Pool: Enabled ({enabled} active routes, {} failures, {}s ban)",
                    pool.failure_threshold.max(1),
                    pool.ban_seconds.max(1)
                ));
            }
            Some(pool) => {
                let configured = pool.candidates.len();
                let enabled = pool
                    .candidates
                    .iter()
                    .filter(|candidate| candidate.enabled)
                    .count();
                if pool.enabled {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("Route Pool: Enabled but empty ({configured} configured, {enabled} active)"),
                    );
                } else {
                    ui.label(format!(
                        "Route Pool: Disabled ({configured} configured routes)"
                    ));
                }
                if pool.enabled {
                    ui.label("Runtime: Not applicable until Route Pool has active routes");
                } else {
                    ui.label("Runtime: Not applicable while Route Pool is disabled");
                }
                self.route_pool_status = None;
                self.route_pool_last_refresh = None;
                return;
            }
            None => {
                ui.label("Route Pool: Not configured");
                ui.label("Runtime: Not applicable");
                self.route_pool_status = None;
                self.route_pool_last_refresh = None;
                return;
            }
        }

        if self.snapshot.server.is_running() {
            self.refresh_route_pool_status_if_due(config.api_key.as_deref());
            match &self.route_pool_status {
                Some(Ok(status)) => show_route_pool_runtime(ui, status),
                Some(Err(error)) => {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("Route Pool runtime unavailable: {error}"),
                    );
                }
                None => {
                    ui.label("Route Pool runtime: Not loaded yet.");
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
                    ui.label("Real traffic metrics: Not loaded yet.");
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
                    ui.label("TTFT metrics from real client traffic: Not loaded yet.");
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

    fn refresh_route_pool_status_if_due(&mut self, api_key: Option<&str>) {
        let now = Instant::now();
        let should_refresh = self
            .route_pool_last_refresh
            .map(|last| now.duration_since(last) >= ROUTE_POOL_STATUS_REFRESH_INTERVAL)
            .unwrap_or(true);
        if !should_refresh {
            return;
        }

        let port = self.snapshot.server.port();
        self.route_pool_status = Some(fetch_route_pool_status(port, api_key));
        self.runtime_metrics_summary = Some(fetch_runtime_metrics_summary(port, api_key));
        self.ttft_metrics_summary = Some(fetch_ttft_metrics_summary(port, api_key));
        self.route_pool_last_refresh = Some(now);
    }

    fn show_claude(&mut self, ui: &mut egui::Ui) {
        ui.heading("Claude Config Switch");

        show_injection_snapshot(ui, &self.snapshot.claude);

        ui.add_space(4.0);

        let server_running = self.snapshot.server.is_running();
        let activated = self.snapshot.claude.is_activated();
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

        show_injection_snapshot(ui, &self.snapshot.codex);

        ui.add_space(4.0);

        let server_running = self.snapshot.server.is_running();
        let activated = self.snapshot.codex.is_activated();
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
        show_additive_client_snapshot(ui, &self.snapshot.opencode);

        let server_running = self.snapshot.server.is_running();
        let provider_present = self.snapshot.opencode.provider_present();
        let current = self.snapshot.opencode.is_current();
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
        show_additive_client_snapshot(ui, &self.snapshot.openclaw);

        let server_running = self.snapshot.server.is_running();
        let provider_present = self.snapshot.openclaw.provider_present();
        let current = self.snapshot.openclaw.is_current();
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
}

impl Default for StatusTab {
    fn default() -> Self {
        Self::new()
    }
}

fn read_status_snapshot() -> StatusSnapshot {
    let server = read_server_snapshot();
    let port = server.port();
    StatusSnapshot {
        server,
        claude: claude_injection_snapshot(port),
        codex: codex_injection_snapshot(port),
        opencode: opencode_snapshot(port),
        openclaw: openclaw_snapshot(port),
    }
}

fn opencode_snapshot(port: u16) -> AdditiveClientSnapshot {
    let path = opencode_config_path().display().to_string();
    if opencode_provider_present(port) {
        AdditiveClientSnapshot::ProviderCurrent { path }
    } else if opencode_provider_exists() {
        AdditiveClientSnapshot::ProviderDrifted { path }
    } else {
        AdditiveClientSnapshot::Missing { path }
    }
}

fn openclaw_snapshot(port: u16) -> AdditiveClientSnapshot {
    let path = openclaw_config_path().display().to_string();
    if openclaw_provider_present(port) {
        AdditiveClientSnapshot::ProviderCurrent { path }
    } else if openclaw_provider_exists() {
        AdditiveClientSnapshot::ProviderDrifted { path }
    } else {
        AdditiveClientSnapshot::Missing { path }
    }
}

fn read_server_snapshot() -> ServerSnapshot {
    let port = load_config(&default_config_path())
        .ok()
        .and_then(|c| c.port)
        .unwrap_or(3456);
    read_server_snapshot_from_core(port)
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

fn fetch_route_pool_status(
    port: u16,
    api_key: Option<&str>,
) -> Result<RoutePoolStatusResponse, String> {
    let url = format!("http://127.0.0.1:{}/api/route-pool/status", port);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(800))
        .build()
        .map_err(|error| error.to_string())?;
    let mut request = client.get(&url);
    if let Some(api_key) = normalized_api_key(api_key) {
        request = request.bearer_auth(api_key);
    }
    let response = request.send().map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        if response.status().as_u16() == 401 {
            return Err(
                "unauthorized. The UI config API key does not match the running server."
                    .to_string(),
            );
        }
        return Err(format!("HTTP {}", response.status()));
    }
    response.json().map_err(|error| error.to_string())
}

fn fetch_runtime_metrics_summary(
    port: u16,
    api_key: Option<&str>,
) -> Result<Vec<RuntimeMetricSummary>, String> {
    let url = format!("http://127.0.0.1:{}/api/runtime-metrics/summary", port);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(800))
        .build()
        .map_err(|error| error.to_string())?;
    let mut request = client.get(&url);
    if let Some(api_key) = normalized_api_key(api_key) {
        request = request.bearer_auth(api_key);
    }
    let response = request.send().map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        if response.status().as_u16() == 401 {
            return Err(
                "unauthorized. The UI config API key does not match the running server."
                    .to_string(),
            );
        }
        return Err(format!("HTTP {}", response.status()));
    }
    response.json().map_err(|error| error.to_string())
}

fn fetch_ttft_metrics_summary(
    port: u16,
    api_key: Option<&str>,
) -> Result<Vec<TtftMetricSummary>, String> {
    let url = format!(
        "http://127.0.0.1:{}/api/runtime-metrics/ttft-summary?window=300",
        port
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(800))
        .build()
        .map_err(|error| error.to_string())?;
    let mut request = client.get(&url);
    if let Some(api_key) = normalized_api_key(api_key) {
        request = request.bearer_auth(api_key);
    }
    let response = request.send().map_err(|error| error.to_string())?;
    if !response.status().is_success() {
        if response.status().as_u16() == 401 {
            return Err(
                "unauthorized. The UI config API key does not match the running server."
                    .to_string(),
            );
        }
        return Err(format!("HTTP {}", response.status()));
    }
    response.json().map_err(|error| error.to_string())
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

fn show_runtime_metrics_summary(ui: &mut egui::Ui, summary: &[RuntimeMetricSummary]) {
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
        self.health_checking = true;
        self.health_status.clear();
        self.health_status = check_health(port).ui_message();
        self.health_checking = false;
    }

    fn activate_claude_config(&mut self) {
        self.claude_activation_checking = true;
        self.claude_activation_status.clear();

        match activate_ccr() {
            Ok(_) => {
                self.claude_activation_status =
                    "✓ Activated! Claude Code is now using CCR router".to_string();
            }
            Err(e) => {
                self.claude_activation_status = format!("✗ Activation failed: {}", e);
            }
        }

        self.claude_activation_checking = false;
        self.refresh_snapshot();
    }

    fn deactivate_claude_config(&mut self) {
        self.claude_activation_checking = true;
        self.claude_activation_status.clear();

        match deactivate_ccr() {
            Ok(_) => {
                self.claude_activation_status =
                    "✓ Deactivated! Claude Code is now using original configuration".to_string();
            }
            Err(e) => {
                self.claude_activation_status = format!("✗ Deactivation failed: {}", e);
            }
        }

        self.claude_activation_checking = false;
        self.refresh_snapshot();
    }

    fn activate_codex_config(&mut self) {
        self.codex_activation_checking = true;
        self.codex_activation_status.clear();

        match activate_codex_ccr() {
            Ok(_) => {
                self.codex_activation_status =
                    "✓ Activated! Codex is now using CCR router".to_string();
            }
            Err(e) => {
                self.codex_activation_status = format!("✗ Activation failed: {}", e);
            }
        }

        self.codex_activation_checking = false;
        self.refresh_snapshot();
    }

    fn deactivate_codex_config(&mut self) {
        self.codex_activation_checking = true;
        self.codex_activation_status.clear();

        match deactivate_codex_ccr() {
            Ok(_) => {
                self.codex_activation_status =
                    "✓ Deactivated! Codex is now using original configuration".to_string();
            }
            Err(e) => {
                self.codex_activation_status = format!("✗ Deactivation failed: {}", e);
            }
        }

        self.codex_activation_checking = false;
        self.refresh_snapshot();
    }

    fn activate_opencode_config(&mut self) {
        self.opencode_checking = true;
        self.opencode_status.clear();

        match activate_opencode_ccr() {
            Ok(_) => {
                self.opencode_status = "✓ Added CCR provider to OpenCode config".to_string();
            }
            Err(e) => {
                self.opencode_status = format!("✗ OpenCode update failed: {}", e);
            }
        }

        self.opencode_checking = false;
        self.refresh_snapshot();
    }

    fn deactivate_opencode_config(&mut self) {
        self.opencode_checking = true;
        self.opencode_status.clear();

        match deactivate_opencode_ccr() {
            Ok(_) => {
                self.opencode_status = "✓ Removed CCR provider from OpenCode config".to_string();
            }
            Err(e) => {
                self.opencode_status = format!("✗ OpenCode update failed: {}", e);
            }
        }

        self.opencode_checking = false;
        self.refresh_snapshot();
    }

    fn activate_openclaw_config(&mut self) {
        self.openclaw_checking = true;
        self.openclaw_status.clear();

        match activate_openclaw_ccr() {
            Ok(_) => {
                self.openclaw_status = "✓ Added CCR provider to OpenClaw config".to_string();
            }
            Err(e) => {
                self.openclaw_status = format!("✗ OpenClaw update failed: {}", e);
            }
        }

        self.openclaw_checking = false;
        self.refresh_snapshot();
    }

    fn deactivate_openclaw_config(&mut self) {
        self.openclaw_checking = true;
        self.openclaw_status.clear();

        match deactivate_openclaw_ccr() {
            Ok(_) => {
                self.openclaw_status = "✓ Removed CCR provider from OpenClaw config".to_string();
            }
            Err(e) => {
                self.openclaw_status = format!("✗ OpenClaw update failed: {}", e);
            }
        }

        self.openclaw_checking = false;
        self.refresh_snapshot();
    }

    fn start_server(&mut self) {
        self.server_operation_checking = true;
        self.server_operation_status.clear();

        match ccr_server_executable() {
            Some(exe_path) => match start_server(&exe_path) {
                Ok(operation) => self.server_operation_status = operation.ui_message(),
                Err(e) => self.server_operation_status = format!("✗ Failed to start server: {}", e),
            },
            None => {
                self.server_operation_status = "✗ Cannot find ccr-server executable".to_string();
            }
        }

        self.server_operation_checking = false;
        self.refresh_snapshot();
    }

    fn stop_server(&mut self) {
        self.server_operation_checking = true;
        self.server_operation_status.clear();

        match stop_server() {
            Ok(operation) => {
                self.server_operation_status = operation.ui_message();
            }
            Err(e) => {
                self.server_operation_status = format!("✗ Failed to stop server: {}", e);
            }
        }

        self.server_operation_checking = false;
        self.refresh_snapshot();
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
