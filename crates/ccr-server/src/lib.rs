use ccr_app_core::official_provider::resolve_provider_api_key;
use ccr_app_core::provider_kind::{
    EndpointTestMode, provider_api_kind_defaults, resolve_provider_api_kind,
};
use ccr_transformer::TransformerRegistry;
use ccr_types::{Config, Provider, RoutePoolCandidate};
use serde_json::Value;

/// Returns true if the request is authorized to access the config API.
/// - If APIKEY is set: require matching Bearer token
/// - Otherwise: only allow loopback addresses
pub fn check_auth(api_key: Option<&str>, auth_header: Option<&str>, peer_ip: &str) -> bool {
    if let Some(key) = api_key.map(str::trim).filter(|key| !key.is_empty()) {
        let expected = format!("Bearer {key}");
        return auth_header.map(|v| v == expected).unwrap_or(false);
    }
    peer_ip == "127.0.0.1" || peer_ip == "::1"
}

/// Redact api_key fields in config JSON for safe display.
pub fn redact_config(config: &Config) -> Value {
    let mut val = serde_json::to_value(config).unwrap();
    if let Some(providers) = val.get_mut("Providers").and_then(|p| p.as_array_mut()) {
        for p in providers.iter_mut() {
            if let Some(k) = p.get_mut("api_key") {
                *k = Value::String("***".into());
            }
        }
    }
    val
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboundProtocol {
    AnthropicMessages,
    OpenAiResponses,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamRequest {
    pub route: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
    pub stream: bool,
    pub model: String,
    pub mode: EndpointTestMode,
}

/// Rewrite the Responses payload model from CCR route format to upstream model name.
pub fn prepare_responses_body(body: Value, model_route: &str) -> Value {
    let upstream_model =
        route_model_override(model_route).unwrap_or_else(|| ccr_router::model_name(model_route));
    prepare_responses_body_for_mode(body, upstream_model, EndpointTestMode::OpenAiResponses)
        .expect("OpenAI Responses mode should not require input conversion")
}

pub fn build_upstream_request(
    inbound: InboundProtocol,
    route: &str,
    provider: &Provider,
    body: Value,
    transformers: &TransformerRegistry,
) -> Result<UpstreamRequest, String> {
    let mode = provider_api_kind_defaults(resolve_provider_api_kind(provider).kind, None)
        .endpoint_test_mode;
    let upstream_model = upstream_model_name(&body, route, provider)?;
    let mut upstream_body = match inbound {
        InboundProtocol::AnthropicMessages => {
            prepare_anthropic_body_for_mode(body, &upstream_model, mode)?
        }
        InboundProtocol::OpenAiResponses => {
            prepare_responses_body_for_mode(body, &upstream_model, mode)?
        }
    };

    let transformer_names = transformer_names(provider);
    let transformer_refs: Vec<&str> = transformer_names.iter().map(String::as_str).collect();
    upstream_body = transformers.apply_chain(&transformer_refs, upstream_body);
    let stream = upstream_body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    validate_stream_compatibility(inbound, mode, stream)?;

    Ok(UpstreamRequest {
        route: route.to_string(),
        url: provider.api_base_url.clone(),
        headers: upstream_headers_resolved(provider)
            .map_err(|error| format!("provider credential unavailable: {error}"))?,
        body: upstream_body,
        stream,
        model: upstream_model,
        mode,
    })
}

pub fn upstream_headers(provider: &Provider) -> Vec<(String, String)> {
    let api_key = resolve_provider_api_key(provider).unwrap_or_else(|_| provider.api_key.clone());
    upstream_headers_with_api_key(provider, &api_key)
}

pub fn upstream_headers_resolved(provider: &Provider) -> Result<Vec<(String, String)>, String> {
    let api_key = resolve_provider_api_key(provider).map_err(|error| error.to_string())?;
    Ok(upstream_headers_with_api_key(provider, &api_key))
}

fn upstream_headers_with_api_key(provider: &Provider, api_key: &str) -> Vec<(String, String)> {
    let mode = provider_api_kind_defaults(resolve_provider_api_kind(provider).kind, None)
        .endpoint_test_mode;
    let mut headers = vec![("content-type".to_string(), "application/json".to_string())];
    match mode {
        EndpointTestMode::AnthropicMessages => {
            headers.push(("x-api-key".to_string(), api_key.to_string()));
            headers.push(("anthropic-version".to_string(), "2023-06-01".to_string()));
        }
        EndpointTestMode::OpenAiChat
        | EndpointTestMode::OpenAiResponses
        | EndpointTestMode::BasicPost => {
            headers.push(("authorization".to_string(), format!("Bearer {api_key}")));
        }
    }
    headers
}

fn prepare_anthropic_body_for_mode(
    mut body: Value,
    upstream_model: &str,
    mode: EndpointTestMode,
) -> Result<Value, String> {
    body["model"] = Value::String(upstream_model.to_string());
    match mode {
        EndpointTestMode::AnthropicMessages | EndpointTestMode::BasicPost => {
            normalize_messages_content_for_anthropic(&mut body)?;
            Ok(body)
        }
        EndpointTestMode::OpenAiChat => anthropic_body_to_openai_chat(body, upstream_model),
        EndpointTestMode::OpenAiResponses => {
            anthropic_body_to_openai_responses(body, upstream_model)
        }
    }
}

fn prepare_responses_body_for_mode(
    mut body: Value,
    upstream_model: &str,
    mode: EndpointTestMode,
) -> Result<Value, String> {
    body["model"] = Value::String(upstream_model.to_string());
    if mode == EndpointTestMode::OpenAiResponses {
        ensure_responses_input(&mut body)?;
        validate_responses_has_input_or_state(&body)?;
        return Ok(body);
    }

    reject_state_only_responses_request(&body, mode)?;
    let messages = responses_input_to_messages_for_mode(&body, mode)?;
    let mut converted = serde_json::Map::new();
    converted.insert(
        "model".to_string(),
        Value::String(upstream_model.to_string()),
    );
    converted.insert("messages".to_string(), messages);
    if let Some(stream) = body.get("stream") {
        converted.insert("stream".to_string(), stream.clone());
    }
    if let Some(max_tokens) = body
        .get("max_output_tokens")
        .or_else(|| body.get("max_tokens"))
    {
        converted.insert("max_tokens".to_string(), max_tokens.clone());
    }
    Ok(Value::Object(converted))
}

fn anthropic_body_to_openai_chat(mut body: Value, upstream_model: &str) -> Result<Value, String> {
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| "Anthropic Messages request is missing messages".to_string())?;
    let mut converted_messages: Vec<Value> = Vec::new();

    if let Some(system) = body.get("system")
        && let Some(text) = text_from_system(system)
    {
        converted_messages.push(serde_json::json!({"role": "system", "content": text}));
    }

    for message in messages {
        converted_messages.push(message_to_openai_chat(message)?);
    }

    let mut converted = serde_json::Map::new();
    converted.insert(
        "model".to_string(),
        Value::String(upstream_model.to_string()),
    );
    converted.insert("messages".to_string(), Value::Array(converted_messages));
    copy_common_request_fields(&body, &mut converted, EndpointTestMode::OpenAiChat);
    body = Value::Object(converted);
    Ok(body)
}

fn anthropic_body_to_openai_responses(body: Value, upstream_model: &str) -> Result<Value, String> {
    let messages = body
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| "Anthropic Messages request is missing messages".to_string())?;
    let mut input = Vec::new();

    if let Some(system) = body.get("system").and_then(text_from_system) {
        input.push(serde_json::json!({
            "role": "system",
            "content": [{"type": "input_text", "text": system}]
        }));
    }
    for message in messages {
        input.push(message_to_responses_input(message)?);
    }

    let mut converted = serde_json::Map::new();
    converted.insert(
        "model".to_string(),
        Value::String(upstream_model.to_string()),
    );
    converted.insert("input".to_string(), Value::Array(input));
    copy_common_request_fields(&body, &mut converted, EndpointTestMode::OpenAiResponses);
    Ok(Value::Object(converted))
}

