use anyhow::{Context, Result};
use ccr_types::{
    Config, Provider, ProviderApiKind, ProviderApiKindSource, RoutePoolCandidate, RoutePoolConfig,
    TransformerConfig,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaField {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default = "default_field_type")]
    pub r#type: String,
    #[serde(default)]
    pub prompt: String,
}

fn default_field_type() -> String {
    "text".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "Providers", default)]
    pub providers: Vec<serde_json::Value>,
    #[serde(rename = "RoutePool", default, skip_serializing_if = "Option::is_none")]
    pub route_pool: Option<RoutePoolConfig>,
    #[serde(default)]
    pub schema: Vec<SchemaField>,
    #[serde(default, rename = "requiredEnv")]
    pub required_env: Vec<String>,
    #[serde(default)]
    pub experimental: bool,
}

fn default_version() -> String {
    "1.0.0".into()
}

pub fn presets_dir() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude-code-router")
        .join("presets")
}

/// Sanitize sensitive fields: replace api_key values with {{api_key}}
fn sanitize_providers(providers: &[Provider]) -> Vec<serde_json::Value> {
    providers
        .iter()
        .map(|p| {
            let mut v = serde_json::to_value(p).unwrap();
            if let Some(k) = v.get_mut("api_key") {
                *k = serde_json::Value::String("{{api_key}}".into());
            }
            v
        })
        .collect()
}

pub fn export_preset(name: &str, config: &Config) -> Result<PathBuf> {
    let dir = presets_dir().join(name);
    std::fs::create_dir_all(&dir)?;
    let manifest = Manifest {
        name: name.to_string(),
        version: "1.0.0".into(),
        description: String::new(),
        providers: sanitize_providers(&config.providers),
        route_pool: config.route_pool.clone(),
        schema: vec![],
        required_env: vec![],
        experimental: false,
    };
    let path = dir.join("manifest.json");
    std::fs::write(&path, serde_json::to_string_pretty(&manifest)?)?;
    Ok(path)
}

pub fn load_preset(source: &str) -> Result<Manifest> {
    let path = PathBuf::from(source).join("manifest.json");
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("Cannot read {}", path.display()))?;
    serde_json::from_str(&raw).context("Invalid manifest.json")
}

pub fn install_preset(manifest: &Manifest, config: &mut Config) -> Result<Vec<String>> {
    install_preset_with_options(manifest, config, false)
}

pub fn install_preset_with_options(
    manifest: &Manifest,
    config: &mut Config,
    overwrite_existing: bool,
) -> Result<Vec<String>> {
    let mut added = Vec::new();
    for pval in &manifest.providers {
        let p: Provider = serde_json::from_value(pval.clone())?;
        if let Some(existing) = config
            .providers
            .iter_mut()
            .find(|candidate| candidate.name == p.name)
        {
            if overwrite_existing {
                *existing = p.clone();
                added.push(p.name);
            }
        } else {
            added.push(p.name.clone());
            config.providers.push(p);
        }
    }
    if let Some(route_pool) = &manifest.route_pool {
        config.route_pool = Some(route_pool.clone());
    }
    Ok(added)
}

#[derive(Debug, Clone)]
pub struct BuiltinProfile {
    pub id: &'static str,
    pub manifest: Manifest,
}

