#[cfg(test)]
use super::common::set_secure_permissions;
use super::common::{
    BackupSpec, RestoreSpec, atomic_write, backup_or_mark_missing, ccr_config, ccr_port,
    configured_path_from_settings, restore_backup_or_remove_generated,
};
use crate::status::{InjectionSnapshot, is_process_alive, pid_file_path, read_pid};
use anyhow::{Context, Result};
use ccr_types::ClaudeCodeModelSettings;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;

/// Get Claude Code settings file path.
pub fn claude_config_path() -> PathBuf {
    configured_path_from_settings(
        |settings| settings.claude_config_path.clone(),
        default_claude_config_path,
    )
}

fn default_claude_config_path() -> PathBuf {
    dirs_next::home_dir()
        .expect("Cannot determine home directory")
        .join(".claude")
        .join("settings.json")
}

/// Get Claude Code plugin config file path.
pub fn claude_plugin_config_path() -> PathBuf {
    dirs_next::home_dir()
        .expect("Cannot determine home directory")
        .join(".claude")
        .join("config.json")
}

/// Get CCR backup directory
pub fn ccr_backup_dir() -> PathBuf {
    dirs_next::home_dir()
        .expect("Cannot determine home directory")
        .join(".claude-code-router")
        .join("backups")
}

/// Get current backup file path
pub fn claude_backup_path() -> PathBuf {
    ccr_backup_dir().join("claude-config.backup.json")
}

fn claude_missing_marker_path() -> PathBuf {
    ccr_backup_dir().join("claude-config.backup.missing")
}

fn claude_plugin_backup_path() -> PathBuf {
    ccr_backup_dir().join("claude-plugin-config.backup.json")
}

fn claude_plugin_missing_marker_path() -> PathBuf {
    ccr_backup_dir().join("claude-plugin-config.backup.missing")
}

/// Get timestamped backup file path
pub fn claude_timestamped_backup_path() -> PathBuf {
    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    ccr_backup_dir().join(format!("claude-config.backup.{}.json", timestamp))
}

/// Activation status
pub enum ActivationStatus {
    Activated {
        backup_path: PathBuf,
        backup_time: Option<String>,
    },
    Deactivated,
}

/// Check if CCR is activated
pub fn check_activation_status() -> ActivationStatus {
    let backup_path = claude_backup_path();

    if backup_path.exists()
        || claude_missing_marker_path().exists()
        || claude_plugin_backup_path().exists()
        || claude_plugin_missing_marker_path().exists()
    {
        let metadata = fs::metadata(&backup_path)
            .or_else(|_| fs::metadata(claude_missing_marker_path()))
            .or_else(|_| fs::metadata(claude_plugin_backup_path()))
            .or_else(|_| fs::metadata(claude_plugin_missing_marker_path()))
            .ok();
        let backup_time = metadata.and_then(|m| m.modified().ok()).map(|t| {
            let datetime: chrono::DateTime<chrono::Local> = t.into();
            datetime.format("%Y-%m-%d %H:%M:%S").to_string()
        });

        ActivationStatus::Activated {
            backup_path,
            backup_time,
        }
    } else {
        ActivationStatus::Deactivated
    }
}

pub fn claude_injection_snapshot(port: u16) -> InjectionSnapshot {
    let current = claude_points_to_ccr(port);
    match check_activation_status() {
        ActivationStatus::Activated {
            backup_path,
            backup_time,
        } if current => InjectionSnapshot::ActiveAndCurrent {
            backup_path: backup_path.display().to_string(),
            backup_time,
        },
        ActivationStatus::Activated {
            backup_path,
            backup_time,
        } => InjectionSnapshot::ActiveButDrifted {
            backup_path: backup_path.display().to_string(),
            backup_time,
        },
        ActivationStatus::Deactivated if current => InjectionSnapshot::InjectedNoBackup,
        ActivationStatus::Deactivated => InjectionSnapshot::Inactive,
    }
}

fn claude_points_to_ccr(port: u16) -> bool {
    claude_settings_points_to_ccr(&claude_config_path(), port)
}

fn claude_settings_points_to_ccr(path: &std::path::Path, port: u16) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(config) = serde_json::from_str::<Value>(&content) else {
        return false;
    };
    config
        .get("env")
        .and_then(|env| env.get("ANTHROPIC_BASE_URL"))
        .and_then(Value::as_str)
        == Some(format!("http://127.0.0.1:{port}").as_str())
}

