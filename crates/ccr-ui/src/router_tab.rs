use ccr_app_core::settings::{route_pool_routes, SettingsState};
use eframe::egui;

pub struct RouterTab {
    state: SettingsState,
    status: String,
    selected_available_route: Option<String>,
    selected_pool_index: Option<usize>,
}

impl RouterTab {
    pub fn new() -> Self {
        Self {
            state: SettingsState::load_default(),
            status: String::new(),
            selected_available_route: None,
            selected_pool_index: None,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Router");
            if ui.button("Reload").clicked() {
                self.state.reload();
                self.status.clear();
                self.selected_available_route = None;
                self.selected_pool_index = None;
            }
            if ui.button("Save").clicked() {
                self.state.save();
                self.status = self.state.status.clone();
            }
        });
        show_message(ui, &self.status);
        show_message(ui, &self.state.status);

        ui.separator();
        self.show_route_pool(ui);
    }

    fn show_route_pool(&mut self, ui: &mut egui::Ui) {
        ui.heading("Route Pool");
        ui.label("Ordered provider health policy. Provider-only routes keep inbound model names.");
        self.state.ensure_route_pool();
        let available_routes = available_route_pool_routes(&self.state.config);

        {
            let pool = self.state.route_pool_mut().expect("route pool initialized");

            ui.checkbox(&mut pool.enabled, "Enable Route Pool");
            ui.horizontal(|ui| {
                ui.label("Failure threshold");
                ui.add(egui::DragValue::new(&mut pool.failure_threshold).range(1..=100));
                ui.label("Ban seconds");
                ui.add(egui::DragValue::new(&mut pool.ban_seconds).range(1..=86_400));
            });
            pool.candidates.sort_by_key(|candidate| candidate.priority);
        }

        ui.add_space(8.0);
        let spacing = ui.spacing().item_spacing.x;
        let move_width = 72.0;
        let list_width = ((ui.available_width() - move_width - spacing * 2.0) / 2.0).max(220.0);

        ui.horizontal_top(|ui| {
            ui.set_width(list_width * 2.0 + move_width + spacing * 2.0);
            ui.allocate_ui_with_layout(
                egui::vec2(list_width, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    route_list_frame(ui, "Available Routes", |ui| {
                        self.show_available_routes(ui, &available_routes);
                    });
                },
            );

            ui.allocate_ui_with_layout(
                egui::vec2(move_width, 0.0),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    ui.heading("Move");
                    ui.add_space(116.0);
                    let can_add = self.selected_available_route.is_some();
                    if ui
                        .add_enabled(
                            can_add,
                            egui::Button::new(">").min_size(egui::vec2(48.0, 32.0)),
                        )
                        .on_disabled_hover_text("Select a route on the left first")
                        .clicked()
                    {
                        self.move_selected_available_into_pool();
                    }
                    ui.add_space(8.0);
                    let can_remove = self.selected_pool_index.is_some();
                    if ui
                        .add_enabled(
                            can_remove,
                            egui::Button::new("<").min_size(egui::vec2(48.0, 32.0)),
                        )
                        .on_disabled_hover_text("Select a route on the right first")
                        .clicked()
                    {
                        self.remove_selected_pool_route();
                    }
                },
            );

            ui.allocate_ui_with_layout(
                egui::vec2(list_width, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    route_list_frame(ui, "Active Route Pool", |ui| {
                        ui.horizontal(|ui| {
                            if ui.button("Up").clicked() {
                                self.move_selected_pool_up();
                            }
                            if ui.button("Down").clicked() {
                                self.move_selected_pool_down();
                            }
                        });
                        ui.add_space(4.0);
                        self.show_active_pool(ui);
                    });
                },
            );
        });
    }

    fn show_available_routes(&mut self, ui: &mut egui::Ui, routes: &[String]) {
        if routes.is_empty() {
            ui.label("No configured provider routes.");
            self.selected_available_route = None;
            return;
        }

        for route in routes {
            let selected = self
                .selected_available_route
                .as_ref()
                .is_some_and(|selected| selected == route);
            let warning = route_config_warning(&self.state, route);
            if show_route_row(ui, route, selected, true, warning).clicked() {
                self.selected_available_route = Some(route.clone());
                self.selected_pool_index = None;
            }
        }
    }

    fn show_active_pool(&mut self, ui: &mut egui::Ui) {
        let len = self
            .state
            .route_pool()
            .map(|pool| pool.candidates.len())
            .unwrap_or(0);
        if len == 0 {
            ui.label("No active routes.");
            self.selected_pool_index = None;
            return;
        }

        if self.selected_pool_index.is_some_and(|index| index >= len) {
            self.selected_pool_index = None;
        }

        for index in 0..len {
            let route = self
                .state
                .route_pool()
                .and_then(|pool| pool.candidates.get(index))
                .map(|candidate| candidate.route.clone())
                .unwrap_or_default();
            let warning = route_config_warning(&self.state, &route);
            ui.horizontal(|ui| {
                let pool = self.state.route_pool_mut().expect("route pool initialized");
                ui.checkbox(&mut pool.candidates[index].enabled, "");
                let selected = self.selected_pool_index == Some(index);
                if show_route_row(ui, &route, selected, false, warning).clicked() {
                    self.selected_pool_index = Some(index);
                    self.selected_available_route = None;
                }
            });
        }
    }

    fn move_selected_available_into_pool(&mut self) {
        let Some(route) = self.selected_available_route.take() else {
            return;
        };
        self.state.add_route_pool_route(route);
        self.selected_pool_index = self
            .state
            .route_pool()
            .map(|pool| pool.candidates.len().saturating_sub(1));
    }

    fn remove_selected_pool_route(&mut self) {
        let Some(index) = self.selected_pool_index.take() else {
            return;
        };
        if self.state.remove_route_pool_index(index) {
            self.selected_available_route = None;
        }
    }

    fn move_selected_pool_up(&mut self) {
        let Some(index) = self.selected_pool_index else {
            return;
        };
        if self.state.move_route_pool_up(index) {
            self.selected_pool_index = Some(index - 1);
        }
    }

    fn move_selected_pool_down(&mut self) {
        let Some(index) = self.selected_pool_index else {
            return;
        };
        if self.state.move_route_pool_down(index) {
            self.selected_pool_index = Some(index + 1);
        }
    }
}

