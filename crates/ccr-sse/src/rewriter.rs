use crate::SseEvent;
use serde_json::Value;

/// Intercepted tool call, ready for execution.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: Value,
}

#[derive(Debug, Default)]
enum State {
    #[default]
    Passthrough,
    BufferingTool {
        id: String,
        name: String,
        json_buf: String,
        index: u32,
    },
}

/// Stateful SSE rewriter that intercepts tool_use content blocks.
///
/// Feed events one at a time. Returns:
/// - `(events_to_forward, None)` for normal events
/// - `(events_to_forward, Some(tool_call))` when a tool call is complete
pub struct SseRewriter {
    state: State,
}

impl SseRewriter {
    pub fn new() -> Self {
        Self {
            state: State::Passthrough,
        }
    }

    pub fn feed(&mut self, event: SseEvent) -> (Vec<SseEvent>, Option<ToolCall>) {
        let data = match event.parse_data() {
            Some(d) => d,
            None => return (vec![event], None), // [DONE] or unparseable — pass through
        };

        let event_type = data.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match event_type {
            "content_block_start" => {
                let block = &data["content_block"];
                if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                    let id = block
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = block
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let index = data.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                    self.state = State::BufferingTool {
                        id,
                        name,
                        json_buf: String::new(),
                        index,
                    };
                    // Suppress this event — don't forward tool_use block start
                    return (vec![], None);
                }
                (vec![event], None)
            }

            "content_block_delta" => {
                if let State::BufferingTool {
                    ref mut json_buf, ..
                } = self.state
                {
                    let delta = &data["delta"];
                    if delta.get("type").and_then(|t| t.as_str()) == Some("input_json_delta") {
                        if let Some(partial) = delta.get("partial_json").and_then(|v| v.as_str()) {
                            json_buf.push_str(partial);
                        }
                    }
                    return (vec![], None); // suppress delta while buffering
                }
                (vec![event], None)
            }

            "content_block_stop" => {
                let index = data.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                if let State::BufferingTool {
                    ref id,
                    ref name,
                    ref json_buf,
                    index: buf_index,
                } = self.state
                {
                    if index == buf_index {
                        let input = serde_json::from_str(json_buf)
                            .unwrap_or(Value::Object(Default::default()));
                        let tool_call = ToolCall {
                            id: id.clone(),
                            name: name.clone(),
                            input,
                        };
                        self.state = State::Passthrough;
                        return (vec![], Some(tool_call));
                    }
                }
                (vec![event], None)
            }

            _ => (vec![event], None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SseEvent;
    use serde_json::json;

    fn ev(event: &str, data: serde_json::Value) -> SseEvent {
        SseEvent {
            event: Some(event.into()),
            data: data.to_string(),
        }
    }

    #[test]
    fn passthrough_non_tool_events() {
        let mut rw = SseRewriter::new();
        let e = ev(
            "message_start",
            json!({"type": "message_start", "message": {}}),
        );
        let (fwd, tool) = rw.feed(e);
        assert_eq!(fwd.len(), 1);
        assert!(tool.is_none());
    }

    #[test]
    fn intercepts_tool_call() {
        let mut rw = SseRewriter::new();

        let (fwd, tool) = rw.feed(ev("content_block_start", json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": {"type": "tool_use", "id": "tool_1", "name": "read_file", "input": {}}
        })));
        assert!(fwd.is_empty());
        assert!(tool.is_none());

        let (fwd, tool) = rw.feed(ev(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "input_json_delta", "partial_json": "{\"path\":\"main"}
            }),
        ));
        assert!(fwd.is_empty());
        assert!(tool.is_none());

        let (fwd, tool) = rw.feed(ev(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "input_json_delta", "partial_json": ".rs\"}"}
            }),
        ));
        assert!(fwd.is_empty());
        assert!(tool.is_none());

        let (fwd, tool) = rw.feed(ev(
            "content_block_stop",
            json!({
                "type": "content_block_stop",
                "index": 0
            }),
        ));
        assert!(fwd.is_empty());
        let tool = tool.unwrap();
        assert_eq!(tool.id, "tool_1");
        assert_eq!(tool.name, "read_file");
        assert_eq!(tool.input["path"], "main.rs");
    }

    #[test]
    fn non_tool_block_passes_through() {
        let mut rw = SseRewriter::new();
        let (fwd, tool) = rw.feed(ev(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "text", "text": ""}
            }),
        ));
        assert_eq!(fwd.len(), 1);
        assert!(tool.is_none());
    }
}
