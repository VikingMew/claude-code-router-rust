use std::process::Command;

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

    pub fn init(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let config = load_config(&default_config_path()).unwrap_or_default();
        self.provider_routes = provider_routes_from_config(&config);
        let menu = Self::create_menu(&self.provider_routes)?;
        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Claude Code Router")
            .with_icon(Self::create_icon()?)
            .build()?;

        self.tray_icon = Some(tray_icon);
        Ok(())
    }

    pub fn poll_event(&mut self) -> TrayEvent {
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
                run_ccr_command("start");
                TrayEvent::None
            }
            MENU_STOP_SERVER => {
                run_ccr_command("stop");
                TrayEvent::None
            }
            MENU_RESTART_SERVER => {
                run_ccr_command("restart");
                TrayEvent::None
            }
            MENU_SERVER_STATUS => TrayEvent::ShowStatus,
            MENU_INJECT_CLAUDE => {
                run_ccr_command("claude-activate");
                TrayEvent::None
            }
            MENU_INJECT_CODEX => {
                run_ccr_command("codex-activate");
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

fn run_ccr_command(command: &str) {
    let executable = bundled_ccr_path().unwrap_or_else(|| "ccr".into());
    if let Err(error) = Command::new(&executable).arg(command).spawn() {
        eprintln!("Failed to run 'ccr {command}': {error}");
    }
}

fn bundled_ccr_path() -> Option<std::path::PathBuf> {
    let current_exe = std::env::current_exe().ok()?;
    let resources = current_exe
        .parent()?
        .parent()?
        .join("Resources")
        .join("bin");
    let ccr = resources.join(if cfg!(windows) { "ccr.exe" } else { "ccr" });
    ccr.exists().then_some(ccr)
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
}
