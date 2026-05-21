use crate::Transformer;
use serde_json::{Value, json};

pub struct AnthropicTransformer;
impl Transformer for AnthropicTransformer {
    fn name(&self) -> &str {
        "anthropic"
    }
    fn transform_request(&self, req: Value) -> Value {
        req
    }
    fn transform_response(&self, res: Value) -> Value {
        res
    }
}

pub struct OpenAITransformer;
impl Transformer for OpenAITransformer {
    fn name(&self) -> &str {
        "openai"
    }

    fn transform_request(&self, req: Value) -> Value {
        let mut obj = match req {
            Value::Object(m) => m,
            other => return other,
        };

        let system_msg: Option<Value> = obj.remove("system").and_then(|s| {
            let text = match &s {
                Value::String(t) => Some(t.clone()),
                Value::Array(arr) => arr.iter().find_map(|b| {
                    if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                        b.get("text")
                            .and_then(|t| t.as_str())
                            .map(|t| t.to_string())
                    } else {
                        None
                    }
                }),
                _ => None,
            };
            text.map(|t| json!({"role": "system", "content": t}))
        });

        if let Some(Value::Array(msgs)) = obj.get_mut("messages") {
            for msg in msgs.iter_mut() {
                if let Some(Value::Array(blocks)) = msg.get("content") {
                    let text: Option<String> = if blocks.len() == 1 {
                        blocks[0]
                            .get("text")
                            .and_then(|t| t.as_str())
                            .map(|t| t.to_string())
                    } else {
                        None
                    };
                    if let Some(t) = text
                        && let Some(m) = msg.as_object_mut()
                    {
                        m.insert("content".into(), Value::String(t));
                    }
                }
            }
        }

        if let Some(sys) = system_msg
            && let Some(Value::Array(msgs)) = obj.get_mut("messages")
        {
            msgs.insert(0, sys);
        }

        obj.remove("thinking");
        Value::Object(obj)
    }

    fn transform_response(&self, res: Value) -> Value {
        res
    }
}

pub struct DeepSeekTransformer;
impl Transformer for DeepSeekTransformer {
    fn name(&self) -> &str {
        "deepseek"
    }

    fn transform_request(&self, mut req: Value) -> Value {
        if let Some(mt) = req.get("max_tokens").and_then(|v| v.as_u64())
            && mt > 8192
        {
            req["max_tokens"] = json!(8192u64);
        }
        req
    }

    fn transform_response(&self, res: Value) -> Value {
        res
    }
}

pub struct MaxTokenTransformer(pub u64);
impl Transformer for MaxTokenTransformer {
    fn name(&self) -> &str {
        "maxtoken"
    }
    fn transform_request(&self, mut req: Value) -> Value {
        if let Some(mt) = req.get("max_tokens").and_then(|v| v.as_u64())
            && mt > self.0
        {
            req["max_tokens"] = json!(self.0);
        }
        req
    }
    fn transform_response(&self, res: Value) -> Value {
        res
    }
}

pub struct GeminiTransformer;
impl Transformer for GeminiTransformer {
    fn name(&self) -> &str {
        "gemini"
    }

    fn transform_request(&self, req: Value) -> Value {
        let mut obj = match req {
            Value::Object(m) => m,
            other => return other,
        };

        if let Some(system) = obj.remove("system") {
            let text = match &system {
                Value::String(t) => Some(t.clone()),
                Value::Array(arr) => arr.iter().find_map(|b| {
                    if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                        b.get("text")
                            .and_then(|t| t.as_str())
                            .map(|s| s.to_string())
                    } else {
                        None
                    }
                }),
                _ => None,
            };
            if let Some(t) = text {
                obj.insert("systemInstruction".into(), json!({"parts": [{"text": t}]}));
            }
        }

