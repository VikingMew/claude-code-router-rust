use anyhow::Result;
use ccr_types::Config;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// ReloadableConfig provides thread-safe config access with reload capability
pub struct ReloadableConfig {
    config: Arc<RwLock<Config>>,
    config_path: PathBuf,
}

impl ReloadableConfig {
    pub fn new(config: Config, config_path: PathBuf) -> Self {
        Self {
            config: Arc::new(RwLock::new(config)),
            config_path,
        }
    }

    /// Get current config (read-only)
    pub async fn get(&self) -> Config {
        self.config.read().await.clone()
    }

    /// Reload config from file
    pub async fn reload(&self) -> Result<()> {
        log::info!("Reloading config from: {}", self.config_path.display());

        // Load new config
        let new_config = crate::load_config(&self.config_path).map_err(|e| {
            log::error!("Failed to load new config: {}", e);
            e
        })?;

        // Validate new config
        validate_config(&new_config)?;

        // Apply new config
        let mut config = self.config.write().await;
        *config = new_config;

        log::info!("✅ Config reloaded successfully");
        Ok(())
    }

    /// Get Arc reference (for sharing)
    pub fn arc(&self) -> Arc<RwLock<Config>> {
        Arc::clone(&self.config)
    }

    /// Get config path
    pub fn path(&self) -> &PathBuf {
        &self.config_path
    }
}

fn validate_config(config: &Config) -> Result<()> {
    // Validation logic
    if config.providers.is_empty() {
        anyhow::bail!("No providers configured");
    }

    for provider in &config.providers {
        if provider.api_key.is_empty() {
            anyhow::bail!("Provider {} missing API key", provider.name);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccr_types::{Provider, ProviderApiKindSource, RoutePoolCandidate, RoutePoolConfig};
    use tempfile::NamedTempFile;

    fn create_test_config() -> Config {
        Config {
            providers: vec![Provider {
                name: "test".to_string(),
                api_kind: None,
                api_kind_source: ProviderApiKindSource::Inferred,
                api_base_url: "https://api.test.com".to_string(),
                api_key: "test-key".to_string(),
                models: vec![],
                endpoint_candidates: vec![],
                transformer: Default::default(),
            }],
            route_pool: Some(RoutePoolConfig {
                enabled: true,
                failure_threshold: 3,
                ban_seconds: 3600,
                candidates: vec![RoutePoolCandidate {
                    route: "test,model".to_string(),
                    enabled: true,
                    priority: 1,
                }],
            }),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_get_config() {
        let config = create_test_config();
        let path = PathBuf::from("/tmp/test.json");
        let reloadable = ReloadableConfig::new(config, path);

        let retrieved = reloadable.get().await;
        assert_eq!(retrieved.first_route_pool_route(), Some("test,model"));
    }

    #[tokio::test]
    async fn test_reload_valid_config() {
        use std::io::{Seek, Write};

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(
            r#"{"Providers": [{"name": "test", "api_key": "key", "api_base_url": "https://api.test.com", "models": []}], "Router": {}, "RoutePool": {"enabled": true, "candidates": [{"route": "initial,model"}]}}"#.as_bytes()
        ).unwrap();
        temp_file.flush().unwrap();

        let config = crate::load_config(temp_file.path()).unwrap();
        let reloadable = ReloadableConfig::new(config, temp_file.path().to_path_buf());

        // Modify config file
        temp_file.as_file().set_len(0).unwrap();
        temp_file.seek(std::io::SeekFrom::Start(0)).unwrap();
        temp_file.write_all(
            r#"{"Providers": [{"name": "test", "api_key": "key", "api_base_url": "https://api.test.com", "models": []}], "Router": {}, "RoutePool": {"enabled": true, "candidates": [{"route": "new,model"}]}}"#.as_bytes()
        ).unwrap();
        temp_file.flush().unwrap();

        // Reload
        reloadable.reload().await.unwrap();

        // Verify
        let config = reloadable.get().await;
        assert_eq!(config.first_route_pool_route(), Some("new,model"));
    }

    #[tokio::test]
    async fn test_reload_invalid_config_keeps_old() {
        use std::io::{Seek, Write};

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(
            r#"{"Providers": [{"name": "test", "api_key": "key", "api_base_url": "https://api.test.com", "models": []}], "Router": {}, "RoutePool": {"enabled": true, "candidates": [{"route": "old,model"}]}}"#.as_bytes()
        ).unwrap();
        temp_file.flush().unwrap();

        let config = crate::load_config(temp_file.path()).unwrap();
        let reloadable = ReloadableConfig::new(config, temp_file.path().to_path_buf());

        let old_route = reloadable
            .get()
            .await
            .first_route_pool_route()
            .unwrap()
            .to_string();

        // Write invalid config
        temp_file.as_file().set_len(0).unwrap();
        temp_file.seek(std::io::SeekFrom::Start(0)).unwrap();
        temp_file.write_all(b"invalid json").unwrap();
        temp_file.flush().unwrap();

        // Reload should fail
        assert!(reloadable.reload().await.is_err());

        // Old config should be preserved
        let config = reloadable.get().await;
        assert_eq!(config.first_route_pool_route(), Some(old_route.as_str()));
    }

    #[tokio::test]
    async fn test_validate_config_fails_on_empty_providers() {
        let config = Config {
            providers: vec![],
            ..Default::default()
        };

        assert!(validate_config(&config).is_err());
    }

    #[tokio::test]
    async fn test_validate_config_fails_on_empty_api_key() {
        let config = Config {
            providers: vec![Provider {
                name: "test".to_string(),
                api_kind: None,
                api_kind_source: ProviderApiKindSource::Inferred,
                api_base_url: "https://api.test.com".to_string(),
                api_key: "".to_string(),
                models: vec![],
                endpoint_candidates: vec![],
                transformer: Default::default(),
            }],
            ..Default::default()
        };

        assert!(validate_config(&config).is_err());
    }
}