pub fn builtin_profiles() -> Vec<BuiltinProfile> {
    vec![
        BuiltinProfile {
            id: "anthropic-official",
            manifest: Manifest {
                name: "Anthropic Official".to_string(),
                version: "1.0.0".to_string(),
                description: "Direct Anthropic Messages provider with Claude defaults.".to_string(),
                providers: vec![provider_value(Provider {
                    name: "anthropic".to_string(),
                    api_kind: Some(ProviderApiKind::AnthropicMessages),
                    api_kind_source: ProviderApiKindSource::Explicit,
                    api_base_url: "https://api.anthropic.com/v1/messages".to_string(),
                    api_key: "$ANTHROPIC_API_KEY".to_string(),
                    models: vec!["claude-sonnet-4".to_string(), "claude-opus-4".to_string()],
                    endpoint_candidates: vec![],
                    transformer: transformer(&["anthropic"]),
                })],
                route_pool: Some(route_pool("anthropic,claude-sonnet-4")),
                schema: vec![],
                required_env: vec!["ANTHROPIC_API_KEY".to_string()],
                experimental: false,
            },
        },
        BuiltinProfile {
            id: "openai-codex-official",
            manifest: Manifest {
                name: "OpenAI / Codex Official".to_string(),
                version: "1.0.0".to_string(),
                description: "Direct OpenAI Responses provider with GPT/Codex defaults."
                    .to_string(),
                providers: vec![provider_value(Provider {
                    name: "openai".to_string(),
                    api_kind: Some(ProviderApiKind::OpenAiResponses),
                    api_kind_source: ProviderApiKindSource::Explicit,
                    api_base_url: "https://api.openai.com/v1/responses".to_string(),
                    api_key: "$OPENAI_API_KEY".to_string(),
                    models: vec![
                        "gpt-5-codex".to_string(),
                        "gpt-5".to_string(),
                        "gpt-4.1".to_string(),
                    ],
                    endpoint_candidates: vec![],
                    transformer: transformer(&["openai"]),
                })],
                route_pool: Some(route_pool("openai,gpt-5-codex")),
                schema: vec![],
                required_env: vec!["OPENAI_API_KEY".to_string()],
                experimental: false,
            },
        },
        BuiltinProfile {
            id: "claude-code-with-gpt",
            manifest: Manifest {
                name: "Claude Code with GPT / Codex".to_string(),
                version: "1.0.0".to_string(),
                description: "Routes Claude Code through CCR to OpenAI GPT/Codex models."
                    .to_string(),
                providers: vec![provider_value(Provider {
                    name: "openai".to_string(),
                    api_kind: Some(ProviderApiKind::OpenAiResponses),
                    api_kind_source: ProviderApiKindSource::Explicit,
                    api_base_url: "https://api.openai.com/v1/responses".to_string(),
                    api_key: "$OPENAI_API_KEY".to_string(),
                    models: vec![
                        "gpt-5-codex".to_string(),
                        "gpt-5".to_string(),
                        "gpt-4.1".to_string(),
                    ],
                    endpoint_candidates: vec![],
                    transformer: transformer(&["openai"]),
                })],
                route_pool: Some(route_pool("openai,gpt-5-codex")),
                schema: vec![],
                required_env: vec!["OPENAI_API_KEY".to_string()],
                experimental: false,
            },
        },
        BuiltinProfile {
            id: "codex-with-sonnet",
            manifest: Manifest {
                name: "Codex with Claude Sonnet".to_string(),
                version: "1.0.0".to_string(),
                description: "Routes Codex through CCR to Anthropic Claude Sonnet.".to_string(),
                providers: vec![provider_value(Provider {
                    name: "anthropic".to_string(),
                    api_kind: Some(ProviderApiKind::AnthropicMessages),
                    api_kind_source: ProviderApiKindSource::Explicit,
                    api_base_url: "https://api.anthropic.com/v1/messages".to_string(),
                    api_key: "$ANTHROPIC_API_KEY".to_string(),
                    models: vec!["claude-sonnet-4".to_string()],
                    endpoint_candidates: vec![],
                    transformer: transformer(&["anthropic"]),
                })],
                route_pool: Some(route_pool("anthropic,claude-sonnet-4")),
                schema: vec![],
                required_env: vec!["ANTHROPIC_API_KEY".to_string()],
                experimental: true,
            },
        },
    ]
}

pub fn builtin_profile(id: &str) -> Option<BuiltinProfile> {
    builtin_profiles()
        .into_iter()
        .find(|profile| profile.id == id)
}

fn provider_value(provider: Provider) -> serde_json::Value {
    serde_json::to_value(provider).expect("builtin provider must serialize")
}

fn transformer(names: &[&str]) -> TransformerConfig {
    TransformerConfig {
        use_transformers: names
            .iter()
            .map(|name| serde_json::Value::String((*name).to_string()))
            .collect(),
        model_overrides: HashMap::new(),
    }
}

fn route_pool(route: &str) -> RoutePoolConfig {
    RoutePoolConfig {
        enabled: true,
        failure_threshold: 3,
        ban_seconds: 3600,
        candidates: vec![RoutePoolCandidate {
            route: route.to_string(),
            enabled: true,
            priority: 1,
        }],
    }
}

/// Replace {{field_id}} placeholders in provider JSON values with user-supplied values.
pub fn apply_user_values(manifest: &mut Manifest, values: &HashMap<String, String>) {
    for pval in manifest.providers.iter_mut() {
        replace_placeholders(pval, values);
    }
}

