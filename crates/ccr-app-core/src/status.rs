use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
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

pub fn start_server(exe_path: &Path) -> Result<ServerOperation> {
    let pid_path = pid_file_path();
    if let Some(pid) = read_pid(&pid_path) {
        if is_process_alive(pid) {
            return Ok(ServerOperation::AlreadyRunning { pid });
        }
    }

    let child = Command::new(exe_path)
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
