use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
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
    if is_loopback_endpoint(endpoint) {
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

fn is_loopback_endpoint(endpoint: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(endpoint) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|addr| addr.is_loopback())
            .unwrap_or(false)
}
