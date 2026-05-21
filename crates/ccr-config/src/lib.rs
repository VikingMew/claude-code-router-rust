pub mod backup;
pub mod reloadable;

use anyhow::{Context, Result};
use ccr_types::Config;
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::info;

pub use reloadable::ReloadableConfig;

#[cfg(test)]
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn default_config_path() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude-code-router")
        .join("config.json")
}

pub fn load_config(path: &Path) -> Result<Config> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config: {}", path.display()))?;
    let interpolated = interpolate_env(&raw);
    let mut config: Config = json5::from_str(&interpolated).context("Failed to parse config")?;

    // Load proxy from environment variables if not configured
    if config.proxy.is_none() {
        config.proxy = load_proxy_from_env();
    }

    Ok(config)
}

pub fn save_config(config: &Config, path: &Path) -> Result<()> {
    // Create backup before saving
    if path.exists() {
        let backup_path = backup::create_backup(path)
            .with_context(|| format!("Failed to create backup before saving {}", path.display()))?;
        info!("Created backup: {}", backup_path.display());
    }

    let json_value = merge_with_existing_unknown_fields(config, path)?;
    let json = serde_json::to_string_pretty(&json_value)?;
    atomic_replace(path, json.as_bytes())
        .with_context(|| format!("Failed to atomically save config: {}", path.display()))?;
    Ok(())
}

fn atomic_replace(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    let temp_dir = parent.unwrap_or_else(|| Path::new("."));

    let mut temp = tempfile::Builder::new()
        .prefix(".config.")
        .suffix(".tmp")
        .tempfile_in(temp_dir)
        .with_context(|| format!("Failed to create temp file in {}", temp_dir.display()))?;

    temp.write_all(contents)
        .context("Failed to write config temp file")?;
    temp.as_file_mut()
        .sync_all()
        .context("Failed to sync config temp file")?;

    temp.persist(path)
        .map_err(|err| err.error)
        .with_context(|| format!("Failed to replace {}", path.display()))?;

    sync_parent_dir(temp_dir)
}

fn sync_parent_dir(dir: &Path) -> Result<()> {
    match File::open(dir) {
        Ok(file) => match file.sync_all() {
            Ok(()) => Ok(()),
            Err(err) if is_unsupported_dir_sync(&err) => Ok(()),
            Err(err) => Err(err).context("Failed to sync config directory"),
        },
        Err(err) if is_unsupported_dir_sync(&err) => Ok(()),
        Err(err) => {
            Err(err).with_context(|| format!("Failed to open config directory {}", dir.display()))
        }
    }
}

fn is_unsupported_dir_sync(err: &std::io::Error) -> bool {
    matches!(
        err.kind(),
        std::io::ErrorKind::Unsupported | std::io::ErrorKind::PermissionDenied
    )
}

fn merge_with_existing_unknown_fields(config: &Config, path: &Path) -> Result<serde_json::Value> {
    let mut next = serde_json::to_value(config)?;
    let existing = std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| json5::from_str::<serde_json::Value>(&raw).ok());

    if let (Some(existing_obj), Some(next_obj)) = (
        existing.and_then(|v| v.as_object().cloned()),
        next.as_object_mut(),
    ) {
        for (key, value) in existing_obj {
            next_obj.entry(key).or_insert(value);
        }
    }

    Ok(next)
}

fn interpolate_env(s: &str) -> String {
    let mut result = s.to_string();
    // Replace ${VAR} and $VAR patterns
    let re = regex::Regex::new(r"\$\{([^}]+)\}|\$([A-Za-z_][A-Za-z0-9_]*)").unwrap();
    result = re
        .replace_all(&result, |caps: &regex::Captures| {
            let var = caps
                .get(1)
                .or(caps.get(2))
                .map(|m| m.as_str())
                .unwrap_or("");
            env::var(var).unwrap_or_else(|_| caps[0].to_string())
        })
        .to_string();
    result
}

