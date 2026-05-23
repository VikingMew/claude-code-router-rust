use super::common::{
    additive_client_snapshot, atomic_write, ccr_config, ccr_port, configured_path_from_settings,
    first_route_pool_client_model, local_v1_base_url,
};
use crate::status::AdditiveClientSnapshot;
use anyhow::{Context, Result};
use ccr_types::Config;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_MODEL: &str = "gpt-5-codex";

pub fn openclaw_config_path() -> PathBuf {
    configured_path_from_settings(
        |settings| settings.openclaw_config_path.clone(),
        default_openclaw_config_path,
    )
}

fn default_openclaw_config_path() -> PathBuf {
    dirs_next::home_dir()
        .expect("Cannot determine home directory")
        .join(".openclaw")
        .join("openclaw.json")
}

pub fn activate_openclaw_ccr() -> Result<()> {
    let config = ccr_config()?;
    activate_openclaw_ccr_with_config(&config, &openclaw_config_path())
}

fn activate_openclaw_ccr_with_config(config: &Config, path: &Path) -> Result<()> {
    let port = ccr_port(config);
    let model = first_route_pool_client_model(config, DEFAULT_MODEL)?;
    install_openclaw_ccr(path, port, &model)
}

pub fn deactivate_openclaw_ccr() -> Result<()> {
    remove_openclaw_ccr(&openclaw_config_path())
}

pub fn openclaw_provider_present(port: u16) -> bool {
    openclaw_points_to_ccr(&openclaw_config_path(), port)
}

pub fn openclaw_provider_exists() -> bool {
    openclaw_has_ccr_provider(&openclaw_config_path())
}

pub fn openclaw_snapshot(port: u16) -> AdditiveClientSnapshot {
    additive_client_snapshot(
        openclaw_config_path(),
        port,
        openclaw_points_to_ccr,
        openclaw_has_ccr_provider,
    )
}

pub fn openclaw_points_to_ccr(path: &Path, port: u16) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(config) = parse_openclaw_json5(&content) else {
        return false;
    };
    config
        .get("models")
        .and_then(|models| models.get("providers"))
        .and_then(|providers| providers.get("ccr"))
        .and_then(|ccr| ccr.get("baseUrl"))
        .and_then(Value::as_str)
        == Some(local_v1_base_url(port).as_str())
}

pub(super) fn openclaw_has_ccr_provider(path: &Path) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(config) = parse_openclaw_json5(&content) else {
        return false;
    };
    config
        .get("models")
        .and_then(|models| models.get("providers"))
        .and_then(|providers| providers.get("ccr"))
        .is_some()
}

fn install_openclaw_ccr(path: &Path, port: u16, model: &str) -> Result<()> {
    let original = fs::read_to_string(path).unwrap_or_default();
    let config = build_openclaw_config(&original, port, model)?;
    atomic_write_json(path, &config)
}

fn remove_openclaw_ccr(path: &Path) -> Result<()> {
    let original = fs::read_to_string(path).unwrap_or_default();
    let config = remove_openclaw_ccr_provider(&original)?;
    atomic_write_json(path, &config)
}

pub fn build_openclaw_config(original_json5: &str, port: u16, model: &str) -> Result<Value> {
    let mut config = parse_openclaw_json5(original_json5)?;
    ensure_object_field(&mut config, "models")?;
    config["models"]["mode"] = json!("merge");
    ensure_nested_object_field(&mut config, &["models"], "providers")?;

    config["models"]["providers"]["ccr"] = json!({
        "baseUrl": local_v1_base_url(port),
        "apiKey": "any",
        "api": "openai-responses",
        "models": [
            {
                "id": model,
                "name": model
            }
        ]
    });

    ensure_object_field(&mut config, "agents")?;
    ensure_nested_object_field(&mut config, &["agents"], "defaults")?;
    ensure_nested_object_field(&mut config, &["agents", "defaults"], "models")?;
    let model_key = format!("ccr/{model}");
    config["agents"]["defaults"]["models"][&model_key] = json!({ "alias": "CCR" });
    if config["agents"]["defaults"]
        .get("model")
        .and_then(|model| model.get("primary"))
        .and_then(Value::as_str)
        .is_none()
    {
        config["agents"]["defaults"]["model"] = json!({
            "primary": model_key,
            "fallbacks": []
        });
    }

    Ok(config)
}

pub fn remove_openclaw_ccr_provider(original_json5: &str) -> Result<Value> {
    let mut config = parse_openclaw_json5(original_json5)?;
    if let Some(providers) = config
        .get_mut("models")
        .and_then(|models| models.get_mut("providers"))
        .and_then(Value::as_object_mut)
    {
        providers.remove("ccr");
    }
    if let Some(default_model) = config
        .get_mut("agents")
        .and_then(|agents| agents.get_mut("defaults"))
        .and_then(|defaults| defaults.get_mut("model"))
        .and_then(Value::as_object_mut)
        && default_model
            .get("primary")
            .and_then(Value::as_str)
            .is_some_and(|primary| primary.starts_with("ccr/"))
    {
        default_model.remove("primary");
    }
    Ok(config)
}

