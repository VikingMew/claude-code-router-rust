use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, error};

#[derive(Debug, Serialize)]
struct TokenizeRequest {
    text: String,
}

#[derive(Debug, Deserialize)]
struct TokenizeResponse {
    token_count: usize,
}

pub async fn count_tokens_api(text: &str, endpoint: &str) -> Result<usize, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let request_body = TokenizeRequest {
        text: text.to_string(),
    };

    debug!(endpoint = %endpoint, text_len = text.len(), "Calling API tokenizer");

    let response = client
        .post(endpoint)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("API request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        error!(status = %status, "API tokenizer returned error");
        return Err(format!("API returned status: {}", status));
    }

    let result: TokenizeResponse = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    debug!(count = result.token_count, "API tokenizer result");

    Ok(result.token_count)
}
