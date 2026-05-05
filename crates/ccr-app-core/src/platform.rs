use anyhow::{Result, anyhow};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoLaunchState {
    Enabled,
    Disabled,
    Unsupported,
}

pub trait AutoLaunchAdapter {
    fn status(&self) -> AutoLaunchState;
    fn enable(&self, app_exe: Option<PathBuf>) -> Result<()>;
    fn disable(&self) -> Result<()>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemAutoLaunchAdapter;

impl AutoLaunchAdapter for SystemAutoLaunchAdapter {
    fn status(&self) -> AutoLaunchState {
        auto_launch_status()
    }

    fn enable(&self, app_exe: Option<PathBuf>) -> Result<()> {
        enable_auto_launch(app_exe)
    }

    fn disable(&self) -> Result<()> {
        disable_auto_launch()
    }
}

pub fn auto_launch_status() -> AutoLaunchState {
    if !platform_supported() {
        return AutoLaunchState::Unsupported;
    }
    if auto_launch_marker_path().is_some_and(|path| path.exists()) {
        AutoLaunchState::Enabled
    } else {
        AutoLaunchState::Disabled
    }
}

pub fn enable_auto_launch(app_exe: Option<PathBuf>) -> Result<()> {
    if !platform_supported() {
        return Err(anyhow!("Auto launch is not supported on this platform"));
    }
    let app_exe = app_exe.unwrap_or(std::env::current_exe()?);
    let marker = auto_launch_marker_path().ok_or_else(|| anyhow!("No auto launch path"))?;
    if let Some(parent) = marker.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&marker, auto_launch_file_content(&app_exe))?;
    Ok(())
}

pub fn disable_auto_launch() -> Result<()> {
    if !platform_supported() {
        return Err(anyhow!("Auto launch is not supported on this platform"));
    }
    if let Some(marker) = auto_launch_marker_path() {
        std::fs::remove_file(marker).ok();
    }
    Ok(())
}

fn platform_supported() -> bool {
    if std::env::var("CCR_AUTO_LAUNCH_PATH").is_ok() {
        return true;
    }
    cfg!(target_os = "macos") || cfg!(target_os = "windows")
}

fn auto_launch_marker_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CCR_AUTO_LAUNCH_PATH") {
        return Some(PathBuf::from(path));
    }

    #[cfg(target_os = "macos")]
    {
        return dirs_next::home_dir().map(|home| {
            home.join("Library")
                .join("LaunchAgents")
                .join("studio.musi.ccr-ui.plist")
        });
    }

    #[cfg(target_os = "windows")]
    {
        return dirs_next::data_dir().map(|data| {
            data.join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Startup")
                .join("ccr-ui.cmd")
        });
    }

    #[allow(unreachable_code)]
    None
}

fn auto_launch_file_content(app_exe: &std::path::Path) -> String {
    #[cfg(target_os = "macos")]
    {
        return format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>studio.musi.ccr-ui</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
</dict>
</plist>
"#,
            app_exe.display()
        );
    }

    #[cfg(target_os = "windows")]
    {
        return format!(r#"start "" "{}""#, app_exe.display());
    }

    #[allow(unreachable_code)]
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use tempfile::TempDir;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn unsupported_status_is_stable() {
        if !platform_supported() {
            assert_eq!(auto_launch_status(), AutoLaunchState::Unsupported);
        }
    }

    #[test]
    fn generated_content_contains_executable_path() {
        let path = PathBuf::from("/tmp/ccr-ui");
        let content = auto_launch_file_content(&path);
        if platform_supported() {
            assert!(content.contains("ccr-ui"));
        }
    }

    #[test]
    fn enable_and_disable_auto_launch_with_override_path() {
        let _guard = ENV_LOCK.lock().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let marker = temp_dir.path().join("startup.marker");
        unsafe { std::env::set_var("CCR_AUTO_LAUNCH_PATH", &marker) };

        assert_eq!(auto_launch_status(), AutoLaunchState::Disabled);
        enable_auto_launch(Some(PathBuf::from("/tmp/ccr-ui"))).unwrap();
        assert_eq!(auto_launch_status(), AutoLaunchState::Enabled);
        assert!(std::fs::read_to_string(&marker).unwrap().contains("ccr-ui"));
        disable_auto_launch().unwrap();
        assert_eq!(auto_launch_status(), AutoLaunchState::Disabled);

        unsafe { std::env::remove_var("CCR_AUTO_LAUNCH_PATH") };
    }
}
