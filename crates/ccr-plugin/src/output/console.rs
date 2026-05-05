use super::{OutputFormat, OutputHandler, OutputOptions};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

/// Console output handler - prints to stdout/stderr
pub struct ConsoleHandler {
    pub colors: bool,
    pub level: LogLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
}

impl ConsoleHandler {
    pub fn new(colors: bool, level: LogLevel) -> Self {
        Self { colors, level }
    }
}

impl Default for ConsoleHandler {
    fn default() -> Self {
        Self {
            colors: true,
            level: LogLevel::Info,
        }
    }
}

#[async_trait]
impl OutputHandler for ConsoleHandler {
    fn handler_type(&self) -> &str {
        "console"
    }

    async fn output(&self, data: &Value, options: &OutputOptions) -> Result<bool> {
        let output = match options.format {
            OutputFormat::Json => serde_json::to_string_pretty(data)?,
            OutputFormat::Text => format_text(data),
            OutputFormat::Markdown => format_markdown(data),
        };

        let final_output = if let Some(prefix) = &options.prefix {
            format!("{} {}", prefix, output)
        } else {
            output
        };

        if self.colors {
            println!("{}", colorize(&final_output, self.level));
        } else {
            println!("{}", final_output);
        }

        Ok(true)
    }
}

fn format_text(data: &Value) -> String {
    if let Some(s) = data.as_str() {
        s.to_string()
    } else {
        data.to_string()
    }
}

fn format_markdown(data: &Value) -> String {
    format!(
        "```json\n{}\n```",
        serde_json::to_string_pretty(data).unwrap_or_default()
    )
}

fn colorize(text: &str, level: LogLevel) -> String {
    // Simple ANSI color codes
    match level {
        LogLevel::Info => format!("\x1b[32m{}\x1b[0m", text), // Green
        LogLevel::Warn => format!("\x1b[33m{}\x1b[0m", text), // Yellow
        LogLevel::Error => format!("\x1b[31m{}\x1b[0m", text), // Red
        LogLevel::Debug => format!("\x1b[36m{}\x1b[0m", text), // Cyan
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_console_output_json() {
        let handler = ConsoleHandler::default();
        let data = json!({"test": "value"});
        let options = OutputOptions {
            format: OutputFormat::Json,
            timestamp: false,
            prefix: Some("[TEST]".to_string()),
            metadata: HashMap::new(),
        };

        let result = handler.output(&data, &options).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_console_output_text() {
        let handler = ConsoleHandler::new(false, LogLevel::Info);
        let data = json!("simple text");
        let options = OutputOptions {
            format: OutputFormat::Text,
            ..Default::default()
        };

        let result = handler.output(&data, &options).await;
        assert!(result.is_ok());
    }
}
