use ccr_app_core::config_reload::{
    admin_reload_enabled, admin_reload_unavailable_message, reload_running_server_via_admin,
    ConfigReloadResult,
};
use ccr_app_core::endpoint::{test_endpoint, EndpointStatus, EndpointTestResult};
use ccr_app_core::logging::append_app_log;
use ccr_app_core::provider_kind::{
    ensure_inferred_provider_api_kinds, mark_provider_api_kind_explicit,
    mark_provider_api_kind_inferred, provider_api_kind_defaults, resolve_provider_api_kind,
};
use ccr_config::{default_config_path, save_config};
use ccr_types::{Config, Provider, ProviderApiKind, ProviderApiKindSource};
use eframe::egui;
use std::collections::HashMap;

pub struct ConfigTab {
    config: Config,
    status: String,
    provider_test_results: HashMap<String, EndpointTestResult>,
    testing_providers: bool,
    reload_status: String,
}

impl ConfigTab {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            status: String::new(),
            provider_test_results: HashMap::new(),
            testing_providers: false,
            reload_status: String::new(),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Config");
            if ui.button("Save").clicked() {
                ensure_inferred_provider_api_kinds(&mut self.config);
                match save_config(&self.config, &default_config_path()) {
                    Ok(_) => self.status = "Saved.".into(),
                    Err(e) => self.status = format!("Error: {e}"),
                }
            }
            if ui.button("Refresh From Disk").clicked() {
                self.refresh_config_display();
            }
            let admin_reload_enabled = can_reload_running_server(&self.config);
            if ui
                .add_enabled(
                    admin_reload_enabled,
                    egui::Button::new("Reload Running Server"),
                )
                .clicked()
            {
                self.reload_running_server_config();
            }
            if ui
                .add_enabled(
                    !self.testing_providers && !self.config.providers.is_empty(),
                    egui::Button::new("Test Providers"),
                )
                .clicked()
            {
                self.test_providers();
            }
            if self.testing_providers {
                ui.spinner();
                ui.label("Testing providers...");
            }
        });
        show_status_message(ui, &self.status);
        show_status_message(ui, &self.reload_status);
        if !can_reload_running_server(&self.config) {
            ui.label(admin_reload_unavailable_message());
        }

        ui.separator();
        ui.heading("Providers");

        let mut to_remove: Option<usize> = None;
        for (i, p) in self.config.providers.iter_mut().enumerate() {
            let test_key = provider_test_key(p);
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut p.name);
                    if ui.button("Remove").clicked() {
                        to_remove = Some(i);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("URL:");
                    ui.text_edit_singleline(&mut p.api_base_url);
                });
                show_provider_kind_choice(ui, p);
                ui.horizontal(|ui| {
                    ui.label("Key:");
                    ui.text_edit_singleline(&mut p.api_key);
                });
                ui.add_space(4.0);
                show_provider_test_result(ui, p, self.provider_test_results.get(&test_key));
            });
        }
        if let Some(i) = to_remove {
            if let Some(provider) = self.config.providers.get(i) {
                self.provider_test_results
                    .remove(&provider_test_key(provider));
            }
            self.config.providers.remove(i);
        }

        if ui.button("Add Provider").clicked() {
            self.config.providers.push(Provider {
                name: String::new(),
                api_kind: Some(ProviderApiKind::OpenAiResponses),
                api_kind_source: ProviderApiKindSource::Explicit,
                api_base_url: String::new(),
                api_key: String::new(),
                models: vec![],
                endpoint_candidates: vec![],
                transformer: Default::default(),
            });
        }

        ui.add_space(8.0);
    }

    fn reload_running_server_config(&mut self) {
        self.reload_status = "Reloading running server...".to_string();

        let port = self.config.port.unwrap_or(3456);
        let result = reload_running_server_via_admin(port, self.config.api_key.as_deref());
        let reload_succeeded = matches!(result, ConfigReloadResult::Success(_));
        self.reload_status = result.ui_message();
        if reload_succeeded {
            self.refresh_config_display();
        }
    }

    fn refresh_config_display(&mut self) {
        // Reload config from file to update UI display
        match ccr_config::load_config(&default_config_path()) {
            Ok(config) => {
                self.config = config;
                self.status = "✓ Config refreshed from disk".to_string();
            }
            Err(e) => {
                self.status = format!("✗ Could not refresh display: {}", e);
            }
        }
    }

    fn test_providers(&mut self) {
        self.testing_providers = true;
        self.provider_test_results.clear();
        append_app_log(
            "ui",
            "providers_test_clicked",
            &[("providers", self.config.providers.len().to_string())],
        );

        for provider in self.config.providers.clone() {
            let key = provider_test_key(&provider);
            let result = if provider.api_base_url.trim().is_empty() {
                EndpointTestResult {
                    provider_name: provider.name.clone(),
                    endpoint: provider.api_base_url.clone(),
                    status: EndpointStatus::InvalidUrl,
                    latency_ms: None,
                    stream_available: None,
                    error: Some("Endpoint is empty".to_string()),
                }
            } else {
                test_endpoint(&provider, &provider.api_base_url)
            };
            self.provider_test_results.insert(key, result);
        }

        self.testing_providers = false;
        append_app_log(
            "ui",
            "providers_test_completed",
            &[("results", self.provider_test_results.len().to_string())],
        );
    }
}

