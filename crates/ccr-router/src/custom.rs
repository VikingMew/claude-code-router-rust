use anyhow::{Context, Result};
use ccr_types::MessagesRequest;
use rquickjs::{CatchResultExt, Ctx, Function, Runtime};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Routing context passed to custom router JavaScript function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingContext {
    pub tokens: u64,
    pub model: String,
    pub scenario: String,
    #[serde(rename = "projectId")]
    pub project_id: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl RoutingContext {
    pub fn new(req: &MessagesRequest, tokens: u64, scenario: &str) -> Self {
        Self {
            tokens,
            model: req.model.clone(),
            scenario: scenario.to_string(),
            project_id: extract_project_id(req),
            metadata: HashMap::new(),
        }
    }
}

/// Custom JavaScript router
pub struct CustomRouter {
    _runtime: Runtime, // Keep runtime alive
    context: rquickjs::Context,
    source_path: String,
}

// Safety: QuickJS runtime is not Send/Sync, but we're using it in a single-threaded context
unsafe impl Send for CustomRouter {}
unsafe impl Sync for CustomRouter {}

impl CustomRouter {
    /// Load custom router from JavaScript file
    pub fn load(path: &str) -> Result<Self> {
        let source = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read custom router file: {}", path))?;

        let runtime = Runtime::new().context("Failed to create QuickJS runtime")?;
        let context =
            rquickjs::Context::full(&runtime).context("Failed to create QuickJS context")?;

        // Execute the JavaScript source to define the routeRequest function
        context.with(|ctx| {
            ctx.eval::<(), _>(source.as_bytes())
                .catch(&ctx)
                .map_err(|e| {
                    anyhow::anyhow!("Failed to evaluate custom router JavaScript: {}", e)
                })?;
            Ok::<(), anyhow::Error>(())
        })?;

        log::info!("Loaded custom router from: {}", path);

        Ok(Self {
            _runtime: runtime,
            context,
            source_path: path.to_string(),
        })
    }

    /// Execute routing function with given context
    pub fn route(&self, ctx: &RoutingContext) -> Result<String> {
        self.context.with(|js_ctx: Ctx| {
            // Get the routeRequest function
            let global = js_ctx.globals();
            let route_fn: Function = global.get("routeRequest").catch(&js_ctx).map_err(|e| {
                anyhow::anyhow!("routeRequest function not found in custom router: {}", e)
            })?;

            // Convert Rust context to JS value
            let ctx_json = serde_json::to_string(ctx)?;
            let ctx_value = js_ctx.json_parse(ctx_json)?;

            // Call the function
            let result: rquickjs::Value = route_fn
                .call((ctx_value,))
                .catch(&js_ctx)
                .map_err(|e| anyhow::anyhow!("Failed to execute routeRequest function: {}", e))?;

            // Handle null return (fallback to default routing)
            if result.is_null() || result.is_undefined() {
                anyhow::bail!("Custom router returned null/undefined, fallback to default routing");
            }

            // Convert result to string
            let route_str: String = result
                .as_string()
                .context("routeRequest must return a string")?
                .to_string()?;

            log::debug!("Custom router returned: {}", route_str);

            Ok(route_str)
        })
    }

    pub fn source_path(&self) -> &str {
        &self.source_path
    }
}

/// Extract project ID from request
fn extract_project_id(_req: &MessagesRequest) -> Option<String> {
    // Method 1: From environment variable
    if let Ok(project_id) = std::env::var("CLAUDE_PROJECT_ID") {
        return Some(project_id);
    }

    // Method 2: From current working directory .claude/project.json
    if let Ok(content) = std::fs::read_to_string(".claude/project.json") {
        if let Ok(project) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(id) = project["id"].as_str() {
                return Some(id.to_string());
            }
        }
    }

    // Method 3: From request metadata (if we add this field in the future)
    // This would require extending MessagesRequest

    None
}

