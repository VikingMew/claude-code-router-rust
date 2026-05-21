pub mod custom;
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

fn has_web_search(req: &MessagesRequest) -> bool {
    req.tools.as_ref().map_or(false, |tools| {
        tools
            .iter()
            .any(|t| t.get("name").and_then(|n| n.as_str()) == Some("web_search"))
    })
}

fn subagent_model(req: &MessagesRequest) -> Option<String> {
    let sys = req.system.as_ref()?.as_str()?;
    let start = sys.find("<CCR-SUBAGENT-MODEL>")? + 20;
    let end = sys.find("</CCR-SUBAGENT-MODEL>")?;
    Some(sys[start..end].trim().to_string())
}

/// Returns (model_string, reason)
pub fn select_model(req: &MessagesRequest, config: &Config) -> (String, &'static str) {
    if let Some(m) = subagent_model(req) {
        return (m, "subagent");
    }
    if req.model.contains("-haiku-") {
        if let Some(bg) = configured_route(config.router.background.as_deref()) {
            return (bg.to_string(), "background");
        }
    }
    if has_web_search(req) {
        if let Some(ws) = configured_route(config.router.web_search.as_deref()) {
            return (ws.to_string(), "webSearch");
        }
    }
    if req.thinking.is_some() {
        if let Some(think) = configured_route(config.router.think.as_deref()) {
            return (think.to_string(), "think");
        }
    }
    let threshold = config.router.long_context_threshold.unwrap_or(60000);
    if count_tokens(req, &config.router.tokenizer_backend) as u64 > threshold {
        if let Some(lc) = configured_route(config.router.long_context.as_deref()) {
            return (lc.to_string(), "longContext");
        }
    }
    (
        config
            .first_route_pool_route()
            .unwrap_or_default()
            .to_string(),
        "routePool",
    )
}

/// Async variant of `select_model` for server/runtime callers that may use an
/// API tokenizer for long-context route detection.
pub async fn select_model_async(req: &MessagesRequest, config: &Config) -> (String, &'static str) {
    if let Some(m) = subagent_model(req) {
        return (m, "subagent");
    }
    if req.model.contains("-haiku-") {
        if let Some(bg) = configured_route(config.router.background.as_deref()) {
            return (bg.to_string(), "background");
        }
    }
    if has_web_search(req) {
        if let Some(ws) = configured_route(config.router.web_search.as_deref()) {
            return (ws.to_string(), "webSearch");
        }
    }
    if req.thinking.is_some() {
        if let Some(think) = configured_route(config.router.think.as_deref()) {
            return (think.to_string(), "think");
        }
    }
    let threshold = config.router.long_context_threshold.unwrap_or(60000);
    if count_tokens_async(req, &config.router.tokenizer_backend).await as u64 > threshold {
        if let Some(lc) = configured_route(config.router.long_context.as_deref()) {
            return (lc.to_string(), "longContext");
        }
    }
    (
        config
            .first_route_pool_route()
            .unwrap_or_default()
            .to_string(),
        "routePool",
    )
}

fn configured_route(route: Option<&str>) -> Option<&str> {
    let route = route?.trim();
    if route.is_empty() { None } else { Some(route) }
}

pub fn find_provider<'a>(model_str: &str, config: &'a Config) -> Option<&'a Provider> {
    let provider_name = model_str.split(',').next()?;
    config.providers.iter().find(|p| p.name == provider_name)
}

pub fn model_name(model_str: &str) -> &str {
    model_str.splitn(2, ',').nth(1).unwrap_or(model_str)
}