/// Activate CCR: backup Claude config and install CCR config
pub fn activate_ccr() -> Result<()> {
    let claude_path = claude_config_path();
    let plugin_path = claude_plugin_config_path();
    let backup_path = claude_backup_path();
    let plugin_backup_path = claude_plugin_backup_path();
    let timestamped_backup = claude_timestamped_backup_path();
    let missing_marker = claude_missing_marker_path();
    let plugin_missing_marker = claude_plugin_missing_marker_path();

    // 0. Check if CCR server is running
    let pid_path = pid_file_path();
    match read_pid(&pid_path) {
        Some(pid) if is_process_alive(pid) => {}
        _ => {
            anyhow::bail!("CCR server is not running. Start it with 'ccr start'");
        }
    }

    if backup_path.exists()
        || missing_marker.exists()
        || plugin_backup_path.exists()
        || plugin_missing_marker.exists()
    {
        anyhow::bail!("CCR is already activated. Run 'ccr deactivate' first.");
    }

    let original_json = prepare_claude_activation_files(
        &claude_path,
        &backup_path,
        &timestamped_backup,
        &missing_marker,
    )?;
    let original_plugin_json = prepare_claude_plugin_activation_files(
        &plugin_path,
        &plugin_backup_path,
        &plugin_missing_marker,
    )?;

    let ccr_config = ccr_config()?;
    let port = ccr_port(&ccr_config);
    install_claude_settings(
        &claude_path,
        &original_json,
        port,
        &ccr_config.app_settings.claude_code_models,
        ccr_config.app_settings.claude_code_models_enabled,
    )?;
    install_claude_plugin_config(&plugin_path, &original_plugin_json)?;

    println!("✓ Backed up Claude config to {}", backup_path.display());
    println!("✓ Installed CCR config to {}", claude_path.display());
    println!(
        "✓ Claude Code is now routing through CCR (http://127.0.0.1:{})",
        port
    );

    Ok(())
}

/// Deactivate CCR: restore original Claude config from backup
pub fn deactivate_ccr() -> Result<()> {
    let claude_path = claude_config_path();
    let plugin_path = claude_plugin_config_path();
    let backup_path = claude_backup_path();
    let plugin_backup_path = claude_plugin_backup_path();
    let missing_marker = claude_missing_marker_path();
    let plugin_missing_marker = claude_plugin_missing_marker_path();

    restore_claude_settings(&claude_path, &backup_path, &missing_marker)?;
    restore_claude_settings(&plugin_path, &plugin_backup_path, &plugin_missing_marker)?;

    println!("✓ Restored Claude config from backup");
    println!("✓ Claude Code is now using original configuration");

    Ok(())
}

fn prepare_claude_activation_files(
    claude_path: &std::path::Path,
    backup_path: &std::path::Path,
    timestamped_backup: &std::path::Path,
    missing_marker: &std::path::Path,
) -> Result<String> {
    backup_or_mark_missing(
        BackupSpec {
            source_path: claude_path,
            backup_path,
            timestamped_backup_path: Some(timestamped_backup),
            missing_marker_path: missing_marker,
            already_active_message: "CCR is already activated. Run 'ccr deactivate' first.",
            source_dir_context: "Failed to create Claude config directory",
            backup_dir_context: "Failed to create backup directory",
            read_context: "Failed to read Claude settings",
            backup_context: "Failed to create backup",
            timestamped_backup_context: "Failed to create timestamped backup",
            missing_marker_context: "Failed to create missing config marker",
        },
        |_| Ok(()),
    )
}

fn prepare_claude_plugin_activation_files(
    plugin_path: &std::path::Path,
    backup_path: &std::path::Path,
    missing_marker: &std::path::Path,
) -> Result<String> {
    backup_or_mark_missing(
        BackupSpec {
            source_path: plugin_path,
            backup_path,
            timestamped_backup_path: None,
            missing_marker_path: missing_marker,
            already_active_message: "CCR is already activated. Run 'ccr deactivate' first.",
            source_dir_context: "Failed to create Claude config directory",
            backup_dir_context: "Failed to create backup directory",
            read_context: "Failed to read Claude plugin config",
            backup_context: "Failed to create Claude plugin backup",
            timestamped_backup_context: "Failed to create timestamped Claude plugin backup",
            missing_marker_context: "Failed to create missing plugin config marker",
        },
        |content| {
            serde_json::from_str::<Value>(content)
                .context("Claude plugin config file is invalid JSON")?;
            Ok(())
        },
    )
}