fn provider_test_key(provider: &Provider) -> String {
    format!("{}|{}", provider.name, provider.api_base_url)
}

fn can_reload_running_server(config: &Config) -> bool {
    admin_reload_enabled(config)
}

fn show_provider_test_result(
    ui: &mut egui::Ui,
    provider: &Provider,
    result: Option<&EndpointTestResult>,
) {
    let key = provider_test_key(provider);
    ui.horizontal(|ui| {
        if let Some(result) = result {
            show_endpoint_test_summary(ui, result);
        } else {
            ui.label("Last test: Not tested");
        }
    });

    if provider.api_base_url.trim().is_empty() {
        ui.colored_label(
            egui::Color32::YELLOW,
            "Endpoint is required before testing.",
        );
    }
    if let Some(result) = result {
        egui::CollapsingHeader::new("Test details")
            .id_salt(format!("provider_test_details_{key}"))
            .show(ui, |ui| {
                ui.label(format!("Endpoint: {}", result.endpoint));
                ui.label(format!("Status: {}", endpoint_result_label(result)));
                ui.label(format!(
                    "Latency: {}",
                    result
                        .latency_ms
                        .map(|latency| format!("{latency}ms"))
                        .unwrap_or_else(|| "-".to_string())
                ));
                if let Some(stream_available) = result.stream_available {
                    ui.label(format!("Stream: {stream_available}"));
                }
                if let Some(error) = &result.error {
                    ui.label(format!("Note: {}", truncate(error, 240)));
                }
            });
    }
}

fn show_endpoint_test_summary(ui: &mut egui::Ui, result: &EndpointTestResult) {
    match result.status {
        EndpointStatus::Available => {
            let latency = result
                .latency_ms
                .map(|latency| format!(", {latency}ms"))
                .unwrap_or_default();
            ui.colored_label(
                egui::Color32::GREEN,
                format!("Last test: Available{latency}"),
            );
        }
        _ => {
            ui.colored_label(
                egui::Color32::RED,
                format!("Last test: {}", endpoint_result_label(result)),
            );
        }
    }
}

fn endpoint_result_label(result: &EndpointTestResult) -> String {
    match result.status {
        EndpointStatus::Available => "Available".to_string(),
        EndpointStatus::HttpError(status) => format!("Failed, HTTP {status}"),
        EndpointStatus::Timeout => "Timeout".to_string(),
        EndpointStatus::NetworkError => result
            .error
            .clone()
            .unwrap_or_else(|| "Network error".to_string()),
        EndpointStatus::InvalidUrl => result
            .error
            .clone()
            .unwrap_or_else(|| "Invalid endpoint URL".to_string()),
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn show_status_message(ui: &mut egui::Ui, message: &str) {
    if message.is_empty() {
        return;
    }

    if message.starts_with("Error") || message.starts_with("✗") || message.contains("❌") {
        ui.colored_label(egui::Color32::RED, message);
    } else if message.starts_with("Warning") {
        ui.colored_label(egui::Color32::YELLOW, message);
    } else {
        ui.colored_label(egui::Color32::GREEN, message);
    }
}

fn show_provider_kind_choice(ui: &mut egui::Ui, provider: &mut Provider) {
    let choice = resolve_provider_api_kind(provider);
    let mut selected_kind = choice.kind;
    let source_label = match choice.source {
        ProviderApiKindSource::Explicit => "explicit",
        ProviderApiKindSource::Inferred => "inferred",
    };

    ui.horizontal(|ui| {
        ui.label("API kind:");
        let mut changed = false;
        egui::ComboBox::from_id_salt(format!("provider_api_kind_{}", provider.name))
            .selected_text(selected_kind.label())
            .show_ui(ui, |ui| {
                for kind in ProviderApiKind::ALL {
                    changed |= ui
                        .selectable_value(&mut selected_kind, kind, kind.label())
                        .changed();
                }
            });

        if changed {
            mark_provider_api_kind_explicit(provider, selected_kind);
        }

        ui.label(source_label);

        if ui.button("Re-infer").clicked() {
            mark_provider_api_kind_inferred(provider);
        }
    });

    let choice = resolve_provider_api_kind(provider);
    if let Some(warning) = choice.warning {
        ui.colored_label(egui::Color32::YELLOW, warning);
    }

    let defaults =
        provider_api_kind_defaults(choice.kind, provider.models.first().map(String::as_str));
    if let Some(endpoint) = defaults.default_endpoint {
        ui.label(format!("Default endpoint: {endpoint}"));
    }
    if !defaults.recommended_transformers.is_empty() {
        ui.label(format!(
            "Recommended transformers: {}",
            defaults.recommended_transformers.join(", ")
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccr_types::AppSettings;

    #[test]
    fn running_server_reload_is_disabled_by_default() {
        assert!(!can_reload_running_server(&Config::default()));
    }

    #[test]
    fn running_server_reload_requires_explicit_admin_api_enablement() {
        let config = Config {
            app_settings: AppSettings {
                admin_api_enabled: true,
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(can_reload_running_server(&config));
    }
}
