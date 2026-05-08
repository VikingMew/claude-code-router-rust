use std::panic::{catch_unwind, AssertUnwindSafe};

use ccr_app_core::client_config::claude::activate_ccr;
use ccr_app_core::client_config::codex::activate_codex_ccr;
use ccr_app_core::status::{ccr_server_executable_from, start_server, stop_server};
use ccr_config::{default_config_path, load_config, save_config};
use ccr_types::Config;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu},
    Icon, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

const MENU_SHOW_WINDOW: &str = "show_window";
const MENU_HIDE_WINDOW: &str = "hide_window";
const MENU_OPEN_UI: &str = "open_ui";
const MENU_START_SERVER: &str = "start_server";
const MENU_STOP_SERVER: &str = "stop_server";
const MENU_RESTART_SERVER: &str = "restart_server";
const MENU_SERVER_STATUS: &str = "server_status";
const MENU_INJECT_CLAUDE: &str = "inject_claude";
const MENU_INJECT_CODEX: &str = "inject_codex";
const MENU_PROVIDER_PREFIX: &str = "provider:";
const MENU_QUIT: &str = "quit";

pub struct TrayManager {
    tray_icon: Option<TrayIcon>,
    provider_routes: Vec<String>,
}

impl TrayManager {
    pub fn new() -> Self {
        Self {
            tray_icon: None,
            provider_routes: Vec::new(),
        }
    }

    pub fn init(&mut self) -> anyhow::Result<()> {
        if tray_disabled_by_env() {
            anyhow::bail!("system tray disabled by CCR_DISABLE_TRAY");
        }
        init_platform_tray()?;

        catch_tray_init(AssertUnwindSafe(|| self.init_inner()))
    }

    fn init_inner(&mut self) -> anyhow::Result<()> {
        let config = load_config(&default_config_path()).unwrap_or_default();
        self.provider_routes = provider_routes_from_config(&config);
        let menu = Self::create_menu(&self.provider_routes)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Claude Code Router")
            .with_icon(Self::create_icon()?)
            .build()?;

        self.tray_icon = Some(tray_icon);
        Ok(())
    }

    pub fn poll_event(&mut self) -> TrayEvent {
        if self.tray_icon.is_none() {
            return TrayEvent::None;
        }

        if let Ok(event) = MenuEvent::receiver().try_recv() {
            return self.handle_menu_event(event);
        }

        if let Ok(event) = TrayIconEvent::receiver().try_recv() {
            return self.handle_tray_event(event);
        }

        TrayEvent::None
    }

    fn create_menu(provider_routes: &[String]) -> Result<Menu, Box<dyn std::error::Error>> {
        let menu = Menu::new();

        menu.append(&MenuItem::with_id(MENU_OPEN_UI, "Open CCR UI", true, None))?;
        menu.append(&MenuItem::with_id(
            MENU_HIDE_WINDOW,
            "Hide Window",
            true,
            None,
        ))?;
        menu.append(&PredefinedMenuItem::separator())?;

        let server_menu = Submenu::new("Server", true);
        server_menu.append(&MenuItem::with_id(
            MENU_START_SERVER,
            "Start Server",
            true,
            None,
        ))?;
        server_menu.append(&MenuItem::with_id(
            MENU_STOP_SERVER,
            "Stop Server",
            true,
            None,
        ))?;
        server_menu.append(&MenuItem::with_id(
            MENU_RESTART_SERVER,
            "Restart Server",
            true,
            None,
        ))?;
        server_menu.append(&PredefinedMenuItem::separator())?;
        server_menu.append(&MenuItem::with_id(
            MENU_SERVER_STATUS,
            "Show Status",
            true,
            None,
        ))?;
        menu.append(&server_menu)?;

        menu.append(&PredefinedMenuItem::separator())?;

        let inject_menu = Submenu::new("Inject", true);
        inject_menu.append(&MenuItem::with_id(
            MENU_INJECT_CLAUDE,
            "Inject Claude Code",
            true,
            None,
        ))?;
        inject_menu.append(&MenuItem::with_id(
            MENU_INJECT_CODEX,
            "Inject Codex",
            true,
            None,
        ))?;
        menu.append(&inject_menu)?;

        let provider_menu = Submenu::new("Switch Provider", true);
        if provider_routes.is_empty() {
            provider_menu.append(&MenuItem::with_id(
                "provider:none",
                "No providers configured",
                false,
                None,
            ))?;
        } else {
            for (index, route) in provider_routes.iter().enumerate() {
                provider_menu.append(&MenuItem::with_id(
                    format!("{MENU_PROVIDER_PREFIX}{index}"),
                    route,
                    true,
                    None,
                ))?;
            }
        }
        menu.append(&provider_menu)?;

        menu.append(&PredefinedMenuItem::separator())?;
        menu.append(&MenuItem::with_id(MENU_QUIT, "Quit", true, None))?;

        Ok(menu)
    }

