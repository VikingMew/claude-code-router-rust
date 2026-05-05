use super::{OutputHandler, OutputOptions};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

/// Webhook output handler - sends data to HTTP endpoint
pub struct WebhookHandler {
    url: String,
    method: HttpMethod,
    headers: HashMap<String, String>,
    auth: Option<AuthConfig>,
    retry: RetryConfig,
    client: Client,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Post,
    Put,
    Patch,
}

#[derive(Debug, Clone)]
pub enum AuthConfig {
    Bearer { token: String },
    Basic { username: String, password: String },
    Custom { header: String, value: String },
}

#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_attempts: u32,
    pub backoff_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff_ms: 1000,
        }
    }
}

impl WebhookHandler {
    pub fn new(url: String) -> Self {
        Self {
            url,
            method: HttpMethod::Post,
            headers: HashMap::new(),
            auth: None,
            retry: RetryConfig::default(),
            client: Client::new(),
        }
    }

    pub fn with_method(mut self, method: HttpMethod) -> Self {
        self.method = method;
        self
    }

    pub fn with_auth(mut self, auth: AuthConfig) -> Self {
        self.auth = Some(auth);
        self
    }

    pub fn with_retry(mut self, retry: RetryConfig) -> Self {
        self.retry = retry;
        self
    }

    pub fn with_header(mut self, key: String, value: String) -> Self {
        self.headers.insert(key, value);
        self
    }

    async fn send_request(&self, payload: &Value) -> Result<()> {
        let mut req = match self.method {
            HttpMethod::Post => self.client.post(&self.url),
            HttpMethod::Put => self.client.put(&self.url),
            HttpMethod::Patch => self.client.patch(&self.url),
        };

        // Add authentication
        if let Some(auth) = &self.auth {
            req = match auth {
                AuthConfig::Bearer { token } => {
                    req.header("Authorization", format!("Bearer {}", token))
                }
                AuthConfig::Basic { username, password } => {
                    req.basic_auth(username, Some(password))
                }
                AuthConfig::Custom { header, value } => req.header(header, value),
            };
        }

        // Add custom headers
        for (key, value) in &self.headers {
            req = req.header(key, value);
        }

        let resp = req.json(payload).send().await?;

        if !resp.status().is_success() {
            anyhow::bail!("HTTP {}: {}", resp.status(), resp.text().await?);
        }

        Ok(())
    }
}

#[async_trait]
impl OutputHandler for WebhookHandler {
    fn handler_type(&self) -> &str {
        "webhook"
    }

    async fn output(&self, data: &Value, options: &OutputOptions) -> Result<bool> {
        let payload = json!({
            "data": data,
            "timestamp": if options.timestamp {
                Some(Utc::now().to_rfc3339())
            } else {
                None
            },
            "prefix": options.prefix,
            "metadata": options.metadata,
        });

        // Retry logic
        for attempt in 0..self.retry.max_attempts {
            match self.send_request(&payload).await {
                Ok(_) => return Ok(true),
                Err(e) => {
                    if attempt < self.retry.max_attempts - 1 {
                        let backoff = self.retry.backoff_ms * 2_u64.pow(attempt);
                        log::warn!(
                            "Webhook attempt {} failed: {}, retrying in {}ms",
                            attempt + 1,
                            e,
                            backoff
                        );
                        tokio::time::sleep(Duration::from_millis(backoff)).await;
                    } else {
                        log::error!(
                            "Webhook failed after {} attempts: {}",
                            self.retry.max_attempts,
                            e
                        );
                        return Ok(false);
                    }
                }
            }
        }

        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webhook_builder() {
        let handler = WebhookHandler::new("https://example.com/webhook".to_string())
            .with_method(HttpMethod::Post)
            .with_header("X-Custom".to_string(), "value".to_string())
            .with_auth(AuthConfig::Bearer {
                token: "token123".to_string(),
            });

        assert_eq!(handler.handler_type(), "webhook");
        assert_eq!(handler.url, "https://example.com/webhook");
        assert!(handler.auth.is_some());
    }
}
