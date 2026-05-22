use crate::status::AdditiveClientSnapshot;
use anyhow::{Context, Result};
use ccr_config::{default_config_path, load_config};
use ccr_types::{AppSettings, Config};
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn ccr_config() -> Result<Config> {
    load_config(&default_config_path()).context("Failed to load CCR config")
}

pub(super) fn ccr_port(config: &Config) -> u16 {
    config.port.unwrap_or(3456)
}

pub(super) fn configured_path_from_settings(
    configured_path: impl FnOnce(&AppSettings) -> Option<String>,
    default_path: impl FnOnce() -> PathBuf,
) -> PathBuf {
    let configured_path = load_config(&default_config_path())
        .ok()
        .and_then(|config| configured_path(&config.app_settings));
    configured_path_or_default(configured_path, default_path)
}

pub(super) fn configured_path_or_default(
    configured_path: Option<String>,
    default_path: impl FnOnce() -> PathBuf,
) -> PathBuf {
    configured_path
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_path)
}

pub(super) fn first_route_pool_client_model(
    config: &Config,
    default_model: &str,
) -> Result<String> {
    let route = config
        .first_route_pool_route()
        .ok_or_else(|| anyhow::anyhow!("Route Pool is not configured or has no enabled routes"))?;
    Ok(client_model_from_route(route, default_model))
}

pub(super) fn client_model_from_route(route: &str, default_model: &str) -> String {
    let route = route.trim();
    if route.is_empty() {
        return default_model.to_string();
    }
    let model = route.split_once(',').map(|(_, model)| model.trim());
    match model {
        Some(model) if !model.is_empty() => model.to_string(),
        _ => default_model.to_string(),
    }
}

pub(super) fn local_v1_base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/v1")
}

pub(super) fn additive_client_snapshot(
    path: PathBuf,
    port: u16,
    points_to_ccr: impl FnOnce(&Path, u16) -> bool,
    provider_exists: impl FnOnce(&Path) -> bool,
) -> AdditiveClientSnapshot {
    let path_label = path.display().to_string();
    if points_to_ccr(&path, port) {
        AdditiveClientSnapshot::ProviderCurrent { path: path_label }
    } else if provider_exists(&path) {
        AdditiveClientSnapshot::ProviderDrifted { path: path_label }
    } else {
        AdditiveClientSnapshot::Missing { path: path_label }
    }
}

pub(super) fn atomic_write(
    path: &Path,
    temp_extension: &str,
    content: impl AsRef<[u8]>,
    create_dir_context: &'static str,
    write_context: &'static str,
    rename_context: &'static str,
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context(create_dir_context)?;
    }
    let temp_path = path.with_extension(temp_extension);
    fs::write(&temp_path, content).context(write_context)?;
    set_secure_permissions(&temp_path)?;
    fs::rename(&temp_path, path).context(rename_context)?;
    Ok(())
}

pub(super) struct BackupSpec<'a> {
    pub source_path: &'a Path,
    pub backup_path: &'a Path,
    pub timestamped_backup_path: Option<&'a Path>,
    pub missing_marker_path: &'a Path,
    pub already_active_message: &'static str,
    pub source_dir_context: &'static str,
    pub backup_dir_context: &'static str,
    pub read_context: &'static str,
    pub backup_context: &'static str,
    pub timestamped_backup_context: &'static str,
    pub missing_marker_context: &'static str,
}

pub(super) fn backup_or_mark_missing(
    spec: BackupSpec<'_>,
    validate_existing: impl FnOnce(&str) -> Result<()>,
) -> Result<String> {
    if spec.backup_path.exists() || spec.missing_marker_path.exists() {
        anyhow::bail!("{}", spec.already_active_message);
    }

    if let Some(parent) = spec.backup_path.parent() {
        fs::create_dir_all(parent).context(spec.backup_dir_context)?;
    }
    if let Some(parent) = spec.source_path.parent() {
        fs::create_dir_all(parent).context(spec.source_dir_context)?;
    }

    if spec.source_path.exists() {
        let content = fs::read_to_string(spec.source_path).context(spec.read_context)?;
        validate_existing(&content)?;
        fs::copy(spec.source_path, spec.backup_path).context(spec.backup_context)?;
        set_secure_permissions(spec.backup_path)?;
        if let Some(timestamped_backup_path) = spec.timestamped_backup_path {
            fs::copy(spec.source_path, timestamped_backup_path)
                .context(spec.timestamped_backup_context)?;
            set_secure_permissions(timestamped_backup_path)?;
        }
        Ok(content)
    } else {
        fs::write(spec.missing_marker_path, b"missing").context(spec.missing_marker_context)?;
        set_secure_permissions(spec.missing_marker_path)?;
        Ok(String::new())
    }
}

pub(super) struct RestoreSpec<'a> {
    pub target_path: &'a Path,
    pub backup_path: &'a Path,
    pub missing_marker_path: &'a Path,
    pub temp_extension: &'a str,
    pub missing_backup_is_ok: bool,
    pub read_backup_context: &'static str,
    pub copy_context: &'static str,
    pub rename_context: &'static str,
    pub remove_backup_context: &'static str,
    pub remove_marker_context: &'static str,
    pub no_backup_message: &'static str,
}

