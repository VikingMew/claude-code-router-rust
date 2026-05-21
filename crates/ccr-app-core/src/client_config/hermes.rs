use anyhow::{Context, Result};
use ccr_config::{default_config_path, load_config};
use serde_yaml::{Mapping, Value};
use std::fs;
use std::path::{Path, PathBuf};

const CCR_PROVIDER_NAME: &str = "ccr";
const CCR_MODEL_PROVIDER: &str = "custom:ccr";
const DEFAULT_MODEL: &str = "gpt-5-codex";

pub fn hermes_config_path() -> PathBuf {
    configured_hermes_path().unwrap_or_else(default_hermes_config_path)
}

pub fn hermes_config_path_from_option(configured_path: Option<String>) -> PathBuf {
    configured_path
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_hermes_config_path)
}

fn configured_hermes_path() -> Option<PathBuf> {
    load_config(&default_config_path())
        .ok()
        .and_then(|config| config.app_settings.hermes_config_path)
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
}

pub fn default_hermes_config_path() -> PathBuf {
    dirs_next::home_dir()
        .expect("Cannot determine home directory")
        .join(".hermes")
        .join("config.yaml")
}

pub fn activate_hermes_ccr() -> Result<()> {
    let config = load_config(&default_config_path()).context("Failed to load CCR config")?;
    let port = config.port.unwrap_or(3456);
    let route = config
        .first_route_pool_route()
        .ok_or_else(|| anyhow::anyhow!("Route Pool is not configured or has no enabled routes"))?;
    let model = client_model_from_route(route);
    install_hermes_ccr(&hermes_config_path(), port, &model)
}

pub fn deactivate_hermes_ccr() -> Result<()> {
    remove_hermes_ccr(&hermes_config_path())
}

pub fn hermes_provider_present(port: u16) -> bool {
    hermes_points_to_ccr(&hermes_config_path(), port)
}

pub fn hermes_provider_exists() -> bool {
    hermes_has_ccr_provider(&hermes_config_path())
}

pub fn hermes_points_to_ccr(path: &Path, port: u16) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(config) = parse_hermes_yaml(&content) else {
        return false;
    };
    ccr_provider(&config)
        .and_then(|provider| provider.get(Value::String("base_url".to_string())))
        .and_then(Value::as_str)
        == Some(format!("http://127.0.0.1:{port}/v1").as_str())
}

fn hermes_has_ccr_provider(path: &Path) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(config) = parse_hermes_yaml(&content) else {
        return false;
    };
    ccr_provider(&config).is_some()
}

fn install_hermes_ccr(path: &Path, port: u16, model: &str) -> Result<()> {
    let original = fs::read_to_string(path).unwrap_or_default();
    let config = build_hermes_config(&original, port, model)?;
    atomic_write_yaml(path, &config)
}

fn remove_hermes_ccr(path: &Path) -> Result<()> {
    let original = fs::read_to_string(path).unwrap_or_default();
    let config = remove_hermes_ccr_provider(&original)?;
    atomic_write_yaml(path, &config)
}

pub fn build_hermes_config(original_yaml: &str, port: u16, model: &str) -> Result<Value> {
    let mut config = parse_hermes_yaml(original_yaml)?;
    let root = config
        .as_mapping_mut()
        .expect("parse_hermes_yaml returns a mapping root");

    upsert_ccr_custom_provider(root, port)?;
    let model_map = ensure_mapping_field(root, "model")?;
    model_map.insert(
        Value::String("provider".to_string()),
        Value::String(CCR_MODEL_PROVIDER.to_string()),
    );
    model_map.insert(
        Value::String("default".to_string()),
        Value::String(model.to_string()),
    );

    Ok(config)
}

pub fn remove_hermes_ccr_provider(original_yaml: &str) -> Result<Value> {
    let mut config = parse_hermes_yaml(original_yaml)?;
    let root = config
        .as_mapping_mut()
        .expect("parse_hermes_yaml returns a mapping root");

    if let Some(providers) = root
        .get_mut(Value::String("custom_providers".to_string()))
        .and_then(Value::as_sequence_mut)
    {
        providers.retain(|provider| provider_name(provider) != Some(CCR_PROVIDER_NAME));
    }

    if let Some(model) = root
        .get_mut(Value::String("model".to_string()))
        .and_then(Value::as_mapping_mut)
    {
        let provider_is_ccr = model
            .get(Value::String("provider".to_string()))
            .and_then(Value::as_str)
            == Some(CCR_MODEL_PROVIDER);
        if provider_is_ccr {
            model.remove(Value::String("provider".to_string()));
            model.remove(Value::String("default".to_string()));
            model.remove(Value::String("model".to_string()));
        }
    }

    Ok(config)
}

