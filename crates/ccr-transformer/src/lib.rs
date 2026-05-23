pub mod transformers;

use serde_json::Value;
use std::collections::HashMap;

pub trait Transformer: Send + Sync {
    fn name(&self) -> &str;
    fn transform_request(&self, req: Value) -> Value;
    fn transform_response(&self, res: Value) -> Value;
}

pub struct TransformerRegistry(HashMap<String, Box<dyn Transformer>>);

#[allow(clippy::new_without_default)]
impl TransformerRegistry {
    pub fn new() -> Self {
        let mut map: HashMap<String, Box<dyn Transformer>> = HashMap::new();
        map.insert(
            "anthropic".into(),
            Box::new(transformers::AnthropicTransformer),
        );
        map.insert("openai".into(), Box::new(transformers::OpenAITransformer));
        map.insert(
            "deepseek".into(),
            Box::new(transformers::DeepSeekTransformer),
        );
        map.insert(
            "cleancache".into(),
            Box::new(transformers::CleanCacheTransformer),
        );
        map.insert("gemini".into(), Box::new(transformers::GeminiTransformer));
        map.insert("tooluse".into(), Box::new(transformers::ToolUseTransformer));
        map.insert(
            "reasoning".into(),
            Box::new(transformers::ReasoningTransformer),
        );
        map.insert(
            "forcereasoning".into(),
            Box::new(transformers::ForceReasoningTransformer),
        );
        map.insert(
            "streamoptions".into(),
            Box::new(transformers::StreamOptionsTransformer),
        );
        map.insert(
            "openrouter".into(),
            Box::new(transformers::OpenRouterTransformer),
        );
        map.insert("groq".into(), Box::new(transformers::GroqTransformer));

        // Phase 30: New transformers
        map.insert(
            "enhancetool".into(),
            Box::new(transformers::EnhanceToolTransformer),
        );
        map.insert(
            "vertex-claude".into(),
            Box::new(transformers::VertexClaudeTransformer),
        );
        map.insert(
            "vertex-gemini".into(),
            Box::new(transformers::VertexGeminiTransformer),
        );
        map.insert(
            "cerebras".into(),
            Box::new(transformers::CerebrasTransformer),
        );
        map.insert("vercel".into(), Box::new(transformers::VercelTransformer));

        // Parameterized transformers with default values
        map.insert(
            "maxtoken".into(),
            Box::new(transformers::MaxTokenTransformer(4096)),
        );
        map.insert(
            "maxcompletiontokens".into(),
            Box::new(transformers::MaxCompletionTokensTransformer(4096)),
        );

        // Note: customparams and sampling require configuration and should be added dynamically

        Self(map)
    }

    pub fn names(&self) -> Vec<&str> {
        self.0.keys().map(|s| s.as_str()).collect()
    }

    pub fn apply_chain(&self, names: &[&str], mut req: Value) -> Value {
        for name in names {
            if let Some(t) = self.0.get(*name) {
                req = t.transform_request(req);
            }
        }
        req
    }
}