fn copy_common_request_fields(
    body: &Value,
    converted: &mut serde_json::Map<String, Value>,
    mode: EndpointTestMode,
) {
    if let Some(stream) = body.get("stream") {
        converted.insert("stream".to_string(), stream.clone());
    }
    match mode {
        EndpointTestMode::OpenAiResponses => {
            if let Some(max_tokens) = body
                .get("max_output_tokens")
                .or_else(|| body.get("max_tokens"))
            {
                converted.insert("max_output_tokens".to_string(), max_tokens.clone());
            }
            if let Some(reasoning) = body.get("reasoning") {
                converted.insert("reasoning".to_string(), reasoning.clone());
            }
        }
        EndpointTestMode::OpenAiChat
        | EndpointTestMode::AnthropicMessages
        | EndpointTestMode::BasicPost => {
            if let Some(max_tokens) = body
                .get("max_tokens")
                .or_else(|| body.get("max_output_tokens"))
            {
                converted.insert("max_tokens".to_string(), max_tokens.clone());
            }
        }
    }
}

fn ensure_responses_input(body: &mut Value) -> Result<(), String> {
    if body.get("input").is_some() {
        return Ok(());
    }
    let Some(messages) = body.get("messages").cloned() else {
        return Ok(());
    };
    if !messages.is_array() {
        return Err("Unsupported Responses messages shape".to_string());
    }
    if let Some(obj) = body.as_object_mut() {
        obj.remove("messages");
        obj.insert("input".to_string(), messages);
    }
    Ok(())
}

fn validate_responses_has_input_or_state(body: &Value) -> Result<(), String> {
    if body.get("input").is_some()
        || body.get("previous_response_id").is_some()
        || body.get("conversation_id").is_some()
        || body.get("prompt").is_some()
    {
        Ok(())
    } else {
        Err(
            "Responses request is missing input, previous_response_id, conversation_id or prompt"
                .to_string(),
        )
    }
}

fn reject_state_only_responses_request(body: &Value, mode: EndpointTestMode) -> Result<(), String> {
    if matches!(mode, EndpointTestMode::OpenAiResponses) {
        return Ok(());
    }
    let has_state = body.get("previous_response_id").is_some()
        || body.get("conversation_id").is_some()
        || body.get("prompt").is_some();
    let has_expandable_input = body.get("input").is_some() || body.get("messages").is_some();
    if has_state && !has_expandable_input {
        return Err(
            "Responses stateful fields require input/messages for non-Responses providers"
                .to_string(),
        );
    }
    Ok(())
}

pub fn route_model_override(route: &str) -> Option<&str> {
    let (_, model) = route.split_once(',')?;
    let model = model.trim();
    if model.is_empty() { None } else { Some(model) }
}

pub fn upstream_model_name(
    body: &Value,
    route: &str,
    provider: &Provider,
) -> Result<String, String> {
    if let Some(model) = route_model_override(route) {
        return Ok(model.to_string());
    }
    if let Some(model) = body.get("model").and_then(|value| value.as_str()) {
        let model = model.trim();
        if !model.is_empty() {
            return Ok(model.to_string());
        }
    }
    match provider.models.as_slice() {
        [model] if !model.trim().is_empty() => Ok(model.clone()),
        [] => Err(format!(
            "missing upstream model for provider-only route '{}'",
            route
        )),
        _ => Err(format!(
            "ambiguous upstream model for provider-only route '{}'",
            route
        )),
    }
}

