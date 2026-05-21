use super::common::{
    BackupSpec, RestoreSpec, atomic_write, backup_or_mark_missing, ccr_config, ccr_port,
    configured_path_from_settings, local_v1_base_url, restore_backup_or_remove_generated,
};
use crate::status::{InjectionSnapshot, is_process_alive, pid_file_path, read_pid};
use anyhow::{Context, Result};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use toml_edit::{DocumentMut, Item, Table, value};

const DEFAULT_CODEX_MODEL: &str = "gpt-5.5";

/// Get Codex config file path.
pub fn codex_config_path() -> PathBuf {
    configured_path_from_settings(
        |settings| settings.codex_config_path.clone(),
        default_codex_config_path,
    )
}

fn default_codex_config_path() -> PathBuf {
    codex_config_dir().join("config.toml")
}

/// Get Codex auth file path.
pub fn codex_auth_path() -> PathBuf {
    codex_config_dir().join("auth.json")
}

fn codex_config_dir() -> PathBuf {
    dirs_next::home_dir()
        .expect("Cannot determine home directory")
        .join(".codex")
}

/// Get CCR backup directory.
pub fn ccr_backup_dir() -> PathBuf {
    dirs_next::home_dir()
        .expect("Cannot determine home directory")
        .join(".claude-code-router")
        .join("backups")
}

/// Get current Codex backup file path.
pub fn codex_backup_path() -> PathBuf {
    ccr_backup_dir().join("codex-config.backup.toml")
}

fn codex_auth_backup_path() -> PathBuf {
    ccr_backup_dir().join("codex-auth.backup.json")
}

fn codex_missing_marker_path() -> PathBuf {
    ccr_backup_dir().join("codex-config.backup.missing")
}

fn codex_auth_missing_marker_path() -> PathBuf {
    ccr_backup_dir().join("codex-auth.backup.missing")
}

/// Get timestamped Codex backup file path.
pub fn codex_timestamped_backup_path() -> PathBuf {
    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    ccr_backup_dir().join(format!("codex-config.backup.{}.toml", timestamp))
}

pub enum CodexActivationStatus {
    Activated {
        backup_path: PathBuf,
        backup_time: Option<String>,
    },
    Deactivated,
}

/// Check if Codex is configured through CCR.
pub fn check_codex_activation_status() -> CodexActivationStatus {
    let backup_path = codex_backup_path();

    if backup_path.exists()
        || codex_missing_marker_path().exists()
        || codex_auth_backup_path().exists()
        || codex_auth_missing_marker_path().exists()
    {
        let metadata = fs::metadata(&backup_path)
            .or_else(|_| fs::metadata(codex_missing_marker_path()))
            .or_else(|_| fs::metadata(codex_auth_backup_path()))
            .or_else(|_| fs::metadata(codex_auth_missing_marker_path()))
            .ok();
        let backup_time = metadata.and_then(|m| m.modified().ok()).map(|t| {
            let datetime: chrono::DateTime<chrono::Local> = t.into();
            datetime.format("%Y-%m-%d %H:%M:%S").to_string()
        });

        CodexActivationStatus::Activated {
            backup_path,
            backup_time,
        }
    } else {
        CodexActivationStatus::Deactivated
    }
}

pub fn codex_injection_snapshot(port: u16) -> InjectionSnapshot {
    let current = codex_points_to_ccr(port);
    match check_codex_activation_status() {
        CodexActivationStatus::Activated {
            backup_path,
            backup_time,
        } if current => InjectionSnapshot::ActiveAndCurrent {
            backup_path: backup_path.display().to_string(),
            backup_time,
        },
        CodexActivationStatus::Activated {
            backup_path,
            backup_time,
        } => InjectionSnapshot::ActiveButDrifted {
            backup_path: backup_path.display().to_string(),
            backup_time,
        },
        CodexActivationStatus::Deactivated if current => InjectionSnapshot::InjectedNoBackup,
        CodexActivationStatus::Deactivated => InjectionSnapshot::Inactive,
    }
}

fn codex_points_to_ccr(port: u16) -> bool {
    codex_config_points_to_ccr(&codex_config_path(), port)
}

