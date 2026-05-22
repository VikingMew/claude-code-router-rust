use ccr_config::{default_config_path, load_config};
use ccr_types::{AppSettings, Provider};
use serde_json::Value;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

pub const ANTHROPIC_OFFICIAL_SECRET: &str = "ccr-secret://official/anthropic";
pub const OPENAI_CODEX_OFFICIAL_SECRET: &str = "ccr-secret://official/openai-codex";
pub const GITHUB_COPILOT_OFFICIAL_SECRET: &str = "ccr-secret://official/github-copilot";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficialProviderKind {
    Anthropic,
    OpenAiCodex,
    GitHubCopilot,
}

impl OfficialProviderKind {
    pub fn label(self) -> &'static str {
        match self {
            OfficialProviderKind::Anthropic => "Anthropic Official",
            OfficialProviderKind::OpenAiCodex => "OpenAI / Codex Official",
            OfficialProviderKind::GitHubCopilot => "GitHub Copilot Official",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficialCredentialState {
    Available,
    AvailableFromCcrBackup,
    Missing,
    UnsupportedSafeRead,
}

impl OfficialCredentialState {
    pub fn label(self) -> &'static str {
        match self {
            OfficialCredentialState::Available => "Available",
            OfficialCredentialState::AvailableFromCcrBackup => "Available from CCR backup",
            OfficialCredentialState::Missing => "Not logged in or not found",
            OfficialCredentialState::UnsupportedSafeRead => "Official source cannot be safely read",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfficialCredentialStatus {
    pub kind: OfficialProviderKind,
    pub state: OfficialCredentialState,
    pub detail: String,
}

impl OfficialCredentialStatus {
    pub fn is_usable(&self) -> bool {
        matches!(
            self.state,
            OfficialCredentialState::Available | OfficialCredentialState::AvailableFromCcrBackup
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretResolutionError {
    Missing {
        kind: OfficialProviderKind,
        detail: String,
    },
    UnsupportedSafeRead {
        kind: OfficialProviderKind,
        detail: String,
    },
}

impl fmt::Display for SecretResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecretResolutionError::Missing { kind, detail } => {
                write!(f, "{} credential is unavailable: {detail}", kind.label())
            }
            SecretResolutionError::UnsupportedSafeRead { kind, detail } => {
                write!(f, "{} credential is unsupported: {detail}", kind.label())
            }
        }
    }
}

impl std::error::Error for SecretResolutionError {}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SecretCandidate {
    value: String,
    from_backup: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OfficialPaths {
    claude_settings_path: PathBuf,
    claude_backup_path: PathBuf,
    codex_auth_path: PathBuf,
    codex_auth_backup_path: PathBuf,
}

pub fn official_kind_from_secret_marker(marker: &str) -> Option<OfficialProviderKind> {
    match marker.trim() {
        ANTHROPIC_OFFICIAL_SECRET => Some(OfficialProviderKind::Anthropic),
        OPENAI_CODEX_OFFICIAL_SECRET => Some(OfficialProviderKind::OpenAiCodex),
        GITHUB_COPILOT_OFFICIAL_SECRET => Some(OfficialProviderKind::GitHubCopilot),
        _ => None,
    }
}

pub fn is_official_secret_marker(marker: &str) -> bool {
    official_kind_from_secret_marker(marker).is_some()
}

pub fn official_statuses() -> Vec<OfficialCredentialStatus> {
    let paths = official_paths_from_settings(&load_app_settings());
    vec![
        anthropic_status_for_paths(&paths),
        openai_codex_status_for_paths(&paths),
        github_copilot_status(),
    ]
}

pub fn official_status_for_provider(provider: &Provider) -> Option<OfficialCredentialStatus> {
    let kind = official_kind_from_secret_marker(&provider.api_key)?;
    let paths = official_paths_from_settings(&load_app_settings());
    Some(match kind {
        OfficialProviderKind::Anthropic => anthropic_status_for_paths(&paths),
        OfficialProviderKind::OpenAiCodex => openai_codex_status_for_paths(&paths),
        OfficialProviderKind::GitHubCopilot => github_copilot_status(),
    })
}

pub fn resolve_provider_api_key(provider: &Provider) -> Result<String, SecretResolutionError> {
    let Some(kind) = official_kind_from_secret_marker(&provider.api_key) else {
        return Ok(provider.api_key.clone());
    };

    let paths = official_paths_from_settings(&load_app_settings());
    let candidate = match kind {
        OfficialProviderKind::Anthropic => anthropic_secret_for_paths(&paths),
        OfficialProviderKind::OpenAiCodex => openai_codex_secret_for_paths(&paths),
        OfficialProviderKind::GitHubCopilot => None,
    };

    match (kind, candidate) {
        (_, Some(candidate)) => Ok(candidate.value),
        (OfficialProviderKind::GitHubCopilot, None) => Err(
            SecretResolutionError::UnsupportedSafeRead {
                kind,
                detail: "GitHub documents Copilot LLM auth for Copilot agents via an agent-provided GitHub token, but CCR has no stable official local token source to read."
                    .to_string(),
            },
        ),
        (_, None) => Err(SecretResolutionError::Missing {
            kind,
            detail: "No usable login credential was found in the official client file or CCR-managed activation backup."
                .to_string(),
        }),
    }
}

fn load_app_settings() -> AppSettings {
    load_config(&default_config_path())
        .map(|config| config.app_settings)
        .unwrap_or_default()
}

fn official_paths_from_settings(settings: &AppSettings) -> OfficialPaths {
    let home = dirs_next::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let ccr_backups = home.join(".claude-code-router").join("backups");
    let claude_settings_path = settings
        .claude_config_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude").join("settings.json"));
    let codex_config_path = settings
        .codex_config_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex").join("config.toml"));
    let codex_auth_path = codex_config_path
        .parent()
        .map(|parent| parent.join("auth.json"))
        .unwrap_or_else(|| home.join(".codex").join("auth.json"));

    OfficialPaths {
        claude_settings_path,
        claude_backup_path: ccr_backups.join("claude-config.backup.json"),
        codex_auth_path,
        codex_auth_backup_path: ccr_backups.join("codex-auth.backup.json"),
    }
}

fn anthropic_status_for_paths(paths: &OfficialPaths) -> OfficialCredentialStatus {
    match anthropic_secret_for_paths(paths) {
        Some(secret) if secret.from_backup => OfficialCredentialStatus {
            kind: OfficialProviderKind::Anthropic,
            state: OfficialCredentialState::AvailableFromCcrBackup,
            detail: "Claude/Anthropic login credential found in CCR activation backup.".to_string(),
        },
        Some(_) => OfficialCredentialStatus {
            kind: OfficialProviderKind::Anthropic,
            state: OfficialCredentialState::Available,
            detail: "Claude/Anthropic login credential found in the official client settings."
                .to_string(),
        },
        None => OfficialCredentialStatus {
            kind: OfficialProviderKind::Anthropic,
            state: OfficialCredentialState::Missing,
            detail: "No usable Anthropic credential found in Claude settings or CCR backup."
                .to_string(),
        },
    }
}

fn openai_codex_status_for_paths(paths: &OfficialPaths) -> OfficialCredentialStatus {
    match openai_codex_secret_for_paths(paths) {
        Some(secret) if secret.from_backup => OfficialCredentialStatus {
            kind: OfficialProviderKind::OpenAiCodex,
            state: OfficialCredentialState::AvailableFromCcrBackup,
            detail: "Codex/OpenAI credential found in CCR activation backup.".to_string(),
        },
        Some(_) => OfficialCredentialStatus {
            kind: OfficialProviderKind::OpenAiCodex,
            state: OfficialCredentialState::Available,
            detail: "Codex/OpenAI credential found in the official Codex auth file.".to_string(),
        },
        None => OfficialCredentialStatus {
            kind: OfficialProviderKind::OpenAiCodex,
            state: OfficialCredentialState::Missing,
            detail: "No usable OpenAI credential found in Codex auth or CCR backup.".to_string(),
        },
    }
}

fn github_copilot_status() -> OfficialCredentialStatus {
    OfficialCredentialStatus {
        kind: OfficialProviderKind::GitHubCopilot,
        state: OfficialCredentialState::UnsupportedSafeRead,
        detail: "GitHub documents Copilot LLM auth for Copilot agents using a GitHub token sent to the agent; CCR does not have a stable official local Copilot token source to read."
            .to_string(),
    }
}

fn anthropic_secret_for_paths(paths: &OfficialPaths) -> Option<SecretCandidate> {
    read_anthropic_secret(&paths.claude_settings_path, false)
        .or_else(|| read_anthropic_secret(&paths.claude_backup_path, true))
}

fn openai_codex_secret_for_paths(paths: &OfficialPaths) -> Option<SecretCandidate> {
    read_openai_secret(&paths.codex_auth_path, false)
        .or_else(|| read_openai_secret(&paths.codex_auth_backup_path, true))
}

fn read_anthropic_secret(path: &Path, from_backup: bool) -> Option<SecretCandidate> {
    let content = fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&content).ok()?;
    let env = value.get("env").and_then(Value::as_object);
    ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"]
        .iter()
        .find_map(|key| {
            env.and_then(|env| env.get(*key))
                .or_else(|| value.get(*key))
                .and_then(Value::as_str)
                .and_then(|secret| usable_secret(secret, from_backup))
        })
}

fn read_openai_secret(path: &Path, from_backup: bool) -> Option<SecretCandidate> {
    let content = fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&content).ok()?;
    ["OPENAI_API_KEY", "api_key"]
        .iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .and_then(|secret| usable_secret(secret, from_backup))
}

fn usable_secret(secret: &str, from_backup: bool) -> Option<SecretCandidate> {
    let secret = secret.trim();
    if secret.is_empty()
        || secret == "any"
        || secret.eq_ignore_ascii_case("placeholder")
        || secret.starts_with("ccr-secret://")
    {
        return None;
    }
    Some(SecretCandidate {
        value: secret.to_string(),
        from_backup,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn paths(root: &Path) -> OfficialPaths {
        OfficialPaths {
            claude_settings_path: root.join("claude").join("settings.json"),
            claude_backup_path: root.join("backups").join("claude.json"),
            codex_auth_path: root.join("codex").join("auth.json"),
            codex_auth_backup_path: root.join("backups").join("codex-auth.json"),
        }
    }

    #[test]
    fn anthropic_status_uses_current_without_exposing_secret() {
        let temp = TempDir::new().unwrap();
        let paths = paths(temp.path());
        fs::create_dir_all(paths.claude_settings_path.parent().unwrap()).unwrap();
        fs::write(
            &paths.claude_settings_path,
            r#"{"env":{"ANTHROPIC_API_KEY":"sk-ant-real"}}"#,
        )
        .unwrap();

        let status = anthropic_status_for_paths(&paths);
        assert_eq!(status.state, OfficialCredentialState::Available);
        assert!(!status.detail.contains("sk-ant-real"));
        assert_eq!(
            anthropic_secret_for_paths(&paths).unwrap().value,
            "sk-ant-real"
        );
    }

    #[test]
    fn anthropic_status_uses_backup_after_activation_placeholder() {
        let temp = TempDir::new().unwrap();
        let paths = paths(temp.path());
        fs::create_dir_all(paths.claude_settings_path.parent().unwrap()).unwrap();
        fs::create_dir_all(paths.claude_backup_path.parent().unwrap()).unwrap();
        fs::write(
            &paths.claude_settings_path,
            r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"any"}}"#,
        )
        .unwrap();
        fs::write(
            &paths.claude_backup_path,
            r#"{"env":{"ANTHROPIC_AUTH_TOKEN":"sk-ant-backup"}}"#,
        )
        .unwrap();

        let status = anthropic_status_for_paths(&paths);
        assert_eq!(
            status.state,
            OfficialCredentialState::AvailableFromCcrBackup
        );
        assert!(!status.detail.contains("sk-ant-backup"));
        assert_eq!(
            anthropic_secret_for_paths(&paths).unwrap().value,
            "sk-ant-backup"
        );
    }

    #[test]
    fn codex_status_uses_backup_after_activation_placeholder() {
        let temp = TempDir::new().unwrap();
        let paths = paths(temp.path());
        fs::create_dir_all(paths.codex_auth_path.parent().unwrap()).unwrap();
        fs::create_dir_all(paths.codex_auth_backup_path.parent().unwrap()).unwrap();
        fs::write(&paths.codex_auth_path, r#"{"OPENAI_API_KEY":"any"}"#).unwrap();
        fs::write(
            &paths.codex_auth_backup_path,
            r#"{"OPENAI_API_KEY":"sk-openai-backup"}"#,
        )
        .unwrap();

        let status = openai_codex_status_for_paths(&paths);
        assert_eq!(
            status.state,
            OfficialCredentialState::AvailableFromCcrBackup
        );
        assert!(!status.detail.contains("sk-openai-backup"));
        assert_eq!(
            openai_codex_secret_for_paths(&paths).unwrap().value,
            "sk-openai-backup"
        );
    }

    #[test]
    fn copilot_status_is_unsupported_safe_read() {
        let status = github_copilot_status();
        assert_eq!(status.state, OfficialCredentialState::UnsupportedSafeRead);
        assert!(
            status
                .detail
                .contains("does not have a stable official local")
        );
    }

    #[test]
    fn recognizes_only_official_secret_markers() {
        assert_eq!(
            official_kind_from_secret_marker(ANTHROPIC_OFFICIAL_SECRET),
            Some(OfficialProviderKind::Anthropic)
        );
        assert_eq!(official_kind_from_secret_marker("sk-real"), None);
    }
}