impl Default for TransformerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn anthropic_passthrough() {
        let reg = TransformerRegistry::new();
        let req =
            json!({"model": "claude-3-5-sonnet", "messages": [{"role": "user", "content": "hi"}]});
        let out = reg.apply_chain(&["anthropic"], req.clone());
        assert_eq!(out, req);
    }

    #[test]
    fn maxtoken_clamps() {
        use crate::transformers::MaxTokenTransformer;
        let t = MaxTokenTransformer(4096);
        let req = json!({"max_tokens": 8000});
        let out = t.transform_request(req);
        assert_eq!(out["max_tokens"], 4096);
    }

    #[test]
    fn maxtoken_keeps_under_limit() {
        use crate::transformers::MaxTokenTransformer;
        let t = MaxTokenTransformer(4096);
        let req = json!({"max_tokens": 1000});
        let out = t.transform_request(req);
        assert_eq!(out["max_tokens"], 1000);
    }

    #[test]
    fn openai_moves_system_to_messages() {
        let reg = TransformerRegistry::new();
        let req = json!({
            "model": "gpt-4o",
            "system": "You are helpful.",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 100
        });
        let out = reg.apply_chain(&["openai"], req);
        assert!(out.get("system").is_none());
        let msgs = out["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "You are helpful.");
        assert_eq!(msgs[1]["role"], "user");
    }

    #[test]
    fn openai_removes_thinking_field() {
        let reg = TransformerRegistry::new();
        let req = json!({
            "model": "gpt-4o",
            "messages": [],
            "thinking": {"type": "enabled"},
            "max_tokens": 100
        });
        let out = reg.apply_chain(&["openai"], req);
        assert!(out.get("thinking").is_none());
    }

    #[test]
    fn deepseek_clamps_max_tokens() {
        let reg = TransformerRegistry::new();
        let req = json!({"model": "deepseek-chat", "messages": [], "max_tokens": 16000});
        let out = reg.apply_chain(&["deepseek"], req);
        assert_eq!(out["max_tokens"], 8192);
    }

    #[test]
    fn deepseek_keeps_max_tokens_under_limit() {
        let reg = TransformerRegistry::new();
        let req = json!({"model": "deepseek-chat", "messages": [], "max_tokens": 4096});
        let out = reg.apply_chain(&["deepseek"], req);
        assert_eq!(out["max_tokens"], 4096);
    }

    #[test]
    fn cleancache_removes_cache_control() {
        let reg = TransformerRegistry::new();
        let req = json!({
            "messages": [{"role": "user", "content": "hi", "cache_control": {"type": "ephemeral"}}]
        });
        let out = reg.apply_chain(&["cleancache"], req);
        assert!(out["messages"][0].get("cache_control").is_none());
    }

    #[test]
    fn chain_applies_multiple() {
        let reg = TransformerRegistry::new();
        let req = json!({
            "model": "deepseek-chat",
            "system": "Be concise.",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 20000,
            "thinking": {"type": "enabled"}
        });
        let out = reg.apply_chain(&["openai", "deepseek"], req);
        assert!(out.get("system").is_none());
        assert!(out.get("thinking").is_none());
        assert_eq!(out["max_tokens"], 8192);
        assert_eq!(out["messages"][0]["role"], "system");
    }

    #[test]
    fn enhancetool_adds_missing_id() {
        use crate::transformers::EnhanceToolTransformer;
        let t = EnhanceToolTransformer;
        let res = json!({
            "content": [
                {
                    "type": "tool_use",
                    "name": "search",
                    "input": {"query": "test"}
                }
            ]
        });
        let out = t.transform_response(res);
        assert!(out["content"][0].get("id").is_some());
        let id = out["content"][0]["id"].as_str().unwrap();
        assert!(id.starts_with("toolu_"));
    }

    #[test]
    fn enhancetool_adds_missing_name() {
        use crate::transformers::EnhanceToolTransformer;
        let t = EnhanceToolTransformer;
        let res = json!({
            "content": [
                {
                    "type": "tool_use",
                    "id": "toolu_123",
                    "input": {"query": "test"}
                }
            ]
        });
        let out = t.transform_response(res);
        assert_eq!(out["content"][0]["name"], "unknown");
    }

    #[test]
    fn enhancetool_fixes_null_input() {
        use crate::transformers::EnhanceToolTransformer;
        let t = EnhanceToolTransformer;
        let res = json!({
            "content": [
                {
                    "type": "tool_use",
                    "id": "toolu_123",
                    "name": "search",
                    "input": null
                }
            ]
        });
        let out = t.transform_response(res);
        assert_eq!(out["content"][0]["input"], json!({}));
    }

    #[test]
    fn maxcompletiontokens_clamps() {
        use crate::transformers::MaxCompletionTokensTransformer;
        let t = MaxCompletionTokensTransformer(4096);
        let req = json!({"max_completion_tokens": 8000});
        let out = t.transform_request(req);
        assert_eq!(out["max_completion_tokens"], 4096);
    }

    #[test]
    fn maxcompletiontokens_keeps_under_limit() {
        use crate::transformers::MaxCompletionTokensTransformer;
        let t = MaxCompletionTokensTransformer(4096);
        let req = json!({"max_completion_tokens": 2000});
        let out = t.transform_request(req);
        assert_eq!(out["max_completion_tokens"], 2000);
    }

    #[test]
    fn vertex_claude_transforms_request() {
        use crate::transformers::VertexClaudeTransformer;
        let t = VertexClaudeTransformer;
        let req = json!({
            "model": "claude-3-opus",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 100
        });
        let out = t.transform_request(req);
        // Vertex Claude should preserve the basic structure
        assert_eq!(out["model"], "claude-3-opus");
        assert_eq!(out["messages"][0]["role"], "user");
    }

    #[test]
    fn vertex_gemini_removes_unsupported_fields() {
        use crate::transformers::VertexGeminiTransformer;
        let t = VertexGeminiTransformer;
        let req = json!({
            "model": "gemini-2.0-flash",
            "messages": [{"role": "user", "content": "hi"}],
            "thinking": {"type": "enabled"},
            "stream_options": {"include_usage": true}
        });
        let out = t.transform_request(req);
        assert!(out.get("thinking").is_none());
        assert!(out.get("stream_options").is_none());
    }

    #[test]
    fn cerebras_removes_unsupported_fields() {
        use crate::transformers::CerebrasTransformer;
        let t = CerebrasTransformer;
        let req = json!({
            "model": "llama-3.3-70b",
            "messages": [{"role": "user", "content": "hi"}],
            "thinking": {"type": "enabled"},
            "stream_options": {"include_usage": true},
            "tools": []
        });
        let out = t.transform_request(req);
        assert!(out.get("thinking").is_none());
        assert!(out.get("stream_options").is_none());
        assert!(out.get("tools").is_none());
    }

    #[test]
    fn vercel_removes_unsupported_fields() {
        use crate::transformers::VercelTransformer;
        let t = VercelTransformer;
        let req = json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "hi"}],
            "thinking": {"type": "enabled"}
        });
        let out = t.transform_request(req);
        assert!(out.get("thinking").is_none());
    }

    #[test]
    fn enhancetool_passthrough_non_tool_content() {
        use crate::transformers::EnhanceToolTransformer;
        let t = EnhanceToolTransformer;
        let res = json!({
            "content": [
                {
                    "type": "text",
                    "text": "Hello"
                }
            ]
        });
        let out = t.transform_response(res.clone());
        assert_eq!(out, res);
    }

    #[test]
    fn gemini_converts_system_and_messages() {
        let reg = TransformerRegistry::new();
        let req = json!({
            "model": "gemini-2.5-pro",
            "system": "Be helpful.",
            "messages": [
                {"role": "user", "content": "hello"},
                {"role": "assistant", "content": "hi there"}
            ],
            "max_tokens": 1000
        });
        let out = reg.apply_chain(&["gemini"], req);
        assert!(out.get("system").is_none());
        assert!(out.get("max_tokens").is_none());
        assert!(out.get("messages").is_none());
        assert_eq!(out["systemInstruction"]["parts"][0]["text"], "Be helpful.");
        let contents = out["contents"].as_array().unwrap();
        assert_eq!(contents[0]["role"], "user");
        assert_eq!(contents[0]["parts"][0]["text"], "hello");
        assert_eq!(contents[1]["role"], "model");
    }

    #[test]
    fn gemini_removes_thinking() {
        let reg = TransformerRegistry::new();
        let req = json!({
            "model": "gemini-2.5-pro",
            "messages": [],
            "thinking": {"type": "enabled"}
        });
        let out = reg.apply_chain(&["gemini"], req);
        assert!(out.get("thinking").is_none());
    }

    #[test]
    fn openai_array_system_to_messages() {
        let reg = TransformerRegistry::new();
        let req = json!({
            "model": "gpt-4o",
            "system": [{"type": "text", "text": "Be concise."}],
            "messages": [{"role": "user", "content": "hi"}]
        });
        let out = reg.apply_chain(&["openai"], req);
        assert!(out.get("system").is_none());
        let msgs = out["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["content"], "Be concise.");
    }

    #[test]
    fn openai_content_blocks_to_string() {
        let reg = TransformerRegistry::new();
        let req = json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": [{"type": "text", "text": "hello"}]}]
        });
        let out = reg.apply_chain(&["openai"], req);
        assert_eq!(out["messages"][0]["content"], "hello");
    }

    #[test]
    fn gemini_array_system() {
        let reg = TransformerRegistry::new();
        let req = json!({
            "model": "gemini-2.5-pro",
            "system": [{"type": "text", "text": "Be brief."}],
            "messages": []
        });
        let out = reg.apply_chain(&["gemini"], req);
        assert_eq!(out["systemInstruction"]["parts"][0]["text"], "Be brief.");
    }

    #[test]
    fn tooluse_converts_tools() {
        let reg = TransformerRegistry::new();
        let req = json!({
            "tools": [{"name": "search", "description": "Search", "input_schema": {"type": "object"}}],
            "messages": []
        });
        let out = reg.apply_chain(&["tooluse"], req);
        assert_eq!(out["tools"][0]["type"], "function");
        assert_eq!(out["tools"][0]["function"]["name"], "search");
    }

    #[test]
    fn reasoning_strips_thinking() {
        let reg = TransformerRegistry::new();
        let req = json!({"messages": [], "thinking": {"type": "enabled"}});
        let out = reg.apply_chain(&["reasoning"], req);
        assert!(out.get("thinking").is_none());
    }

    #[test]
    fn forcereasoning_injects_thinking() {
        let reg = TransformerRegistry::new();
        let req = json!({"messages": []});
        let out = reg.apply_chain(&["forcereasoning"], req);
        assert_eq!(out["thinking"]["type"], "enabled");
    }

    #[test]
    fn streamoptions_adds_usage_when_streaming() {
        let reg = TransformerRegistry::new();
        let req = json!({"messages": [], "stream": true});
        let out = reg.apply_chain(&["streamoptions"], req);
        assert_eq!(out["stream_options"]["include_usage"], true);
    }

    #[test]
    fn groq_removes_unsupported_fields() {
        let reg = TransformerRegistry::new();
        let req = json!({"messages": [], "thinking": {"type": "enabled"}, "stream_options": {}});
        let out = reg.apply_chain(&["groq"], req);
        assert!(out.get("thinking").is_none());
        assert!(out.get("stream_options").is_none());
    }
}