fn codex_config_points_to_ccr(path: &std::path::Path, port: u16) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(doc) = content.parse::<DocumentMut>() else {
        return false;
    };
    let Some(model_provider) = doc.get("model_provider").and_then(|item| item.as_str()) else {
        return false;
    };
    let Some(provider) = doc.get("model_providers").and_then(|item| item.get("ccr")) else {
        return false;
    };
    model_provider == "ccr"
        && provider.get("base_url").and_then(|item| item.as_str())
            == Some(local_v1_base_url(port).as_str())
        && provider.get("wire_api").and_then(|item| item.as_str()) == Some("responses")
}

/// Activate CCR for Codex by writing a local model provider to ~/.codex/config.toml.
pub fn activate_codex_ccr() -> Result<()> {
    let codex_path = codex_config_path();
    let auth_path = codex_auth_path();
    let backup_path = codex_backup_path();
    let auth_backup_path = codex_auth_backup_path();
    let timestamped_backup = codex_timestamped_backup_path();
    let missing_marker = codex_missing_marker_path();
    let auth_missing_marker = codex_auth_missing_marker_path();

    let pid_path = pid_file_path();
    match read_pid(&pid_path) {
        Some(pid) if is_process_alive(pid) => {}
        _ => anyhow::bail!("CCR server is not running. Start it with 'ccr start'"),
    }

    if backup_path.exists()
        || missing_marker.exists()
        || auth_backup_path.exists()
        || auth_missing_marker.exists()
    {
        anyhow::bail!("Codex is already activated. Run 'ccr codex-deactivate' first.");
    }

    let original = prepare_codex_activation_files(
        &codex_path,
        &backup_path,
        &timestamped_backup,
        &missing_marker,
    )?;
    prepare_codex_auth_activation_files(&auth_path, &auth_backup_path, &auth_missing_marker)?;

    let ccr_config = ccr_config()?;
    let port = codex_port_from_config(&ccr_config);
    install_codex_config(&codex_path, port, &original)?;
    install_codex_auth(&auth_path)?;

    println!("✓ Backed up Codex config to {}", backup_path.display());
    println!("✓ Installed CCR Codex provider to {}", codex_path.display());
    println!(
        "✓ Codex is now routing through CCR (http://127.0.0.1:{}/v1)",
        port
    );

    Ok(())
}

/// Deactivate CCR for Codex by restoring the previous config.
pub fn deactivate_codex_ccr() -> Result<()> {
    let codex_path = codex_config_path();
    let auth_path = codex_auth_path();
    let backup_path = codex_backup_path();
    let auth_backup_path = codex_auth_backup_path();
    let missing_marker = codex_missing_marker_path();
    let auth_missing_marker = codex_auth_missing_marker_path();

    restore_codex_config(&codex_path, &backup_path, &missing_marker)?;
    restore_codex_auth(&auth_path, &auth_backup_path, &auth_missing_marker)?;

    println!("✓ Restored Codex config");
    println!("✓ Codex is now using original configuration");

    Ok(())
}

fn codex_port_from_config(config: &ccr_types::Config) -> u16 {
    ccr_port(config)
}

fn prepare_codex_activation_files(
    codex_path: &std::path::Path,
    backup_path: &std::path::Path,
    timestamped_backup: &std::path::Path,
    missing_marker: &std::path::Path,
) -> Result<String> {
    backup_or_mark_missing(
        BackupSpec {
            source_path: codex_path,
            backup_path,
            timestamped_backup_path: Some(timestamped_backup),
            missing_marker_path: missing_marker,
            already_active_message: "Codex is already activated. Run 'ccr codex-deactivate' first.",
            source_dir_context: "Failed to create Codex config directory",
            backup_dir_context: "Failed to create backup directory",
            read_context: "Failed to read Codex config",
            backup_context: "Failed to create Codex backup",
            timestamped_backup_context: "Failed to create timestamped Codex backup",
            missing_marker_context: "Failed to create missing config marker",
        },
        |_| Ok(()),
    )
}

fn prepare_codex_auth_activation_files(
    auth_path: &std::path::Path,
    backup_path: &std::path::Path,
    missing_marker: &std::path::Path,
) -> Result<()> {
    backup_or_mark_missing(
        BackupSpec {
            source_path: auth_path,
            backup_path,
            timestamped_backup_path: None,
            missing_marker_path: missing_marker,
            already_active_message: "Codex is already activated. Run 'ccr codex-deactivate' first.",
            source_dir_context: "Failed to create Codex config directory",
            backup_dir_context: "Failed to create backup directory",
            read_context: "Failed to read Codex auth",
            backup_context: "Failed to create Codex auth backup",
            timestamped_backup_context: "Failed to create timestamped Codex auth backup",
            missing_marker_context: "Failed to create missing Codex auth marker",
        },
        |content| {
            serde_json::from_str::<serde_json::Value>(content)
                .context("Codex auth file is invalid JSON")?;
            Ok(())
        },
    )?;
    Ok(())
}

