#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerSnapshot {
    Running { pid: u32, port: u16 },
    Stopped { port: u16 },
    StalePid { pid: u32, port: u16 },
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
}
