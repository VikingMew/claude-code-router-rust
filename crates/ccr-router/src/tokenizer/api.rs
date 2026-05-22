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
    let mut builder = Client::builder().timeout(Duration::from_secs(5));
    if endpoint_uses_loopback(endpoint) {
        builder = builder.no_proxy();
    }
    let client = builder
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

fn endpoint_uses_loopback(endpoint: &str) -> bool {
    reqwest::Url::parse(endpoint)
        .ok()
        .and_then(|url| url.host_str().map(is_loopback_host))
        .unwrap_or(false)
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
}
