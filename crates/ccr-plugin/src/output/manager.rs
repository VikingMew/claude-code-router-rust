use super::{OutputHandler, OutputOptions};
use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;

/// Output manager handles multiple output handlers
pub struct OutputManager {
    handlers: HashMap<String, Box<dyn OutputHandler>>,
}

impl OutputManager {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// Register an output handler
    pub fn register(&mut self, name: String, handler: Box<dyn OutputHandler>) {
        self.handlers.insert(name, handler);
    }

    /// Output to all handlers
    pub async fn output_all(&self, data: &Value, options: &OutputOptions) -> Result<()> {
        let futures: Vec<_> = self
            .handlers
            .values()
            .map(|h| h.output(data, options))
            .collect();

        let results = futures::future::join_all(futures).await;

        // Log errors but don't fail
        for (i, result) in results.iter().enumerate() {
            if let Err(e) = result {
                log::warn!("Handler {} failed: {}", i, e);
            }
        }

        Ok(())
    }

    /// Output to specific handler type
    pub async fn output_to_type(
        &self,
        handler_type: &str,
        data: &Value,
        options: &OutputOptions,
    ) -> Result<()> {
        for handler in self.handlers.values() {
            if handler.handler_type() == handler_type {
                handler.output(data, options).await?;
            }
        }
        Ok(())
    }

    /// Output to specific handler by name
    pub async fn output_to_name(
        &self,
        name: &str,
        data: &Value,
        options: &OutputOptions,
    ) -> Result<()> {
        if let Some(handler) = self.handlers.get(name) {
            handler.output(data, options).await?;
        }
        Ok(())
    }

    /// List all registered handlers
    pub fn list_handlers(&self) -> Vec<&str> {
        self.handlers.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for OutputManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::console::{ConsoleHandler, LogLevel};
    use serde_json::json;

    #[tokio::test]
    async fn test_output_manager_registration() {
        let mut manager = OutputManager::new();
        let handler = Box::new(ConsoleHandler::new(false, LogLevel::Info));

        manager.register("console".to_string(), handler);

        assert_eq!(manager.list_handlers().len(), 1);
        assert!(manager.list_handlers().contains(&"console"));
    }

    #[tokio::test]
    async fn test_output_all() {
        let mut manager = OutputManager::new();
        manager.register("console".to_string(), Box::new(ConsoleHandler::default()));

        let data = json!({"test": "value"});
        let options = OutputOptions::default();

        let result = manager.output_all(&data, &options).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_output_to_type() {
        let mut manager = OutputManager::new();
        manager.register("console1".to_string(), Box::new(ConsoleHandler::default()));
        manager.register("console2".to_string(), Box::new(ConsoleHandler::default()));

        let data = json!({"test": "value"});
        let options = OutputOptions::default();

        let result = manager.output_to_type("console", &data, &options).await;
        assert!(result.is_ok());
    }
}
