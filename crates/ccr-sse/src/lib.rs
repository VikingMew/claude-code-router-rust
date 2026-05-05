pub mod continuation;
pub mod rewriter;

pub use continuation::invoke_continuation;
pub use rewriter::{SseRewriter, ToolCall};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
}

impl SseEvent {
    pub fn parse_data(&self) -> Option<Value> {
        if self.data == "[DONE]" {
            return None;
        }
        serde_json::from_str(&self.data).ok()
    }

    pub fn serialize(&self) -> String {
        let mut out = String::new();
        if let Some(ev) = &self.event {
            out.push_str(&format!("event: {ev}\n"));
        }
        out.push_str(&format!("data: {}\n\n", self.data));
        out
    }
}

/// Parse a raw SSE byte buffer into events.
/// Handles multiple events separated by \n\n.
pub fn parse_sse_chunk(chunk: &str) -> Vec<SseEvent> {
    chunk
        .split("\n\n")
        .filter(|block| !block.trim().is_empty())
        .filter_map(|block| {
            let mut event = None;
            let mut data = None;
            for line in block.lines() {
                if let Some(v) = line.strip_prefix("event: ") {
                    event = Some(v.to_string());
                } else if let Some(v) = line.strip_prefix("data: ") {
                    data = Some(v.to_string());
                }
            }
            data.map(|d| SseEvent { event, data: d })
        })
        .collect()
}

/// Stateful SSE parser that handles events split across multiple byte chunks.
#[derive(Default)]
pub struct SseParser {
    buf: String,
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed raw bytes, returns any complete events parsed.
    pub fn feed(&mut self, chunk: &str) -> Vec<SseEvent> {
        self.buf.push_str(chunk);
        let mut events = Vec::new();

        while let Some(pos) = self.buf.find("\n\n") {
            let block = self.buf[..pos].to_string();
            self.buf = self.buf[pos + 2..].to_string();

            if block.trim().is_empty() {
                continue;
            }
            let mut event = None;
            let mut data = None;
            for line in block.lines() {
                if let Some(v) = line.strip_prefix("event: ") {
                    event = Some(v.to_string());
                } else if let Some(v) = line.strip_prefix("data: ") {
                    data = Some(v.to_string());
                }
            }
            if let Some(d) = data {
                events.push(SseEvent { event, data: d });
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_event() {
        let events = parse_sse_chunk("event: ping\ndata: {\"type\":\"ping\"}\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.as_deref(), Some("ping"));
        assert_eq!(events[0].data, "{\"type\":\"ping\"}");
    }

    #[test]
    fn parse_multiple_events() {
        let raw = "event: a\ndata: 1\n\nevent: b\ndata: 2\n\n";
        let events = parse_sse_chunk(raw);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "1");
        assert_eq!(events[1].data, "2");
    }

    #[test]
    fn serialize_roundtrip() {
        let ev = SseEvent {
            event: Some("ping".into()),
            data: "{}".into(),
        };
        let s = ev.serialize();
        let parsed = parse_sse_chunk(&s);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].event.as_deref(), Some("ping"));
        assert_eq!(parsed[0].data, "{}");
    }

    #[test]
    fn parser_handles_split_chunks() {
        let mut p = SseParser::new();
        // First chunk is incomplete
        let e1 = p.feed("event: ping\ndata: {");
        assert!(e1.is_empty());
        // Second chunk completes the event
        let e2 = p.feed("}\n\n");
        assert_eq!(e2.len(), 1);
        assert_eq!(e2[0].data, "{}");
    }

    #[test]
    fn parser_handles_multiple_events_in_one_chunk() {
        let mut p = SseParser::new();
        let events = p.feed("event: a\ndata: 1\n\nevent: b\ndata: 2\n\n");
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn parser_retains_partial_across_feeds() {
        let mut p = SseParser::new();
        p.feed("event: x\ndata: hel");
        let events = p.feed("lo\n\n");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "hello");
    }
}
