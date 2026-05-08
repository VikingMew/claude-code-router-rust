mod app;
mod config_tab;
mod logs_tab;
mod preset_tab;
mod router_tab;
mod settings_tab;
mod status_tab;
mod token_counter_tab;
mod transformers_tab;
mod tray_manager;

use std::panic::{catch_unwind, AssertUnwindSafe};

fn main() {
    configure_linux_startup_env();
    let options = native_options();

    match catch_ui_startup(AssertUnwindSafe(|| {
        eframe::run_native(
            "CCR",
            options,
            Box::new(|_cc| Ok(Box::new(app::CcrApp::new()))),
        )
    })) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            eprintln!("Failed to start CCR UI: {error}");
            print_linux_fallback();
            std::process::exit(1);
        }
        Err(message) => {
            eprintln!("CCR UI startup panicked: {message}");
            print_linux_fallback();
            std::process::exit(1);
        }
    }
}

fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        hardware_acceleration: if software_rendering_requested() {
            eframe::HardwareAcceleration::Off
        } else {
            eframe::HardwareAcceleration::Preferred
        },
        ..Default::default()
    }
}

fn configure_linux_startup_env() {
    #[cfg(target_os = "linux")]
    {
        if should_prefer_x11_backend() {
            remove_env_if_present("WAYLAND_DISPLAY");
            remove_env_if_present("WAYLAND_SOCKET");
            set_env_if_missing("GDK_BACKEND", "x11");
            set_env_if_missing("LIBGL_ALWAYS_SOFTWARE", "1");
        }
    }
}

#[cfg(target_os = "linux")]
fn should_prefer_x11_backend() -> bool {
    std::env::var_os("DISPLAY").is_some()
        && (running_under_wsl()
            || env_flag_enabled("CCR_DISABLE_TRAY")
            || env_flag_enabled("LIBGL_ALWAYS_SOFTWARE")
            || env_flag_enabled("CCR_SOFTWARE_RENDERING"))
        && !env_flag_enabled("CCR_ALLOW_WAYLAND")
}

#[cfg(target_os = "linux")]
fn set_env_if_missing(name: &str, value: &str) {
    if std::env::var_os(name).is_none() {
        std::env::set_var(name, value);
    }
}

#[cfg(target_os = "linux")]
fn remove_env_if_present(name: &str) {
    if std::env::var_os(name).is_some() {
        std::env::remove_var(name);
    }
}

fn software_rendering_requested() -> bool {
    env_flag_enabled("CCR_SOFTWARE_RENDERING")
        || env_flag_enabled("LIBGL_ALWAYS_SOFTWARE")
        || running_under_wsl()
}

fn running_under_wsl() -> bool {
    std::env::var_os("WSL_DISTRO_NAME").is_some()
        || std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .map(|content| content.to_ascii_lowercase().contains("microsoft"))
            .unwrap_or(false)
}

fn env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .map(|value| env_value_enabled(&value))
        .unwrap_or(false)
}

fn env_value_enabled(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    !value.is_empty() && value != "0" && value != "false" && value != "no"
}

fn catch_ui_startup<F>(f: F) -> Result<eframe::Result, String>
where
    F: FnOnce() -> eframe::Result + std::panic::UnwindSafe,
{
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = catch_unwind(f);
    std::panic::set_hook(previous_hook);

    result.map_err(panic_payload_message)
}

fn panic_payload_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&'static str>() {
        (*message).to_string()
    } else {
        "unknown panic payload".to_string()
    }
}

fn print_linux_fallback() {
    eprintln!(
        "Linux/WSL fallback: try `env -u WAYLAND_DISPLAY -u WAYLAND_SOCKET CCR_DISABLE_TRAY=1 GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1 cargo run --bin ccr-ui`"
    );
    print_linux_display_env();
}

fn print_linux_display_env() {
    #[cfg(target_os = "linux")]
    {
        eprintln!(
            "Linux display env: DISPLAY={:?} WAYLAND_DISPLAY={:?} WAYLAND_SOCKET={:?} GDK_BACKEND={:?} LIBGL_ALWAYS_SOFTWARE={:?}",
            std::env::var_os("DISPLAY"),
            std::env::var_os("WAYLAND_DISPLAY"),
            std::env::var_os("WAYLAND_SOCKET"),
            std::env::var_os("GDK_BACKEND"),
            std::env::var_os("LIBGL_ALWAYS_SOFTWARE")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_value_enabled_parses_false_values() {
        assert!(!env_value_enabled(""));
        assert!(!env_value_enabled("0"));
        assert!(!env_value_enabled("false"));
        assert!(!env_value_enabled("no"));
    }

    #[test]
    fn env_value_enabled_parses_true_values() {
        assert!(env_value_enabled("1"));
        assert!(env_value_enabled("true"));
        assert!(env_value_enabled("yes"));
    }

    #[test]
    fn panic_payload_message_reads_common_payloads() {
        assert_eq!(
            panic_payload_message(Box::new("static message")),
            "static message"
        );
        assert_eq!(
            panic_payload_message(Box::new(String::from("owned message"))),
            "owned message"
        );
    }
}
