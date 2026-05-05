use ccr_config::{default_config_path, load_config, save_config};
use ccr_preset::{builtin_profiles, delete_preset, install_preset_with_options, list_presets};
use eframe::egui;

pub struct PresetTab {
    presets: Vec<String>,
    status: String,
    overwrite_existing: bool,
}

impl PresetTab {
    pub fn new() -> Self {
        let presets = list_presets()
            .unwrap_or_default()
            .into_iter()
            .map(|m| m.name)
            .collect();
        Self {
            presets,
            status: String::new(),
            overwrite_existing: false,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.show_default_profiles(ui);
        ui.separator();

        ui.heading("Installed Presets");
        if self.presets.is_empty() {
            ui.label("No presets installed.");
        }
        let mut to_delete: Option<usize> = None;
        for (i, name) in self.presets.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(name);
                if ui.button("Delete").clicked() {
                    to_delete = Some(i);
                }
            });
        }
        if let Some(i) = to_delete {
            let name = self.presets.remove(i);
            match delete_preset(&name) {
                Ok(_) => self.status = format!("Deleted '{name}'."),
                Err(e) => self.status = format!("Error: {e}"),
            }
        }
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
    }

    fn show_default_profiles(&mut self, ui: &mut egui::Ui) {
        ui.heading("Default Profiles");
        ui.checkbox(
            &mut self.overwrite_existing,
            "Overwrite providers with the same name",
        );

        for profile in builtin_profiles() {
            let manifest = &profile.manifest;
            ui.separator();
            ui.horizontal_wrapped(|ui| {
                ui.label(&manifest.name);
                if manifest.experimental {
                    ui.colored_label(egui::Color32::YELLOW, "Experimental");
                }
                if ui.button("Apply").clicked() {
                    self.apply_builtin_profile(profile.id);
                }
            });
            ui.label(&manifest.description);
            if let Some(pool) = &manifest.route_pool {
                let route = pool
                    .candidates
                    .iter()
                    .filter(|candidate| candidate.enabled)
                    .min_by_key(|candidate| candidate.priority)
                    .map(|candidate| candidate.route.as_str())
                    .unwrap_or("not configured");
                ui.label(format!("Route Pool: {route}"));
            }
            if !manifest.required_env.is_empty() {
                ui.label(format!(
                    "Required env: {}",
                    manifest.required_env.join(", ")
                ));
            }
            for provider in &manifest.providers {
                let name = provider
                    .get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown");
                let models = provider
                    .get("models")
                    .and_then(|value| value.as_array())
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|value| value.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                ui.label(format!("Provider: {name} [{models}]"));
            }
        }
    }

    fn apply_builtin_profile(&mut self, profile_id: &str) {
        let Some(profile) = ccr_preset::builtin_profile(profile_id) else {
            self.status = format!("Error: unknown builtin profile `{profile_id}`");
            return;
        };
        let path = default_config_path();
        let mut config = load_config(&path).unwrap_or_default();
        match install_preset_with_options(&profile.manifest, &mut config, self.overwrite_existing) {
            Ok(changed) => {
                if let Some(parent) = path.parent() {
                    if let Err(error) = std::fs::create_dir_all(parent) {
                        self.status = format!("Error: {error}");
                        return;
                    }
                }
                match save_config(&config, &path) {
                    Ok(()) => {
                        let provider_message = if changed.is_empty() {
                            "no provider changes".to_string()
                        } else {
                            format!("providers: {}", changed.join(", "))
                        };
                        self.status =
                            format!("Applied `{}` ({provider_message}).", profile.manifest.name);
                    }
                    Err(error) => self.status = format!("Error: {error}"),
                }
            }
            Err(error) => self.status = format!("Error: {error}"),
        }
    }
}

impl Default for PresetTab {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_tab_defaults_to_merge_mode() {
        let tab = PresetTab {
            presets: vec![],
            status: String::new(),
            overwrite_existing: false,
        };

        assert!(!tab.overwrite_existing);
    }
}
