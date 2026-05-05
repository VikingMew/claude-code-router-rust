use ccr_config::{default_config_path, load_config, save_config};
use ccr_types::{ClaudeCodeModelSettings, Config, RoutePoolCandidate, RoutePoolConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Startup,
    Server,
    Clients,
    Network,
    Logs,
    Backups,
    Appearance,
    Advanced,
}

#[derive(Debug, Clone)]
pub struct SettingsState {
    pub config: Config,
    pub section: SettingsSection,
    pub status: String,
    pub restart_required: bool,
}

impl SettingsState {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            section: SettingsSection::Startup,
            status: String::new(),
            restart_required: false,
        }
    }

    pub fn load_default() -> Self {
        Self::new(load_config(&default_config_path()).unwrap_or_default())
    }

    pub fn reload(&mut self) {
        self.config = load_config(&default_config_path()).unwrap_or_default();
        self.status = "Reloaded settings.".to_string();
        self.restart_required = false;
    }

    pub fn save(&mut self) {
        match save_config(&self.config, &default_config_path()) {
            Ok(_) => self.status = "Saved settings.".to_string(),
            Err(error) => self.status = format!("Error: {error}"),
        }
    }

    pub fn set_host(&mut self, host: String) {
        self.config.host = if host.trim().is_empty() {
            None
        } else {
            Some(host)
        };
        self.restart_required = true;
    }

    pub fn set_port_from_text(&mut self, port: &str) -> Result<(), String> {
        let value = port
            .parse::<u16>()
            .map_err(|_| "invalid port".to_string())?;
        self.config.port = Some(value);
        self.restart_required = true;
        Ok(())
    }

    pub fn set_log_level(&mut self, log_level: String) {
        self.config.app_settings.log_level = log_level;
    }

    pub fn set_server_auto_start(&mut self, enabled: bool) {
        self.config.app_settings.server_auto_start = enabled;
    }

    pub fn set_auto_launch(&mut self, enabled: bool) {
        self.config.app_settings.auto_launch = enabled;
    }

    pub fn set_claude_code_models(&mut self, models: ClaudeCodeModelSettings) {
        self.config.app_settings.claude_code_models = models;
        self.config.app_settings.claude_code_models_enabled =
            !self.config.app_settings.claude_code_models.is_empty();
    }

    pub fn set_optional_string(target: &mut Option<String>, value: String) {
        *target = if value.trim().is_empty() {
            None
        } else {
            Some(value)
        };
    }

    pub fn ensure_route_pool(&mut self) {
        ensure_route_pool(&mut self.config);
    }

    pub fn route_pool(&self) -> Option<&RoutePoolConfig> {
        self.config.route_pool.as_ref()
    }

    pub fn route_pool_mut(&mut self) -> Option<&mut RoutePoolConfig> {
        self.config.route_pool.as_mut()
    }

    pub fn add_route_pool_route(&mut self, route: String) {
        ensure_route_pool(&mut self.config);
        let pool = self
            .config
            .route_pool
            .as_mut()
            .expect("route pool initialized");
        if pool
            .candidates
            .iter()
            .any(|candidate| candidate.route == route)
        {
            return;
        }
        let priority = pool.candidates.len() as u32 + 1;
        pool.candidates.push(RoutePoolCandidate {
            route,
            enabled: true,
            priority,
        });
    }

    pub fn remove_route_pool_index(&mut self, index: usize) -> bool {
        let Some(pool) = self.config.route_pool.as_mut() else {
            return false;
        };
        if index >= pool.candidates.len() {
            return false;
        }
        pool.candidates.sort_by_key(|candidate| candidate.priority);
        pool.candidates.remove(index);
        normalize_route_pool_priorities(pool);
        true
    }

    pub fn move_route_pool_up(&mut self, index: usize) -> bool {
        let Some(pool) = self.config.route_pool.as_mut() else {
            return false;
        };
        if index == 0 || index >= pool.candidates.len() {
            return false;
        }
        pool.candidates.sort_by_key(|candidate| candidate.priority);
        pool.candidates.swap(index, index - 1);
        normalize_route_pool_priorities(pool);
        true
    }

    pub fn move_route_pool_down(&mut self, index: usize) -> bool {
        let Some(pool) = self.config.route_pool.as_mut() else {
            return false;
        };
        if index + 1 >= pool.candidates.len() {
            return false;
        }
        pool.candidates.sort_by_key(|candidate| candidate.priority);
        pool.candidates.swap(index, index + 1);
        normalize_route_pool_priorities(pool);
        true
    }
}

