use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

const RESPONSE_LOG_LIMIT: usize = 2048;
const UI_ERROR_LIMIT: usize = 300;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogQuery {
    pub target: Option<String>,
    pub event: Option<String>,
    pub provider: Option<String>,
    pub route: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedLogEvent {
    pub timestamp: String,
    pub target: String,
    pub event: Option<String>,
    pub fields: BTreeMap<String, String>,
    pub raw: String,
}

pub fn app_log_path() -> PathBuf {
    app_log_path_for_date(Local::now().date_naive())
}

pub fn app_log_path_for_date(date: NaiveDate) -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude-code-router")
        .join(app_log_file_name_for_date(date))
}

pub fn app_log_file_name_for_date(date: NaiveDate) -> String {
    format!("claude-code-router-{}.log", date.format("%Y-%m-%d"))
}

pub fn append_app_log(target: &str, event: &str, fields: &[(&str, String)]) {
    let line = format_app_log_line(target, event, fields);
    #[cfg(not(test))]
    {
        let _ = append_app_log_line(&line);
    }
    #[cfg(test)]
    {
        let _ = line;
    }
}

pub fn append_app_log_line(line: &str) -> std::io::Result<()> {
    append_app_log_line_to_path(&app_log_path(), line)
}

pub fn append_app_log_line_to_path(path: &Path, line: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{line}")?;
    Ok(())
}

pub fn format_app_log_line(target: &str, event: &str, fields: &[(&str, String)]) -> String {
    let timestamp = Local::now().to_rfc3339();
    let mut line = format!(
        "{timestamp} [{target}] event={}",
        quote_value(&sanitize_value(event))
    );
    for (key, value) in fields {
        line.push(' ');
        line.push_str(key);
        line.push('=');
        line.push_str(&quote_value(&sanitize_value(value)));
    }
    line
}

pub fn query_app_log(path: &Path, query: &LogQuery) -> std::io::Result<Vec<ParsedLogEvent>> {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    Ok(query_app_log_content(&content, query))
}

pub fn query_app_log_content(content: &str, query: &LogQuery) -> Vec<ParsedLogEvent> {
    let limit = query.limit.unwrap_or(200);
    content
        .lines()
        .filter_map(parse_app_log_line)
        .filter(|event| log_event_matches(event, query))
        .rev()
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

pub fn parse_app_log_line(line: &str) -> Option<ParsedLogEvent> {
    let raw = line.to_string();
    let (timestamp, rest) = line.split_once(' ')?;
    let rest = rest.strip_prefix('[')?;
    let (target, fields_text) = rest.split_once("] ")?;
    let mut fields = BTreeMap::new();
    for (key, value) in parse_key_values(fields_text) {
        fields.insert(key, value);
    }
    let event = fields.get("event").cloned();
    Some(ParsedLogEvent {
        timestamp: timestamp.to_string(),
        target: target.to_string(),
        event,
        fields,
        raw,
    })
}

fn log_event_matches(event: &ParsedLogEvent, query: &LogQuery) -> bool {
    if query.target.as_deref().is_some_and(|v| event.target != v) {
        return false;
    }
    if query
        .event
        .as_deref()
        .is_some_and(|v| event.event.as_deref() != Some(v))
    {
        return false;
    }
    if query
        .provider
        .as_deref()
        .is_some_and(|v| event.fields.get("provider").map(String::as_str) != Some(v))
    {
        return false;
    }
    if query
        .route
        .as_deref()
        .is_some_and(|v| event.fields.get("route").map(String::as_str) != Some(v))
    {
        return false;
    }
    true
}

fn parse_key_values(input: &str) -> Vec<(String, String)> {
    let mut values = Vec::new();
    let mut chars = input.chars().peekable();
    loop {
        while chars.peek().is_some_and(|ch| ch.is_whitespace()) {
            chars.next();
        }
        if chars.peek().is_none() {
            break;
        }
        let mut key = String::new();
        while let Some(ch) = chars.peek().copied() {
            if ch == '=' {
                chars.next();
                break;
            }
            if ch.is_whitespace() {
                break;
            }
            key.push(ch);
            chars.next();
        }
        if key.is_empty() {
            break;
        }
        let value = if chars.peek() == Some(&'"') {
            chars.next();
            parse_quoted_value(&mut chars)
        } else {
            let mut value = String::new();
            while let Some(ch) = chars.peek().copied() {
                if ch.is_whitespace() {
                    break;
                }
                value.push(ch);
                chars.next();
            }
            value
        };
        values.push((key, value));
    }
    values
}

fn parse_quoted_value<I>(chars: &mut std::iter::Peekable<I>) -> String
where
    I: Iterator<Item = char>,
{
    let mut value = String::new();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => break,
            '\\' => match chars.next() {
                Some('"') => value.push('"'),
                Some('\\') => value.push('\\'),
                Some('n') => value.push('\n'),
                Some('r') => value.push('\r'),
                Some('t') => value.push('\t'),
                Some(other) => value.push(other),
                None => break,
            },
            other => value.push(other),
        }
    }
    value
}

pub fn redact_headers(headers: &[(String, String)]) -> String {
    let values: Vec<String> = headers
        .iter()
        .map(|(name, value)| {
            let redacted = match name.to_ascii_lowercase().as_str() {
                "authorization" => {
                    if value.to_ascii_lowercase().starts_with("bearer ") {
                        "Bearer <redacted>".to_string()
                    } else {
                        "<redacted>".to_string()
                    }
                }
                "x-api-key" | "api-key" | "apikey" => "<redacted>".to_string(),
                _ => value.clone(),
            };
            format!("{name}: {redacted}")
        })
        .collect();
    values.join(", ")
}