fn responses_input_to_messages_for_mode(
    body: &Value,
    mode: EndpointTestMode,
) -> Result<Value, String> {
    let target = match mode {
        EndpointTestMode::AnthropicMessages | EndpointTestMode::BasicPost => {
            TargetContentKind::Anthropic
        }
        EndpointTestMode::OpenAiChat => TargetContentKind::OpenAiChat,
        EndpointTestMode::OpenAiResponses => TargetContentKind::OpenAiResponses,
    };
    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        let converted: Result<Vec<Value>, String> = messages
            .iter()
            .map(|message| normalize_message_content(message, target))
            .collect();
        return Ok(Value::Array(converted?));
    }
    match body.get("input") {
        Some(Value::String(input)) => Ok(serde_json::json!([
            {"role": "user", "content": input}
        ])),
        Some(Value::Array(items)) => {
            let converted: Result<Vec<Value>, String> = items
                .iter()
                .map(|message| normalize_message_content(message, target))
                .collect();
            Ok(Value::Array(converted?))
        }
        Some(_) => Err("Unsupported Responses input shape".to_string()),
        None => Err("Responses request is missing input/messages".to_string()),
    }
}

fn normalize_message_content(message: &Value, target: TargetContentKind) -> Result<Value, String> {
    let mut message = message.clone();
    normalize_message_content_in_place(&mut message, target)?;
    Ok(message)
}