pub fn ensure_route_pool(config: &mut Config) {
    config.route_pool.get_or_insert_with(|| RoutePoolConfig {
        enabled: false,
        failure_threshold: 3,
        ban_seconds: 3600,
        candidates: Vec::new(),
    });
}

pub fn provider_routes(config: &Config) -> Vec<String> {
    config
        .providers
        .iter()
        .flat_map(|provider| {
            provider
                .models
                .iter()
                .map(move |model| format!("{},{}", provider.name, model))
        })
        .collect()
}

pub fn route_pool_routes(config: &Config) -> Vec<String> {
    let mut routes = Vec::new();
    for provider in &config.providers {
        routes.push(provider.name.clone());
        routes.extend(
            provider
                .models
                .iter()
                .map(|model| format!("{},{}", provider.name, model)),
        );
    }
    routes
}

pub fn route_pool_config(config: &Config) -> Option<&RoutePoolConfig> {
    config.route_pool.as_ref()
}

pub fn normalize_route_pool_priorities(pool: &mut RoutePoolConfig) {
    for (index, candidate) in pool.candidates.iter_mut().enumerate() {
        candidate.priority = index as u32 + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccr_types::{Provider, ProviderApiKindSource};

    fn provider(name: &str, models: Vec<&str>) -> Provider {
        Provider {
            name: name.into(),
            api_kind: None,
            api_kind_source: ProviderApiKindSource::Inferred,
            api_base_url: "https://example.com".into(),
            api_key: "key".into(),
            models: models.into_iter().map(str::to_string).collect(),
            endpoint_candidates: vec![],
            transformer: Default::default(),
        }
    }

    #[test]
    fn settings_state_defaults_to_startup() {
        let state = SettingsState::new(Config::default());
        assert_eq!(state.section, SettingsSection::Startup);
    }

    #[test]
    fn set_port_marks_restart_required() {
        let mut state = SettingsState::new(Config::default());
        state.set_port_from_text("4567").unwrap();
        assert_eq!(state.config.port, Some(4567));
        assert!(state.restart_required);
    }

    #[test]
    fn set_port_rejects_invalid_text() {
        let mut state = SettingsState::new(Config::default());
        assert_eq!(state.set_port_from_text("bad").unwrap_err(), "invalid port");
    }

    #[test]
    fn set_host_trims_empty_to_none_and_marks_restart() {
        let mut state = SettingsState::new(Config::default());

        state.set_host("   ".to_string());

        assert_eq!(state.config.host, None);
        assert!(state.restart_required);
    }

    #[test]
    fn set_host_stores_non_empty_value() {
        let mut state = SettingsState::new(Config::default());

        state.set_host("127.0.0.1".to_string());

        assert_eq!(state.config.host.as_deref(), Some("127.0.0.1"));
        assert!(state.restart_required);
    }

    #[test]
    fn set_log_level_and_startup_flags_update_app_settings() {
        let mut state = SettingsState::new(Config::default());

        state.set_log_level("debug".to_string());
        state.set_server_auto_start(false);
        state.set_auto_launch(true);

        assert_eq!(state.config.app_settings.log_level, "debug");
        assert!(!state.config.app_settings.server_auto_start);
        assert!(state.config.app_settings.auto_launch);
        assert!(!state.restart_required);
    }

    #[test]
    fn set_client_path_does_not_mark_restart() {
        let mut state = SettingsState::new(Config::default());
        SettingsState::set_optional_string(
            &mut state.config.app_settings.claude_config_path,
            "/tmp/claude".to_string(),
        );
        assert!(!state.restart_required);
        assert_eq!(
            state.config.app_settings.claude_config_path,
            Some("/tmp/claude".to_string())
        );
    }

    #[test]
    fn set_claude_code_models_does_not_mark_restart() {
        let mut state = SettingsState::new(Config::default());

        state.set_claude_code_models(ClaudeCodeModelSettings {
            model: "claude-main".to_string(),
            haiku_model: "claude-haiku".to_string(),
            sonnet_model: "claude-sonnet".to_string(),
            opus_model: "claude-opus".to_string(),
        });

        assert_eq!(
            state.config.app_settings.claude_code_models.model,
            "claude-main"
        );
        assert_eq!(
            state.config.app_settings.claude_code_models.haiku_model,
            "claude-haiku"
        );
        assert!(state.config.app_settings.claude_code_models_enabled);
        assert!(!state.restart_required);
    }

    #[test]
    fn set_claude_code_models_disables_empty_mapping() {
        let mut state = SettingsState::new(Config::default());
        state.config.app_settings.claude_code_models_enabled = true;

        state.set_claude_code_models(ClaudeCodeModelSettings::default());

        assert!(!state.config.app_settings.claude_code_models_enabled);
    }

    #[test]
    fn set_optional_string_clears_blank_values() {
        let mut value = Some("existing".to_string());

        SettingsState::set_optional_string(&mut value, " \t ".to_string());

        assert_eq!(value, None);
    }

    #[test]
    fn provider_routes_include_all_models() {
        let mut config = Config::default();
        config.providers.push(provider("openai", vec!["a", "b"]));
        assert_eq!(provider_routes(&config), vec!["openai,a", "openai,b"]);
    }

    #[test]
    fn route_pool_routes_include_provider_and_model_routes() {
        let mut config = Config::default();
        config.providers.push(provider("openai", vec!["a", "b"]));

        assert_eq!(
            route_pool_routes(&config),
            vec!["openai", "openai,a", "openai,b"]
        );
    }

    #[test]
    fn ensure_route_pool_preserves_existing_configuration() {
        let mut config = Config {
            route_pool: Some(RoutePoolConfig {
                enabled: true,
                failure_threshold: 5,
                ban_seconds: 10,
                candidates: vec![RoutePoolCandidate {
                    route: "p".to_string(),
                    enabled: false,
                    priority: 7,
                }],
            }),
            ..Default::default()
        };

        ensure_route_pool(&mut config);

        let pool = config.route_pool.unwrap();
        assert!(pool.enabled);
        assert_eq!(pool.failure_threshold, 5);
        assert_eq!(pool.ban_seconds, 10);
        assert_eq!(pool.candidates[0].priority, 7);
    }

    #[test]
    fn state_ensure_route_pool_initializes_default_configuration() {
        let mut state = SettingsState::new(Config::default());

        state.ensure_route_pool();

        let pool = state.config.route_pool.unwrap();
        assert!(!pool.enabled);
        assert_eq!(pool.failure_threshold, 3);
        assert_eq!(pool.ban_seconds, 3600);
        assert!(pool.candidates.is_empty());
    }

    #[test]
    fn add_route_pool_route_appends_priority_and_deduplicates() {
        let mut state = SettingsState::new(Config::default());

        state.add_route_pool_route("p".to_string());
        state.add_route_pool_route("p,a".to_string());
        state.add_route_pool_route("p".to_string());

        let candidates = &state.config.route_pool.unwrap().candidates;
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].priority, 1);
        assert_eq!(candidates[1].priority, 2);
    }

    #[test]
    fn move_route_pool_up_reorders_sorted_routes() {
        let mut state = SettingsState::new(Config {
            route_pool: Some(RoutePoolConfig {
                enabled: true,
                failure_threshold: 3,
                ban_seconds: 3600,
                candidates: vec![
                    RoutePoolCandidate {
                        route: "p,b".to_string(),
                        enabled: true,
                        priority: 2,
                    },
                    RoutePoolCandidate {
                        route: "p,a".to_string(),
                        enabled: true,
                        priority: 1,
                    },
                ],
            }),
            ..Default::default()
        });

        assert!(state.move_route_pool_up(1));

        let candidates = &state.config.route_pool.unwrap().candidates;
        assert_eq!(candidates[0].route, "p,b");
        assert_eq!(candidates[0].priority, 1);
        assert_eq!(candidates[1].route, "p,a");
        assert_eq!(candidates[1].priority, 2);
    }

    #[test]
    fn move_route_pool_down_and_remove_normalize_priorities() {
        let mut state = SettingsState::new(Config::default());
        state.add_route_pool_route("p,a".to_string());
        state.add_route_pool_route("p,b".to_string());
        state.add_route_pool_route("p,c".to_string());

        assert!(state.move_route_pool_down(0));
        assert!(state.remove_route_pool_index(1));

        let candidates = &state.config.route_pool.unwrap().candidates;
        assert_eq!(candidates[0].route, "p,b");
        assert_eq!(candidates[0].priority, 1);
        assert_eq!(candidates[1].route, "p,c");
        assert_eq!(candidates[1].priority, 2);
    }

    #[test]
    fn route_pool_index_operations_return_false_when_invalid() {
        let mut state = SettingsState::new(Config::default());

        assert!(!state.remove_route_pool_index(0));
        assert!(!state.move_route_pool_up(1));
        assert!(!state.move_route_pool_down(0));

        state.add_route_pool_route("p".to_string());

        assert!(!state.remove_route_pool_index(2));
        assert!(!state.move_route_pool_up(0));
        assert!(!state.move_route_pool_down(0));
    }
}
