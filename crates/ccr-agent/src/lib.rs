pub mod cache;

use async_trait::async_trait;
use ccr_types::{Config, MessagesRequest};

pub use cache::{CacheStats, ImageCache};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[async_trait]
pub trait Agent: Send + Sync {
    fn name(&self) -> &str;

    /// Returns a model override if this agent should handle the request.
    fn detect(&self, req: &MessagesRequest, config: &Config) -> Option<String>;

    /// Check if this agent has a specific tool
    fn has_tool(&self, _tool_name: &str) -> bool {
        false
    }

    /// Get all tools provided by this agent
    fn get_tools(&self) -> Vec<ToolDefinition> {
        Vec::new()
    }

    /// Modify the request before sending (e.g., inject system prompts, replace images)
    fn modify_request(&self, _req: &mut MessagesRequest, _config: &Config) {}

    /// Execute a tool and return the result
    async fn execute_tool(
        &self,
        _tool_name: &str,
        _args: serde_json::Value,
        _req: &MessagesRequest,
        _config: &Config,
    ) -> Option<String> {
        None
    }
}

/// Detects image content blocks and routes to Router.image model.
pub struct ImageAgent;

#[async_trait]
impl Agent for ImageAgent {
    fn name(&self) -> &str {
        "image"
    }

    fn detect(&self, req: &MessagesRequest, config: &Config) -> Option<String> {
        // If already routed to image model, don't handle
        if let Some(image_model) = &config.router.image {
            if req.model.contains(image_model) {
                return None;
            }
        }

        let image_model = config.router.image.as_ref()?;
        let has_image = req.messages.iter().any(|msg| {
            if let Some(arr) = msg.content.as_array() {
                arr.iter()
                    .any(|block| block.get("type").and_then(|t| t.as_str()) == Some("image"))
            } else {
                false
            }
        });
        if has_image {
            Some(image_model.clone())
        } else {
            None
        }
    }

    fn has_tool(&self, tool_name: &str) -> bool {
        tool_name == "analyzeImage"
    }

