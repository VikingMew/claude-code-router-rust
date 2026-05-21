use crate::client_config::claude::claude_injection_snapshot;
use crate::client_config::codex::codex_injection_snapshot;
use crate::client_config::hermes::{
    hermes_config_path, hermes_provider_exists, hermes_provider_present,
};
use crate::client_config::openclaw::{
    openclaw_config_path, openclaw_provider_exists, openclaw_provider_present,
};
use crate::client_config::opencode::{
    opencode_config_path, opencode_provider_exists, opencode_provider_present,
};
use crate::metrics::{RouteMetricSummary, TtftMetricSummary};
use crate::runtime_status::{
    RoutePoolStatusResponse, fetch_route_pool_status, fetch_runtime_metrics_summary,
    fetch_ttft_metrics_summary,
};
use crate::settings::route_pool_config;
use anyhow::{Context, Result};
use ccr_config::{default_config_path, load_config};
use ccr_types::Config;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerSnapshot {
    Running { pid: u32, port: u16 },
    Stopped { port: u16 },
    StalePid { pid: u32, port: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerOperation {
    Started { pid: u32 },
    Stopped,
    AlreadyRunning { pid: u32 },
    NotRunning,
    Unsupported,
}

impl ServerOperation {
    pub fn ui_message(&self) -> String {
        match self {
            ServerOperation::Started { pid } => format!("✓ Server started (PID {pid})"),
            ServerOperation::Stopped => "✓ Server stopped".to_string(),
            ServerOperation::AlreadyRunning { pid } => {
                format!("✗ Server is already running (PID {pid})")
            }
            ServerOperation::NotRunning => "✗ Server is not running".to_string(),
            ServerOperation::Unsupported => {
                "✗ Stop server not supported on this platform".to_string()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthCheck {
    Healthy,
    HttpStatus(String),
    ConnectionError(String),
}

impl HealthCheck {
    pub fn ui_message(&self) -> String {
        match self {
            HealthCheck::Healthy => "✓ Server is healthy".to_string(),
            HealthCheck::HttpStatus(status) => format!("✗ Server returned status: {status}"),
            HealthCheck::ConnectionError(error) => format!("✗ Connection error: {error}"),
        }
    }
}

impl ServerSnapshot {
    pub fn is_running(&self) -> bool {
        matches!(self, ServerSnapshot::Running { .. })
    }

    pub fn port(&self) -> u16 {
        match self {
            ServerSnapshot::Running { port, .. }
            | ServerSnapshot::Stopped { port }
            | ServerSnapshot::StalePid { port, .. } => *port,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InjectionSnapshot {
    ActiveAndCurrent {
        backup_path: String,
        backup_time: Option<String>,
    },
    ActiveButDrifted {
        backup_path: String,
        backup_time: Option<String>,
    },
    InjectedNoBackup,
    Inactive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdditiveClientSnapshot {
    ProviderCurrent { path: String },
    ProviderDrifted { path: String },
    Missing { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutePoolConfigSnapshot {
    Enabled {
        active_routes: usize,
        failure_threshold: u32,
        ban_seconds: u64,
    },
    EnabledEmpty {
        configured_routes: usize,
        active_routes: usize,
    },
    Disabled {
        configured_routes: usize,
    },
    NotConfigured,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusSnapshot {
    pub server: ServerSnapshot,
    pub claude: InjectionSnapshot,
    pub codex: InjectionSnapshot,
    pub opencode: AdditiveClientSnapshot,
    pub openclaw: AdditiveClientSnapshot,
    pub hermes: AdditiveClientSnapshot,
    pub route_pool: RoutePoolConfigSnapshot,
    pub api_key: Option<String>,
    pub server_auto_start: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeStatusSnapshot {
    pub route_pool_status: Result<RoutePoolStatusResponse, String>,
    pub runtime_metrics_summary: Result<Vec<RouteMetricSummary>, String>,
    pub ttft_metrics_summary: Result<Vec<TtftMetricSummary>, String>,
}

impl AdditiveClientSnapshot {
    pub fn provider_present(&self) -> bool {
        matches!(
            self,
            AdditiveClientSnapshot::ProviderCurrent { .. }
                | AdditiveClientSnapshot::ProviderDrifted { .. }
        )
    }

    pub fn is_current(&self) -> bool {
        matches!(self, AdditiveClientSnapshot::ProviderCurrent { .. })
    }

    pub fn path(&self) -> &str {
        match self {
            AdditiveClientSnapshot::ProviderCurrent { path }
            | AdditiveClientSnapshot::ProviderDrifted { path }
            | AdditiveClientSnapshot::Missing { path } => path,
        }
    }
}

pub fn pid_file_path() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude-code-router")
        .join(".ccr.pid")
}

pub fn read_pid(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

pub fn write_pid(path: &Path, pid: u32) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, pid.to_string())?;
    Ok(())
}

#[cfg(unix)]
pub fn is_process_alive(pid: u32) -> bool {
    use nix::sys::signal;
    use nix::unistd::Pid;
    signal::kill(Pid::from_raw(pid as i32), None).is_ok()
}

#[cfg(not(unix))]
pub fn is_process_alive(_pid: u32) -> bool {
    false
}

pub fn read_server_snapshot(port: u16) -> ServerSnapshot {
    let pid_path = pid_file_path();
    match read_pid(&pid_path) {
        Some(pid) if is_process_alive(pid) => ServerSnapshot::Running { pid, port },
        Some(pid) => ServerSnapshot::StalePid { pid, port },
        None => ServerSnapshot::Stopped { port },
    }
}

pub fn read_status_snapshot() -> StatusSnapshot {
    let config = load_config(&default_config_path()).unwrap_or_default();
    read_status_snapshot_from_config(&config)
}

pub fn read_status_snapshot_from_config(config: &Config) -> StatusSnapshot {
    let port = config.port.unwrap_or(3456);
    let server = read_server_snapshot(port);
    let port = server.port();
    StatusSnapshot {
        server,
        claude: claude_injection_snapshot(port),
        codex: codex_injection_snapshot(port),
        opencode: opencode_snapshot(port),
        openclaw: openclaw_snapshot(port),
        hermes: hermes_snapshot(port),
        route_pool: route_pool_config_snapshot(config),
        api_key: config.api_key.clone(),
        server_auto_start: config.app_settings.server_auto_start,
    }
}

pub fn route_pool_config_snapshot(config: &Config) -> RoutePoolConfigSnapshot {
    match route_pool_config(config) {
        Some(pool) if pool.enabled && pool.candidates.iter().any(|candidate| candidate.enabled) => {
            RoutePoolConfigSnapshot::Enabled {
                active_routes: pool
                    .candidates
                    .iter()
                    .filter(|candidate| candidate.enabled)
                    .count(),
                failure_threshold: pool.failure_threshold.max(1),
                ban_seconds: pool.ban_seconds.max(1),
            }
        }
        Some(pool) if pool.enabled => RoutePoolConfigSnapshot::EnabledEmpty {
            configured_routes: pool.candidates.len(),
            active_routes: pool
                .candidates
                .iter()
                .filter(|candidate| candidate.enabled)
                .count(),
        },
        Some(pool) => RoutePoolConfigSnapshot::Disabled {
            configured_routes: pool.candidates.len(),
        },
        None => RoutePoolConfigSnapshot::NotConfigured,
    }
}

pub fn fetch_runtime_status_snapshot(port: u16, api_key: Option<&str>) -> RuntimeStatusSnapshot {
    RuntimeStatusSnapshot {
        route_pool_status: fetch_route_pool_status(port, api_key),
        runtime_metrics_summary: fetch_runtime_metrics_summary(port, api_key),
        ttft_metrics_summary: fetch_ttft_metrics_summary(port, api_key),
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

fn hermes_snapshot(port: u16) -> AdditiveClientSnapshot {
    let path = hermes_config_path().display().to_string();
    if hermes_provider_present(port) {
        AdditiveClientSnapshot::ProviderCurrent { path }
    } else if hermes_provider_exists() {
        AdditiveClientSnapshot::ProviderDrifted { path }
    } else {
        AdditiveClientSnapshot::Missing { path }
    }
}

pub fn start_server(exe_path: &Path) -> Result<ServerOperation> {
    let pid_path = pid_file_path();
    if let Some(pid) = read_pid(&pid_path)
        && is_process_alive(pid)
    {
        return Ok(ServerOperation::AlreadyRunning { pid });
    }

    let mut command = Command::new(exe_path);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    let child = command
        .spawn()
        .with_context(|| format!("failed to start {}", exe_path.display()))?;
    let pid = child.id();
    write_pid(&pid_path, pid)?;
    Ok(ServerOperation::Started { pid })
}

pub fn stop_server() -> Result<ServerOperation> {
    let pid_path = pid_file_path();
    let Some(pid) = read_pid(&pid_path) else {
        return Ok(ServerOperation::NotRunning);
    };

    #[cfg(unix)]
    {
        use nix::sys::signal::{self, Signal};
        use nix::unistd::Pid;
        signal::kill(Pid::from_raw(pid as i32), Signal::SIGTERM)?;
        std::fs::remove_file(&pid_path).ok();
        Ok(ServerOperation::Stopped)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        Ok(ServerOperation::Unsupported)
    }
}

pub fn check_health(port: u16) -> HealthCheck {
    let url = format!("http://127.0.0.1:{port}/health");
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(client) => client,
        Err(error) => return HealthCheck::ConnectionError(error.to_string()),
    };

    match client.get(&url).send() {
        Ok(response) if response.status().is_success() => HealthCheck::Healthy,
        Ok(response) => HealthCheck::HttpStatus(response.status().to_string()),
        Err(error) => HealthCheck::ConnectionError(error.to_string()),
    }
}

pub fn ccr_server_executable_from(current_exe: &Path) -> Option<PathBuf> {
    let file_name = if cfg!(windows) {
        "ccr-server.exe"
    } else {
        "ccr-server"
    };

    let sibling = current_exe.parent()?.join(file_name);
    if sibling.exists() {
        return Some(sibling);
    }

    let bundled = current_exe
        .parent()?
        .parent()?
        .join("Resources")
        .join("bin")
        .join(file_name);
    bundled.exists().then_some(bundled)
}

impl InjectionSnapshot {
    pub fn is_activated(&self) -> bool {
        !matches!(self, InjectionSnapshot::Inactive)
    }

    pub fn has_current_injection(&self) -> bool {
        matches!(
            self,
            InjectionSnapshot::ActiveAndCurrent { .. } | InjectionSnapshot::InjectedNoBackup
        )
    }
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
    fn server_snapshot_stopped_reports_port() {
        let snapshot = ServerSnapshot::Stopped { port: 3456 };
        assert!(!snapshot.is_running());
        assert_eq!(snapshot.port(), 3456);
    }

    #[test]
    fn injection_snapshot_reports_activation() {
        assert!(!InjectionSnapshot::Inactive.is_activated());
        assert!(
            InjectionSnapshot::ActiveAndCurrent {
                backup_path: "/tmp/backup".to_string(),
                backup_time: None,
            }
            .is_activated()
        );
        assert!(
            InjectionSnapshot::ActiveButDrifted {
                backup_path: "/tmp/backup".to_string(),
                backup_time: None,
            }
            .is_activated()
        );
        assert!(InjectionSnapshot::InjectedNoBackup.has_current_injection());
        assert!(!InjectionSnapshot::Inactive.has_current_injection());
    }

    #[test]
    fn additive_client_snapshot_reports_current_and_present() {
        let current = AdditiveClientSnapshot::ProviderCurrent {
            path: "/tmp/config".into(),
        };
        let drifted = AdditiveClientSnapshot::ProviderDrifted {
            path: "/tmp/config".into(),
        };
        let missing = AdditiveClientSnapshot::Missing {
            path: "/tmp/config".into(),
        };

        assert!(current.provider_present());
        assert!(current.is_current());
        assert!(drifted.provider_present());
        assert!(!drifted.is_current());
        assert!(!missing.provider_present());
        assert_eq!(missing.path(), "/tmp/config");
    }

    #[test]
    fn route_pool_config_snapshot_reports_enabled_pool() {
        let config = Config {
            route_pool: Some(ccr_types::RoutePoolConfig {
                enabled: true,
                failure_threshold: 0,
                ban_seconds: 0,
                candidates: vec![
                    ccr_types::RoutePoolCandidate {
                        route: "anthropic,claude".into(),
                        enabled: true,
                        priority: 1,
                    },
                    ccr_types::RoutePoolCandidate {
                        route: "openai,gpt".into(),
                        enabled: false,
                        priority: 2,
                    },
                ],
            }),
            ..Config::default()
        };

        assert_eq!(
            route_pool_config_snapshot(&config),
            RoutePoolConfigSnapshot::Enabled {
                active_routes: 1,
                failure_threshold: 1,
                ban_seconds: 1,
            }
        );
    }

    #[test]
    fn route_pool_config_snapshot_reports_empty_disabled_and_missing() {
        let mut config = Config::default();
        assert_eq!(
            route_pool_config_snapshot(&config),
            RoutePoolConfigSnapshot::NotConfigured
        );

        config.route_pool = Some(ccr_types::RoutePoolConfig {
            enabled: false,
            failure_threshold: 3,
            ban_seconds: 30,
            candidates: vec![ccr_types::RoutePoolCandidate {
                route: "anthropic,claude".into(),
                enabled: true,
                priority: 1,
            }],
        });
        assert_eq!(
            route_pool_config_snapshot(&config),
            RoutePoolConfigSnapshot::Disabled {
                configured_routes: 1
            }
        );

        config.route_pool = Some(ccr_types::RoutePoolConfig {
            enabled: true,
            failure_threshold: 3,
            ban_seconds: 30,
            candidates: vec![ccr_types::RoutePoolCandidate {
                route: "anthropic,claude".into(),
                enabled: false,
                priority: 1,
            }],
        });
        assert_eq!(
            route_pool_config_snapshot(&config),
            RoutePoolConfigSnapshot::EnabledEmpty {
                configured_routes: 1,
                active_routes: 0,
            }
        );
    }

    #[test]
    fn server_operation_messages_are_stable() {
        assert_eq!(
            ServerOperation::Started { pid: 42 }.ui_message(),
            "✓ Server started (PID 42)"
        );
        assert_eq!(
            ServerOperation::AlreadyRunning { pid: 42 }.ui_message(),
            "✗ Server is already running (PID 42)"
        );
    }

    #[test]
    fn health_check_messages_are_stable() {
        assert_eq!(HealthCheck::Healthy.ui_message(), "✓ Server is healthy");
        assert_eq!(
            HealthCheck::HttpStatus("500 Internal Server Error".into()).ui_message(),
            "✗ Server returned status: 500 Internal Server Error"
        );
    }
}