fn parse_hermes_yaml(original_yaml: &str) -> Result<Value> {
    if original_yaml.trim().is_empty() {
        return Ok(Value::Mapping(Mapping::new()));
    }
    let value: Value =
        serde_yaml::from_str(original_yaml).context("Hermes config file is invalid YAML")?;
    match value {
        Value::Mapping(_) => Ok(value),
        _ => anyhow::bail!("Hermes config root must be a YAML mapping"),
    }
}

fn upsert_ccr_custom_provider(root: &mut Mapping, port: u16) -> Result<()> {
    let key = Value::String("custom_providers".to_string());
    if !root.contains_key(&key) {
        root.insert(key.clone(), Value::Sequence(Vec::new()));
    }
    let providers = root
        .get_mut(&key)
        .and_then(Value::as_sequence_mut)
        .ok_or_else(|| anyhow::anyhow!("Hermes `custom_providers` must be a YAML sequence"))?;

    providers.retain(|provider| provider_name(provider) != Some(CCR_PROVIDER_NAME));
    providers.push(Value::Mapping(ccr_provider_mapping(port)));
    Ok(())
}

fn ccr_provider_mapping(port: u16) -> Mapping {
    let mut provider = Mapping::new();
    provider.insert(
        Value::String("name".to_string()),
        Value::String(CCR_PROVIDER_NAME.to_string()),
    );
    provider.insert(
        Value::String("base_url".to_string()),
        Value::String(format!("http://127.0.0.1:{port}/v1")),
    );
    provider.insert(
        Value::String("api_mode".to_string()),
        Value::String("chat_completions".to_string()),
    );
    provider
}

fn ensure_mapping_field<'a>(root: &'a mut Mapping, field: &str) -> Result<&'a mut Mapping> {
    let key = Value::String(field.to_string());
    if !root.contains_key(&key) {
        root.insert(key.clone(), Value::Mapping(Mapping::new()));
    }
    root.get_mut(&key)
        .and_then(Value::as_mapping_mut)
        .ok_or_else(|| anyhow::anyhow!("Hermes `{field}` must be a YAML mapping"))
}

fn ccr_provider(config: &Value) -> Option<&Mapping> {
    config
        .as_mapping()?
        .get(Value::String("custom_providers".to_string()))?
        .as_sequence()?
        .iter()
        .find(|provider| provider_name(provider) == Some(CCR_PROVIDER_NAME))?
        .as_mapping()
}

fn provider_name(provider: &Value) -> Option<&str> {
    provider
        .as_mapping()?
        .get(Value::String("name".to_string()))?
        .as_str()
}

