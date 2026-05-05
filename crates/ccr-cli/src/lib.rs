use anyhow::Result;
use std::path::PathBuf;

pub mod claude_config;
pub mod codex_config;
pub mod openclaw_config;
pub mod opencode_config;

pub fn pid_file_path() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude-code-router")
        .join(".ccr.pid")
}

pub fn read_pid(path: &std::path::Path) -> Option<u32> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

pub fn write_pid(path: &std::path::Path, pid: u32) -> Result<()> {
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

pub fn activate_output(port: u16, api_key: Option<&str>) -> String {
    let key = api_key.unwrap_or("any");
    format!("export ANTHROPIC_BASE_URL=http://127.0.0.1:{port}\nexport ANTHROPIC_AUTH_TOKEN={key}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn write_and_read_pid() {
        let f = NamedTempFile::new().unwrap();
        write_pid(f.path(), 12345).unwrap();
        assert_eq!(read_pid(f.path()), Some(12345));
    }

    #[test]
    fn read_pid_missing_file() {
        assert!(read_pid(std::path::Path::new("/tmp/ccr_no_such_file.pid")).is_none());
    }

    #[test]
    fn read_pid_rejects_invalid_content() {
        let f = NamedTempFile::new().unwrap();
        std::fs::write(f.path(), "not-a-pid").unwrap();
        assert_eq!(read_pid(f.path()), None);
    }

    #[test]
    fn write_pid_creates_parent_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nested").join(".ccr.pid");
        write_pid(&path, 6789).unwrap();
        assert_eq!(read_pid(&path), Some(6789));
    }

    #[cfg(unix)]
    #[test]
    fn current_process_is_alive() {
        assert!(is_process_alive(std::process::id()));
    }

    #[test]
    fn activate_output_format() {
        let out = activate_output(3456, Some("sk-test"));
        assert!(out.contains("ANTHROPIC_BASE_URL=http://127.0.0.1:3456"));
        assert!(out.contains("ANTHROPIC_AUTH_TOKEN=sk-test"));
    }

    #[test]
    fn activate_output_no_key() {
        let out = activate_output(3456, None);
        assert!(out.contains("ANTHROPIC_AUTH_TOKEN=any"));
    }
}