pub(super) fn restore_backup_or_remove_generated(
    spec: RestoreSpec<'_>,
    validate_backup: impl FnOnce(&str) -> Result<()>,
) -> Result<()> {
    if spec.backup_path.exists() {
        let backup_content =
            fs::read_to_string(spec.backup_path).context(spec.read_backup_context)?;
        validate_backup(&backup_content)?;

        let temp_path = spec.target_path.with_extension(spec.temp_extension);
        fs::copy(spec.backup_path, &temp_path).context(spec.copy_context)?;
        set_secure_permissions(&temp_path)?;
        fs::rename(&temp_path, spec.target_path).context(spec.rename_context)?;
        fs::remove_file(spec.backup_path).context(spec.remove_backup_context)?;
        fs::remove_file(spec.missing_marker_path).ok();
    } else if spec.missing_marker_path.exists() {
        fs::remove_file(spec.target_path).ok();
        fs::remove_file(spec.missing_marker_path).context(spec.remove_marker_context)?;
    } else if !spec.missing_backup_is_ok {
        anyhow::bail!("{}", spec.no_backup_message);
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn set_secure_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(windows)]
pub(super) fn set_secure_permissions(path: &Path) -> Result<()> {
    use anyhow::Context;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, LocalFree,
    };
    use windows_sys::Win32::Security::Authorization::{
        EXPLICIT_ACCESS_W, SE_FILE_OBJECT, SET_ACCESS, SetEntriesInAclW, SetNamedSecurityInfoW,
        TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_W,
    };
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, GetTokenInformation, NO_INHERITANCE,
        PROTECTED_DACL_SECURITY_INFORMATION, TOKEN_QUERY, TOKEN_USER, TokenUser,
    };
    use windows_sys::Win32::Storage::FileSystem::{FILE_GENERIC_READ, FILE_GENERIC_WRITE};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token = null_mut();
    let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) };
    if opened == 0 {
        return Err(std::io::Error::last_os_error())
            .context("Failed to open process token for client config ACL");
    }

    let result = (|| {
        let mut token_info_len = 0;
        let queried =
            unsafe { GetTokenInformation(token, TokenUser, null_mut(), 0, &mut token_info_len) };
        if queried == 0
            && std::io::Error::last_os_error().raw_os_error()
                != Some(ERROR_INSUFFICIENT_BUFFER as i32)
        {
            return Err(std::io::Error::last_os_error())
                .context("Failed to size current user token information");
        }

        let mut token_info = vec![0u8; token_info_len as usize];
        let queried = unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                token_info.as_mut_ptr().cast(),
                token_info_len,
                &mut token_info_len,
            )
        };
        if queried == 0 {
            return Err(std::io::Error::last_os_error())
                .context("Failed to read current user token information");
        }

        let token_user = unsafe { &*(token_info.as_ptr().cast::<TOKEN_USER>()) };
        let user_sid = token_user.User.Sid;
        let trustee = TRUSTEE_W {
            pMultipleTrustee: null_mut(),
            MultipleTrusteeOperation: 0,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_USER,
            ptstrName: user_sid.cast(),
        };
        let access = EXPLICIT_ACCESS_W {
            grfAccessPermissions: FILE_GENERIC_READ | FILE_GENERIC_WRITE,
            grfAccessMode: SET_ACCESS,
            grfInheritance: NO_INHERITANCE,
            Trustee: trustee,
        };

        let mut acl = null_mut();
        let status = unsafe { SetEntriesInAclW(1, &access, null(), &mut acl) };
        if status != ERROR_SUCCESS {
            return Err(std::io::Error::from_raw_os_error(status as i32))
                .context("Failed to build client config ACL");
        }

        let mut path_wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let security_info = DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION;
        let status = unsafe {
            SetNamedSecurityInfoW(
                path_wide.as_mut_ptr(),
                SE_FILE_OBJECT,
                security_info,
                null_mut(),
                null_mut(),
                acl,
                null(),
            )
        };

        unsafe {
            LocalFree(acl.cast());
        }

        if status != ERROR_SUCCESS {
            return Err(std::io::Error::from_raw_os_error(status as i32)).with_context(|| {
                format!("Failed to set client config ACL for {}", path.display())
            });
        }

        Ok(())
    })();

    unsafe {
        CloseHandle(token);
    }

    result
}

#[cfg(not(any(unix, windows)))]
pub(super) fn set_secure_permissions(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_model_uses_model_part_only() {
        assert_eq!(
            client_model_from_route("openai,gpt-5-codex", "fallback"),
            "gpt-5-codex"
        );
        assert_eq!(
            client_model_from_route("anthropic, claude-sonnet-4-6 ", "fallback"),
            "claude-sonnet-4-6"
        );
        assert_eq!(client_model_from_route("openai", "fallback"), "fallback");
        assert_eq!(client_model_from_route("openai,", "fallback"), "fallback");
        assert_eq!(client_model_from_route("", "fallback"), "fallback");
    }

    #[test]
    fn configured_path_uses_override_or_default() {
        assert_eq!(
            configured_path_or_default(Some("/tmp/client.json".to_string()), || PathBuf::from(
                "/default"
            )),
            PathBuf::from("/tmp/client.json")
        );
        assert_eq!(
            configured_path_or_default(Some("  ".to_string()), || PathBuf::from("/default")),
            PathBuf::from("/default")
        );
        assert_eq!(
            configured_path_or_default(None, || PathBuf::from("/default")),
            PathBuf::from("/default")
        );
    }

    #[test]
    fn additive_snapshot_reports_current_drifted_and_missing() {
        let path = PathBuf::from("/tmp/client.json");
        assert_eq!(
            additive_client_snapshot(path.clone(), 3456, |_, _| true, |_| false),
            AdditiveClientSnapshot::ProviderCurrent {
                path: path.display().to_string()
            }
        );
        assert_eq!(
            additive_client_snapshot(path.clone(), 3456, |_, _| false, |_| true),
            AdditiveClientSnapshot::ProviderDrifted {
                path: path.display().to_string()
            }
        );
        assert_eq!(
            additive_client_snapshot(path.clone(), 3456, |_, _| false, |_| false),
            AdditiveClientSnapshot::Missing {
                path: path.display().to_string()
            }
        );
    }
}
