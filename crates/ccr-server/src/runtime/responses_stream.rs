use crate::runtime::metrics::{epoch_secs, record_pending_attempt_ttft};
use actix_web::web;
use ccr_app_core::metrics::{RuntimeMetricsStore, UpstreamAttemptMetric};
use ccr_sse::SseParser;
use futures_util::StreamExt;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

pub(crate) fn stream_anthropic_as_responses_with_ttft(
    response: reqwest::Response,
    metrics: Arc<Mutex<RuntimeMetricsStore>>,
    mut pending_attempt_metric: Option<UpstreamAttemptMetric>,
    attempt_started_at: Option<Instant>,
) -> futures_util::stream::BoxStream<'static, Result<web::Bytes, actix_web::Error>> {
    let mut parser = SseParser::new();
    let mut response_id = "resp_ccr".to_string();
    let mut model = String::new();
    let mut block_types: HashMap<i64, String> = HashMap::new();
    let mut block_ids: HashMap<i64, String> = HashMap::new();

    response
        .bytes_stream()
        .map(move |chunk| {
            if chunk.is_ok() {
                record_pending_attempt_ttft(
                    &metrics,
                    pending_attempt_metric.take(),
                    attempt_started_at,
                    "stream_first_chunk",
                );
            }

            let bytes = chunk.map_err(actix_web::error::ErrorBadGateway)?;
            let text = String::from_utf8_lossy(&bytes);
            let mut out = String::new();
            for event in parser.feed(&text) {
                out.push_str(&responses_sse_from_anthropic_event(
                    &event,
                    &mut response_id,
                    &mut model,
                    &mut block_types,
                    &mut block_ids,
                ));
            }
            Ok(web::Bytes::from(out))
        })
        .boxed()
}