        if let Some(Value::Array(arr)) = obj.remove("messages") {
            let contents: Vec<Value> = arr
                .into_iter()
                .map(|mut m| {
                    if m.get("role").and_then(|r| r.as_str()) == Some("assistant")
                        && let Some(o) = m.as_object_mut()
                    {
                        o.insert("role".into(), json!("model"));
                    }
                    if let Some(o) = m.as_object_mut()
                        && let Some(c) = o.remove("content")
                    {
                        let parts = match c {
                            Value::String(t) => json!([{"text": t}]),
                            other => other,
                        };
                        o.insert("parts".into(), parts);
                    }
                    m
                })
                .collect();
            obj.insert("contents".into(), Value::Array(contents));
        }

        obj.remove("thinking");
        obj.remove("max_tokens");
        Value::Object(obj)
    }

    fn transform_response(&self, res: Value) -> Value {
        res
    }
}

pub struct CleanCacheTransformer;
impl Transformer for CleanCacheTransformer {
    fn name(&self) -> &str {
        "cleancache"
    }
    fn transform_request(&self, mut req: Value) -> Value {
        remove_cache_control(&mut req);
        req
    }
    fn transform_response(&self, res: Value) -> Value {
        res
    }
}

// Convert Anthropic tool_use blocks to OpenAI function_call format
pub struct ToolUseTransformer;
impl Transformer for ToolUseTransformer {
    fn name(&self) -> &str {
        "tooluse"
    }

    fn transform_request(&self, mut req: Value) -> Value {
        // Convert tools from Anthropic format to OpenAI functions format
        if let Some(Value::Array(arr)) = req.get("tools").cloned() {
            let functions: Vec<Value> = arr
                .into_iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.get("name").cloned().unwrap_or(Value::Null),
                            "description": t.get("description").cloned().unwrap_or(Value::Null),
                            "parameters": t.get("input_schema").cloned().unwrap_or(json!({}))
                        }
                    })
                })
                .collect();
            req["tools"] = Value::Array(functions);
        }
        req
    }

    fn transform_response(&self, res: Value) -> Value {
        res
    }
}

// Extract thinking blocks and prepend as assistant message
pub struct ReasoningTransformer;
impl Transformer for ReasoningTransformer {
    fn name(&self) -> &str {
        "reasoning"
    }

    fn transform_request(&self, mut req: Value) -> Value {
        // Strip thinking from request if present (provider doesn't support it)
        if let Some(obj) = req.as_object_mut() {
            obj.remove("thinking");
        }
        req
    }

    fn transform_response(&self, res: Value) -> Value {
        res
    }
}

// Force enable thinking/reasoning
pub struct ForceReasoningTransformer;
impl Transformer for ForceReasoningTransformer {
    fn name(&self) -> &str {
        "forcereasoning"
    }

    fn transform_request(&self, mut req: Value) -> Value {
        if req.get("thinking").is_none() {
            req["thinking"] = json!({"type": "enabled", "budget_tokens": 8000});
        }
        req
    }

    fn transform_response(&self, res: Value) -> Value {
        res
    }
}

// Add stream_options for usage tracking
pub struct StreamOptionsTransformer;
impl Transformer for StreamOptionsTransformer {
    fn name(&self) -> &str {
        "streamoptions"
    }

    fn transform_request(&self, mut req: Value) -> Value {
        if req.get("stream").and_then(|v| v.as_bool()).unwrap_or(false) {
            req["stream_options"] = json!({"include_usage": true});
        }
        req
    }

    fn transform_response(&self, res: Value) -> Value {
        res
    }
}

// OpenRouter-specific fields
pub struct OpenRouterTransformer;
impl Transformer for OpenRouterTransformer {
    fn name(&self) -> &str {
        "openrouter"
    }

    fn transform_request(&self, mut req: Value) -> Value {
        // OpenRouter uses same format as OpenAI but needs system moved to messages
        let openai = OpenAITransformer;
        req = openai.transform_request(req);
        req
    }

    fn transform_response(&self, res: Value) -> Value {
        res
    }
}

// Groq: remove unsupported fields
pub struct GroqTransformer;
impl Transformer for GroqTransformer {
    fn name(&self) -> &str {
        "groq"
    }