/// Load project-level router overrides from ~/.claude/projects/<project_id>/claude-code-router.json
pub fn load_project_router(project_id: &str) -> Option<ccr_types::RouterConfig> {
    let path = dirs_next::home_dir()?
        .join(".claude")
        .join("projects")
        .join(project_id)
        .join("claude-code-router.json");
    let raw = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Load custom router rules from a JSON file at the given path.
/// The file should be a RouterConfig JSON object.
pub fn load_custom_router(path: &str) -> Option<ccr_types::RouterConfig> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Merge override router config into base config (non-None fields override)
pub fn merge_router(
    base: &ccr_types::RouterConfig,
    over: &ccr_types::RouterConfig,
) -> ccr_types::RouterConfig {
    ccr_types::RouterConfig {
        background: over.background.clone().or_else(|| base.background.clone()),
        think: over.think.clone().or_else(|| base.think.clone()),
        long_context: over
            .long_context
            .clone()
            .or_else(|| base.long_context.clone()),
        long_context_threshold: over.long_context_threshold.or(base.long_context_threshold),
        web_search: over.web_search.clone().or_else(|| base.web_search.clone()),
        image: over.image.clone().or_else(|| base.image.clone()),
        tokenizer_backend: base.tokenizer_backend.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccr_types::{Config, MessagesRequest};

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

    fn base_config(route: &str) -> Config {
        Config {
            route_pool: Some(ccr_types::RoutePoolConfig {
                enabled: true,
                failure_threshold: 3,
                ban_seconds: 3600,
                candidates: vec![ccr_types::RoutePoolCandidate {
                    route: route.into(),
                    enabled: true,
                    priority: 1,
                }],
            }),
            ..Default::default()
        }
    }

    #[test]
    fn selects_route_pool() {
        let (model, reason) = select_model(&base_req(), &base_config("openai,gpt-4o"));
        assert_eq!(model, "openai,gpt-4o");
        assert_eq!(reason, "routePool");
    }

    #[test]
    fn selects_background_for_haiku() {
        let mut req = base_req();
        req.model = "claude-3-haiku-20240307".into();
        let mut config = base_config("openai,gpt-4o");
        config.router.background = Some("openai,gpt-4o-mini".into());
        let (model, reason) = select_model(&req, &config);
        assert_eq!(model, "openai,gpt-4o-mini");
        assert_eq!(reason, "background");
    }

    #[test]
    fn selects_web_search() {
        let mut req = base_req();
        req.tools = Some(vec![serde_json::json!({"name": "web_search"})]);
        let mut config = base_config("openai,gpt-4o");
        config.router.web_search = Some("openai,gpt-4o-search".into());
        let (model, reason) = select_model(&req, &config);
        assert_eq!(reason, "webSearch");
        assert_eq!(model, "openai,gpt-4o-search");
    }

    #[test]
    fn selects_think() {
        let mut req = base_req();
        req.thinking = Some(serde_json::json!({"type": "enabled"}));
        let mut config = base_config("openai,gpt-4o");
        config.router.think = Some("anthropic,claude-3-7-sonnet".into());
        let (_, reason) = select_model(&req, &config);
        assert_eq!(reason, "think");
    }

    #[test]
    fn empty_scenario_routes_fall_back_to_route_pool() {
        let mut req = base_req();
        req.thinking = Some(serde_json::json!({"type": "enabled"}));
        req.tools = Some(vec![serde_json::json!({"name": "web_search"})]);
        let mut config = base_config("zenmux");
        config.router.background = Some("".into());
        config.router.think = Some("".into());
        config.router.web_search = Some("   ".into());
        config.router.long_context = Some("".into());

        let (model, reason) = select_model(&req, &config);

        assert_eq!(model, "zenmux");
        assert_eq!(reason, "routePool");
    }

    #[test]
    fn subagent_tag_overrides() {
        let mut req = base_req();
        req.system = Some(serde_json::Value::String(
            "<CCR-SUBAGENT-MODEL>custom,model-x</CCR-SUBAGENT-MODEL> do stuff".into(),
        ));
        let (model, reason) = select_model(&req, &base_config("openai,gpt-4o"));
        assert_eq!(model, "custom,model-x");
        assert_eq!(reason, "subagent");
    }

    #[test]
    fn find_provider_by_name() {
        use ccr_types::Provider;
        let mut config = base_config("p1,model-a");
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

    #[tokio::test]
    async fn select_model_async_uses_api_tokenizer_for_long_context() {
        use ccr_types::{Message, TokenizerBackend};
        let mut req = base_req();
        req.messages = vec![Message {
            role: "user".into(),
            content: serde_json::json!("long context via async api fallback"),
        }];
        let mut config = base_config("openai,gpt-4o");
        config.router.long_context = Some("anthropic,claude-long".into());
        config.router.long_context_threshold = Some(0);
        config.router.tokenizer_backend = TokenizerBackend::Api {
            endpoint: "http://127.0.0.1:9/tokenize".into(),
        };

        let (model, reason) = select_model_async(&req, &config).await;

        assert_eq!(model, "anthropic,claude-long");
        assert_eq!(reason, "longContext");
    }

    #[test]
    fn merge_router_overrides_scenario_routes() {
        use ccr_types::RouterConfig;
        let base = RouterConfig {
            background: Some("openai,gpt-4o".into()),
            ..Default::default()
        };
        let over = RouterConfig {
            background: Some("groq,llama3".into()),
            ..Default::default()
        };
        let merged = merge_router(&base, &over);
        assert_eq!(merged.background.as_deref(), Some("groq,llama3"));
    }

    #[test]
    fn merge_router_keeps_base_when_override_absent() {
        use ccr_types::RouterConfig;
        let base = RouterConfig {
            background: Some("mini".into()),
            ..Default::default()
        };
        let over = RouterConfig::default();
        let merged = merge_router(&base, &over);
        assert_eq!(merged.background, Some("mini".into()));
    }

    #[test]
    fn load_custom_router_missing_file_returns_none() {
        assert!(load_custom_router("/nonexistent/path/router.json").is_none());
    }

    #[test]
    fn load_project_router_missing_returns_none() {
        assert!(load_project_router("nonexistent-project-id-xyz").is_none());
    }
}