fn parse_openclaw_json5(original_json5: &str) -> Result<Value> {
    if original_json5.trim().is_empty() {
        return Ok(json!({
            "models": {
                "mode": "merge",
                "providers": {}
            }
        }));
    }
    let value: Value =
        json5::from_str(original_json5).context("OpenClaw config file is invalid JSON5")?;
    match value {
        Value::Object(_) => Ok(value),
        _ => anyhow::bail!("OpenClaw config root must be an object"),
    }
}

fn ensure_object_field(config: &mut Value, field: &str) -> Result<()> {
    match config.get(field) {
        Some(Value::Object(_)) => Ok(()),
        Some(_) => anyhow::bail!("OpenClaw `{field}` must be an object"),
        None => {
            config[field] = json!({});
            Ok(())
        }
    }
}

fn ensure_nested_object_field(config: &mut Value, path: &[&str], field: &str) -> Result<()> {
    let mut current = config;
    for segment in path {
        current = current
            .get_mut(*segment)
            .ok_or_else(|| anyhow::anyhow!("OpenClaw `{segment}` is missing"))?;
        if !current.is_object() {
            anyhow::bail!("OpenClaw `{segment}` must be an object");
        }
    }
    match current.get(field) {
        Some(Value::Object(_)) => Ok(()),
        Some(_) => anyhow::bail!("OpenClaw `{field}` must be an object"),
        None => {
            current[field] = json!({});
            Ok(())
        }
    }
}

fn atomic_write_json(path: &Path, config: &Value) -> Result<()> {
    atomic_write(
        path,
        "json.tmp",
        serde_json::to_string_pretty(config)?,
        "Failed to create OpenClaw config directory",
        "Failed to write temporary OpenClaw config",
        "Failed to install OpenClaw CCR provider",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccr_types::{RoutePoolCandidate, RoutePoolConfig};

    fn disabled_route_pool_config() -> Config {
        Config {
            route_pool: Some(RoutePoolConfig {
                enabled: false,
                candidates: vec![RoutePoolCandidate {
                    route: "openai,gpt-4o".to_string(),
                    enabled: true,
                    priority: 0,
                }],
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn build_creates_provider_from_empty_config() {
        let config = build_openclaw_config("", 3456, "gpt-5-codex").unwrap();
        assert_eq!(config["models"]["mode"].as_str(), Some("merge"));
        assert_eq!(
            config["models"]["providers"]["ccr"]["baseUrl"].as_str(),
            Some("http://127.0.0.1:3456/v1")
        );
        assert_eq!(
            config["models"]["providers"]["ccr"]["api"].as_str(),
            Some("openai-responses")
        );
        assert_eq!(
            config["agents"]["defaults"]["model"]["primary"].as_str(),
            Some("ccr/gpt-5-codex")
        );
    }

    #[test]
    fn build_reads_json5_and_preserves_unknown_fields() {
        let original = r#"{
          // json5 comment
          env: { KEEP: "1", },
          tools: { shell: true },
          models: {
            providers: {
              other: { baseUrl: "https://example.com" },
            },
          },
          agents: {
            defaults: {
              model: { primary: "other/model", fallbacks: [] },
            },
          },
        }"#;
        let config = build_openclaw_config(original, 4567, "model-a").unwrap();
        assert_eq!(config["env"]["KEEP"].as_str(), Some("1"));
        assert_eq!(config["tools"]["shell"].as_bool(), Some(true));
        assert_eq!(
            config["models"]["providers"]["other"]["baseUrl"].as_str(),
            Some("https://example.com")
        );
        assert_eq!(
            config["agents"]["defaults"]["model"]["primary"].as_str(),
            Some("other/model")
        );
        assert!(config["agents"]["defaults"]["models"]["ccr/model-a"].is_object());
    }

    #[test]
    fn remove_deletes_ccr_and_clears_ccr_primary_only() {
        let original = r#"{
          models: { providers: { ccr: {}, other: {} } },
          agents: { defaults: { model: { primary: "ccr/model-a", fallbacks: [] } } }
        }"#;
        let config = remove_openclaw_ccr_provider(original).unwrap();
        assert!(config["models"]["providers"]["ccr"].is_null());
        assert!(config["models"]["providers"]["other"].is_object());
        assert!(config["agents"]["defaults"]["model"]["primary"].is_null());
    }

    #[test]
    fn remove_preserves_external_primary() {
        let original = r#"{
          models: { providers: { ccr: {}, other: {} } },
          agents: { defaults: { model: { primary: "other/model", fallbacks: [] } } }
        }"#;
        let config = remove_openclaw_ccr_provider(original).unwrap();
        assert_eq!(
            config["agents"]["defaults"]["model"]["primary"].as_str(),
            Some("other/model")
        );
    }

    #[test]
    fn activate_fails_before_writing_when_route_pool_has_no_active_route() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("openclaw.json");
        let original = r#"{ models: { providers: { other: {} } } }"#;
        fs::write(&path, original).unwrap();

        let error = activate_openclaw_ccr_with_config(&disabled_route_pool_config(), &path)
            .unwrap_err()
            .to_string();

        assert!(error.contains("Route Pool is not configured or has no enabled routes"));
        assert_eq!(fs::read_to_string(path).unwrap(), original);
    }
}
