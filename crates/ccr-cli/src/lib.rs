pub use ccr_app_core::status::{is_process_alive, pid_file_path, read_pid, write_pid};

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
}