    fn transform_request(&self, req: Value) -> Value {
        let openai = OpenAITransformer;
        let mut req = openai.transform_request(req);
        if let Some(obj) = req.as_object_mut() {
            obj.remove("thinking");
            obj.remove("stream_options");
        }
        req
    }

    fn transform_response(&self, res: Value) -> Value {
        res
    }
}

fn remove_cache_control(v: &mut Value) {
    match v {
        Value::Object(map) => {
            map.remove("cache_control");
            for val in map.values_mut() {
                remove_cache_control(val);
            }
        }
        Value::Array(arr) => {
            for val in arr.iter_mut() {
                remove_cache_control(val);
            }
        }
        _ => {}
    }
}

// EnhanceTool: Error tolerance for tool calls
pub struct EnhanceToolTransformer;
impl Transformer for EnhanceToolTransformer {
    fn name(&self) -> &str {
        "enhancetool"
    }

    fn transform_request(&self, req: Value) -> Value {
        req
    }

    fn transform_response(&self, mut res: Value) -> Value {
        // Fix common tool call response errors
        if let Some(Value::Array(blocks)) = res.get_mut("content") {
            for block in blocks.iter_mut() {
                if let Value::Object(obj) = block {
                    // Fix tool_use blocks
                    if obj.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                        // Ensure id field exists
                        if obj.get("id").is_none()
                            || obj["id"].as_str().map(|s| s.is_empty()).unwrap_or(true)
                        {
                            obj.insert(
                                "id".into(),
                                json!(format!("toolu_{}", uuid::Uuid::new_v4())),
                            );
                        }

                        // Ensure name field exists
                        if obj.get("name").is_none() {
                            obj.insert("name".into(), json!("unknown"));
                        }

                        // Ensure input is valid JSON
                        if let Some(input) = obj.get_mut("input") {
                            if input.is_null() {
                                *input = json!({});
                            }
                        } else {
                            obj.insert("input".into(), json!({}));
                        }
                    }
                }
            }
        }
        res
    }
}

// MaxCompletionTokens: Clamp completion_tokens
pub struct MaxCompletionTokensTransformer(pub u64);
impl Transformer for MaxCompletionTokensTransformer {
    fn name(&self) -> &str {
        "maxcompletiontokens"
    }

    fn transform_request(&self, mut req: Value) -> Value {
        // Handle both max_completion_tokens and max_tokens fields
        if let Some(mct) = req.get("max_completion_tokens").and_then(|v| v.as_u64())
            && mct > self.0
        {
            req["max_completion_tokens"] = json!(self.0);
        }

        // Also check max_tokens as fallback
        if req.get("max_completion_tokens").is_none()
            && let Some(mt) = req.get("max_tokens").and_then(|v| v.as_u64())
            && mt > self.0
        {
            req["max_tokens"] = json!(self.0);
        }
        req
    }

    fn transform_response(&self, res: Value) -> Value {
        res
    }
}

// CustomParams: Inject custom parameters
pub struct CustomParamsTransformer {
    params: serde_json::Map<String, Value>,
}

impl CustomParamsTransformer {
    pub fn new(params: serde_json::Map<String, Value>) -> Self {
        Self { params }
    }
}

impl Transformer for CustomParamsTransformer {
    fn name(&self) -> &str {
        "customparams"
    }

    fn transform_request(&self, mut req: Value) -> Value {
        if let Value::Object(ref mut obj) = req {
            // Merge custom params into request
            for (key, value) in &self.params {
                // Don't override existing fields
                if !obj.contains_key(key) {
                    obj.insert(key.clone(), value.clone());
                }
            }
        }
        req
    }

    fn transform_response(&self, res: Value) -> Value {
        res
    }
}

// Sampling: Control sampling parameters
pub struct SamplingTransformer {
    temperature: Option<f64>,
    top_p: Option<f64>,
    top_k: Option<u32>,
}

