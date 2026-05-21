use anyhow::{Context, Result};
use ccr_config::{default_config_path, load_config};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

const OPENCODE_SCHEMA: &str = "https://opencode.ai/config.json";
const DEFAULT_MODEL: &str = "gpt-5-codex";

pub fn opencode_config_path() -> PathBuf {
    configured_opencode_path().unwrap_or_else(default_opencode_config_path)
}

fn configured_opencode_path() -> Option<PathBuf> {
    load_config(&default_config_path())
        .ok()
        .and_then(|config| config.app_settings.opencode_config_path)
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
}

fn default_opencode_config_path() -> PathBuf {
    dirs_next::home_dir()
        .expect("Cannot determine home directory")
        .join(".config")
        .join("opencode")
        .join("opencode.json")
}

pub fn activate_opencode_ccr() -> Result<()> {
    let config = load_config(&default_config_path()).context("Failed to load CCR config")?;
    let port = config.port.unwrap_or(3456);
    let route = config
        .first_route_pool_route()
        .ok_or_else(|| anyhow::anyhow!("Route Pool is not configured or has no enabled routes"))?;
    let model = client_model_from_route(route);
    install_opencode_ccr(&opencode_config_path(), port, &model)
}

pub fn deactivate_opencode_ccr() -> Result<()> {
    remove_opencode_ccr(&opencode_config_path())
}

pub fn opencode_provider_present(port: u16) -> bool {
    opencode_points_to_ccr(&opencode_config_path(), port)
}

pub fn opencode_provider_exists() -> bool {
    opencode_has_ccr_provider(&opencode_config_path())
}

pub fn opencode_points_to_ccr(path: &Path, port: u16) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(config) = serde_json::from_str::<Value>(&content) else {
        return false;
    };
    config
        .get("provider")
        .and_then(|provider| provider.get("ccr"))
        .and_then(|ccr| ccr.get("options"))
        .and_then(|options| options.get("baseURL"))
        .and_then(Value::as_str)
        == Some(format!("http://127.0.0.1:{port}/v1").as_str())
}

fn opencode_has_ccr_provider(path: &Path) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(config) = serde_json::from_str::<Value>(&content) else {
        return false;
    };
    config
        .get("provider")
        .and_then(|provider| provider.get("ccr"))
        .is_some()
}

fn install_opencode_ccr(path: &Path, port: u16, model: &str) -> Result<()> {
    let original = fs::read_to_string(path).unwrap_or_default();
    let config = build_opencode_config(&original, port, model)?;
    atomic_write_json(path, &config)
}

fn remove_opencode_ccr(path: &Path) -> Result<()> {
    let original = fs::read_to_string(path).unwrap_or_default();
    let config = remove_opencode_ccr_provider(&original)?;
    atomic_write_json(path, &config)
}

pub fn build_opencode_config(original_json: &str, port: u16, model: &str) -> Result<Value> {
    let mut config = parse_opencode_json(original_json)?;
    if config.get("$schema").is_none() {
        config["$schema"] = json!(OPENCODE_SCHEMA);
    }
    ensure_object_field(&mut config, "provider")?;

    config["provider"]["ccr"] = json!({
        "npm": "@ai-sdk/openai-compatible",
        "name": "CCR",
        "options": {
            "baseURL": format!("http://127.0.0.1:{port}/v1"),
            "apiKey": "any"
        },
        "models": {
            model: {
                "name": model
            }
        }
    });
    Ok(config)
}

pub fn remove_opencode_ccr_provider(original_json: &str) -> Result<Value> {
    let mut config = parse_opencode_json(original_json)?;
    if let Some(provider) = config.get_mut("provider").and_then(Value::as_object_mut) {
        provider.remove("ccr");
    }
    Ok(config)
}

fn parse_opencode_json(original_json: &str) -> Result<Value> {
    if original_json.trim().is_empty() {
        return Ok(json!({ "$schema": OPENCODE_SCHEMA }));
    }
    let value: Value =
        serde_json::from_str(original_json).context("OpenCode config file is invalid JSON")?;
    match value {
        Value::Object(_) => Ok(value),
        _ => anyhow::bail!("OpenCode config root must be a JSON object"),
    }
}

fn ensure_object_field(config: &mut Value, field: &str) -> Result<()> {
    match config.get(field) {
        Some(Value::Object(_)) => Ok(()),
        Some(_) => anyhow::bail!("OpenCode `{field}` must be a JSON object"),
        None => {
            config[field] = json!({});
            Ok(())
        }
    }
}

fn atomic_write_json(path: &Path, config: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create OpenCode config directory")?;
    }
    let temp_path = path.with_extension("json.tmp");
    fs::write(&temp_path, serde_json::to_string_pretty(config)?)
        .context("Failed to write temporary OpenCode config")?;
    set_secure_permissions(&temp_path)?;
    fs::rename(&temp_path, path).context("Failed to install OpenCode CCR provider")?;
    Ok(())
}

#[cfg(unix)]
fn set_secure_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_secure_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

fn client_model_from_route(route: &str) -> String {
    let route = route.trim();
    if route.is_empty() {
        return DEFAULT_MODEL.to_string();
    }
    route
        .split_once(',')
        .map(|split| split.1)
        .unwrap_or(DEFAULT_MODEL)
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_creates_provider_from_empty_config() {
        let config = build_opencode_config("", 3456, "gpt-5-codex").unwrap();
        assert_eq!(config["$schema"].as_str(), Some(OPENCODE_SCHEMA));
        assert_eq!(
            config["provider"]["ccr"]["options"]["baseURL"].as_str(),
            Some("http://127.0.0.1:3456/v1")
        );
        assert_eq!(
            config["provider"]["ccr"]["options"]["apiKey"].as_str(),
            Some("any")
        );
        assert!(config["provider"]["ccr"]["models"]["gpt-5-codex"].is_object());
    }

    #[test]
    fn build_preserves_other_fields_and_providers() {
        let original = r#"{
          "mcp": {"x": true},
          "provider": {
            "other": {"name": "Other"}
          }
        }"#;
        let config = build_opencode_config(original, 4567, "model-a").unwrap();
        assert_eq!(config["mcp"]["x"].as_bool(), Some(true));
        assert_eq!(config["provider"]["other"]["name"].as_str(), Some("Other"));
        assert_eq!(
            config["provider"]["ccr"]["options"]["baseURL"].as_str(),
            Some("http://127.0.0.1:4567/v1")
        );
    }

    #[test]
    fn build_rejects_non_object_provider() {
        let error = build_opencode_config(r#"{"provider": []}"#, 3456, "m").unwrap_err();
        assert!(error.to_string().contains("provider"));
    }

    #[test]
    fn remove_only_deletes_ccr_provider() {
        let original = r#"{"provider":{"ccr":{"name":"CCR"},"other":{"name":"Other"}}}"#;
        let config = remove_opencode_ccr_provider(original).unwrap();
        assert!(config["provider"]["ccr"].is_null());
        assert_eq!(config["provider"]["other"]["name"].as_str(), Some("Other"));
    }

    #[test]
    fn client_model_uses_model_part_only() {
        assert_eq!(client_model_from_route("openai,gpt-5-codex"), "gpt-5-codex");
        assert_eq!(client_model_from_route("openai"), DEFAULT_MODEL);
        assert_eq!(client_model_from_route(""), DEFAULT_MODEL);
    }
}
