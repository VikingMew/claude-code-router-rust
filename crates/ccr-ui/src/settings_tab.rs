use ccr_app_core::platform::{
    auto_launch_status, disable_auto_launch, enable_auto_launch, AutoLaunchState,
};
use ccr_app_core::settings::{SettingsSection, SettingsState};
use ccr_config::default_config_path;
use eframe::egui;

pub struct SettingsTab {
    state: SettingsState,
}

impl SettingsTab {
    pub fn new() -> Self {
        Self {
            state: SettingsState::load_default(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Settings");
            if ui.button("Reload").clicked() {
                self.state.reload();
            }
            if ui.button("Save").clicked() {
                self.state.save();
            }
        });

        if self.state.restart_required {
            ui.colored_label(
                egui::Color32::YELLOW,
                "Some changes require server restart.",
            );
        }
        show_message(ui, &self.state.status);

        ui.separator();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                section_button(
                    ui,
                    &mut self.state.section,
                    SettingsSection::Startup,
                    "Startup",
                );
                section_button(
                    ui,
                    &mut self.state.section,
                    SettingsSection::Server,
                    "Server",
                );
                section_button(
                    ui,
                    &mut self.state.section,
                    SettingsSection::Clients,
                    "Clients",
                );
                section_button(
                    ui,
                    &mut self.state.section,
                    SettingsSection::Network,
                    "Network",
                );
                section_button(ui, &mut self.state.section, SettingsSection::Logs, "Logs");
                section_button(
                    ui,
                    &mut self.state.section,
                    SettingsSection::Backups,
                    "Backups",
                );
                section_button(
                    ui,
                    &mut self.state.section,
                    SettingsSection::Appearance,
                    "Appearance",
                );
                section_button(
                    ui,
                    &mut self.state.section,
                    SettingsSection::Advanced,
                    "Advanced",
                );
            });
            ui.separator();
            ui.vertical(|ui| match self.state.section {
                SettingsSection::Startup => self.show_startup(ui),
                SettingsSection::Server => self.show_server(ui),
                SettingsSection::Clients => self.show_clients(ui),
                SettingsSection::Network => self.show_network(ui),
                SettingsSection::Logs => self.show_logs(ui),
                SettingsSection::Backups => self.show_backups(ui),
                SettingsSection::Appearance => self.show_appearance(ui),
                SettingsSection::Advanced => self.show_advanced(ui),
            });
        });
    }

    fn show_startup(&mut self, ui: &mut egui::Ui) {
        ui.heading("Startup");
        let auto_status = match auto_launch_status() {
            AutoLaunchState::Enabled => "enabled",
            AutoLaunchState::Disabled => "disabled",
            AutoLaunchState::Unsupported => "unsupported",
        };
        ui.label(format!("Platform auto launch status: {auto_status}"));

        let mut auto_launch = self.state.config.app_settings.auto_launch;
        if ui.checkbox(&mut auto_launch, "Launch at login").changed() {
            self.state.set_auto_launch(auto_launch);
            let result = if auto_launch {
                enable_auto_launch(None)
            } else {
                disable_auto_launch()
            };
            if let Err(error) = result {
                self.state.status = format!("Error: {error}");
            }
        }

        let mut server_auto_start = self.state.config.app_settings.server_auto_start;
        if ui
            .checkbox(&mut server_auto_start, "Start server when app opens")
            .changed()
        {
            self.state.set_server_auto_start(server_auto_start);
        }
    }

    fn show_server(&mut self, ui: &mut egui::Ui) {
        ui.heading("Server");
        let mut host = self.state.config.host.clone().unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label("Host");
            if ui.text_edit_singleline(&mut host).changed() {
                self.state.set_host(host.clone());
            }
        });

        let mut port = self.state.config.port.unwrap_or(3456).to_string();
        ui.horizontal(|ui| {
            ui.label("Port");
            if ui.text_edit_singleline(&mut port).changed() {
                if let Err(error) = self.state.set_port_from_text(&port) {
                    self.state.status = format!("Error: {error}");
                }
            }
        });

        ui.horizontal(|ui| {
            ui.label("Log level");
            let mut log_level = self.state.config.app_settings.log_level.clone();
            if ui.text_edit_singleline(&mut log_level).changed() {
                self.state.set_log_level(log_level);
            }
        });
    }

    fn show_clients(&mut self, ui: &mut egui::Ui) {
        ui.heading("Clients");
        ui.label("Changing paths does not inject or overwrite client files.");
        edit_optional_string(
            ui,
            "Claude config path",
            &mut self.state.config.app_settings.claude_config_path,
        );
        edit_optional_string(
            ui,
            "Codex config path",
            &mut self.state.config.app_settings.codex_config_path,
        );
        edit_optional_string(
            ui,
            "OpenCode config path",
            &mut self.state.config.app_settings.opencode_config_path,
        );
        edit_optional_string(
            ui,
            "OpenClaw config path",
            &mut self.state.config.app_settings.openclaw_config_path,
        );
        edit_optional_string(
            ui,
            "Hermes config path",
            &mut self.state.config.app_settings.hermes_config_path,
        );
        ui.label("OpenCode, OpenClaw and Hermes use additive provider entries; saving paths does not move files.");
    }

    fn show_network(&mut self, ui: &mut egui::Ui) {
        ui.heading("Network");
        edit_optional_string(
            ui,
            "Global proxy",
            &mut self.state.config.app_settings.global_proxy,
        );
        ui.label("Per-provider proxy is reserved for a later implementation.");
    }

    fn show_logs(&mut self, ui: &mut egui::Ui) {
        ui.heading("Logs");
        let log_dir = dirs_next::home_dir()
            .unwrap_or_default()
            .join(".claude-code-router")
            .join("logs");
        ui.label(format!("Log directory: {}", log_dir.display()));
    }

    fn show_backups(&mut self, ui: &mut egui::Ui) {
        ui.heading("Backups");
        let backup_dir = dirs_next::home_dir()
            .unwrap_or_default()
            .join(".claude-code-router")
            .join("backups");
        ui.label(format!("Backup directory: {}", backup_dir.display()));
        ui.label("Restore UI is reserved for a later phase.");
    }

    fn show_appearance(&mut self, ui: &mut egui::Ui) {
        ui.heading("Appearance");
        ui.horizontal(|ui| {
            ui.label("Theme");
            ui.text_edit_singleline(&mut self.state.config.app_settings.theme);
        });
    }

    fn show_advanced(&mut self, ui: &mut egui::Ui) {
        ui.heading("Advanced");
        ui.label(format!("Config path: {}", default_config_path().display()));
        ui.label(format!("Providers: {}", self.state.config.providers.len()));
        ui.separator();
        ui.heading("Claude Code model mapping");
        let mut models = self.state.config.app_settings.claude_code_models.clone();
        let mut changed = false;
        changed |= edit_required_string(ui, "ANTHROPIC_MODEL", &mut models.model);
        changed |=
            edit_required_string(ui, "ANTHROPIC_DEFAULT_HAIKU_MODEL", &mut models.haiku_model);
        changed |= edit_required_string(
            ui,
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            &mut models.sonnet_model,
        );
        changed |= edit_required_string(ui, "ANTHROPIC_DEFAULT_OPUS_MODEL", &mut models.opus_model);
        if changed {
            self.state.set_claude_code_models(models);
        }
    }
}

impl Default for SettingsTab {
    fn default() -> Self {
        Self::new()
    }
}

fn section_button(
    ui: &mut egui::Ui,
    selected: &mut SettingsSection,
    section: SettingsSection,
    label: &str,
) {
    if ui.selectable_label(*selected == section, label).clicked() {
        *selected = section;
    }
}

fn edit_optional_string(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) {
    let mut text = value.clone().unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.text_edit_singleline(&mut text).changed() {
            *value = if text.trim().is_empty() {
                None
            } else {
                Some(text.clone())
            };
        }
    });
}

fn edit_required_string(ui: &mut egui::Ui, label: &str, value: &mut String) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        changed = ui.text_edit_singleline(value).changed();
    });
    changed
}

fn show_message(ui: &mut egui::Ui, message: &str) {
    if message.is_empty() {
        return;
    }
    if message.starts_with("Error") {
        ui.colored_label(egui::Color32::RED, message);
    } else {
        ui.colored_label(egui::Color32::GREEN, message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_tab_defaults_to_startup() {
        let tab = SettingsTab::new();
        assert_eq!(tab.state.section, SettingsSection::Startup);
    }
}
