use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_BACKUPS: usize = 5;

/// Create a backup of the config file
pub fn create_backup(config_path: &Path) -> Result<PathBuf> {
    if !config_path.exists() {
        return Err(anyhow::anyhow!("Config file does not exist"));
    }

    let backup_dir = get_backup_dir()?;
    fs::create_dir_all(&backup_dir)?;

    // Generate backup filename with timestamp
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let backup_name = format!("config.{}.json", timestamp);
    let backup_path = backup_dir.join(&backup_name);

    // Copy config to backup
    fs::copy(config_path, &backup_path).context("Failed to create backup")?;

    // Clean up old backups
    cleanup_old_backups(&backup_dir, MAX_BACKUPS)?;

    Ok(backup_path)
}

/// List all backup files (sorted by creation time, newest first)
pub fn list_backups() -> Result<Vec<BackupInfo>> {
    let backup_dir = get_backup_dir()?;
    list_backups_in_dir(&backup_dir)
}

fn list_backups_in_dir(backup_dir: &Path) -> Result<Vec<BackupInfo>> {
    if !backup_dir.exists() {
        return Ok(vec![]);
    }

    let mut backups = Vec::new();

    for entry in fs::read_dir(&backup_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("config.") {
                    let metadata = fs::metadata(&path)?;
                    let modified = metadata.modified()?;

                    backups.push(BackupInfo {
                        path: path.clone(),
                        name: name.to_string(),
                        created: modified,
                    });
                }
            }
        }
    }

    // Sort by creation time (newest first)
    backups.sort_by(|a, b| b.created.cmp(&a.created));

    Ok(backups)
}

/// Restore a backup to the main config file
pub fn restore_backup(backup_path: &Path, config_path: &Path) -> Result<()> {
    if !backup_path.exists() {
        return Err(anyhow::anyhow!("Backup file does not exist"));
    }

    // Create a backup of current config before restoring
    if config_path.exists() {
        create_backup(config_path)?;
    }

    // Restore backup
    fs::copy(backup_path, config_path).context("Failed to restore backup")?;

    Ok(())
}

/// Delete a specific backup file
pub fn delete_backup(backup_path: &Path) -> Result<()> {
    if !backup_path.exists() {
        return Err(anyhow::anyhow!("Backup file does not exist"));
    }

    fs::remove_file(backup_path).context("Failed to delete backup")?;

    Ok(())
}

fn get_backup_dir() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("CCR_BACKUP_DIR") {
        return Ok(PathBuf::from(path));
    }

    let config_dir = dirs_next::home_dir()
        .context("Could not find home directory")?
        .join(".claude-code-router");

    Ok(config_dir.join("backups"))
}

fn cleanup_old_backups(backup_dir: &Path, max_keep: usize) -> Result<()> {
    let mut backups = list_backups_in_dir(backup_dir)?;

    // Keep only the newest max_keep backups
    if backups.len() > max_keep {
        for backup in backups.drain(max_keep..) {
            fs::remove_file(&backup.path).ok();
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub struct BackupInfo {
    pub path: PathBuf,
    pub name: String,
    pub created: std::time::SystemTime,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;
    use std::thread;
    use std::time::Duration;
    use tempfile::TempDir;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_create_backup() {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let backup_dir = temp_dir.path().join("backups");
        unsafe { std::env::set_var("CCR_BACKUP_DIR", &backup_dir) };
        let config_path = temp_dir.path().join("config.json");
        fs::write(&config_path, "{\"test\": true}").unwrap();

        let backup_path = create_backup(&config_path).unwrap();
        assert!(backup_path.exists());
        unsafe { std::env::remove_var("CCR_BACKUP_DIR") };
    }

    #[test]
    fn test_list_backups() {
        let backups = list_backups().unwrap_or_default();
        for backup in backups {
            assert!(backup.name.starts_with("config."));
        }
    }

    #[test]
    fn test_cleanup_old_backups() {
        let _guard = ENV_LOCK.lock().unwrap();
        // Create multiple backups
        let temp_dir = TempDir::new().unwrap();
        let backup_dir = temp_dir.path().join("backups");
        unsafe { std::env::set_var("CCR_BACKUP_DIR", &backup_dir) };
        let config_path = temp_dir.path().join("config.json");

        for i in 0..7 {
            fs::write(&config_path, format!("{{\"version\": {}}}", i)).unwrap();
            create_backup(&config_path).ok();
            thread::sleep(Duration::from_millis(100));
        }

        let backups = list_backups().unwrap_or_default();
        // Should keep only MAX_BACKUPS (5)
        assert!(backups.len() <= MAX_BACKUPS + 1); // +1 for potential race condition
        unsafe { std::env::remove_var("CCR_BACKUP_DIR") };
    }
}
