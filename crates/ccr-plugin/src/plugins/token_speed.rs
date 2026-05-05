use crate::output::{OutputFormat, OutputManager, OutputOptions};
use crate::{Plugin, RequestContext, StreamChunk, StreamContext, Usage};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// Token Speed Plugin - monitors and reports token generation speed
pub struct TokenSpeedPlugin {
    output_manager: Arc<OutputManager>,
    sessions: Arc<RwLock<HashMap<String, SessionMetrics>>>,
}

struct SessionMetrics {
    start_time: Instant,
    first_token_time: Option<Instant>,
    token_count: u64,
    last_update: Instant,
    tokens_per_second: f64,
}

impl Default for SessionMetrics {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            start_time: now,
            first_token_time: None,
            token_count: 0,
            last_update: now,
            tokens_per_second: 0.0,
        }
    }
}

impl TokenSpeedPlugin {
    pub fn new(output_manager: Arc<OutputManager>) -> Self {
        Self {
            output_manager,
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn process_delta(&self, session_id: &str, delta: &str) -> Result<()> {
        let mut sessions = self.sessions.write().await;

        if let Some(metrics) = sessions.get_mut(session_id) {
            // Record first token time
            if metrics.first_token_time.is_none() {
                metrics.first_token_time = Some(Instant::now());
            }

            // Estimate token count (simple heuristic)
            let tokens = estimate_tokens(delta);
            metrics.token_count += tokens;

            // Calculate tokens/sec (1-second sliding window)
            let now = Instant::now();
            let window_duration = now.duration_since(metrics.last_update);

            if window_duration.as_secs() >= 1 {
                metrics.tokens_per_second = tokens as f64 / window_duration.as_secs_f64();
                metrics.last_update = now;

                // Output progress
                self.output_progress(session_id, metrics).await?;
            }
        }

        Ok(())
    }

    async fn finalize_session(&self, session_id: &str, usage: &Option<Usage>) -> Result<()> {
        let sessions = self.sessions.read().await;

        if let Some(metrics) = sessions.get(session_id) {
            let total_duration = metrics.start_time.elapsed();
            let ttft = metrics
                .first_token_time
                .map(|t| t.duration_since(metrics.start_time).as_millis() as u64);

            let token_count = usage
                .as_ref()
                .and_then(|u| u.output_tokens)
                .unwrap_or(metrics.token_count);

            let avg_speed = token_count as f64 / total_duration.as_secs_f64();

            let data = json!({
                "sessionId": session_id,
                "tokenCount": token_count,
                "tokensPerSecond": avg_speed,
                "timeToFirstToken": ttft,
                "totalDuration": total_duration.as_millis(),
                "stream": true,
            });

            let options = OutputOptions {
                format: OutputFormat::Json,
                timestamp: true,
                prefix: Some("[Token Speed Final]".to_string()),
                metadata: HashMap::new(),
            };

            self.output_manager.output_all(&data, &options).await?;
        }

        Ok(())
    }

    async fn output_progress(&self, session_id: &str, metrics: &SessionMetrics) -> Result<()> {
        let data = json!({
            "sessionId": session_id,
            "tokenCount": metrics.token_count,
            "tokensPerSecond": metrics.tokens_per_second,
            "elapsed": metrics.start_time.elapsed().as_millis(),
        });

        let options = OutputOptions {
            format: OutputFormat::Json,
            timestamp: true,
            prefix: Some("[Token Speed]".to_string()),
            metadata: HashMap::new(),
        };

        self.output_manager.output_all(&data, &options).await?;

        Ok(())
    }
}

#[async_trait]
impl Plugin for TokenSpeedPlugin {
    fn name(&self) -> &str {
        "token-speed"
    }

    fn description(&self) -> &str {
        "Monitors and reports token generation speed"
    }

    async fn on_request_start(&self, ctx: &mut RequestContext) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        sessions.insert(
            ctx.session_id.clone(),
            SessionMetrics {
                start_time: Instant::now(),
                last_update: Instant::now(),
                ..Default::default()
            },
        );
        Ok(())
    }

    async fn on_stream_chunk(&self, ctx: &mut StreamContext) -> Result<()> {
        match &ctx.chunk {
            StreamChunk::ContentBlockDelta { delta } => {
                self.process_delta(&ctx.session_id, delta).await?;
            }
            StreamChunk::MessageEnd { usage } => {
                self.finalize_session(&ctx.session_id, usage).await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<()> {
        self.sessions.write().await.clear();
        Ok(())
    }
}

/// Simple token estimation
fn estimate_tokens(text: &str) -> u64 {
    // Check if text contains Chinese/Japanese/Korean characters
    let is_cjk = text.chars().any(|c| {
        let code = c as u32;
        (0x4E00..=0x9FFF).contains(&code) // CJK Unified Ideographs
            || (0x3040..=0x309F).contains(&code) // Hiragana
            || (0x30A0..=0x30FF).contains(&code) // Katakana
            || (0xAC00..=0xD7AF).contains(&code) // Hangul
    });

    let divisor = if is_cjk { 1.5 } else { 4.0 };
    (text.len() as f64 / divisor).ceil() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_estimate_tokens_english() {
        assert_eq!(estimate_tokens("Hello world"), 3); // 11 chars / 4 = 2.75 -> 3
    }

    #[test]
    fn test_estimate_tokens_chinese() {
        // Chinese chars: "你好世界" = 4 chars * 3 bytes each = 12 bytes
        // 12 / 1.5 = 8
        assert_eq!(estimate_tokens("你好世界"), 8);
    }

    #[test]
    fn test_estimate_tokens_mixed() {
        let text = "Hello 世界";
        let tokens = estimate_tokens(text);
        assert!(tokens > 0);
    }

    #[tokio::test]
    async fn test_token_speed_plugin_lifecycle() {
        let output_manager = Arc::new(OutputManager::new());
        let mut plugin = TokenSpeedPlugin::new(output_manager);

        assert_eq!(plugin.name(), "token-speed");
        assert!(!plugin.description().is_empty());

        let mut ctx = RequestContext::new("test-session".to_string(), "claude-opus-4".to_string());
        plugin.on_request_start(&mut ctx).await.unwrap();

        plugin.shutdown().await.unwrap();
    }
}