fn atomic_write_yaml(path: &Path, config: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("Failed to create Hermes config directory")?;
    }
    let temp_path = path.with_extension("yaml.tmp");
    fs::write(&temp_path, serde_yaml::to_string(config)?)
        .context("Failed to write temporary Hermes config")?;
    set_secure_permissions(&temp_path)?;
    fs::rename(&temp_path, path).context("Failed to install Hermes CCR provider")?;
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
    let model = route.split_once(',').map(|(_, model)| model.trim());
    match model {
        Some(model) if !model.is_empty() => model.to_string(),
        _ => DEFAULT_MODEL.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_creates_keyless_custom_provider_from_empty_config() {
        let config = build_hermes_config("", 3456, "gpt-5-codex").unwrap();
        let provider = ccr_provider(&config).unwrap();
        assert_eq!(
            provider
                .get(Value::String("base_url".to_string()))
                .and_then(Value::as_str),
            Some("http://127.0.0.1:3456/v1")
        );
        assert_eq!(
            provider
                .get(Value::String("api_mode".to_string()))
                .and_then(Value::as_str),
            Some("chat_completions")
        );
        assert!(provider.get(Value::String("api_key".to_string())).is_none());
        assert_eq!(
            config["model"]["provider"].as_str(),
            Some(CCR_MODEL_PROVIDER)
        );
        assert_eq!(config["model"]["default"].as_str(), Some("gpt-5-codex"));
    }

    #[test]
    fn build_preserves_unrelated_fields_and_providers() {
        let original = r#"
terminal:
  backend: docker
profiles:
  work:
    model: other
custom_providers:
  - name: other
    base_url: https://example.com/v1
    key_env: OTHER_API_KEY
model:
  provider: custom:other
  default: other-model
"#;
        let config = build_hermes_config(original, 4567, "model-a").unwrap();
        assert_eq!(config["terminal"]["backend"].as_str(), Some("docker"));
        assert_eq!(config["profiles"]["work"]["model"].as_str(), Some("other"));
        assert_eq!(
            config["custom_providers"][0]["name"].as_str(),
            Some("other")
        );
        assert_eq!(config["model"]["provider"].as_str(), Some("custom:ccr"));
        assert_eq!(config["model"]["default"].as_str(), Some("model-a"));
    }

    #[test]
    fn build_replaces_existing_ccr_provider_only() {
        let original = r#"
custom_providers:
  - name: ccr
    base_url: http://127.0.0.1:1111/v1
    api_key: old
  - name: other
    base_url: https://example.com/v1
"#;
        let config = build_hermes_config(original, 4567, "model-a").unwrap();
        let providers = config["custom_providers"].as_sequence().unwrap();
        assert_eq!(providers.len(), 2);
        assert_eq!(providers[0]["name"].as_str(), Some("other"));
        assert_eq!(providers[1]["name"].as_str(), Some("ccr"));
        assert!(providers[1]["api_key"].is_null());
        assert_eq!(
            providers[1]["base_url"].as_str(),
            Some("http://127.0.0.1:4567/v1")
        );
    }

    #[test]
    fn build_rejects_invalid_custom_providers() {
        let error = build_hermes_config("custom_providers: {}", 3456, "m").unwrap_err();
        assert!(error.to_string().contains("custom_providers"));
    }

    #[test]
    fn build_rejects_invalid_yaml() {
        let error = build_hermes_config("custom_providers: [", 3456, "m").unwrap_err();
        assert!(error.to_string().contains("invalid YAML"));
    }

    #[test]
    fn points_to_ccr_detects_current_and_drifted_provider() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        fs::write(
            &path,
            r#"
custom_providers:
  - name: ccr
    base_url: http://127.0.0.1:3456/v1
"#,
        )
        .unwrap();
        assert!(hermes_points_to_ccr(&path, 3456));
        assert!(!hermes_points_to_ccr(&path, 4567));
    }

    #[test]
    fn remove_deletes_ccr_provider_and_ccr_model_selection_only() {
        let original = r#"
custom_providers:
  - name: ccr
    base_url: http://127.0.0.1:3456/v1
  - name: other
    base_url: https://example.com/v1
model:
  provider: custom:ccr
  default: model-a
  temperature: 0.2
tools:
  shell: true
"#;
        let config = remove_hermes_ccr_provider(original).unwrap();
        assert_eq!(
            config["custom_providers"][0]["name"].as_str(),
            Some("other")
        );
        assert!(config["model"]["provider"].is_null());
        assert!(config["model"]["default"].is_null());
        assert_eq!(config["model"]["temperature"].as_f64(), Some(0.2));
        assert_eq!(config["tools"]["shell"].as_bool(), Some(true));
    }

    #[test]
    fn remove_preserves_external_model_selection() {
        let original = r#"
custom_providers:
  - name: ccr
    base_url: http://127.0.0.1:3456/v1
model:
  provider: custom:other
  default: other-model
"#;
        let config = remove_hermes_ccr_provider(original).unwrap();
        assert!(config["custom_providers"].as_sequence().unwrap().is_empty());
        assert_eq!(config["model"]["provider"].as_str(), Some("custom:other"));
        assert_eq!(config["model"]["default"].as_str(), Some("other-model"));
    }

    #[test]
    fn client_model_uses_model_part_only() {
        assert_eq!(client_model_from_route("openai,gpt-5-codex"), "gpt-5-codex");
        assert_eq!(
            client_model_from_route("anthropic,claude-sonnet-4-6"),
            "claude-sonnet-4-6"
        );
        assert_eq!(client_model_from_route("openai"), DEFAULT_MODEL);
        assert_eq!(client_model_from_route("openai,"), DEFAULT_MODEL);
    }

    #[test]
    fn config_path_uses_override_or_default() {
        assert_eq!(
            hermes_config_path_from_option(Some("/tmp/hermes.yaml".to_string())),
            PathBuf::from("/tmp/hermes.yaml")
        );
        assert!(
            hermes_config_path_from_option(Some("  ".to_string())).ends_with(".hermes/config.yaml")
        );
        assert!(hermes_config_path_from_option(None).ends_with(".hermes/config.yaml"));
    }
}