    fn create_icon() -> Result<Icon, tray_icon::BadIcon> {
        const SIZE: u32 = 32;
        let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);

        for y in 0..SIZE {
            for x in 0..SIZE {
                let dx = x as f32 - 15.5;
                let dy = y as f32 - 15.5;
                let dist = (dx * dx + dy * dy).sqrt();
                let alpha = if dist <= 14.0 { 255 } else { 0 };
                let ring = dist > 10.5 && dist <= 14.0;
                let core = dist <= 7.0;
                let (r, g, b) = if ring {
                    (45, 120, 255)
                } else if core {
                    (22, 28, 45)
                } else {
                    (236, 242, 255)
                };
                rgba.extend_from_slice(&[r, g, b, alpha]);
            }
        }

        Icon::from_rgba(rgba, SIZE, SIZE)
    }

    fn handle_menu_event(&mut self, event: MenuEvent) -> TrayEvent {
        let id = event.id.0;
        if let Some(index) = provider_index_from_menu_id(&id) {
            if let Some(route) = self.provider_routes.get(index) {
                if let Err(error) = switch_route_pool_provider(route) {
                    eprintln!("Failed to switch provider to '{route}': {error}");
                }
            }
            return TrayEvent::None;
        }

        match id.as_str() {
            MENU_OPEN_UI | MENU_SHOW_WINDOW => TrayEvent::ShowWindow,
            MENU_HIDE_WINDOW => TrayEvent::HideWindow,
            MENU_START_SERVER => {
                run_server_start();
                TrayEvent::None
            }
            MENU_STOP_SERVER => {
                if let Err(error) = stop_server() {
                    eprintln!("Failed to stop CCR server: {error}");
                }
                TrayEvent::None
            }
            MENU_RESTART_SERVER => {
                if let Err(error) = stop_server() {
                    eprintln!("Failed to stop CCR server: {error}");
                }
                run_server_start();
                TrayEvent::None
            }
            MENU_SERVER_STATUS => TrayEvent::ShowStatus,
            MENU_INJECT_CLAUDE => {
                if let Err(error) = activate_ccr() {
                    eprintln!("Failed to update Claude Code config: {error}");
                }
                TrayEvent::None
            }
            MENU_INJECT_CODEX => {
                if let Err(error) = activate_codex_ccr() {
                    eprintln!("Failed to update Codex config: {error}");
                }
                TrayEvent::None
            }
            MENU_QUIT => TrayEvent::Quit,
            _ => TrayEvent::None,
        }
    }

    fn handle_tray_event(&self, event: TrayIconEvent) -> TrayEvent {
        match event {
            TrayIconEvent::Click {
                button_state: MouseButtonState::Up,
                ..
            } => TrayEvent::ShowWindow,
            _ => TrayEvent::None,
        }
    }
}

fn tray_disabled_by_env() -> bool {
    env_flag_enabled("CCR_DISABLE_TRAY")
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

fn init_platform_tray() -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        gtk::init().map_err(|error| anyhow::anyhow!("GTK init failed: {error}"))?;
    }
    Ok(())
}

fn catch_tray_init<F>(f: F) -> anyhow::Result<()>
where
    F: FnOnce() -> anyhow::Result<()> + std::panic::UnwindSafe,
{
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = catch_unwind(f);
    std::panic::set_hook(previous_hook);

    match result {
        Ok(result) => result,
        Err(payload) => {
            let message = panic_payload_message(payload);
            anyhow::bail!("system tray initialization panicked: {message}");
        }
    }
}

fn panic_payload_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_string();
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    "unknown panic".to_string()
}