pub fn response_log_summary(text: &str) -> String {
    truncate_for_log(text, RESPONSE_LOG_LIMIT)
}

pub fn ui_error_summary(text: &str) -> String {
    truncate_for_log(text, UI_ERROR_LIMIT)
}

pub fn truncate_for_log(text: &str, limit: usize) -> String {
    let sanitized = sanitize_value(text);
    if sanitized.chars().count() <= limit {
        return sanitized;
    }
    let mut truncated: String = sanitized.chars().take(limit).collect();
    truncated.push_str("...");
    truncated
}

fn sanitize_value(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '\n' | '\r' | '\t' => ' ',
            _ => ch,
        })
        .collect()
}

fn quote_value(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\u{08}' => quoted.push_str("\\b"),
            '\u{0c}' => quoted.push_str("\\f"),
            ch if ch.is_control() => {
                quoted.push_str(&format!("\\u{:04x}", ch as u32));
            }
            _ => quoted.push(ch),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn redact_headers_hides_auth_values() {
        let headers = vec![
            ("authorization".to_string(), "Bearer sk-secret".to_string()),
            ("x-api-key".to_string(), "secret".to_string()),
            ("content-type".to_string(), "application/json".to_string()),
        ];

        let summary = redact_headers(&headers);

        assert!(summary.contains("Bearer <redacted>"));
        assert!(summary.contains("x-api-key: <redacted>"));
        assert!(summary.contains("content-type: application/json"));
        assert!(!summary.contains("sk-secret"));
        assert!(!summary.contains("secret"));
    }

    #[test]
    fn truncate_for_log_limits_and_sanitizes_text() {
        let summary = truncate_for_log("abc\ndefgh", 5);

        assert_eq!(summary, "abc d...");
    }

    #[test]
    fn append_app_log_line_creates_parent_directory() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("nested").join("app.log");

        append_app_log_line_to_path(&path, "line").unwrap();

        assert_eq!(std::fs::read_to_string(path).unwrap(), "line\n");
    }

    #[test]
    fn format_app_log_line_is_single_line_and_quoted() {
        let line = format_app_log_line("ui", "run_all_clicked", &[("status", "a\nb".into())]);

        assert!(line.contains("[ui]"));
        assert!(line.contains("event=\"run_all_clicked\""));
        assert!(line.contains("status=\"a b\""));
        assert!(!line.contains('\n'));
    }

    #[test]
    fn format_app_log_line_preserves_chinese() {
        let line = format_app_log_line(
            "server",
            "message",
            &[("prompt", "这是一个什么项目？".into())],
        );

        assert!(line.contains("prompt=\"这是一个什么项目？\""));
        assert!(!line.contains("\\u8fd9"));
    }

    #[test]
    fn format_app_log_line_escapes_quotes_and_backslashes() {
        let line = format_app_log_line("server", "message", &[("value", "a \"b\" c\\d".into())]);

        assert!(line.contains("value=\"a \\\"b\\\" c\\\\d\""));
    }

    #[test]
    fn truncate_for_log_preserves_chinese_boundaries() {
        let summary = truncate_for_log("你好世界", 3);

        assert_eq!(summary, "你好世...");
    }

    #[test]
    fn app_log_path_uses_daily_file_name() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();

        assert_eq!(
            app_log_file_name_for_date(date),
            "claude-code-router-2026-05-01.log"
        );
        assert!(app_log_path_for_date(date).ends_with("claude-code-router-2026-05-01.log"));
    }

    #[test]
    fn parse_app_log_line_extracts_fields() {
        let line = r#"2026-05-07T12:00:00+08:00 [upstream] event="result" provider="openai" route="openai,gpt-4o" status="200""#;

        let parsed = parse_app_log_line(line).unwrap();

        assert_eq!(parsed.timestamp, "2026-05-07T12:00:00+08:00");
        assert_eq!(parsed.target, "upstream");
        assert_eq!(parsed.event.as_deref(), Some("result"));
        assert_eq!(parsed.fields["provider"], "openai");
        assert_eq!(parsed.fields["route"], "openai,gpt-4o");
    }

    #[test]
    fn parse_app_log_line_unescapes_quotes() {
        let line = r#"2026-05-07T12:00:00+08:00 [server] event="message" value="a \"b\" c\\d""#;

        let parsed = parse_app_log_line(line).unwrap();

        assert_eq!(parsed.fields["value"], "a \"b\" c\\d");
    }

    #[test]
    fn query_app_log_content_filters_and_limits() {
        let content = concat!(
            "2026-05-07T12:00:00+08:00 [upstream] event=\"result\" provider=\"openai\" route=\"openai,a\"\n",
            "2026-05-07T12:00:01+08:00 [upstream] event=\"result\" provider=\"anthropic\" route=\"anthropic,b\"\n",
            "2026-05-07T12:00:02+08:00 [server] event=\"started\"\n",
        );
        let query = LogQuery {
            target: Some("upstream".to_string()),
            event: Some("result".to_string()),
            provider: Some("anthropic".to_string()),
            route: None,
            limit: Some(10),
        };

        let events = query_app_log_content(content, &query);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].fields["route"], "anthropic,b");
    }
}