fn normalize_messages_content_for_anthropic(body: &mut Value) -> Result<(), String> {
    let Some(messages) = body.get_mut("messages").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    for message in messages {
        normalize_message_content_in_place(message, TargetContentKind::Anthropic)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum TargetContentKind {
    Anthropic,
    OpenAiChat,
    OpenAiResponses,
}

fn normalize_message_content_in_place(
    message: &mut Value,
    target: TargetContentKind,
) -> Result<(), String> {
    let Some(content) = message.get_mut("content") else {
        return Ok(());
    };
    if content.is_string() {
        return Ok(());
    }
    let Some(blocks) = content.as_array_mut() else {
        return Err("Unsupported message content shape".to_string());
    };
    let converted: Result<Vec<Value>, String> = blocks
        .iter()
        .map(|block| content_block_for_target(block, target))
        .collect();
    *blocks = converted?;
    Ok(())
}

fn message_to_openai_chat(message: &Value) -> Result<Value, String> {
    let role = message
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("user")
        .to_string();
    if let Some(tool_call_id) = tool_result_id(message) {
        return Ok(serde_json::json!({
            "role": "tool",
            "tool_call_id": tool_call_id,
            "content": message_content_as_text(message)
        }));
    }

    let content = match message.get("content") {
        Some(Value::String(text)) => Value::String(text.clone()),
        Some(Value::Array(blocks)) => {
            let converted: Result<Vec<Value>, String> = blocks
                .iter()
                .filter(|block| {
                    !matches!(
                        block.get("type").and_then(Value::as_str),
                        Some("tool_use" | "tool_result")
                    )
                })
                .map(|block| content_block_for_target(block, TargetContentKind::OpenAiChat))
                .collect();
            let converted = converted?;
            if converted.is_empty() {
                Value::String(String::new())
            } else {
                Value::Array(converted)
            }
        }
        Some(_) => return Err("Unsupported message content shape".to_string()),
        None => Value::String(String::new()),
    };

    let mut obj = serde_json::Map::new();
    obj.insert("role".to_string(), Value::String(role));
    obj.insert("content".to_string(), content);
    if let Some(tool_calls) = anthropic_tool_use_blocks_to_openai_tool_calls(message)? {
        obj.insert("tool_calls".to_string(), tool_calls);
    }
    Ok(Value::Object(obj))
}

fn message_to_responses_input(message: &Value) -> Result<Value, String> {
    let role = message
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or("user")
        .to_string();
    let content = match message.get("content") {
        Some(Value::String(text)) => Value::Array(vec![
            serde_json::json!({"type": "input_text", "text": text}),
        ]),
        Some(Value::Array(blocks)) => {
            let converted: Result<Vec<Value>, String> = blocks
                .iter()
                .map(|block| content_block_for_target(block, TargetContentKind::OpenAiResponses))
                .collect();
            Value::Array(converted?)
        }
        Some(_) => return Err("Unsupported message content shape".to_string()),
        None => Value::Array(Vec::new()),
    };
    Ok(serde_json::json!({
        "role": role,
        "content": content
    }))
}

fn content_block_for_target(block: &Value, target: TargetContentKind) -> Result<Value, String> {
    let Some(block_type) = block.get("type").and_then(Value::as_str) else {
        return Ok(block.clone());
    };
    match target {
        TargetContentKind::Anthropic => content_block_for_anthropic(block, block_type),
        TargetContentKind::OpenAiChat => content_block_for_openai_chat(block, block_type),
        TargetContentKind::OpenAiResponses => content_block_for_openai_responses(block, block_type),
    }
}

fn content_block_for_anthropic(block: &Value, block_type: &str) -> Result<Value, String> {
    match block_type {
        "input_text" | "output_text" => Ok(serde_json::json!({
            "type": "text",
            "text": block.get("text").and_then(Value::as_str).unwrap_or_default()
        })),
        "text" | "tool_use" | "tool_result" | "image" => Ok(block.clone()),
        "image_url" | "input_image" => openai_image_to_anthropic(block),
        "input_audio" | "audio" | "file" | "input_file" => Err(format!(
            "Unsupported content block type for Anthropic Messages: {block_type}"
        )),
        _ => Ok(block.clone()),
    }
}

fn content_block_for_openai_chat(block: &Value, block_type: &str) -> Result<Value, String> {
    match block_type {
        "text" | "input_text" | "output_text" => Ok(serde_json::json!({
            "type": "text",
            "text": block.get("text").and_then(Value::as_str).unwrap_or_default()
        })),
        "image" => anthropic_image_to_openai_chat(block),
        "image_url" => Ok(block.clone()),
        "tool_use" | "tool_result" => Ok(block.clone()),
        "input_audio" | "audio" | "file" | "input_file" => Err(format!(
            "Unsupported content block type for OpenAI Chat: {block_type}"
        )),
        _ => Ok(block.clone()),
    }
}

fn content_block_for_openai_responses(block: &Value, block_type: &str) -> Result<Value, String> {
    match block_type {
        "text" | "input_text" | "output_text" => Ok(serde_json::json!({
            "type": "input_text",
            "text": block.get("text").and_then(Value::as_str).unwrap_or_default()
        })),
        "image" => anthropic_image_to_openai_responses(block),
        "image_url" | "input_image" => responses_image_block(block),
        "tool_use" | "tool_result" => Ok(block.clone()),
        "input_audio" | "audio" | "file" | "input_file" => Err(format!(
            "Unsupported content block type for OpenAI Responses: {block_type}"
        )),
        _ => Ok(block.clone()),
    }
}

fn openai_image_to_anthropic(block: &Value) -> Result<Value, String> {
    let Some(url) = image_url_from_block(block) else {
        return Err("Image block is missing image URL".to_string());
    };
    if let Some((media_type, data)) = parse_data_url(&url) {
        return Ok(serde_json::json!({
            "type": "image",
            "source": {
                "type": "base64",
                "media_type": media_type,
                "data": data
            }
        }));
    }
    Ok(serde_json::json!({
        "type": "image",
        "source": {
            "type": "url",
            "url": url
        }
    }))
}

fn anthropic_image_to_openai_chat(block: &Value) -> Result<Value, String> {
    let url = image_url_from_anthropic_source(block)?;
    Ok(serde_json::json!({
        "type": "image_url",
        "image_url": {"url": url}
    }))
}

fn anthropic_image_to_openai_responses(block: &Value) -> Result<Value, String> {
    let url = image_url_from_anthropic_source(block)?;
    Ok(serde_json::json!({
        "type": "input_image",
        "image_url": url
    }))
}

fn responses_image_block(block: &Value) -> Result<Value, String> {
    let url = image_url_from_block(block)
        .ok_or_else(|| "Image block is missing image URL".to_string())?;
    Ok(serde_json::json!({
        "type": "input_image",
        "image_url": url
    }))
}

fn image_url_from_block(block: &Value) -> Option<String> {
    block
        .get("image_url")
        .and_then(|image_url| {
            image_url
                .as_str()
                .map(str::to_string)
                .or_else(|| image_url.get("url")?.as_str().map(str::to_string))
        })
        .or_else(|| block.get("url").and_then(Value::as_str).map(str::to_string))
}

fn image_url_from_anthropic_source(block: &Value) -> Result<String, String> {
    let source = block
        .get("source")
        .ok_or_else(|| "Anthropic image block is missing source".to_string())?;
    match source.get("type").and_then(Value::as_str) {
        Some("url") => source
            .get("url")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "Anthropic image URL source is missing url".to_string()),
        Some("base64") => {
            let media_type = source
                .get("media_type")
                .and_then(Value::as_str)
                .ok_or_else(|| "Anthropic base64 image is missing media_type".to_string())?;
            let data = source
                .get("data")
                .and_then(Value::as_str)
                .ok_or_else(|| "Anthropic base64 image is missing data".to_string())?;
            Ok(format!("data:{media_type};base64,{data}"))
        }
        _ => Err("Unsupported Anthropic image source type".to_string()),
    }
}

fn parse_data_url(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("data:")?;
    let (media_type, data) = rest.split_once(";base64,")?;
    Some((media_type, data))
}

fn text_from_system(system: &Value) -> Option<String> {
    match system {
        Value::String(text) => Some(text.clone()),
        Value::Array(blocks) => blocks.iter().find_map(|block| {
            if block.get("type").and_then(Value::as_str) == Some("text") {
                block
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            } else {
                None
            }
        }),
        _ => None,
    }
}

fn tool_result_id(message: &Value) -> Option<String> {
    let content = message.get("content")?.as_array()?;
    content.iter().find_map(|block| {
        if block.get("type").and_then(Value::as_str) == Some("tool_result") {
            block
                .get("tool_use_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        } else {
            None
        }
    })
}

fn message_content_as_text(message: &Value) -> String {
    match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|block| {
                block
                    .get("text")
                    .or_else(|| block.get("content"))
                    .and_then(Value::as_str)
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn anthropic_tool_use_blocks_to_openai_tool_calls(
    message: &Value,
) -> Result<Option<Value>, String> {
    let Some(content) = message.get("content").and_then(Value::as_array) else {
        return Ok(None);
    };
    let mut tool_calls = Vec::new();
    for block in content {
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            continue;
        }
        let id = block
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| "tool_use block is missing id".to_string())?;
        let name = block
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "tool_use block is missing name".to_string())?;
        let arguments = block
            .get("input")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        tool_calls.push(serde_json::json!({
            "id": id,
            "type": "function",
            "function": {
                "name": name,
                "arguments": arguments.to_string()
            }
        }));
    }
    if tool_calls.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Value::Array(tool_calls)))
    }
}

fn validate_stream_compatibility(
    inbound: InboundProtocol,
    mode: EndpointTestMode,
    stream: bool,
) -> Result<(), String> {
    if !stream {
        return Ok(());
    }
    let compatible = matches!(
        (inbound, mode),
        (
            InboundProtocol::AnthropicMessages,
            EndpointTestMode::AnthropicMessages
        ) | (
            InboundProtocol::OpenAiResponses,
            EndpointTestMode::OpenAiResponses
        ) | (
            InboundProtocol::OpenAiResponses,
            EndpointTestMode::AnthropicMessages
        ) | (
            InboundProtocol::OpenAiResponses,
            EndpointTestMode::BasicPost
        )
    );
    if compatible {
        Ok(())
    } else {
        Err("Cross-protocol streaming conversion is not implemented".to_string())
    }
}

