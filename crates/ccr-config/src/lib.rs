pub mod backup;
pub mod reloadable;

use anyhow::{Context, Result};
use ccr_types::Config;
use std::env;
use std::path::{Path, PathBuf};
use tracing::warn;

pub use reloadable::ReloadableConfig;

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
        match backup::create_backup(path) {
            Ok(backup_path) => {
                warn!("Created backup: {}", backup_path.display());
            }
            Err(e) => {
                warn!("Failed to create backup: {}", e);
                // Continue saving even if backup fails
            }
        }
    }

    let json_value = merge_with_existing_unknown_fields(config, path)?;
    let json = serde_json::to_string_pretty(&json_value)?;
    std::fs::write(path, json)?;
    Ok(())
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
    use std::io::Write;
    use tempfile::NamedTempFile;

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
    }

    #[test]
    fn save_preserves_unknown_top_level_fields() {
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
    }
}
