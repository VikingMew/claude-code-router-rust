pub mod tokenizer;

use ccr_types::{Config, MessagesRequest, Provider, TokenizerBackend};
use once_cell::sync::Lazy;
use tiktoken_rs::cl100k_base;
use tracing::warn;

use tokenizer::{api::count_tokens_api, cache::TokenCache};

static TOKEN_CACHE: Lazy<TokenCache> = Lazy::new(|| TokenCache::new(1000));

fn count_tokens_tiktoken(text: &str) -> usize {
    cl100k_base()
        .unwrap()
        .encode_with_special_tokens(text)
        .len()
}

fn count_tokens_hf(text: &str) -> usize {
    // Use a simple whitespace/punctuation split as fallback when no model file available
    // In production, load from a HuggingFace tokenizer file via tokenizers crate
    use tokenizers::tokenizer::Tokenizer;
    // Try to load from well-known path, fall back to tiktoken
    let path = dirs_next::home_dir()
        .unwrap_or_default()
        .join(".claude-code-router")
        .join("tokenizer.json");
    if let Ok(tok) = Tokenizer::from_file(&path) {
        tok.encode(text, false)
            .map(|e| e.len())
            .unwrap_or_else(|_| count_tokens_tiktoken(text))
    } else {
        count_tokens_tiktoken(text)
    }
}

/// Synchronous token counting for local backends and non-async callers.
///
/// If the API tokenizer is requested while already inside a Tokio runtime, this
/// uses the tiktoken fallback instead of creating a nested runtime.
pub fn count_tokens(req: &MessagesRequest, backend: &TokenizerBackend) -> usize {
    let count = |text: &str| match backend {
        TokenizerBackend::Tiktoken => count_tokens_tiktoken(text),
        TokenizerBackend::Huggingface => count_tokens_hf(text),
        TokenizerBackend::Api { endpoint } => {
            // Check cache first
            if let Some(cached) = TOKEN_CACHE.get(text) {
                return cached;
            }

            if tokio::runtime::Handle::try_current().is_ok() {
                warn!(
                    "API tokenizer requested from sync token counter inside an async runtime; falling back to tiktoken"
                );
                let count = count_tokens_tiktoken(text);
                TOKEN_CACHE.set(text, count);
                return count;
            }

            let rt = tokio::runtime::Runtime::new()
                .expect("failed to create Tokio runtime for sync API tokenizer call");
            rt.block_on(count_tokens_api_with_fallback(text, endpoint))
        }
    };
    count_tokens_with(req, count)
}

pub async fn count_tokens_async(req: &MessagesRequest, backend: &TokenizerBackend) -> usize {
    match backend {
        TokenizerBackend::Tiktoken => count_tokens_with(req, count_tokens_tiktoken),
        TokenizerBackend::Huggingface => count_tokens_with(req, count_tokens_hf),
        TokenizerBackend::Api { endpoint } => {
            let mut total = 0;
            for msg in &req.messages {
                total += count_tokens_api_with_fallback(&msg.content.to_string(), endpoint).await;
            }
            if let Some(sys) = &req.system {
                total += count_tokens_api_with_fallback(&sys.to_string(), endpoint).await;
            }
            total
        }
    }
}

fn count_tokens_with(req: &MessagesRequest, count: impl Fn(&str) -> usize) -> usize {
    let mut total = 0;
    for msg in &req.messages {
        total += count(&msg.content.to_string());
    }
    if let Some(sys) = &req.system {
        total += count(&sys.to_string());
    }
    total
}

async fn count_tokens_api_with_fallback(text: &str, endpoint: &str) -> usize {
    if let Some(cached) = TOKEN_CACHE.get(text) {
        return cached;
    }

    match count_tokens_api(text, endpoint).await {
        Ok(count) => {
            TOKEN_CACHE.set(text, count);
            count
        }
        Err(e) => {
            warn!("API tokenizer failed: {}, falling back to tiktoken", e);
            let count = count_tokens_tiktoken(text);
            TOKEN_CACHE.set(text, count);
            count
        }
    }
}

pub fn find_provider<'a>(model_str: &str, config: &'a Config) -> Option<&'a Provider> {
    let provider_name = model_str.split(',').next()?;
    config.providers.iter().find(|p| p.name == provider_name)
}

pub fn model_name(model_str: &str) -> &str {
    model_str.splitn(2, ',').nth(1).unwrap_or(model_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccr_types::MessagesRequest;

    fn base_req() -> MessagesRequest {
        MessagesRequest {
            model: "claude-3-5-sonnet".into(),
            messages: vec![],
            system: None,
            tools: None,
            max_tokens: Some(100),
            stream: false,
            thinking: None,
        }
    }

    #[test]
    fn find_provider_by_name() {
        use ccr_types::Provider;
        let mut config = ccr_types::Config::default();
        config.providers.push(Provider {
            name: "p1".into(),
            api_kind: None,
            api_kind_source: ccr_types::ProviderApiKindSource::Inferred,
            api_base_url: "https://api.example.com".into(),
            api_key: "key".into(),
            models: vec![],
            endpoint_candidates: vec![],
            transformer: Default::default(),
        });
        assert!(find_provider("p1,model-a", &config).is_some());
        assert!(find_provider("p2,model-b", &config).is_none());
    }

    #[test]
    fn model_name_splits_correctly() {
        assert_eq!(model_name("openai,gpt-4o"), "gpt-4o");
        assert_eq!(model_name("no-comma"), "no-comma");
    }

    #[test]
    fn count_tokens_tiktoken_backend() {
        use ccr_types::{Message, TokenizerBackend};
        let mut req = base_req();
        req.messages = vec![Message {
            role: "user".into(),
            content: serde_json::json!("hello world"),
        }];
        let n = count_tokens(&req, &TokenizerBackend::Tiktoken);
        assert!(n > 0);
    }

    #[test]
    fn count_tokens_hf_falls_back_to_tiktoken() {
        use ccr_types::{Message, TokenizerBackend};
        let mut req = base_req();
        req.messages = vec![Message {
            role: "user".into(),
            content: serde_json::json!("hello world"),
        }];
        let n = count_tokens(&req, &TokenizerBackend::Huggingface);
        assert!(n > 0);
    }

    #[tokio::test]
    async fn count_tokens_async_api_falls_back_to_tiktoken() {
        use ccr_types::{Message, TokenizerBackend};
        let mut req = base_req();
        req.messages = vec![Message {
            role: "user".into(),
            content: serde_json::json!("api async fallback unique"),
        }];

        let expected = count_tokens(&req, &TokenizerBackend::Tiktoken);
        let n = count_tokens_async(
            &req,
            &TokenizerBackend::Api {
                endpoint: "http://127.0.0.1:9/tokenize".into(),
            },
        )
        .await;

        assert_eq!(n, expected);
    }

    #[tokio::test]
    async fn sync_count_tokens_api_inside_runtime_uses_local_fallback() {
        use ccr_types::{Message, TokenizerBackend};
        let mut req = base_req();
        req.messages = vec![Message {
            role: "user".into(),
            content: serde_json::json!("sync api fallback inside runtime unique"),
        }];

        let expected = count_tokens(&req, &TokenizerBackend::Tiktoken);
        let n = count_tokens(
            &req,
            &TokenizerBackend::Api {
                endpoint: "http://127.0.0.1:9/tokenize".into(),
            },
        );

        assert_eq!(n, expected);
    }
}