fn install_codex_config(codex_path: &std::path::Path, port: u16, original: &str) -> Result<()> {
    let updated = build_codex_config(original, port)?;
    atomic_write(
        codex_path,
        "toml.tmp",
        updated,
        "Failed to create Codex config directory",
        "Failed to write temporary Codex config",
        "Failed to install Codex CCR config",
    )
}

fn install_codex_auth(auth_path: &std::path::Path) -> Result<()> {
    let auth = json!({ "OPENAI_API_KEY": "any" });
    atomic_write(
        auth_path,
        "json.tmp",
        serde_json::to_string_pretty(&auth)?,
        "Failed to create Codex config directory",
        "Failed to write temporary Codex auth",
        "Failed to install Codex auth",
    )
}

fn restore_codex_config(
    codex_path: &std::path::Path,
    backup_path: &std::path::Path,
    missing_marker: &std::path::Path,
) -> Result<()> {
    restore_backup_or_remove_generated(
        RestoreSpec {
            target_path: codex_path,
            backup_path,
            missing_marker_path: missing_marker,
            temp_extension: "toml.tmp",
            missing_backup_is_ok: false,
            read_backup_context: "Failed to read backup file",
            copy_context: "Failed to copy backup to temporary file",
            rename_context: "Failed to restore Codex config",
            remove_backup_context: "Failed to remove backup file",
            remove_marker_context: "Failed to remove missing config marker",
            no_backup_message: "No Codex backup found. CCR is not activated for Codex.",
        },
        |backup_toml| {
            backup_toml
                .parse::<DocumentMut>()
                .context("Backup file is corrupted (invalid TOML)")?;
            Ok(())
        },
    )
}

fn restore_codex_auth(
    auth_path: &std::path::Path,
    backup_path: &std::path::Path,
    missing_marker: &std::path::Path,
) -> Result<()> {
    restore_backup_or_remove_generated(
        RestoreSpec {
            target_path: auth_path,
            backup_path,
            missing_marker_path: missing_marker,
            temp_extension: "json.tmp",
            missing_backup_is_ok: true,
            read_backup_context: "Failed to read auth backup",
            copy_context: "Failed to copy auth backup",
            rename_context: "Failed to restore Codex auth",
            remove_backup_context: "Failed to remove auth backup",
            remove_marker_context: "Failed to remove missing auth marker",
            no_backup_message: "",
        },
        |backup_json| {
            serde_json::from_str::<serde_json::Value>(backup_json)
                .context("Auth backup file is corrupted (invalid JSON)")?;
            Ok(())
        },
    )
}