impl SamplingTransformer {
    pub fn new(temperature: Option<f64>, top_p: Option<f64>, top_k: Option<u32>) -> Self {
        Self {
            temperature,
            top_p,
            top_k,
        }
    }
}

impl Transformer for SamplingTransformer {
    fn name(&self) -> &str {
        "sampling"
    }

    fn transform_request(&self, mut req: Value) -> Value {
        if let Some(temp) = self.temperature {
            req["temperature"] = json!(temp);
        }
        if let Some(p) = self.top_p {
            req["top_p"] = json!(p);
        }
        if let Some(k) = self.top_k {
            req["top_k"] = json!(k);
        }
        req
    }

    fn transform_response(&self, res: Value) -> Value {
        res
    }
}

// Vertex Claude: Adapt for Google Vertex AI (Claude)
pub struct VertexClaudeTransformer;
impl Transformer for VertexClaudeTransformer {
    fn name(&self) -> &str {
        "vertex-claude"
    }

    fn transform_request(&self, mut req: Value) -> Value {
        // Vertex AI uses Anthropic format but with different endpoints
        // Most fields are pass-through

        // Remove stream field as Vertex handles it differently
        if let Some(obj) = req.as_object_mut() {
            obj.remove("stream");
        }

        req
    }

    fn transform_response(&self, res: Value) -> Value {
        res
    }
}

// Vertex Gemini: Adapt for Google Vertex AI (Gemini)
pub struct VertexGeminiTransformer;
impl Transformer for VertexGeminiTransformer {
    fn name(&self) -> &str {
        "vertex-gemini"
    }

    fn transform_request(&self, req: Value) -> Value {
        // Use base Gemini transformer first
        let gemini = GeminiTransformer;
        let mut req = gemini.transform_request(req);

        // Vertex Gemini-specific adjustments
        if let Some(obj) = req.as_object_mut() {
            // Remove stream_options (not supported by Vertex)
            obj.remove("stream_options");

            // Vertex uses generationConfig instead of top-level params
            let mut gen_config = json!({});

            if let Some(temp) = obj.remove("temperature") {
                gen_config["temperature"] = temp;
            }
            if let Some(top_p) = obj.remove("topP") {
                gen_config["topP"] = top_p;
            }
            if let Some(top_k) = obj.remove("topK") {
                gen_config["topK"] = top_k;
            }

            if !gen_config.as_object().unwrap().is_empty() {
                obj.insert("generationConfig".into(), gen_config);
            }
        }

        req
    }

    fn transform_response(&self, res: Value) -> Value {
        res
    }
}

// Cerebras: Adapt for Cerebras API
pub struct CerebrasTransformer;
impl Transformer for CerebrasTransformer {
    fn name(&self) -> &str {
        "cerebras"
    }

    fn transform_request(&self, req: Value) -> Value {
        // Cerebras uses OpenAI-compatible format
        let openai = OpenAITransformer;
        let mut req = openai.transform_request(req);

        // Remove unsupported fields
        if let Some(obj) = req.as_object_mut() {
            obj.remove("thinking");
            obj.remove("tools"); // Cerebras doesn't support tools yet
            obj.remove("stream_options");
        }

        req
    }

    fn transform_response(&self, res: Value) -> Value {
        res
    }
}

// Vercel: Adapt for Vercel AI SDK
pub struct VercelTransformer;
impl Transformer for VercelTransformer {
    fn name(&self) -> &str {
        "vercel"
    }

    fn transform_request(&self, mut req: Value) -> Value {
        // Vercel AI SDK uses a simplified format
        if let Some(obj) = req.as_object_mut() {
            // Remove unsupported fields
            obj.remove("thinking");

            // Rename fields for Vercel compatibility
            if let Some(messages) = obj.remove("messages") {
                obj.insert("prompt".into(), messages);
            }

            // Vercel uses simpler parameter names
            if let Some(mt) = obj.remove("max_tokens") {
                obj.insert("maxTokens".into(), mt);
            }
        }

        req
    }

    fn transform_response(&self, res: Value) -> Value {
        res
    }
}