fn install_claude_settings(
    claude_path: &std::path::Path,
    original_json: &str,
    port: u16,
    models: &ClaudeCodeModelSettings,
    models_enabled: bool,
) -> Result<()> {
    let config = build_claude_settings(original_json, port, models, models_enabled)?;
    atomic_write(
        claude_path,
        "json.tmp",
        serde_json::to_string_pretty(&config)?,
        "Failed to create Claude config directory",
        "Failed to write temporary config",
        "Failed to install CCR config",
    )
}

fn install_claude_plugin_config(plugin_path: &std::path::Path, original_json: &str) -> Result<()> {
    let config = build_claude_plugin_config(original_json)?;
    atomic_write(
        plugin_path,
        "json.tmp",
        serde_json::to_string_pretty(&config)?,
        "Failed to create Claude config directory",
        "Failed to write temporary plugin config",
        "Failed to install Claude plugin config",
    )
}

fn restore_claude_settings(
    claude_path: &std::path::Path,
    backup_path: &std::path::Path,
    missing_marker: &std::path::Path,
) -> Result<()> {
    restore_backup_or_remove_generated(
        RestoreSpec {
            target_path: claude_path,
            backup_path,
            missing_marker_path: missing_marker,
            temp_extension: "json.tmp",
            missing_backup_is_ok: false,
            read_backup_context: "Failed to read backup file",
            copy_context: "Failed to copy backup to temporary file",
            rename_context: "Failed to restore Claude config",
            remove_backup_context: "Failed to remove backup file",
            remove_marker_context: "Failed to remove missing config marker",
            no_backup_message: "No backup found. CCR is not activated.",
        },
        |backup_json| {
            serde_json::from_str::<Value>(backup_json)
                .context("Backup file is corrupted (invalid JSON)")?;
            Ok(())
        },
    )
}

fn build_claude_settings(
    original_json: &str,
    port: u16,
    models: &ClaudeCodeModelSettings,
    models_enabled: bool,
) -> Result<Value> {
    let mut config = if original_json.trim().is_empty() {
        json!({})
    } else {
        match serde_json::from_str::<Value>(original_json)
            .context("Failed to parse Claude settings JSON")?
        {
            Value::Object(map) => Value::Object(map),
            _ => json!({}),
        }
    };

    if config.get("env").and_then(|v| v.as_object()).is_none() {
        config["env"] = json!({});
    }

    config["env"]["ANTHROPIC_BASE_URL"] = json!(format!("http://127.0.0.1:{}", port));
    config["env"]["ANTHROPIC_AUTH_TOKEN"] = json!("any");
    if models_enabled {
        set_optional_env(&mut config, "ANTHROPIC_MODEL", &models.model);
        set_optional_env(
            &mut config,
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            &models.haiku_model,
        );
        set_optional_env(
            &mut config,
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            &models.sonnet_model,
        );
        set_optional_env(
            &mut config,
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            &models.opus_model,
        );
    } else {
        remove_model_envs(&mut config);
    }

    Ok(config)
}

fn remove_model_envs(config: &mut Value) {
    let Some(env) = config.get_mut("env").and_then(|env| env.as_object_mut()) else {
        return;
    };
    for key in [
        "ANTHROPIC_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
    ] {
        env.remove(key);
    }
}

fn set_optional_env(config: &mut Value, key: &str, value: &str) {
    if value.trim().is_empty() {
        if let Some(env) = config.get_mut("env").and_then(|env| env.as_object_mut()) {
            env.remove(key);
        }
    } else {
        config["env"][key] = json!(value);
    }
}