pub(crate) fn responses_sse_from_anthropic_event(
    event: &ccr_sse::SseEvent,
    response_id: &mut String,
    model: &mut String,
    block_types: &mut HashMap<i64, String>,
    block_ids: &mut HashMap<i64, String>,
) -> String {
    let Some(data) = event.parse_data() else {
        return String::new();
    };
    let event_type = data
        .get("type")
        .and_then(serde_json::Value::as_str)
        .or(event.event.as_deref())
        .unwrap_or_default();
    let mut out = String::new();

    match event_type {
        "message_start" => {
            if let Some(message) = data.get("message") {
                if let Some(id) = message.get("id").and_then(serde_json::Value::as_str) {
                    *response_id = id.to_string();
                }
                if let Some(message_model) =
                    message.get("model").and_then(serde_json::Value::as_str)
                {
                    *model = message_model.to_string();
                }
            }
            let response = responses_response_json(response_id, model, "in_progress");
            push_responses_event(
                &mut out,
                "response.created",
                serde_json::json!({
                    "type": "response.created",
                    "response": response
                }),
            );
            push_responses_event(
                &mut out,
                "response.in_progress",
                serde_json::json!({
                    "type": "response.in_progress",
                    "response": responses_response_json(response_id, model, "in_progress")
                }),
            );
        }
        "content_block_start" => {
            let index = data
                .get("index")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0);
            let block = data
                .get("content_block")
                .unwrap_or(&serde_json::Value::Null);
            let block_type = block
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            block_types.insert(index, block_type.to_string());
            let item_id = block
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("msg_{index}"));
            block_ids.insert(index, item_id.clone());

            if block_type == "tool_use" {
                push_responses_event(
                    &mut out,
                    "response.output_item.added",
                    serde_json::json!({
                        "type": "response.output_item.added",
                        "output_index": index,
                        "item": {
                            "id": item_id,
                            "type": "function_call",
                            "call_id": block.get("id").and_then(serde_json::Value::as_str).unwrap_or(""),
                            "name": block.get("name").and_then(serde_json::Value::as_str).unwrap_or(""),
                            "arguments": ""
                        }
                    }),
                );
            } else {
                push_responses_event(
                    &mut out,
                    "response.output_item.added",
                    serde_json::json!({
                        "type": "response.output_item.added",
                        "output_index": index,
                        "item": {
                            "id": item_id,
                            "type": "message",
                            "role": "assistant",
                            "content": []
                        }
                    }),
                );
                push_responses_event(
                    &mut out,
                    "response.content_part.added",
                    serde_json::json!({
                        "type": "response.content_part.added",
                        "item_id": item_id,
                        "output_index": index,
                        "content_index": 0,
                        "part": {"type": "output_text", "text": ""}
                    }),
                );
            }
        }
        "content_block_delta" => {
            let index = data
                .get("index")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0);
            let item_id = block_ids
                .get(&index)
                .cloned()
                .unwrap_or_else(|| format!("msg_{index}"));
            let delta = data.get("delta").unwrap_or(&serde_json::Value::Null);
            match delta.get("type").and_then(serde_json::Value::as_str) {
                Some("text_delta") => {
                    let text = delta
                        .get("text")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    push_responses_event(
                        &mut out,
                        "response.output_text.delta",
                        serde_json::json!({
                            "type": "response.output_text.delta",
                            "item_id": item_id,
                            "output_index": index,
                            "content_index": 0,
                            "delta": text
                        }),
                    );
                }
                Some("input_json_delta") => {
                    let text = delta
                        .get("partial_json")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    push_responses_event(
                        &mut out,
                        "response.function_call_arguments.delta",
                        serde_json::json!({
                            "type": "response.function_call_arguments.delta",
                            "item_id": item_id,
                            "output_index": index,
                            "delta": text
                        }),
                    );
                }
                _ => {}
            }
        }
        "content_block_stop" => {
            let index = data
                .get("index")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0);
            let item_id = block_ids
                .get(&index)
                .cloned()
                .unwrap_or_else(|| format!("msg_{index}"));
            if block_types
                .get(&index)
                .is_some_and(|kind| kind == "tool_use")
            {
                push_responses_event(
                    &mut out,
                    "response.function_call_arguments.done",
                    serde_json::json!({
                        "type": "response.function_call_arguments.done",
                        "item_id": item_id,
                        "output_index": index,
                        "arguments": ""
                    }),
                );
            } else {
                push_responses_event(
                    &mut out,
                    "response.output_text.done",
                    serde_json::json!({
                        "type": "response.output_text.done",
                        "item_id": item_id,
                        "output_index": index,
                        "content_index": 0,
                        "text": ""
                    }),
                );
                push_responses_event(
                    &mut out,
                    "response.content_part.done",
                    serde_json::json!({
                        "type": "response.content_part.done",
                        "item_id": item_id,
                        "output_index": index,
                        "content_index": 0,
                        "part": {"type": "output_text", "text": ""}
                    }),
                );
            }
            push_responses_event(
                &mut out,
                "response.output_item.done",
                serde_json::json!({
                    "type": "response.output_item.done",
                    "output_index": index,
                    "item": {"id": item_id}
                }),
            );
        }
        "message_stop" => {
            push_responses_event(
                &mut out,
                "response.completed",
                serde_json::json!({
                    "type": "response.completed",
                    "response": responses_response_json(response_id, model, "completed")
                }),
            );
        }
        "error" => {
            push_responses_event(
                &mut out,
                "error",
                serde_json::json!({
                    "type": "error",
                    "error": data.get("error").cloned().unwrap_or(data)
                }),
            );
        }
        _ => {}
    }

    out
}

fn responses_response_json(response_id: &str, model: &str, status: &str) -> serde_json::Value {
    serde_json::json!({
        "id": response_id,
        "object": "response",
        "created_at": epoch_secs(SystemTime::now()),
        "status": status,
        "model": model,
        "output": []
    })
}

fn push_responses_event(out: &mut String, event: &str, data: serde_json::Value) {
    out.push_str(
        &ccr_sse::SseEvent {
            event: Some(event.to_string()),
            data: data.to_string(),
        }
        .serialize(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_anthropic_text_delta_to_responses_sse_delta() {
        let event = ccr_sse::SseEvent {
            event: Some("content_block_delta".to_string()),
            data: serde_json::json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": "hello"}
            })
            .to_string(),
        };
        let mut response_id = "resp_ccr".to_string();
        let mut model = String::new();
        let mut block_types = HashMap::new();
        let mut block_ids = HashMap::from([(0, "msg_0".to_string())]);

        let out = responses_sse_from_anthropic_event(
            &event,
            &mut response_id,
            &mut model,
            &mut block_types,
            &mut block_ids,
        );

        assert!(out.contains("event: response.output_text.delta"));
        assert!(out.contains("\"delta\":\"hello\""));
    }
}
