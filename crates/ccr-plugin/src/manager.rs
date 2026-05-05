use crate::{Plugin, RequestContext, StreamContext};
use anyhow::Result;
use std::collections::{HashMap, HashSet};

/// Plugin Manager handles registration and execution of plugins
pub struct PluginManager {
    plugins: HashMap<String, Box<dyn Plugin>>,
    enabled: HashSet<String>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            enabled: HashSet::new(),
        }
    }

    /// Register a plugin
    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        let name = plugin.name().to_string();
        self.plugins.insert(name.clone(), plugin);
        self.enabled.insert(name);
    }

    /// Enable a plugin
    pub fn enable(&mut self, name: &str) -> bool {
        if self.plugins.contains_key(name) {
            self.enabled.insert(name.to_string());
            true
        } else {
            false
        }
    }

    /// Disable a plugin
    pub fn disable(&mut self, name: &str) {
        self.enabled.remove(name);
    }

    /// Check if plugin is enabled
    pub fn is_enabled(&self, name: &str) -> bool {
        self.enabled.contains(name)
    }

    /// List all registered plugins
    pub fn list_plugins(&self) -> Vec<&str> {
        self.plugins.keys().map(|s| s.as_str()).collect()
    }

    /// List enabled plugins
    pub fn list_enabled(&self) -> Vec<&str> {
        self.enabled.iter().map(|s| s.as_str()).collect()
    }

    /// Trigger request start hook for all enabled plugins
    pub async fn trigger_request_start(&self, ctx: &mut RequestContext) -> Result<()> {
        for name in &self.enabled {
            if let Some(plugin) = self.plugins.get(name) {
                if let Err(e) = plugin.on_request_start(ctx).await {
                    log::error!("Plugin '{}' on_request_start error: {}", name, e);
                }
            }
        }
        Ok(())
    }

    /// Trigger request end hook for all enabled plugins
    pub async fn trigger_request_end(&self, ctx: &mut RequestContext) -> Result<()> {
        for name in &self.enabled {
            if let Some(plugin) = self.plugins.get(name) {
                if let Err(e) = plugin.on_request_end(ctx).await {
                    log::error!("Plugin '{}' on_request_end error: {}", name, e);
                }
            }
        }
        Ok(())
    }

    /// Trigger stream chunk hook for all enabled plugins
    pub async fn trigger_stream_chunk(&self, ctx: &mut StreamContext) -> Result<()> {
        for name in &self.enabled {
            if let Some(plugin) = self.plugins.get(name) {
                if let Err(e) = plugin.on_stream_chunk(ctx).await {
                    log::error!("Plugin '{}' on_stream_chunk error: {}", name, e);
                }
            }
        }
        Ok(())
    }

    /// Shutdown all plugins
    pub async fn shutdown(&mut self) -> Result<()> {
        for (name, plugin) in self.plugins.iter_mut() {
            if let Err(e) = plugin.shutdown().await {
                log::error!("Plugin '{}' shutdown error: {}", name, e);
            }
        }
        Ok(())
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Plugin;
    use async_trait::async_trait;

    struct TestPlugin {
        name: String,
    }

    impl TestPlugin {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    #[async_trait]
    impl Plugin for TestPlugin {
        fn name(&self) -> &str {
            &self.name
        }
    }

    #[test]
    fn test_plugin_registration() {
        let mut manager = PluginManager::new();
        let plugin = Box::new(TestPlugin::new("test"));
        manager.register(plugin);

        assert!(manager.list_plugins().contains(&"test"));
        assert!(manager.is_enabled("test"));
    }

    #[test]
    fn test_plugin_enable_disable() {
        let mut manager = PluginManager::new();
        let plugin = Box::new(TestPlugin::new("test"));
        manager.register(plugin);

        assert!(manager.is_enabled("test"));

        manager.disable("test");
        assert!(!manager.is_enabled("test"));

        manager.enable("test");
        assert!(manager.is_enabled("test"));
    }

    #[test]
    fn test_enable_nonexistent_plugin() {
        let mut manager = PluginManager::new();
        assert!(!manager.enable("nonexistent"));
    }

    #[test]
    fn test_list_plugins() {
        let mut manager = PluginManager::new();
        manager.register(Box::new(TestPlugin::new("plugin1")));
        manager.register(Box::new(TestPlugin::new("plugin2")));
        manager.disable("plugin2");

        let all_plugins = manager.list_plugins();
        assert_eq!(all_plugins.len(), 2);

        let enabled_plugins = manager.list_enabled();
        assert_eq!(enabled_plugins.len(), 1);
        assert!(enabled_plugins.contains(&"plugin1"));
    }

    #[tokio::test]
    async fn test_trigger_hooks() {
        let mut manager = PluginManager::new();
        manager.register(Box::new(TestPlugin::new("test")));

        let mut ctx = RequestContext::new("session-123".to_string(), "claude-opus-4".to_string());

        // Should not panic
        manager.trigger_request_start(&mut ctx).await.unwrap();
        manager.trigger_request_end(&mut ctx).await.unwrap();
    }
}