fn replace_placeholders(v: &mut serde_json::Value, values: &HashMap<String, String>) {
    match v {
        serde_json::Value::String(s) => {
            for (k, val) in values {
                *s = s.replace(&format!("{{{{{k}}}}}"), val);
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values_mut() {
                replace_placeholders(v, values);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                replace_placeholders(v, values);
            }
        }
        _ => {}
    }
}

pub fn list_presets() -> Result<Vec<Manifest>> {
    let dir = presets_dir();
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let manifest_path = entry.path().join("manifest.json");
        if manifest_path.exists()
            && let Ok(raw) = std::fs::read_to_string(&manifest_path)
            && let Ok(m) = serde_json::from_str::<Manifest>(&raw)
        {
            out.push(m);
        }
    }
    Ok(out)
}

pub fn delete_preset(name: &str) -> Result<()> {
    let dir = presets_dir().join(name);
    std::fs::remove_dir_all(&dir).with_context(|| format!("Preset '{name}' not found"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccr_types::{Config, Provider, ProviderApiKindSource};
    use tempfile::TempDir;

    fn test_config() -> Config {
        Config {
            providers: vec![Provider {
                name: "openai".into(),
                api_kind: None,
                api_kind_source: ProviderApiKindSource::Inferred,
                api_base_url: "https://api.openai.com".into(),
                api_key: "sk-real".into(),
                models: vec!["gpt-4o".into()],
                endpoint_candidates: vec![],
                transformer: Default::default(),
            }],
            route_pool: Some(route_pool("openai,gpt-4o")),
            ..Default::default()
        }
    }

    #[test]
    fn sanitize_replaces_api_key() {
        let config = test_config();
        let sanitized = sanitize_providers(&config.providers);
        assert_eq!(sanitized[0]["api_key"], "{{api_key}}");
        assert_eq!(sanitized[0]["name"], "openai");
    }

    #[test]
    fn export_and_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let preset_dir = tmp.path().join("my-preset");
        std::fs::create_dir_all(&preset_dir).unwrap();

        let config = test_config();
        let manifest = Manifest {
            name: "my-preset".into(),
            version: "1.0.0".into(),
            description: "test".into(),
            providers: sanitize_providers(&config.providers),
            route_pool: config.route_pool.clone(),
            schema: vec![],
            required_env: vec![],
            experimental: false,
        };
        let path = preset_dir.join("manifest.json");
        std::fs::write(&path, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();

        let loaded = load_preset(preset_dir.to_str().unwrap()).unwrap();
        assert_eq!(loaded.name, "my-preset");
        assert_eq!(loaded.providers[0]["name"], "openai");
        assert_eq!(loaded.providers[0]["api_key"], "{{api_key}}");
    }

    #[test]
    fn install_adds_new_provider() {
        let manifest = Manifest {
            name: "p".into(),
            version: "1.0.0".into(),
            description: String::new(),
            providers: vec![serde_json::json!({
                "name": "deepseek",
                "api_base_url": "https://api.deepseek.com",
                "api_key": "{{api_key}}",
                "models": [],
                "transformer": {"use": []}
            })],
            route_pool: None,
            schema: vec![],
            required_env: vec![],
            experimental: false,
        };
        let mut config = Config::default();
        let added = install_preset(&manifest, &mut config).unwrap();
        assert_eq!(added, vec!["deepseek"]);
        assert_eq!(config.providers.len(), 1);
    }

    #[test]
    fn list_and_delete_preset() {
        let tmp = TempDir::new().unwrap();
        // Create two preset dirs
        for name in &["alpha", "beta"] {
            let dir = tmp.path().join(name);
            std::fs::create_dir_all(&dir).unwrap();
            let m = Manifest {
                name: name.to_string(),
                version: "1.0.0".into(),
                description: String::new(),
                providers: vec![],
                route_pool: None,
                schema: vec![],
                required_env: vec![],
                experimental: false,
            };
            std::fs::write(
                dir.join("manifest.json"),
                serde_json::to_string(&m).unwrap(),
            )
            .unwrap();
        }
        // list
        let mut names: Vec<String> = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().join("manifest.json").exists())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        assert_eq!(names, vec!["alpha", "beta"]);
        // delete
        std::fs::remove_dir_all(tmp.path().join("alpha")).unwrap();
        assert!(!tmp.path().join("alpha").exists());
        assert!(tmp.path().join("beta").exists());
    }

    #[test]
    fn apply_user_values_replaces_placeholders() {
        let mut manifest = Manifest {
            name: "p".into(),
            version: "1.0.0".into(),
            description: String::new(),
            providers: vec![serde_json::json!({
                "name": "openai",
                "api_key": "{{api_key}}"
            })],
            route_pool: None,
            schema: vec![SchemaField {
                id: "api_key".into(),
                label: "API Key".into(),
                r#type: "password".into(),
                prompt: String::new(),
            }],
            required_env: vec![],
            experimental: false,
        };
        let mut values = HashMap::new();
        values.insert("api_key".into(), "sk-real-key".into());
        apply_user_values(&mut manifest, &values);
        assert_eq!(manifest.providers[0]["api_key"], "sk-real-key");
    }

    #[test]
    fn install_skips_existing_provider() {
        let manifest = Manifest {
            name: "p".into(),
            version: "1.0.0".into(),
            description: String::new(),
            providers: vec![serde_json::json!({
                "name": "openai",
                "api_base_url": "https://api.openai.com",
                "api_key": "{{api_key}}",
                "models": [],
                "transformer": {"use": []}
            })],
            route_pool: None,
            schema: vec![],
            required_env: vec![],
            experimental: false,
        };
        let mut config = test_config();
        let added = install_preset(&manifest, &mut config).unwrap();
        assert!(added.is_empty());
        assert_eq!(config.providers.len(), 1);
    }

    #[test]
    fn install_preset_updates_route_pool_when_present() {
        let manifest = Manifest {
            name: "p".into(),
            version: "1.0.0".into(),
            description: String::new(),
            providers: vec![],
            route_pool: Some(route_pool("anthropic,claude-sonnet-4")),
            schema: vec![],
            required_env: vec![],
            experimental: false,
        };
        let mut config = test_config();

        install_preset(&manifest, &mut config).unwrap();

        assert_eq!(
            config.first_route_pool_route(),
            Some("anthropic,claude-sonnet-4")
        );
    }

    #[test]
    fn install_preset_can_overwrite_existing_provider() {
        let manifest = Manifest {
            name: "p".into(),
            version: "1.0.0".into(),
            description: String::new(),
            providers: vec![serde_json::json!({
                "name": "openai",
                "api_kind": "openai_responses",
                "api_kind_source": "explicit",
                "api_base_url": "https://api.openai.com/v1/responses",
                "api_key": "$OPENAI_API_KEY",
                "models": ["gpt-5-codex"],
                "transformer": {"use": ["openai"]}
            })],
            route_pool: None,
            schema: vec![],
            required_env: vec![],
            experimental: false,
        };
        let mut config = test_config();

        let changed = install_preset_with_options(&manifest, &mut config, true).unwrap();

        assert_eq!(changed, vec!["openai"]);
        assert_eq!(
            config.providers[0].api_base_url,
            "https://api.openai.com/v1/responses"
        );
        assert_eq!(config.providers[0].api_key, "$OPENAI_API_KEY");
    }

    #[test]
    fn builtin_profiles_include_required_official_profiles() {
        let profiles = builtin_profiles();
        let ids = profiles
            .iter()
            .map(|profile| profile.id)
            .collect::<Vec<_>>();

        assert_eq!(
            ids,
            vec![
                "anthropic-official",
                "openai-codex-official",
                "claude-code-with-gpt",
                "codex-with-sonnet"
            ]
        );
        assert!(
            profiles
                .iter()
                .all(|profile| profile.manifest.route_pool.is_some())
        );
        assert!(
            profiles
                .iter()
                .all(|profile| !profile.manifest.required_env.is_empty())
        );
    }

    #[test]
    fn builtin_profiles_use_explicit_provider_kind_and_env_placeholders() {
        for profile in builtin_profiles() {
            for value in &profile.manifest.providers {
                let provider: Provider = serde_json::from_value(value.clone()).unwrap();
                assert_eq!(provider.api_kind_source, ProviderApiKindSource::Explicit);
                assert!(provider.api_kind.is_some());
                assert!(provider.api_key.starts_with('$'));
            }
        }
    }
}