    fn get_tools(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            name: "analyzeImage".to_string(),
            description: "Analyze image or images by ID and extract information such as OCR text, objects, layout, colors, or safety signals.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "imageId": {
                        "type": "array",
                        "description": "an array of IDs to analyze",
                        "items": {"type": "string"}
                    },
                    "task": {
                        "type": "string",
                        "description": "Details of task to perform on the image. The more detailed, the better"
                    }
                },
                "required": ["imageId", "task"]
            }),
        }]
    }

    fn modify_request(&self, req: &mut MessagesRequest, _config: &Config) {
        // Inject system prompt instructing to use analyzeImage tool
        let system_prompt = serde_json::json!({
            "type": "text",
            "text": "You are a text-only language model and do not possess visual perception. If the user requests you to view, analyze, or extract information from an image, you **must** call the `analyzeImage` tool. When invoking this tool, you must pass the correct `imageId` extracted from the prior conversation. Image identifiers are always provided in the format `[Image #imageId]`. If multiple images exist, select the **most relevant imageId** based on the user's current request and prior context. Do not attempt to describe or analyze the image directly yourself."
        });

        if let Some(sys) = req.system.as_mut() {
            if let Some(arr) = sys.as_array_mut() {
                arr.push(system_prompt);
            }
        } else {
            req.system = Some(serde_json::json!([system_prompt]));
        }

        // Replace image blocks with text placeholders
        let mut image_id = 1;
        for msg in &mut req.messages {
            if msg.role != "user" {
                continue;
            }

            if let Some(arr) = msg.content.as_array_mut() {
                for block in arr.iter_mut() {
                    if block.get("type").and_then(|t| t.as_str()) == Some("image") {
                        *block = serde_json::json!({
                            "type": "text",
                            "text": format!("[Image #{}] This is an image, if you need to view or analyze it, you need to extract the imageId", image_id)
                        });
                        image_id += 1;
                    }
                }
            }
        }

        // Add tools
        let tools = self.get_tools();
        let tools_json: Vec<_> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.input_schema
                })
            })
            .collect();

        if let Some(existing) = req.tools.as_mut() {
            existing.splice(0..0, tools_json);
        } else {
            req.tools = Some(tools_json);
        }
    }

    async fn execute_tool(
        &self,
        tool_name: &str,
        args: serde_json::Value,
        _req: &MessagesRequest,
        config: &Config,
    ) -> Option<String> {
        if tool_name != "analyzeImage" {
            return None;
        }

        // Extract task from args
        let task = args.get("task")?.as_str()?;

        // Build analysis request
        let image_model = config.router.image.as_ref()?;
        let port = config.port.unwrap_or(3456);
        let api_key = config.api_key.as_deref().unwrap_or("");

        let analysis_req = serde_json::json!({
            "model": image_model,
            "system": [{
                "type": "text",
                "text": "You must interpret and analyze images strictly according to the assigned task. When an image placeholder is provided, your role is to parse the image content only within the scope of the user's instructions. Do not ignore or deviate from the task. Always ensure that your response reflects a clear, accurate interpretation of the image aligned with the given objective."
            }],
            "messages": [{
                "role": "user",
                "content": [{"type": "text", "text": task}]
            }],
            "max_tokens": 1024,
            "stream": false
        });

        // Call the analysis endpoint
        let client = reqwest::Client::new();
        let response = client
            .post(format!("http://127.0.0.1:{}/v1/messages", port))
            .header("x-api-key", api_key)
            .header("content-type", "application/json")
            .json(&analysis_req)
            .send()
            .await
            .ok()?;

        let result: serde_json::Value = response.json().await.ok()?;
        let content = result.get("content")?.as_array()?;
        let text = content.first()?.get("text")?.as_str()?;

        Some(text.to_string())
    }
}

/// Run all agents; return the first model override found.
pub fn run_agents(
    agents: &[Box<dyn Agent>],
    req: &MessagesRequest,
    config: &Config,
) -> Option<String> {
    agents.iter().find_map(|a| a.detect(req, config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccr_types::{Config, Message, MessagesRequest, RouterConfig};
    use serde_json::json;

    fn base_req(content: serde_json::Value) -> MessagesRequest {
        MessagesRequest {
            model: "claude-3-5-sonnet".into(),
            messages: vec![Message {
                role: "user".into(),
                content,
            }],
            system: None,
            tools: None,
            max_tokens: Some(100),
            stream: false,
            thinking: None,
        }
    }

    fn config_with_image(model: &str) -> Config {
        Config {
            router: RouterConfig {
                image: Some(model.into()),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn detects_image_block() {
        let agent = ImageAgent;
        let req =
            base_req(json!([{"type": "image", "source": {}}, {"type": "text", "text": "what?"}]));
        let config = config_with_image("openai,gpt-4o");
        assert_eq!(agent.detect(&req, &config), Some("openai,gpt-4o".into()));
    }

    #[test]
    fn no_image_returns_none() {
        let agent = ImageAgent;
        let req = base_req(json!("just text"));
        let config = config_with_image("openai,gpt-4o");
        assert!(agent.detect(&req, &config).is_none());
    }

    #[test]
    fn no_image_route_configured_returns_none() {
        let agent = ImageAgent;
        let req = base_req(json!([{"type": "image", "source": {}}]));
        let config = Config::default();
        assert!(agent.detect(&req, &config).is_none());
    }

    #[test]
    fn run_agents_returns_first_match() {
        let agents: Vec<Box<dyn Agent>> = vec![Box::new(ImageAgent)];
        let req = base_req(json!([{"type": "image", "source": {}}]));
        let config = config_with_image("openai,gpt-4o");
        assert_eq!(
            run_agents(&agents, &req, &config),
            Some("openai,gpt-4o".into())
        );
    }
}