fn build_codex_config(original: &str, port: u16) -> Result<String> {
    let mut doc = if original.trim().is_empty() {
        DocumentMut::new()
    } else {
        original
            .parse::<DocumentMut>()
            .context("Failed to parse Codex config TOML")?
    };

    if doc.get("model").and_then(|item| item.as_str()).is_none() {
        doc["model"] = value(DEFAULT_CODEX_MODEL);
    }
    doc["model_provider"] = value("ccr");
    doc["model_reasoning_effort"] = value("high");
    doc["disable_response_storage"] = value(true);

    if !doc
        .get("model_providers")
        .map(|item| item.is_table())
        .unwrap_or(false)
    {
        doc["model_providers"] = Item::Table(Table::new());
    }
    if !doc["model_providers"]
        .get("ccr")
        .map(|item| item.is_table())
        .unwrap_or(false)
    {
        doc["model_providers"]["ccr"] = Item::Table(Table::new());
    }

    let provider = &mut doc["model_providers"]["ccr"];
    provider["name"] = value("CCR");
    provider["base_url"] = value(local_v1_base_url(port));
    provider["wire_api"] = value("responses");
    provider["requires_openai_auth"] = value(true);

    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccr_types::Config;
    use tempfile::TempDir;

    #[test]
    fn codex_config_path_points_to_codex_toml() {
        let path = codex_config_path();
        assert!(path.to_string_lossy().contains(".codex"));
        assert!(path.ends_with("config.toml"));
    }

    #[test]
    fn build_codex_config_preserves_existing_tables() {
        let original = r#"
personality = "pragmatic"

[projects."/tmp/project"]
trust_level = "trusted"
"#;
        let out = build_codex_config(original, 3456).unwrap();
        let doc = out.parse::<DocumentMut>().unwrap();
        assert_eq!(doc["personality"].as_str(), Some("pragmatic"));
        assert_eq!(doc["model"].as_str(), Some(DEFAULT_CODEX_MODEL));
        assert_eq!(doc["model_provider"].as_str(), Some("ccr"));
        assert_eq!(doc["model_reasoning_effort"].as_str(), Some("high"));
        assert_eq!(doc["disable_response_storage"].as_bool(), Some(true));
        assert_eq!(
            doc["model_providers"]["ccr"]["base_url"].as_str(),
            Some("http://127.0.0.1:3456/v1")
        );
        assert_eq!(
            doc["model_providers"]["ccr"]["wire_api"].as_str(),
            Some("responses")
        );
        assert_eq!(
            doc["projects"]["/tmp/project"]["trust_level"].as_str(),
            Some("trusted")
        );
    }

    #[test]
    fn build_codex_config_from_empty_creates_minimal_provider() {
        let out = build_codex_config("", 4567).unwrap();
        let doc = out.parse::<DocumentMut>().unwrap();
        assert_eq!(doc["model"].as_str(), Some(DEFAULT_CODEX_MODEL));
        assert_eq!(doc["model_provider"].as_str(), Some("ccr"));
        assert_eq!(
            doc["model_providers"]["ccr"]["base_url"].as_str(),
            Some("http://127.0.0.1:4567/v1")
        );
        assert_eq!(
            doc["model_providers"]["ccr"]["requires_openai_auth"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn build_codex_config_points_to_local_v1_responses_provider() {
        let out = build_codex_config("", 3456).unwrap();
        let doc = out.parse::<DocumentMut>().unwrap();

        assert_eq!(doc["model_provider"].as_str(), Some("ccr"));
        assert_eq!(
            doc["model_providers"]["ccr"]["base_url"].as_str(),
            Some("http://127.0.0.1:3456/v1")
        );
        assert_eq!(
            doc["model_providers"]["ccr"]["wire_api"].as_str(),
            Some("responses")
        );
    }

    #[test]
    fn codex_points_to_ccr_matches_expected_local_v1_endpoint() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("config.toml");
        let out = build_codex_config("", 3456).unwrap();
        fs::write(&path, out).unwrap();

        assert!(codex_config_points_to_ccr(&path, 3456));
        assert!(!codex_config_points_to_ccr(&path, 4567));
    }

    #[test]
    fn codex_points_to_ccr_rejects_wrong_wire_api() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("config.toml");
        fs::write(
            &path,
            r#"
model_provider = "ccr"

[model_providers.ccr]
base_url = "http://127.0.0.1:3456/v1"
wire_api = "chat"
"#,
        )
        .unwrap();

        assert!(!codex_config_points_to_ccr(&path, 3456));
    }

    #[test]
    fn codex_points_to_ccr_rejects_missing_provider_without_panic() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("config.toml");
        fs::write(&path, r#"model_provider = "openai""#).unwrap();

        assert!(!codex_config_points_to_ccr(&path, 3456));
    }

    #[test]
    fn build_codex_config_replaces_existing_ccr_provider() {
        let original = r#"
model = "old-model"
model_provider = "openai"

[model_providers.ccr]
name = "Old CCR"
base_url = "http://127.0.0.1:1111/v1"
wire_api = "chat"
requires_openai_auth = true
"#;
        let out = build_codex_config(original, 6789).unwrap();
        let doc = out.parse::<DocumentMut>().unwrap();
        assert_eq!(doc["model"].as_str(), Some("old-model"));
        assert_eq!(doc["model_provider"].as_str(), Some("ccr"));
        assert_eq!(doc["model_providers"]["ccr"]["name"].as_str(), Some("CCR"));
        assert_eq!(
            doc["model_providers"]["ccr"]["base_url"].as_str(),
            Some("http://127.0.0.1:6789/v1")
        );
        assert_eq!(
            doc["model_providers"]["ccr"]["wire_api"].as_str(),
            Some("responses")
        );
        assert_eq!(
            doc["model_providers"]["ccr"]["requires_openai_auth"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn build_codex_config_rejects_invalid_toml() {
        let err = build_codex_config("not = [valid", 3456)
            .unwrap_err()
            .to_string();
        assert!(err.contains("Failed to parse Codex config TOML"));
    }

    #[test]
    fn codex_port_from_config_uses_config_port() {
        let config = Config {
            port: Some(4567),
            ..Default::default()
        };

        assert_eq!(codex_port_from_config(&config), 4567);
    }

    #[test]
    fn codex_port_from_config_does_not_require_route_pool() {
        let config = Config::default();

        assert_eq!(codex_port_from_config(&config), 3456);
    }

    #[test]
    fn install_codex_config_writes_provider_atomically() {
        let temp = TempDir::new().unwrap();
        let codex_path = temp.path().join("config.toml");

        install_codex_config(&codex_path, 7890, "").unwrap();

        let installed = fs::read_to_string(codex_path).unwrap();
        let doc = installed.parse::<DocumentMut>().unwrap();
        assert_eq!(
            doc["model_providers"]["ccr"]["base_url"].as_str(),
            Some("http://127.0.0.1:7890/v1")
        );
        assert!(!temp.path().join("config.toml.tmp").exists());
    }

    #[test]
    fn install_codex_config_rejects_invalid_existing_config() {
        let temp = TempDir::new().unwrap();
        let codex_path = temp.path().join("config.toml");

        let err = install_codex_config(&codex_path, 3456, "bad = [")
            .unwrap_err()
            .to_string();

        assert!(err.contains("Failed to parse Codex config TOML"));
        assert!(!codex_path.exists());
    }

    #[test]
    fn install_codex_auth_writes_openai_key_placeholder() {
        let temp = TempDir::new().unwrap();
        let auth_path = temp.path().join("auth.json");

        install_codex_auth(&auth_path).unwrap();

        let auth: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(auth_path).unwrap()).unwrap();
        assert_eq!(auth["OPENAI_API_KEY"], "any");
    }

    #[test]
    fn prepare_codex_auth_activation_files_backs_up_existing_auth() {
        let temp = TempDir::new().unwrap();
        let auth_path = temp.path().join(".codex").join("auth.json");
        let backup_path = temp.path().join("backups").join("auth.json");
        let marker_path = temp.path().join("backups").join("auth.missing");
        fs::create_dir_all(auth_path.parent().unwrap()).unwrap();
        fs::write(&auth_path, r#"{"OPENAI_API_KEY":"real"}"#).unwrap();

        prepare_codex_auth_activation_files(&auth_path, &backup_path, &marker_path).unwrap();

        assert_eq!(
            fs::read_to_string(&backup_path).unwrap(),
            r#"{"OPENAI_API_KEY":"real"}"#
        );
        assert!(!marker_path.exists());
    }

    #[test]
    fn restore_codex_auth_restores_backup() {
        let temp = TempDir::new().unwrap();
        let auth_path = temp.path().join("auth.json");
        let backup_path = temp.path().join("backup.json");
        let marker_path = temp.path().join("missing");

        fs::write(&auth_path, r#"{"OPENAI_API_KEY":"any"}"#).unwrap();
        fs::write(&backup_path, r#"{"OPENAI_API_KEY":"real"}"#).unwrap();

        restore_codex_auth(&auth_path, &backup_path, &marker_path).unwrap();

        assert_eq!(
            fs::read_to_string(&auth_path).unwrap(),
            r#"{"OPENAI_API_KEY":"real"}"#
        );
        assert!(!backup_path.exists());
    }

    #[test]
    fn prepare_codex_activation_files_backs_up_existing_config() {
        let temp = TempDir::new().unwrap();
        let codex_path = temp.path().join(".codex").join("config.toml");
        let backup_path = temp.path().join("backups").join("backup.toml");
        let timestamped_backup = temp.path().join("backups").join("backup.1.toml");
        let marker_path = temp.path().join("backups").join("missing");
        fs::create_dir_all(codex_path.parent().unwrap()).unwrap();
        fs::write(&codex_path, "model = \"original\"\n").unwrap();

        let original = prepare_codex_activation_files(
            &codex_path,
            &backup_path,
            &timestamped_backup,
            &marker_path,
        )
        .unwrap();

        assert_eq!(original, "model = \"original\"\n");
        assert_eq!(
            fs::read_to_string(&backup_path).unwrap(),
            "model = \"original\"\n"
        );
        assert_eq!(
            fs::read_to_string(&timestamped_backup).unwrap(),
            "model = \"original\"\n"
        );
        assert!(!marker_path.exists());
    }

    #[test]
    fn prepare_codex_activation_files_marks_missing_original() {
        let temp = TempDir::new().unwrap();
        let codex_path = temp.path().join(".codex").join("config.toml");
        let backup_path = temp.path().join("backups").join("backup.toml");
        let timestamped_backup = temp.path().join("backups").join("backup.1.toml");
        let marker_path = temp.path().join("backups").join("missing");

        let original = prepare_codex_activation_files(
            &codex_path,
            &backup_path,
            &timestamped_backup,
            &marker_path,
        )
        .unwrap();

        assert_eq!(original, "");
        assert!(marker_path.exists());
        assert!(!backup_path.exists());
        assert!(!timestamped_backup.exists());
    }

    #[test]
    fn prepare_codex_activation_files_rejects_existing_backup() {
        let temp = TempDir::new().unwrap();
        let codex_path = temp.path().join(".codex").join("config.toml");
        let backup_path = temp.path().join("backups").join("backup.toml");
        let timestamped_backup = temp.path().join("backups").join("backup.1.toml");
        let marker_path = temp.path().join("backups").join("missing");
        fs::create_dir_all(backup_path.parent().unwrap()).unwrap();
        fs::write(&backup_path, "model = \"original\"\n").unwrap();

        let err = prepare_codex_activation_files(
            &codex_path,
            &backup_path,
            &timestamped_backup,
            &marker_path,
        )
        .unwrap_err()
        .to_string();

        assert!(err.contains("Codex is already activated"));
    }

    #[test]
    fn restore_codex_config_restores_backup_and_removes_marker() {
        let temp = TempDir::new().unwrap();
        let codex_path = temp.path().join("config.toml");
        let backup_path = temp.path().join("backup.toml");
        let marker_path = temp.path().join("missing");

        fs::write(&codex_path, "model = \"ccr\"\n").unwrap();
        fs::write(&backup_path, "model = \"original\"\n").unwrap();
        fs::write(&marker_path, "missing").unwrap();

        restore_codex_config(&codex_path, &backup_path, &marker_path).unwrap();

        assert_eq!(
            fs::read_to_string(&codex_path).unwrap(),
            "model = \"original\"\n"
        );
        assert!(!backup_path.exists());
        assert!(!marker_path.exists());
    }

    #[test]
    fn restore_codex_config_removes_generated_config_when_original_was_missing() {
        let temp = TempDir::new().unwrap();
        let codex_path = temp.path().join("config.toml");
        let backup_path = temp.path().join("backup.toml");
        let marker_path = temp.path().join("missing");

        fs::write(&codex_path, "model = \"ccr\"\n").unwrap();
        fs::write(&marker_path, "missing").unwrap();

        restore_codex_config(&codex_path, &backup_path, &marker_path).unwrap();

        assert!(!codex_path.exists());
        assert!(!marker_path.exists());
    }

    #[test]
    fn restore_codex_config_rejects_invalid_backup() {
        let temp = TempDir::new().unwrap();
        let codex_path = temp.path().join("config.toml");
        let backup_path = temp.path().join("backup.toml");
        let marker_path = temp.path().join("missing");

        fs::write(&backup_path, "bad = [").unwrap();

        let err = restore_codex_config(&codex_path, &backup_path, &marker_path)
            .unwrap_err()
            .to_string();

        assert!(err.contains("Backup file is corrupted"));
        assert!(backup_path.exists());
    }

    #[test]
    fn restore_codex_config_errors_without_backup_or_marker() {
        let temp = TempDir::new().unwrap();
        let codex_path = temp.path().join("config.toml");
        let backup_path = temp.path().join("backup.toml");
        let marker_path = temp.path().join("missing");

        let err = restore_codex_config(&codex_path, &backup_path, &marker_path)
            .unwrap_err()
            .to_string();

        assert!(err.contains("No Codex backup found"));
    }
}