fn run_server_start() {
    let Some(executable) = bundled_server_path() else {
        eprintln!("Cannot find ccr-server executable");
        return;
    };
    if let Err(error) = start_server(&executable) {
        eprintln!("Failed to start CCR server: {error}");
    }
}

fn bundled_server_path() -> Option<std::path::PathBuf> {
    let current_exe = std::env::current_exe().ok()?;
    ccr_server_executable_from(&current_exe)
}

fn provider_routes_from_config(config: &Config) -> Vec<String> {
    config
        .providers
        .iter()
        .filter_map(|provider| {
            provider
                .models
                .first()
                .map(|model| format!("{},{}", provider.name, model))
        })
        .collect()
}

fn provider_index_from_menu_id(id: &str) -> Option<usize> {
    id.strip_prefix(MENU_PROVIDER_PREFIX)?.parse().ok()
}

fn switch_route_pool_provider(route: &str) -> anyhow::Result<()> {
    let config_path = default_config_path();
    let mut config = load_config(&config_path).unwrap_or_default();
    let pool = config
        .route_pool
        .get_or_insert_with(ccr_types::RoutePoolConfig::default);
    pool.enabled = true;
    if let Some(existing_index) = pool
        .candidates
        .iter()
        .position(|candidate| candidate.route == route)
    {
        pool.candidates.swap(0, existing_index);
    } else {
        pool.candidates.insert(
            0,
            ccr_types::RoutePoolCandidate {
                route: route.to_string(),
                enabled: true,
                priority: 1,
            },
        );
    }
    for (index, candidate) in pool.candidates.iter_mut().enumerate() {
        candidate.priority = index as u32 + 1;
    }
    save_config(&config, &config_path)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    None,
    ShowWindow,
    HideWindow,
    ShowStatus,
    Quit,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccr_types::{Provider, ProviderApiKindSource, TransformerConfig};

    #[test]
    fn provider_routes_use_first_model_per_provider() {
        let config = Config {
            providers: vec![
                Provider {
                    name: "openai".to_string(),
                    api_kind: None,
                    api_kind_source: ProviderApiKindSource::Inferred,
                    api_base_url: "https://api.openai.com/v1/responses".to_string(),
                    api_key: "sk-test".to_string(),
                    models: vec!["gpt-5-codex".to_string(), "gpt-4o".to_string()],
                    endpoint_candidates: vec![],
                    transformer: TransformerConfig::default(),
                },
                Provider {
                    name: "anthropic".to_string(),
                    api_kind: None,
                    api_kind_source: ProviderApiKindSource::Inferred,
                    api_base_url: "https://api.anthropic.com/v1/messages".to_string(),
                    api_key: "sk-ant".to_string(),
                    models: vec!["claude-sonnet-4".to_string()],
                    endpoint_candidates: vec![],
                    transformer: TransformerConfig::default(),
                },
            ],
            ..Default::default()
        };

        assert_eq!(
            provider_routes_from_config(&config),
            vec![
                "openai,gpt-5-codex".to_string(),
                "anthropic,claude-sonnet-4".to_string()
            ]
        );
    }

    #[test]
    fn provider_routes_skip_empty_model_lists() {
        let config = Config {
            providers: vec![Provider {
                name: "empty".to_string(),
                api_kind: None,
                api_kind_source: ProviderApiKindSource::Inferred,
                api_base_url: "https://example.com".to_string(),
                api_key: "key".to_string(),
                models: vec![],
                endpoint_candidates: vec![],
                transformer: TransformerConfig::default(),
            }],
            ..Default::default()
        };

        assert!(provider_routes_from_config(&config).is_empty());
    }

    #[test]
    fn provider_index_parses_menu_ids() {
        assert_eq!(provider_index_from_menu_id("provider:2"), Some(2));
        assert_eq!(provider_index_from_menu_id("provider:none"), None);
        assert_eq!(provider_index_from_menu_id("start_server"), None);
    }

    #[test]
    fn tray_env_flag_parses_false_values() {
        assert!(!env_value_enabled(""));
        assert!(!env_value_enabled("0"));
        assert!(!env_value_enabled("false"));
        assert!(!env_value_enabled("no"));
    }

    #[test]
    fn tray_env_flag_parses_true_values() {
        assert!(env_value_enabled("1"));
        assert!(env_value_enabled("true"));
        assert!(env_value_enabled("yes"));
    }
}
