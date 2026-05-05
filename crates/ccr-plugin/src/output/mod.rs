pub mod console;
pub mod manager;
pub mod tempfile;
pub mod webhook;

pub use manager::OutputManager;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

/// Output handler trait - all output handlers must implement
#[async_trait]
pub trait OutputHandler: Send + Sync {
    /// Handler type identifier
    fn handler_type(&self) -> &str;

    /// Output data to the handler
    /// Returns true if successful, false otherwise
    async fn output(&self, data: &Value, options: &OutputOptions) -> Result<bool>;
}

/// Output options
#[derive(Debug, Clone)]
pub struct OutputOptions {
    pub format: OutputFormat,
    pub timestamp: bool,
    pub prefix: Option<String>,
    pub metadata: HashMap<String, Value>,
}

impl Default for OutputOptions {
    fn default() -> Self {
        Self {
            format: OutputFormat::Json,
            timestamp: false,
            prefix: None,
            metadata: HashMap::new(),
        }
    }
}

/// Output format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Json,
    Text,
    Markdown,
}