fn route_list_frame(
    ui: &mut egui::Ui,
    title: &'static str,
    add_contents: impl FnOnce(&mut egui::Ui),
) {
    let stroke = egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
    egui::Frame::group(ui.style())
        .fill(ui.visuals().faint_bg_color)
        .stroke(stroke)
        .show(ui, |ui| {
            ui.set_min_height(360.0);
            ui.set_width(ui.available_width());
            ui.heading(title);
            ui.separator();
            add_contents(ui);
        });
}

fn show_route_row(
    ui: &mut egui::Ui,
    route: &str,
    selected: bool,
    available: bool,
    warning: Option<&'static str>,
) -> egui::Response {
    let description = route_description(route);
    let text = match warning {
        Some(warning) => format!("{route}\n{description} [{warning}]"),
        None => format!("{route}\n{description}"),
    };
    let fill = if selected {
        ui.visuals().selection.bg_fill
    } else if available {
        egui::Color32::from_rgb(35, 43, 48)
    } else {
        egui::Color32::from_rgb(42, 38, 48)
    };
    ui.add_sized(
        [ui.available_width(), 44.0],
        egui::Button::new(egui::RichText::new(text).monospace())
            .selected(selected)
            .fill(fill)
            .wrap(),
    )
}

fn available_route_pool_routes(config: &ccr_types::Config) -> Vec<String> {
    let active_routes = config
        .route_pool
        .as_ref()
        .map(|pool| {
            pool.candidates
                .iter()
                .map(|candidate| candidate.route.as_str())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    route_pool_routes(config)
        .into_iter()
        .filter(|route| !active_routes.contains(&route.as_str()))
        .collect()
}

fn route_description(route: &str) -> &'static str {
    if route.contains(',') {
        "Pins model"
    } else {
        "Keeps inbound model"
    }
}

fn route_config_warning(state: &SettingsState, route: &str) -> Option<&'static str> {
    let provider_name = route.split(',').next().unwrap_or_default().trim();
    let provider = state
        .config
        .providers
        .iter()
        .find(|provider| provider.name == provider_name)?;
    if provider.api_base_url.trim().is_empty() {
        return Some("Missing endpoint");
    }
    if provider.api_key.trim().is_empty() {
        return Some("Missing API key");
    }
    if route.contains(',') {
        let model = route
            .split_once(',')
            .map(|(_, model)| model.trim())
            .unwrap_or("");
        if !provider.models.iter().any(|candidate| candidate == model) {
            return Some("Model not configured");
        }
    }
    None
}

impl Default for RouterTab {
    fn default() -> Self {
        Self::new()
    }
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
    use ccr_types::{Config, Provider, ProviderApiKindSource};

    fn provider(name: &str, api_base_url: &str, api_key: &str, models: Vec<&str>) -> Provider {
        Provider {
            name: name.into(),
            api_kind: None,
            api_kind_source: ProviderApiKindSource::Inferred,
            api_base_url: api_base_url.into(),
            api_key: api_key.into(),
            models: models.into_iter().map(str::to_string).collect(),
            endpoint_candidates: vec![],
            transformer: Default::default(),
        }
    }

    #[test]
    fn route_description_marks_provider_only_and_pinned_routes() {
        assert_eq!(route_description("openai"), "Keeps inbound model");
        assert_eq!(route_description("openai,gpt-5"), "Pins model");
    }

    #[test]
    fn route_config_warning_detects_missing_endpoint_and_api_key() {
        let state = SettingsState::new(Config {
            providers: vec![
                provider("no-endpoint", "", "key", vec![]),
                provider("no-key", "https://example.com", "", vec![]),
            ],
            ..Default::default()
        });

        assert_eq!(
            route_config_warning(&state, "no-endpoint"),
            Some("Missing endpoint")
        );
        assert_eq!(
            route_config_warning(&state, "no-key"),
            Some("Missing API key")
        );
    }

    #[test]
    fn route_config_warning_detects_unconfigured_pinned_model() {
        let state = SettingsState::new(Config {
            providers: vec![provider("openai", "https://example.com", "key", vec!["a"])],
            ..Default::default()
        });

        assert_eq!(
            route_config_warning(&state, "openai,b"),
            Some("Model not configured")
        );
        assert_eq!(route_config_warning(&state, "openai,a"), None);
    }

    #[test]
    fn available_route_pool_routes_filters_active_pool_candidates() {
        let config = Config {
            providers: vec![provider(
                "openai",
                "https://example.com",
                "key",
                vec!["gpt-5", "gpt-5-mini"],
            )],
            route_pool: Some(ccr_types::RoutePoolConfig {
                enabled: true,
                failure_threshold: 3,
                ban_seconds: 3600,
                candidates: vec![ccr_types::RoutePoolCandidate {
                    route: "openai,gpt-5".to_string(),
                    enabled: true,
                    priority: 1,
                }],
            }),
            ..Default::default()
        };

        assert_eq!(
            available_route_pool_routes(&config),
            vec!["openai".to_string(), "openai,gpt-5-mini".to_string()]
        );
    }
}
