use crate::{SseEvent, SseParser};
use ccr_types::{Config, Message, MessagesRequest};

/// Execute continuation: add assistant/user messages and re-invoke LLM
pub async fn invoke_continuation(
    original_req: &MessagesRequest,
    assistant_content: Vec<serde_json::Value>,
    tool_results: Vec<serde_json::Value>,
    config: &Config,
) -> Result<Vec<SseEvent>, Box<dyn std::error::Error + Send + Sync>> {
    // Build new request with continuation messages
    let mut new_req = original_req.clone();

    // Add assistant message with tool_use
    new_req.messages.push(Message {
        role: "assistant".to_string(),
        content: serde_json::Value::Array(assistant_content),
    });

    // Add user message with tool_result
    new_req.messages.push(Message {
        role: "user".to_string(),
        content: serde_json::Value::Array(tool_results),
    });

    // Make recursive HTTP call
    let port = config.port.unwrap_or(3456);
    let api_key = config.api_key.as_deref().unwrap_or("");

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://127.0.0.1:{}/v1/messages", port))
        .header("x-api-key", api_key)
        .header("content-type", "application/json")
        .json(&new_req)
        .send()
        .await
        .map_err(|e| format!("Continuation request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Continuation failed with status {}", response.status()).into());
    }

    // Parse SSE stream from response
    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read continuation response: {}", e))?;

    let mut parser = SseParser::new();
    let events = parser.feed(&text);

    // Filter out message_start and message_stop events
    let filtered: Vec<SseEvent> = events
        .into_iter()
        .filter(|event| {
            if let Some(data) = event.parse_data() {
                let event_type = data.get("type").and_then(|t| t.as_str()).unwrap_or("");
                !matches!(event_type, "message_start" | "message_stop")
            } else {
                true
            }
        })
        .collect();

    Ok(filtered)
}