/// Load custom router from environment variable or default path
pub fn load_custom_router_from_env() -> Option<CustomRouter> {
    // Try CUSTOM_ROUTER_PATH environment variable
    if let Ok(path) = std::env::var("CUSTOM_ROUTER_PATH") {
        match CustomRouter::load(&path) {
            Ok(router) => return Some(router),
            Err(e) => {
                log::warn!("Failed to load custom router from {}: {}", path, e);
            }
        }
    }

    // Try default path: ~/.claude-code-router/router.js
    if let Some(home) = dirs_next::home_dir() {
        let default_path = home.join(".claude-code-router").join("router.js");
        if default_path.exists() {
            match CustomRouter::load(default_path.to_str()?) {
                Ok(router) => return Some(router),
                Err(e) => {
                    log::warn!("Failed to load custom router from default path: {}", e);
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccr_types::MessagesRequest;

    #[test]
    fn test_routing_context_creation() {
        let req = MessagesRequest {
            model: "claude-opus-4".to_string(),
            messages: vec![],
            system: None,
            tools: None,
            max_tokens: Some(100),
            stream: false,
            thinking: None,
        };

        let ctx = RoutingContext::new(&req, 1000, "default");
        assert_eq!(ctx.model, "claude-opus-4");
        assert_eq!(ctx.tokens, 1000);
        assert_eq!(ctx.scenario, "default");
    }

    #[test]
    fn test_custom_router_simple() {
        let js_code = r#"
            function routeRequest(context) {
                if (context.tokens > 10000) {
                    return "anthropic,claude-opus-4";
                }
                return "openai,gpt-4o";
            }
        "#;

        // Write to temp file
        let temp_file = std::env::temp_dir().join("test-router.js");
        std::fs::write(&temp_file, js_code).unwrap();

        let router = CustomRouter::load(temp_file.to_str().unwrap()).unwrap();

        // Test with low tokens
        let ctx1 = RoutingContext {
            tokens: 1000,
            model: "test".to_string(),
            scenario: "default".to_string(),
            project_id: None,
            metadata: HashMap::new(),
        };
        assert_eq!(router.route(&ctx1).unwrap(), "openai,gpt-4o");

        // Test with high tokens
        let ctx2 = RoutingContext {
            tokens: 15000,
            model: "test".to_string(),
            scenario: "default".to_string(),
            project_id: None,
            metadata: HashMap::new(),
        };
        assert_eq!(router.route(&ctx2).unwrap(), "anthropic,claude-opus-4");

        // Cleanup
        std::fs::remove_file(&temp_file).ok();
    }

    #[test]
    fn test_custom_router_with_scenario() {
        let js_code = r#"
            function routeRequest(context) {
                if (context.scenario === "think") {
                    return "deepseek,deepseek-chat";
                }
                if (context.scenario === "background") {
                    return "openai,gpt-4o-mini";
                }
                return "anthropic,claude-sonnet-4";
            }
        "#;

        let temp_file = std::env::temp_dir().join("test-router-scenario.js");
        std::fs::write(&temp_file, js_code).unwrap();

        let router = CustomRouter::load(temp_file.to_str().unwrap()).unwrap();

        let ctx_think = RoutingContext {
            tokens: 1000,
            model: "test".to_string(),
            scenario: "think".to_string(),
            project_id: None,
            metadata: HashMap::new(),
        };
        assert_eq!(router.route(&ctx_think).unwrap(), "deepseek,deepseek-chat");

        let ctx_bg = RoutingContext {
            tokens: 1000,
            model: "test".to_string(),
            scenario: "background".to_string(),
            project_id: None,
            metadata: HashMap::new(),
        };
        assert_eq!(router.route(&ctx_bg).unwrap(), "openai,gpt-4o-mini");

        std::fs::remove_file(&temp_file).ok();
    }

    #[test]
    fn test_custom_router_returns_null() {
        let js_code = r#"
            function routeRequest(context) {
                return null;
            }
        "#;

        let temp_file = std::env::temp_dir().join("test-router-null.js");
        std::fs::write(&temp_file, js_code).unwrap();

        let router = CustomRouter::load(temp_file.to_str().unwrap()).unwrap();

        let ctx = RoutingContext {
            tokens: 1000,
            model: "test".to_string(),
            scenario: "default".to_string(),
            project_id: None,
            metadata: HashMap::new(),
        };

        // Should error when returning null
        assert!(router.route(&ctx).is_err());

        std::fs::remove_file(&temp_file).ok();
    }
}