fn transformer_names(provider: &Provider) -> Vec<String> {
    provider
        .transformer
        .use_transformers
        .iter()
        .filter_map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .or_else(|| value.as_array()?.first()?.as_str().map(str::to_string))
        })
        .collect()
}

pub fn route_pool_candidates(config: &Config, tried_routes: &[String]) -> Vec<String> {
    let Some(pool) = &config.route_pool else {
        return Vec::new();
    };
    if !pool.enabled {
        return Vec::new();
    }

    let mut candidates = pool.candidates.clone();
    normalize_route_pool_order(&mut candidates);
    let mut routes = Vec::new();
    for candidate in candidates {
        let route = candidate.route.trim();
        if !candidate.enabled || route.is_empty() {
            continue;
        }
        if tried_routes.iter().any(|tried| tried == route) || routes.iter().any(|r| r == route) {
            continue;
        }
        routes.push(route.to_string());
    }
    routes
}

pub fn normalize_route_pool_order(candidates: &mut [RoutePoolCandidate]) {
    candidates.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.route.cmp(&b.route))
    });
}

pub fn route_pool_enabled(config: &Config) -> bool {
    config.route_pool.as_ref().is_some_and(|pool| pool.enabled)
}

pub fn route_pool_failure_threshold(config: &Config) -> u32 {
    config
        .route_pool
        .as_ref()
        .map(|pool| pool.failure_threshold.max(1))
        .unwrap_or(3)
}

pub fn route_pool_ban_seconds(config: &Config) -> u64 {
    config
        .route_pool
        .as_ref()
        .map(|pool| pool.ban_seconds.max(1))
        .unwrap_or(3600)
}

pub fn provider_for_route<'a>(route: &str, config: &'a Config) -> Option<(&'a Provider, String)> {
    let provider = ccr_router::find_provider(route, config)?;
    Some((provider, ccr_router::model_name(route).to_string()))
}

pub fn route_for_display(route: &str) -> String {
    if route.trim().is_empty() {
        "<empty>".to_string()
    } else {
        route.to_string()
    }
}

