use super::{OutputHandler, OutputOptions};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use std::path::PathBuf;

/// Temp file output handler - writes data to temporary files
pub struct TempFileHandler {
    subdirectory: String,
    extension: String,
    include_timestamp: bool,
    prefix: String,
}

impl TempFileHandler {
    pub fn new(subdirectory: String, prefix: String) -> Self {
        Self {
            subdirectory,
            extension: "json".to_string(),
            include_timestamp: false,
            prefix,
        }
    }

    pub fn with_extension(mut self, extension: String) -> Self {
        self.extension = extension;
        self
    }

    pub fn with_timestamp(mut self, include_timestamp: bool) -> Self {
        self.include_timestamp = include_timestamp;
        self
    }
}

impl Default for TempFileHandler {
    fn default() -> Self {
        Self {
            subdirectory: "claude-code-router".to_string(),
            extension: "json".to_string(),
            include_timestamp: false,
            prefix: "session".to_string(),
        }
    }
}

#[async_trait]
impl OutputHandler for TempFileHandler {
    fn handler_type(&self) -> &str {
        "temp-file"
    }

    async fn output(&self, data: &Value, _options: &OutputOptions) -> Result<bool> {
        let temp_dir = std::env::temp_dir().join(&self.subdirectory);
        tokio::fs::create_dir_all(&temp_dir).await?;

        let session_id = data["sessionId"].as_str().unwrap_or("unknown");

        let filename = if self.include_timestamp {
            format!(
                "{}-{}-{}.{}",
                self.prefix,
                session_id,
                Utc::now().timestamp(),
                self.extension
            )
        } else {
            format!("{}-{}.{}", self.prefix, session_id, self.extension)
        };

        let path: PathBuf = temp_dir.join(filename);
        let json = serde_json::to_string_pretty(data)?;

        tokio::fs::write(&path, json).await?;
        log::debug!("Wrote output to: {}", path.display());

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::OutputFormat;
    use serde_json::json;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_tempfile_output() {
        let handler = TempFileHandler::default();
        let data = json!({
            "sessionId": "test-123",
            "test": "value"
        });
        let options = OutputOptions {
            format: OutputFormat::Json,
            timestamp: false,
            prefix: None,
            metadata: HashMap::new(),
        };

        let result = handler.output(&data, &options).await;
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Verify file exists
        let temp_dir = std::env::temp_dir().join("claude-code-router");
        let expected_file = temp_dir.join("session-test-123.json");
        assert!(expected_file.exists());

        // Cleanup
        tokio::fs::remove_file(expected_file).await.ok();
    }

    #[tokio::test]
    async fn test_tempfile_with_timestamp() {
        let handler = TempFileHandler::new("test-ccr".to_string(), "prefix".to_string())
            .with_timestamp(true)
            .with_extension("txt".to_string());

        let data = json!({
            "sessionId": "session-456"
        });
        let options = OutputOptions::default();

        let result = handler.output(&data, &options).await;
        assert!(result.is_ok());

        // Cleanup
        let temp_dir = std::env::temp_dir().join("test-ccr");
        tokio::fs::remove_dir_all(temp_dir).await.ok();
    }
}
