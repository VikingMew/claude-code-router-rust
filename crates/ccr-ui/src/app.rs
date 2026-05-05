use crate::config_tab;
use crate::logs_tab;
use crate::preset_tab;
use crate::router_tab;
use crate::settings_tab;
use crate::status_tab;
use crate::token_counter_tab;
use crate::transformers_tab;
use crate::tray_manager::{TrayEvent, TrayManager};
use ccr_config::{default_config_path, load_config};
use eframe::egui;

#[derive(PartialEq)]
enum Tab {
    Status,
    Config,
    Router,
    Presets,
    Logs,
    Transformers,
    TokenCounter,
    Settings,
}

pub struct CcrApp {
    tab: Tab,
    config_tab: config_tab::ConfigTab,
    router_tab: router_tab::RouterTab,
    preset_tab: preset_tab::PresetTab,
    status_tab: status_tab::StatusTab,
    logs_tab: logs_tab::LogsTab,
    transformers_tab: transformers_tab::TransformersTab,
    token_counter_tab: token_counter_tab::TokenCounterTab,
    settings_tab: settings_tab::SettingsTab,
    tray_manager: TrayManager,
    minimize_to_tray: bool,
    allow_quit: bool,
}

impl CcrApp {
    pub fn new() -> Self {
        let config = load_config(&default_config_path()).unwrap_or_default();
        let mut tray_manager = TrayManager::new();
        if let Err(error) = tray_manager.init() {
            eprintln!("Failed to initialize system tray: {error}");
        }

        let mut status_tab = status_tab::StatusTab::new();
        status_tab.start_server_on_launch();

        Self {
            tab: Tab::Status,
            config_tab: config_tab::ConfigTab::new(config),
            router_tab: router_tab::RouterTab::new(),
            preset_tab: preset_tab::PresetTab::new(),
            status_tab,
            logs_tab: logs_tab::LogsTab::new(),
            transformers_tab: transformers_tab::TransformersTab::new(),
            token_counter_tab: token_counter_tab::TokenCounterTab::new(),
            settings_tab: settings_tab::SettingsTab::new(),
            tray_manager,
            minimize_to_tray: true,
            allow_quit: false,
        }
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.viewport().close_requested())
            && self.minimize_to_tray
            && !self.allow_quit
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
    }

    fn handle_tray_events(&mut self, ctx: &egui::Context) {
        match self.tray_manager.poll_event() {
            TrayEvent::ShowWindow => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            TrayEvent::HideWindow => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
            TrayEvent::ShowStatus => {
                self.tab = Tab::Status;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            TrayEvent::Quit => {
                self.allow_quit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            TrayEvent::None => {}
        }
    }
}

impl eframe::App for CcrApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_close_request(ctx);
        self.handle_tray_events(ctx);

        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Status, "Status");
                ui.selectable_value(&mut self.tab, Tab::Config, "Config");
                ui.selectable_value(&mut self.tab, Tab::Router, "Router");
                ui.selectable_value(&mut self.tab, Tab::Presets, "Presets");
                ui.selectable_value(&mut self.tab, Tab::Logs, "Logs");
                ui.selectable_value(&mut self.tab, Tab::Transformers, "Transformers");
                ui.selectable_value(&mut self.tab, Tab::TokenCounter, "Token Counter");
                ui.selectable_value(&mut self.tab, Tab::Settings, "Settings");
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Status => {
                self.logs_tab.set_visible(false);
                show_tab_scroll(ui, "status_tab_scroll", |ui| self.status_tab.show(ui));
            }
            Tab::Config => {
                self.logs_tab.set_visible(false);
                show_tab_scroll(ui, "config_tab_scroll", |ui| self.config_tab.show(ui));
            }
            Tab::Router => {
                self.logs_tab.set_visible(false);
                show_tab_scroll(ui, "router_tab_scroll", |ui| self.router_tab.show(ui));
            }
            Tab::Presets => {
                self.logs_tab.set_visible(false);
                show_tab_scroll(ui, "presets_tab_scroll", |ui| self.preset_tab.show(ui));
            }
            Tab::Logs => self.logs_tab.show(ui),
            Tab::Transformers => {
                self.logs_tab.set_visible(false);
                show_tab_scroll(ui, "transformers_tab_scroll", |ui| {
                    self.transformers_tab.show(ui)
                });
            }
            Tab::TokenCounter => {
                self.logs_tab.set_visible(false);
                show_tab_scroll(ui, "token_counter_tab_scroll", |ui| {
                    self.token_counter_tab.show(ui)
                });
            }
            Tab::Settings => {
                self.logs_tab.set_visible(false);
                show_tab_scroll(ui, "settings_tab_scroll", |ui| self.settings_tab.show(ui));
            }
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}

fn show_tab_scroll(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    egui::ScrollArea::vertical()
        .id_salt(id_salt)
        .auto_shrink([false, false])
        .show(ui, add_contents);
}