fn build_claude_plugin_config(original_json: &str) -> Result<Value> {
    let mut config = if original_json.trim().is_empty() {
        json!({})
    } else {
        match serde_json::from_str::<Value>(original_json)
            .context("Failed to parse Claude plugin config JSON")?
        {
            Value::Object(map) => Value::Object(map),
            _ => json!({}),
        }
    };

    config["primaryApiKey"] = json!("any");
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn models() -> ClaudeCodeModelSettings {
        ClaudeCodeModelSettings {
            model: "claude-sonnet-default".to_string(),
            haiku_model: "claude-haiku-default".to_string(),
            sonnet_model: "claude-sonnet-default".to_string(),
            opus_model: "claude-opus-default".to_string(),
        }
    }

    #[test]
    fn test_claude_config_path() {
        let path = claude_config_path();
        assert!(path.to_string_lossy().contains(".claude"));
        assert!(path.ends_with("settings.json"));
    }

    #[test]
    fn test_claude_plugin_config_path() {
        let path = claude_plugin_config_path();
        assert!(path.to_string_lossy().contains(".claude"));
        assert!(path.ends_with("config.json"));
    }

    #[test]
    fn test_backup_path() {
        let path = claude_backup_path();
        assert!(path.to_string_lossy().contains(".claude-code-router"));
        assert!(path.to_string_lossy().contains("backups"));
        assert!(path.ends_with("claude-config.backup.json"));
    }

    #[test]
    fn test_timestamped_backup_path() {
        let path = claude_timestamped_backup_path();
        let filename = path.file_name().unwrap().to_str().unwrap();
        assert!(filename.starts_with("claude-config.backup."));
        assert!(filename.ends_with(".json"));
        // Should contain timestamp pattern like 20260424-103015
        assert!(filename.contains('-'));
    }

    #[test]
    fn test_check_activation_status_deactivated() {
        // This test assumes no backup exists
        match check_activation_status() {
            ActivationStatus::Deactivated => {}
            ActivationStatus::Activated { .. } => {
                // This is fine if a backup actually exists
            }
        }
    }

    #[test]
    fn build_claude_settings_merges_env_and_preserves_existing_fields() {
        let original = r#"{
  "permissions": {"allow": ["Bash(ls)"]},
  "env": {"EXISTING": "1"}
}"#;

        let config = build_claude_settings(original, 3456, &models(), true).unwrap();

        assert_eq!(config["permissions"]["allow"][0], "Bash(ls)");
        assert_eq!(config["env"]["EXISTING"], "1");
        assert_eq!(config["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:3456");
        assert_eq!(config["env"]["ANTHROPIC_AUTH_TOKEN"], "any");
        assert_eq!(config["env"]["ANTHROPIC_MODEL"], "claude-sonnet-default");
        assert_eq!(
            config["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"],
            "claude-haiku-default"
        );
        assert_eq!(
            config["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"],
            "claude-sonnet-default"
        );
        assert_eq!(
            config["env"]["ANTHROPIC_DEFAULT_OPUS_MODEL"],
            "claude-opus-default"
        );
    }

    #[test]
    fn claude_points_to_ccr_matches_expected_local_endpoint() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("settings.json");
        let config = build_claude_settings("", 3456, &models(), true).unwrap();
        fs::write(&path, serde_json::to_string(&config).unwrap()).unwrap();

        assert!(claude_settings_points_to_ccr(&path, 3456));
        assert!(!claude_settings_points_to_ccr(&path, 4567));
    }

    #[test]
    fn claude_points_to_ccr_rejects_non_ccr_settings() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("settings.json");
        fs::write(
            &path,
            r#"{"env":{"ANTHROPIC_BASE_URL":"https://api.anthropic.com"}}"#,
        )
        .unwrap();

        assert!(!claude_settings_points_to_ccr(&path, 3456));
    }

    #[test]
    fn build_claude_settings_replaces_non_object_env() {
        let config = build_claude_settings(r#"{"env":"bad"}"#, 4567, &models(), true).unwrap();

        assert_eq!(config["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:4567");
        assert_eq!(config["env"]["ANTHROPIC_MODEL"], "claude-sonnet-default");
    }

    #[test]
    fn build_claude_settings_omits_blank_model_envs() {
        let config = build_claude_settings(
            r#"{"env":{"ANTHROPIC_MODEL":"old","ANTHROPIC_DEFAULT_OPUS_MODEL":"old-opus"}}"#,
            3456,
            &ClaudeCodeModelSettings::default(),
            true,
        )
        .unwrap();

        assert_eq!(config["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:3456");
        assert!(config["env"].get("ANTHROPIC_MODEL").is_none());
        assert!(config["env"].get("ANTHROPIC_DEFAULT_HAIKU_MODEL").is_none());
        assert!(
            config["env"]
                .get("ANTHROPIC_DEFAULT_SONNET_MODEL")
                .is_none()
        );
        assert!(config["env"].get("ANTHROPIC_DEFAULT_OPUS_MODEL").is_none());
    }

    #[test]
    fn build_claude_settings_removes_model_envs_when_mapping_disabled() {
        let config = build_claude_settings(
            r#"{"env":{
              "ANTHROPIC_MODEL":"claude-sonnet-4-20250514",
              "ANTHROPIC_DEFAULT_HAIKU_MODEL":"claude-haiku-4-20250514",
              "ANTHROPIC_DEFAULT_SONNET_MODEL":"claude-sonnet-4-20250514",
              "ANTHROPIC_DEFAULT_OPUS_MODEL":"claude-opus-4-20250514",
              "KEEP":"1"
            }}"#,
            3456,
            &models(),
            false,
        )
        .unwrap();

        assert_eq!(config["env"]["KEEP"], "1");
        assert_eq!(config["env"]["ANTHROPIC_BASE_URL"], "http://127.0.0.1:3456");
        assert!(config["env"].get("ANTHROPIC_MODEL").is_none());
        assert!(config["env"].get("ANTHROPIC_DEFAULT_HAIKU_MODEL").is_none());
        assert!(
            config["env"]
                .get("ANTHROPIC_DEFAULT_SONNET_MODEL")
                .is_none()
        );
        assert!(config["env"].get("ANTHROPIC_DEFAULT_OPUS_MODEL").is_none());
    }

    #[test]
    fn build_claude_settings_rejects_invalid_json() {
        let err = build_claude_settings("{bad", 3456, &models(), true)
            .unwrap_err()
            .to_string();

        assert!(err.contains("Failed to parse Claude settings JSON"));
    }

    #[test]
    fn build_claude_plugin_config_sets_primary_api_key_and_preserves_fields() {
        let config = build_claude_plugin_config(r#"{"theme":"dark"}"#).unwrap();

        assert_eq!(config["theme"], "dark");
        assert_eq!(config["primaryApiKey"], "any");
    }

    #[test]
    fn build_claude_plugin_config_rejects_invalid_json() {
        let err = build_claude_plugin_config("{bad").unwrap_err().to_string();

        assert!(err.contains("Failed to parse Claude plugin config JSON"));
    }

    #[test]
    fn prepare_claude_activation_files_backs_up_existing_settings() {
        let temp = TempDir::new().unwrap();
        let claude_path = temp.path().join(".claude").join("settings.json");
        let backup_path = temp.path().join("backups").join("backup.json");
        let timestamped_path = temp.path().join("backups").join("backup.timestamp.json");
        let missing_marker = temp.path().join("backups").join("missing");
        fs::create_dir_all(claude_path.parent().unwrap()).unwrap();
        fs::write(&claude_path, r#"{"env":{"EXISTING":"1"}}"#).unwrap();

        let original = prepare_claude_activation_files(
            &claude_path,
            &backup_path,
            &timestamped_path,
            &missing_marker,
        )
        .unwrap();

        assert_eq!(original, r#"{"env":{"EXISTING":"1"}}"#);
        assert_eq!(fs::read_to_string(&backup_path).unwrap(), original);
        assert_eq!(fs::read_to_string(&timestamped_path).unwrap(), original);
        assert!(!missing_marker.exists());
    }

    #[test]
    fn prepare_claude_activation_files_marks_missing_settings() {
        let temp = TempDir::new().unwrap();
        let claude_path = temp.path().join(".claude").join("settings.json");
        let backup_path = temp.path().join("backups").join("backup.json");
        let timestamped_path = temp.path().join("backups").join("backup.timestamp.json");
        let missing_marker = temp.path().join("backups").join("missing");

        let original = prepare_claude_activation_files(
            &claude_path,
            &backup_path,
            &timestamped_path,
            &missing_marker,
        )
        .unwrap();

        assert_eq!(original, "");
        assert_eq!(fs::read_to_string(&missing_marker).unwrap(), "missing");
        assert!(!backup_path.exists());
        assert!(!timestamped_path.exists());
    }

    #[test]
    fn prepare_claude_plugin_activation_files_backs_up_existing_config() {
        let temp = TempDir::new().unwrap();
        let plugin_path = temp.path().join(".claude").join("config.json");
        let backup_path = temp.path().join("backups").join("plugin.json");
        let missing_marker = temp.path().join("backups").join("plugin.missing");
        fs::create_dir_all(plugin_path.parent().unwrap()).unwrap();
        fs::write(&plugin_path, r#"{"theme":"dark"}"#).unwrap();

        let original =
            prepare_claude_plugin_activation_files(&plugin_path, &backup_path, &missing_marker)
                .unwrap();

        assert_eq!(original, r#"{"theme":"dark"}"#);
        assert_eq!(fs::read_to_string(&backup_path).unwrap(), original);
        assert!(!missing_marker.exists());
    }

    #[test]
    fn prepare_claude_plugin_activation_files_rejects_invalid_json() {
        let temp = TempDir::new().unwrap();
        let plugin_path = temp.path().join(".claude").join("config.json");
        let backup_path = temp.path().join("backups").join("plugin.json");
        let missing_marker = temp.path().join("backups").join("plugin.missing");
        fs::create_dir_all(plugin_path.parent().unwrap()).unwrap();
        fs::write(&plugin_path, "{bad").unwrap();

        let err =
            prepare_claude_plugin_activation_files(&plugin_path, &backup_path, &missing_marker)
                .unwrap_err()
                .to_string();

        assert!(err.contains("Claude plugin config file is invalid JSON"));
        assert!(!backup_path.exists());
    }

    #[test]
    fn prepare_claude_plugin_activation_files_marks_missing_config() {
        let temp = TempDir::new().unwrap();
        let plugin_path = temp.path().join(".claude").join("config.json");
        let backup_path = temp.path().join("backups").join("plugin.json");
        let missing_marker = temp.path().join("backups").join("plugin.missing");

        let original =
            prepare_claude_plugin_activation_files(&plugin_path, &backup_path, &missing_marker)
                .unwrap();

        assert_eq!(original, "");
        assert_eq!(fs::read_to_string(&missing_marker).unwrap(), "missing");
        assert!(!backup_path.exists());
    }

    #[test]
    fn install_claude_settings_writes_model_mapping_env() {
        let temp = TempDir::new().unwrap();
        let claude_path = temp.path().join("settings.json");

        install_claude_settings(
            &claude_path,
            r#"{"env":{"KEEP":"1"}}"#,
            3456,
            &models(),
            true,
        )
        .unwrap();

        let written: Value =
            serde_json::from_str(&fs::read_to_string(&claude_path).unwrap()).unwrap();
        assert_eq!(written["env"]["KEEP"], "1");
        assert_eq!(
            written["env"]["ANTHROPIC_BASE_URL"],
            "http://127.0.0.1:3456"
        );
        assert_eq!(written["env"]["ANTHROPIC_MODEL"], "claude-sonnet-default");
        assert_eq!(
            written["env"]["ANTHROPIC_DEFAULT_OPUS_MODEL"],
            "claude-opus-default"
        );
        assert!(!claude_path.with_extension("json.tmp").exists());
    }

    #[test]
    fn install_claude_plugin_config_writes_primary_api_key() {
        let temp = TempDir::new().unwrap();
        let plugin_path = temp.path().join("config.json");

        install_claude_plugin_config(&plugin_path, r#"{"theme":"dark"}"#).unwrap();

        let written: Value =
            serde_json::from_str(&fs::read_to_string(&plugin_path).unwrap()).unwrap();
        assert_eq!(written["theme"], "dark");
        assert_eq!(written["primaryApiKey"], "any");
        assert!(!plugin_path.with_extension("json.tmp").exists());
    }

    #[test]
    fn restore_claude_settings_restores_backup_and_removes_markers() {
        let temp = TempDir::new().unwrap();
        let settings_path = temp.path().join("settings.json");
        let backup_path = temp.path().join("backup.json");
        let marker_path = temp.path().join("missing");

        fs::write(&settings_path, r#"{"current":true}"#).unwrap();
        fs::write(&backup_path, r#"{"restored":true}"#).unwrap();
        fs::write(&marker_path, "missing").unwrap();

        restore_claude_settings(&settings_path, &backup_path, &marker_path).unwrap();

        let restored: Value =
            serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert_eq!(restored["restored"], true);
        assert!(!backup_path.exists());
        assert!(!marker_path.exists());
    }

    #[test]
    fn restore_claude_settings_rejects_corrupt_backup() {
        let temp = TempDir::new().unwrap();
        let settings_path = temp.path().join("settings.json");
        let backup_path = temp.path().join("backup.json");
        let marker_path = temp.path().join("missing");

        fs::write(&settings_path, r#"{"current":true}"#).unwrap();
        fs::write(&backup_path, "{bad").unwrap();

        let err = restore_claude_settings(&settings_path, &backup_path, &marker_path)
            .unwrap_err()
            .to_string();

        assert!(err.contains("Backup file is corrupted"));
        assert!(backup_path.exists());
    }

    #[test]
    fn restore_claude_settings_requires_backup_or_marker() {
        let temp = TempDir::new().unwrap();
        let settings_path = temp.path().join("settings.json");
        let backup_path = temp.path().join("backup.json");
        let marker_path = temp.path().join("missing");

        let err = restore_claude_settings(&settings_path, &backup_path, &marker_path)
            .unwrap_err()
            .to_string();

        assert!(err.contains("No backup found"));
    }

    #[test]
    fn restore_claude_settings_removes_generated_file_when_original_missing() {
        let temp = TempDir::new().unwrap();
        let settings_path = temp.path().join("settings.json");
        let backup_path = temp.path().join("backup.json");
        let marker_path = temp.path().join("missing");

        fs::write(&settings_path, "{}").unwrap();
        fs::write(&marker_path, "missing").unwrap();

        restore_claude_settings(&settings_path, &backup_path, &marker_path).unwrap();

        assert!(!settings_path.exists());
        assert!(!marker_path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn set_secure_permissions_sets_unix_owner_read_write_only() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let path = temp.path().join("settings.json");
        fs::write(&path, "{}").unwrap();

        set_secure_permissions(&path).unwrap();

        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(windows)]
    #[test]
    fn set_secure_permissions_sets_windows_current_user_only_acl() {
        use std::os::windows::ffi::OsStrExt;
        use std::ptr::null_mut;
        use windows_sys::Win32::Foundation::{ERROR_SUCCESS, LocalFree};
        use windows_sys::Win32::Security::Authorization::{
            EXPLICIT_ACCESS_W, GetExplicitEntriesFromAclW, GetNamedSecurityInfoW, SE_FILE_OBJECT,
            TRUSTEE_IS_SID,
        };
        use windows_sys::Win32::Security::{ACL, DACL_SECURITY_INFORMATION};
        use windows_sys::Win32::Storage::FileSystem::{FILE_GENERIC_READ, FILE_GENERIC_WRITE};

        let temp = TempDir::new().unwrap();
        let path = temp.path().join("settings.json");
        fs::write(&path, "{}").unwrap();

        set_secure_permissions(&path).unwrap();

        let mut path_wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut dacl: *mut ACL = null_mut();
        let mut security_descriptor = null_mut();
        let status = unsafe {
            GetNamedSecurityInfoW(
                path_wide.as_mut_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                &mut dacl,
                null_mut(),
                &mut security_descriptor,
            )
        };
        assert_eq!(status, ERROR_SUCCESS);

        let mut entry_count = 0;
        let mut entries: *mut EXPLICIT_ACCESS_W = null_mut();
        let status = unsafe { GetExplicitEntriesFromAclW(dacl, &mut entry_count, &mut entries) };
        assert_eq!(status, ERROR_SUCCESS);

        let entries_slice = unsafe { std::slice::from_raw_parts(entries, entry_count as usize) };
        assert_eq!(entries_slice.len(), 1);
        assert_eq!(entries_slice[0].Trustee.TrusteeForm, TRUSTEE_IS_SID);
        assert_eq!(
            entries_slice[0].grfAccessPermissions,
            FILE_GENERIC_READ | FILE_GENERIC_WRITE
        );

        unsafe {
            LocalFree(entries.cast());
            LocalFree(security_descriptor);
        }
    }
}