fn load_proxy_from_env() -> Option<ccr_types::ProxyConfig> {
    let http = env::var("HTTP_PROXY")
        .ok()
        .or_else(|| env::var("http_proxy").ok());
    let https = env::var("HTTPS_PROXY")
        .ok()
        .or_else(|| env::var("https_proxy").ok());
    let socks5 = env::var("SOCKS5_PROXY")
        .ok()
        .or_else(|| env::var("socks5_proxy").ok());
    let no_proxy = env::var("NO_PROXY")
        .ok()
        .or_else(|| env::var("no_proxy").ok())
        .map(|s| s.split(',').map(|s| s.trim().to_string()).collect());

    if http.is_some() || https.is_some() || socks5.is_some() {
        Some(ccr_types::ProxyConfig {
            http,
            https,
            socks5,
            no_proxy,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::{NamedTempFile, TempDir};

    fn write_loadable_config(path: &Path, route: &str) {
        fs::write(
            path,
            format!(
                r#"{{ "Providers": [], "Router": {{}}, "RoutePool": {{ "enabled": true, "candidates": [{{ "route": "{route}" }}] }} }}"#
            ),
        )
        .unwrap();
    }

    fn set_backup_dir(path: &Path) {
        unsafe { std::env::set_var("CCR_BACKUP_DIR", path) };
    }

    fn clear_backup_dir() {
        unsafe { std::env::remove_var("CCR_BACKUP_DIR") };
    }

    #[test]
    fn interpolate_dollar_brace() {
        unsafe { std::env::set_var("TEST_KEY_CCR", "hello") };
        let result = interpolate_env("value is ${TEST_KEY_CCR}");
        assert_eq!(result, "value is hello");
    }

    #[test]
    fn interpolate_dollar_plain() {
        unsafe { std::env::set_var("TEST_KEY_CCR2", "world") };
        let result = interpolate_env("value is $TEST_KEY_CCR2");
        assert_eq!(result, "value is world");
    }

    #[test]
    fn interpolate_missing_var_unchanged() {
        let result = interpolate_env("${DEFINITELY_NOT_SET_XYZ}");
        assert_eq!(result, "${DEFINITELY_NOT_SET_XYZ}");
    }

    #[test]
    fn load_config_parses_json5() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, r#"{{ "Providers": [], "Router": {{}} }}"#).unwrap();
        let config = load_config(f.path()).unwrap();
        assert!(config.providers.is_empty());
    }

    #[test]
    fn save_and_reload_config() {
        let _guard = ENV_LOCK.lock().unwrap();
        let backup_dir = TempDir::new().unwrap();
        set_backup_dir(backup_dir.path());

        let mut f = NamedTempFile::new().unwrap();
        write!(
            f,
            r#"{{ "Providers": [], "Router": {{}}, "RoutePool": {{ "enabled": true, "candidates": [{{ "route": "p" }}] }} }}"#
        )
        .unwrap();
        let mut config = load_config(f.path()).unwrap();
        config.route_pool.as_mut().unwrap().candidates[0].route = "new,model".to_string();
        save_config(&config, f.path()).unwrap();
        let reloaded = load_config(f.path()).unwrap();
        assert_eq!(reloaded.first_route_pool_route(), Some("new,model"));
        clear_backup_dir();
    }

    #[test]
    fn save_preserves_unknown_top_level_fields() {
        let _guard = ENV_LOCK.lock().unwrap();
        let backup_dir = TempDir::new().unwrap();
        set_backup_dir(backup_dir.path());

        let mut f = NamedTempFile::new().unwrap();
        write!(
            f,
            r#"{{ "Providers": [], "Router": {{}}, "RoutePool": {{ "enabled": true, "candidates": [{{ "route": "p" }}] }}, "Unknown": {{ "keep": true }} }}"#
        )
        .unwrap();
        let mut config = load_config(f.path()).unwrap();
        config.route_pool.as_mut().unwrap().candidates[0].route = "new,model".to_string();
        save_config(&config, f.path()).unwrap();
        let raw = std::fs::read_to_string(f.path()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["Unknown"]["keep"], true);
        assert_eq!(value["RoutePool"]["candidates"][0]["route"], "new,model");
        clear_backup_dir();
    }

    #[test]
    fn save_new_config_uses_atomic_path_without_backup() {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        set_backup_dir(&temp_dir.path().join("backups"));
        let config_path = temp_dir.path().join("config.json");

        let config = Config::default();
        save_config(&config, &config_path).unwrap();

        assert!(config_path.exists());
        assert!(!temp_dir.path().join("backups").exists());
        load_config(&config_path).unwrap();
        clear_backup_dir();
    }

    #[test]
    fn backup_failure_aborts_save_and_preserves_original() {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");
        write_loadable_config(&config_path, "original");
        let backup_path_that_is_file = temp_dir.path().join("not-a-dir");
        fs::write(&backup_path_that_is_file, "blocking backup dir").unwrap();
        set_backup_dir(&backup_path_that_is_file);

        let mut config = load_config(&config_path).unwrap();
        config.route_pool.as_mut().unwrap().candidates[0].route = "changed".to_string();

        let err = save_config(&config, &config_path).unwrap_err();
        assert!(err.to_string().contains("Failed to create backup"));
        assert!(
            fs::read_to_string(&config_path)
                .unwrap()
                .contains("original")
        );
        clear_backup_dir();
    }

    #[cfg(unix)]
    #[test]
    fn pre_rename_temp_creation_failure_preserves_original() {
        use std::os::unix::fs::PermissionsExt;

        let _guard = ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("config-dir");
        fs::create_dir(&config_dir).unwrap();
        let config_path = config_dir.join("config.json");
        write_loadable_config(&config_path, "original");
        set_backup_dir(&temp_dir.path().join("backups"));

        let mut config = load_config(&config_path).unwrap();
        config.route_pool.as_mut().unwrap().candidates[0].route = "changed".to_string();

        let original_permissions = fs::metadata(&config_dir).unwrap().permissions();
        fs::set_permissions(&config_dir, fs::Permissions::from_mode(0o500)).unwrap();
        let result = save_config(&config, &config_path);
        fs::set_permissions(&config_dir, original_permissions).unwrap();

        assert!(result.is_err());
        assert!(
            fs::read_to_string(&config_path)
                .unwrap()
                .contains("original")
        );
        clear_backup_dir();
    }
}
