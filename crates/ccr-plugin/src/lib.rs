pub mod manager;
pub mod output;
pub mod plugins;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Plugin trait - all plugins must implement
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Plugin name
    fn name(&self) -> &str;

    /// Plugin version
    fn version(&self) -> &str {
        "1.0.0"
    }

    /// Plugin description
    fn description(&self) -> &str {
        ""
    }

    /// Initialize plugin
    async fn initialize(&mut self, _config: &Value) -> Result<()> {
        Ok(())
    }

    /// Request start hook
    async fn on_request_start(&self, _ctx: &mut RequestContext) -> Result<()> {
        Ok(())
    }

    /// Request end hook
    async fn on_request_end(&self, _ctx: &mut RequestContext) -> Result<()> {
        Ok(())
    }

    /// Stream chunk hook
    async fn on_stream_chunk(&self, _ctx: &mut StreamContext) -> Result<()> {
        Ok(())
    }

    /// Cleanup
    async fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Request context passed to plugins
#[derive(Debug, Clone)]
pub struct RequestContext {
    pub session_id: String,
    pub model: String,
    pub start_time: Instant,
    pub metadata: HashMap<String, Value>,
}

impl RequestContext {
    pub fn new(session_id: String, model: String) -> Self {
        Self {
            session_id,
            model,
            start_time: Instant::now(),
            metadata: HashMap::new(),
        }
    }
}

/// Stream context passed to plugins
#[derive(Debug, Clone)]
pub struct StreamContext {
    pub session_id: String,
    pub chunk: StreamChunk,
    pub elapsed: Duration,
}

/// Stream chunk types
#[derive(Debug, Clone)]
pub enum StreamChunk {
    ContentBlockDelta { delta: String },
    MessageStart,
    MessageEnd { usage: Option<Usage> },
}

/// Usage statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockPlugin;

    #[async_trait]
    impl Plugin for MockPlugin {
        fn name(&self) -> &str {
            "mock"
        }
    }

    #[tokio::test]
    async fn test_plugin_trait() {
        let plugin = MockPlugin;
        assert_eq!(plugin.name(), "mock");
        assert_eq!(plugin.version(), "1.0.0");
        assert_eq!(plugin.description(), "");
    }

    #[test]
    fn test_request_context_creation() {
        let ctx = RequestContext::new("session-123".to_string(), "claude-opus-4".to_string());
        assert_eq!(ctx.session_id, "session-123");
        assert_eq!(ctx.model, "claude-opus-4");
        assert!(ctx.metadata.is_empty());
    }
}