pub fn provider_names(config: &Config) -> String {
    if config.providers.is_empty() {
        return "<none>".to_string();
    }
    config
        .providers
        .iter()
        .map(|provider| provider.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn provider_not_found_message(route: &str, config: &Config) -> String {
    format!(
        "No provider found for route '{}'. Available providers: {}",
        route_for_display(route),
        provider_names(config)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccr_types::{Config, Provider, ProviderApiKind, ProviderApiKindSource, TransformerConfig};

    #[test]
    fn auth_with_api_key_correct() {
        assert!(check_auth(Some("secret"), Some("Bearer secret"), "1.2.3.4"));
    }

    #[test]
    fn auth_with_api_key_wrong() {
        assert!(!check_auth(
            Some("secret"),
            Some("Bearer wrong"),
            "127.0.0.1"
        ));
    }

    #[test]
    fn auth_no_key_loopback_allowed() {
        assert!(check_auth(None, None, "127.0.0.1"));
        assert!(check_auth(None, None, "::1"));
    }

    #[test]
    fn auth_blank_api_key_is_treated_as_no_key() {
        assert!(check_auth(Some(""), None, "127.0.0.1"));
        assert!(check_auth(Some("  "), None, "::1"));
        assert!(!check_auth(Some(""), None, "1.2.3.4"));
    }

    #[test]
    fn auth_no_key_remote_denied() {
        assert!(!check_auth(None, None, "1.2.3.4"));
    }

    #[test]
    fn redact_masks_api_keys() {
        let config = Config {
            providers: vec![Provider {
                name: "openai".into(),
                api_kind: None,
                api_kind_source: ccr_types::ProviderApiKindSource::Inferred,
                api_base_url: "https://api.openai.com".into(),
                api_key: "sk-real-key".into(),
                models: vec![],
                endpoint_candidates: vec![],
                transformer: Default::default(),
            }],
            ..Default::default()
        };
        let val = redact_config(&config);
        assert_eq!(val["Providers"][0]["api_key"], "***");
    }

    #[test]
    fn prepare_responses_body_strips_provider_prefix() {
        let body = serde_json::json!({
            "model": "openai,gpt-5-codex",
            "input": "hello"
        });
        let prepared = prepare_responses_body(body, "openai,gpt-5-codex");
        assert_eq!(prepared["model"], "gpt-5-codex");
        assert_eq!(prepared["input"], "hello");
    }

    #[test]
    fn route_model_override_only_returns_explicit_model() {
        assert_eq!(route_model_override("openai,gpt-5"), Some("gpt-5"));
        assert_eq!(route_model_override("zenmux"), None);
        assert_eq!(route_model_override("zenmux,"), None);
    }

    #[test]
    fn provider_not_found_message_explains_empty_route() {
        let config = Config {
            providers: vec![provider_with_kind(
                ProviderApiKind::AnthropicMessages,
                vec![],
            )],
            ..Default::default()
        };

        let message = provider_not_found_message("", &config);

        assert!(message.contains("'<empty>'"));
        assert!(message.contains("Available providers: p"));
    }

    #[test]
    fn provider_names_handles_empty_config() {
        assert_eq!(provider_names(&Config::default()), "<none>");
    }

    fn provider_with_kind(kind: ProviderApiKind, transformer: Vec<serde_json::Value>) -> Provider {
        Provider {
            name: "p".into(),
            api_kind: Some(kind),
            api_kind_source: ProviderApiKindSource::Explicit,
            api_base_url: "https://api.example.com".into(),
            api_key: "sk-key".into(),
            models: vec!["m".into()],
            endpoint_candidates: vec![],
            transformer: TransformerConfig {
                use_transformers: transformer,
                model_overrides: Default::default(),
            },
        }
    }

    #[test]
    fn upstream_headers_use_anthropic_headers() {
        let provider = provider_with_kind(ProviderApiKind::AnthropicMessages, vec![]);
        let headers = upstream_headers(&provider);

        assert!(
            headers
                .iter()
                .any(|(name, value)| name == "x-api-key" && value == "sk-key")
        );
        assert!(
            headers
                .iter()
                .any(|(name, value)| name == "anthropic-version" && value == "2023-06-01")
        );
        assert!(!headers.iter().any(|(name, _)| name == "authorization"));
    }

    #[test]
    fn upstream_headers_use_bearer_for_openai_like() {
        let provider = provider_with_kind(ProviderApiKind::OpenAiChat, vec![]);
        let headers = upstream_headers(&provider);

        assert!(
            headers
                .iter()
                .any(|(name, value)| name == "authorization" && value == "Bearer sk-key")
        );
    }

    #[test]
    fn upstream_builder_rejects_unsupported_official_credential() {
        let mut provider = provider_with_kind(ProviderApiKind::OpenAiChat, vec![]);
        provider.api_key =
            ccr_app_core::official_provider::GITHUB_COPILOT_OFFICIAL_SECRET.to_string();
        let registry = TransformerRegistry::new();

        let err = build_upstream_request(
            InboundProtocol::AnthropicMessages,
            "p,gpt-5",
            &provider,
            serde_json::json!({
                "model": "p,gpt-5",
                "messages": [{"role": "user", "content": "hello"}]
            }),
            &registry,
        )
        .unwrap_err();

        assert!(err.contains("provider credential unavailable"));
        assert!(err.contains("credential is unsupported"));
    }

    #[test]
    fn upstream_builder_keeps_anthropic_messages_for_anthropic_provider() {
        let provider = provider_with_kind(ProviderApiKind::AnthropicMessages, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::AnthropicMessages,
            "p,claude-sonnet-4",
            &provider,
            serde_json::json!({
                "model": "p,claude-sonnet-4",
                "messages": [{"role": "user", "content": "hello"}]
            }),
            &registry,
        )
        .unwrap();

        assert_eq!(request.body["model"], "claude-sonnet-4");
        assert_eq!(request.body["messages"][0]["content"], "hello");
        assert!(request.body.get("input").is_none());
        assert!(request.headers.iter().any(|(name, _)| name == "x-api-key"));
    }

    #[test]
    fn upstream_builder_converts_anthropic_messages_to_openai_chat() {
        let provider = provider_with_kind(ProviderApiKind::OpenAiChat, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::AnthropicMessages,
            "p,gpt-5",
            &provider,
            serde_json::json!({
                "model": "p,gpt-5",
                "system": [{"type": "text", "text": "be brief"}],
                "messages": [{"role": "user", "content": [{"type": "text", "text": "hello"}]}]
            }),
            &registry,
        )
        .unwrap();

        assert_eq!(request.body["model"], "gpt-5");
        assert_eq!(request.body["messages"][0]["role"], "system");
        assert_eq!(request.body["messages"][0]["content"], "be brief");
        assert_eq!(request.body["messages"][1]["content"][0]["type"], "text");
        assert!(request.body.get("input").is_none());
        assert!(
            request
                .headers
                .iter()
                .any(|(name, _)| name == "authorization")
        );
    }

    #[test]
    fn upstream_builder_converts_anthropic_messages_to_openai_responses_input() {
        let provider = provider_with_kind(ProviderApiKind::OpenAiResponses, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::AnthropicMessages,
            "p,gpt-5",
            &provider,
            serde_json::json!({
                "model": "p,gpt-5",
                "system": "be brief",
                "messages": [{"role": "user", "content": "hello"}],
                "max_tokens": 4
            }),
            &registry,
        )
        .unwrap();

        assert_eq!(request.body["model"], "gpt-5");
        assert!(request.body.get("messages").is_none());
        assert_eq!(request.body["input"][0]["role"], "system");
        assert_eq!(request.body["input"][1]["role"], "user");
        assert_eq!(request.body["input"][1]["content"][0]["type"], "input_text");
        assert_eq!(request.body["max_output_tokens"], 4);
    }

    #[test]
    fn upstream_builder_keeps_responses_body_for_responses_provider() {
        let provider = provider_with_kind(ProviderApiKind::OpenAiResponses, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::OpenAiResponses,
            "p,gpt-5-codex",
            &provider,
            serde_json::json!({"model": "p,gpt-5-codex", "input": "hello", "max_output_tokens": 2}),
            &registry,
        )
        .unwrap();

        assert_eq!(request.body["model"], "gpt-5-codex");
        assert_eq!(request.body["input"], "hello");
        assert_eq!(request.body["max_output_tokens"], 2);
        assert!(request.body.get("messages").is_none());
    }

    #[test]
    fn upstream_builder_preserves_responses_state_for_responses_provider() {
        let provider = provider_with_kind(ProviderApiKind::OpenAiResponses, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::OpenAiResponses,
            "p,gpt-5",
            &provider,
            serde_json::json!({
                "model": "p,gpt-5",
                "previous_response_id": "resp_123"
            }),
            &registry,
        )
        .unwrap();

        assert_eq!(request.body["model"], "gpt-5");
        assert_eq!(request.body["previous_response_id"], "resp_123");
        assert!(request.body.get("input").is_none());
    }

    #[test]
    fn upstream_builder_rejects_state_only_responses_for_messages_provider() {
        let provider = provider_with_kind(ProviderApiKind::AnthropicMessages, vec![]);
        let registry = TransformerRegistry::new();
        let err = build_upstream_request(
            InboundProtocol::OpenAiResponses,
            "p,claude-sonnet-4",
            &provider,
            serde_json::json!({
                "model": "p,claude-sonnet-4",
                "conversation_id": "conv_123"
            }),
            &registry,
        )
        .unwrap_err();

        assert!(err.contains("stateful fields require input/messages"));
    }

    #[test]
    fn upstream_builder_rejects_responses_without_input_messages_or_state() {
        let provider = provider_with_kind(ProviderApiKind::OpenAiResponses, vec![]);
        let registry = TransformerRegistry::new();
        let err = build_upstream_request(
            InboundProtocol::OpenAiResponses,
            "p,gpt-5",
            &provider,
            serde_json::json!({"model": "p,gpt-5"}),
            &registry,
        )
        .unwrap_err();

        assert!(err.contains("missing input"));
    }

    #[test]
    fn upstream_builder_maps_codex_messages_to_responses_input() {
        let provider = provider_with_kind(ProviderApiKind::OpenAiResponses, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::OpenAiResponses,
            "p,gpt-5.5",
            &provider,
            serde_json::json!({
                "model": "p,gpt-5.5",
                "messages": [{
                    "role": "user",
                    "content": [{"type": "input_text", "text": "hello"}]
                }],
                "stream": true
            }),
            &registry,
        )
        .unwrap();

        assert_eq!(request.body["model"], "gpt-5.5");
        assert_eq!(request.body["input"][0]["role"], "user");
        assert_eq!(request.body["input"][0]["content"][0]["type"], "input_text");
        assert!(request.body.get("messages").is_none());
    }

    #[test]
    fn upstream_builder_preserves_inbound_model_for_provider_only_route() {
        let provider = provider_with_kind(ProviderApiKind::AnthropicMessages, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::AnthropicMessages,
            "p",
            &provider,
            serde_json::json!({
                "model": "claude-sonnet-4-20250514",
                "messages": [{"role": "user", "content": "hello"}]
            }),
            &registry,
        )
        .unwrap();

        assert_eq!(request.model, "claude-sonnet-4-20250514");
        assert_eq!(request.body["model"], "claude-sonnet-4-20250514");
    }

    #[test]
    fn upstream_builder_uses_route_model_when_explicit() {
        let provider = provider_with_kind(ProviderApiKind::AnthropicMessages, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::AnthropicMessages,
            "p,forced-model",
            &provider,
            serde_json::json!({
                "model": "inbound-model",
                "messages": [{"role": "user", "content": "hello"}]
            }),
            &registry,
        )
        .unwrap();

        assert_eq!(request.model, "forced-model");
        assert_eq!(request.body["model"], "forced-model");
    }

    #[test]
    fn upstream_builder_uses_unique_provider_model_when_inbound_missing() {
        let provider = provider_with_kind(ProviderApiKind::AnthropicMessages, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::AnthropicMessages,
            "p",
            &provider,
            serde_json::json!({
                "messages": [{"role": "user", "content": "hello"}]
            }),
            &registry,
        )
        .unwrap();

        assert_eq!(request.model, "m");
        assert_eq!(request.body["model"], "m");
    }

    #[test]
    fn upstream_builder_rejects_provider_only_without_model() {
        let mut provider = provider_with_kind(ProviderApiKind::AnthropicMessages, vec![]);
        provider.models.clear();
        let registry = TransformerRegistry::new();
        let err = build_upstream_request(
            InboundProtocol::AnthropicMessages,
            "p",
            &provider,
            serde_json::json!({
                "messages": [{"role": "user", "content": "hello"}]
            }),
            &registry,
        )
        .unwrap_err();

        assert!(err.contains("missing upstream model"));
    }

    #[test]
    fn upstream_builder_converts_responses_input_for_anthropic_provider() {
        let provider = provider_with_kind(ProviderApiKind::AnthropicMessages, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::OpenAiResponses,
            "p,claude-sonnet-4",
            &provider,
            serde_json::json!({"model": "p,claude-sonnet-4", "input": "hello", "max_output_tokens": 2}),
            &registry,
        )
        .unwrap();

        assert_eq!(request.body["model"], "claude-sonnet-4");
        assert_eq!(request.body["messages"][0]["role"], "user");
        assert_eq!(request.body["messages"][0]["content"], "hello");
        assert_eq!(request.body["max_tokens"], 2);
        assert!(request.headers.iter().any(|(name, _)| name == "x-api-key"));
    }

    #[test]
    fn upstream_builder_drops_responses_reasoning_for_anthropic_provider() {
        let provider = provider_with_kind(ProviderApiKind::AnthropicMessages, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::OpenAiResponses,
            "p,claude-sonnet-4",
            &provider,
            serde_json::json!({
                "model": "p,claude-sonnet-4",
                "input": "hello",
                "reasoning": {"effort": "high"}
            }),
            &registry,
        )
        .unwrap();

        assert!(request.body.get("reasoning").is_none());
        assert_eq!(request.body["messages"][0]["content"], "hello");
    }

    #[test]
    fn upstream_builder_normalizes_codex_content_for_messages_provider() {
        let provider = provider_with_kind(ProviderApiKind::AnthropicMessages, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::OpenAiResponses,
            "p,claude-sonnet-4",
            &provider,
            serde_json::json!({
                "model": "p,claude-sonnet-4",
                "messages": [{
                    "role": "user",
                    "content": [{"type": "input_text", "text": "hello"}]
                }]
            }),
            &registry,
        )
        .unwrap();

        assert_eq!(request.body["model"], "claude-sonnet-4");
        assert_eq!(request.body["messages"][0]["role"], "user");
        assert_eq!(request.body["messages"][0]["content"][0]["type"], "text");
    }

    #[test]
    fn upstream_builder_applies_openai_transformer_for_messages() {
        let provider = provider_with_kind(
            ProviderApiKind::OpenAiChat,
            vec![serde_json::json!("openai")],
        );
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::AnthropicMessages,
            "p,gpt-4o",
            &provider,
            serde_json::json!({
                "model": "p,gpt-4o",
                "system": "be brief",
                "messages": [{"role": "user", "content": "hello"}]
            }),
            &registry,
        )
        .unwrap();

        assert_eq!(request.body["model"], "gpt-4o");
        assert!(request.body.get("system").is_none());
        assert_eq!(request.body["messages"][0]["role"], "system");
        assert_eq!(request.body["messages"][0]["content"], "be brief");
    }

    #[test]
    fn upstream_builder_maps_anthropic_tool_use_to_openai_chat_tool_call() {
        let provider = provider_with_kind(ProviderApiKind::OpenAiChat, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::AnthropicMessages,
            "p,gpt-5",
            &provider,
            serde_json::json!({
                "model": "p,gpt-5",
                "messages": [{
                    "role": "assistant",
                    "content": [{
                        "type": "tool_use",
                        "id": "toolu_1",
                        "name": "read_file",
                        "input": {"path": "/tmp/a"}
                    }]
                }]
            }),
            &registry,
        )
        .unwrap();

        assert_eq!(request.body["messages"][0]["role"], "assistant");
        assert_eq!(request.body["messages"][0]["content"], "");
        assert_eq!(
            request.body["messages"][0]["tool_calls"][0]["id"],
            "toolu_1"
        );
        assert_eq!(
            request.body["messages"][0]["tool_calls"][0]["function"]["name"],
            "read_file"
        );
    }

    #[test]
    fn upstream_builder_maps_anthropic_image_to_openai_responses_image() {
        let provider = provider_with_kind(ProviderApiKind::OpenAiResponses, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::AnthropicMessages,
            "p,gpt-5",
            &provider,
            serde_json::json!({
                "model": "p,gpt-5",
                "messages": [{
                    "role": "user",
                    "content": [{
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": "image/png",
                            "data": "AAAA"
                        }
                    }]
                }]
            }),
            &registry,
        )
        .unwrap();

        assert_eq!(
            request.body["input"][0]["content"][0]["type"],
            "input_image"
        );
        assert_eq!(
            request.body["input"][0]["content"][0]["image_url"],
            "data:image/png;base64,AAAA"
        );
    }

    #[test]
    fn upstream_builder_rejects_unsupported_audio_content() {
        let provider = provider_with_kind(ProviderApiKind::AnthropicMessages, vec![]);
        let registry = TransformerRegistry::new();
        let err = build_upstream_request(
            InboundProtocol::OpenAiResponses,
            "p,claude-sonnet-4",
            &provider,
            serde_json::json!({
                "model": "p,claude-sonnet-4",
                "input": [{
                    "role": "user",
                    "content": [{"type": "input_audio", "audio": "..."}]
                }]
            }),
            &registry,
        )
        .unwrap_err();

        assert!(err.contains("Unsupported content block type"));
    }

    #[test]
    fn upstream_builder_allows_responses_streaming_to_anthropic_provider() {
        let provider = provider_with_kind(ProviderApiKind::AnthropicMessages, vec![]);
        let registry = TransformerRegistry::new();
        let request = build_upstream_request(
            InboundProtocol::OpenAiResponses,
            "p,claude-sonnet-4",
            &provider,
            serde_json::json!({
                "model": "p,claude-sonnet-4",
                "input": "hello",
                "stream": true
            }),
            &registry,
        )
        .unwrap();

        assert!(request.stream);
        assert_eq!(request.mode, EndpointTestMode::AnthropicMessages);
        assert_eq!(request.body["messages"][0]["content"], "hello");
    }

    #[test]
    fn upstream_builder_rejects_responses_streaming_to_openai_chat_provider() {
        let provider = provider_with_kind(ProviderApiKind::OpenAiChat, vec![]);
        let registry = TransformerRegistry::new();
        let err = build_upstream_request(
            InboundProtocol::OpenAiResponses,
            "p,gpt-4o",
            &provider,
            serde_json::json!({
                "model": "p,gpt-4o",
                "input": "hello",
                "stream": true
            }),
            &registry,
        )
        .unwrap_err();

        assert!(err.contains("Cross-protocol streaming conversion is not implemented"));
    }

    #[test]
    fn upstream_builder_rejects_unsupported_responses_input() {
        let provider = provider_with_kind(ProviderApiKind::OpenAiChat, vec![]);
        let registry = TransformerRegistry::new();
        let err = build_upstream_request(
            InboundProtocol::OpenAiResponses,
            "p,gpt-4o",
            &provider,
            serde_json::json!({"input": {"bad": true}}),
            &registry,
        )
        .unwrap_err();

        assert!(err.contains("Unsupported Responses input"));
    }

    #[test]
    fn route_pool_candidates_sort_skip_disabled_and_deduplicate() {
        let config = Config {
            route_pool: Some(ccr_types::RoutePoolConfig {
                enabled: true,
                failure_threshold: 3,
                ban_seconds: 3600,
                candidates: vec![
                    ccr_types::RoutePoolCandidate {
                        route: "b".into(),
                        enabled: true,
                        priority: 2,
                    },
                    ccr_types::RoutePoolCandidate {
                        route: "a".into(),
                        enabled: true,
                        priority: 1,
                    },
                    ccr_types::RoutePoolCandidate {
                        route: "c".into(),
                        enabled: false,
                        priority: 0,
                    },
                    ccr_types::RoutePoolCandidate {
                        route: "a".into(),
                        enabled: true,
                        priority: 3,
                    },
                ],
            }),
            ..Default::default()
        };

        assert_eq!(route_pool_candidates(&config, &[]), vec!["a", "b"]);
    }

    #[test]
    fn route_pool_defaults_are_clamped() {
        let config = Config {
            route_pool: Some(ccr_types::RoutePoolConfig {
                enabled: true,
                failure_threshold: 0,
                ban_seconds: 0,
                candidates: vec![],
            }),
            ..Default::default()
        };

        assert_eq!(route_pool_failure_threshold(&config), 1);
        assert_eq!(route_pool_ban_seconds(&config), 1);
    }
}
