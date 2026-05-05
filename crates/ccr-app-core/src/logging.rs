use chrono::{Local, NaiveDate};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

const RESPONSE_LOG_LIMIT: usize = 2048;
const UI_ERROR_LIMIT: usize = 300;

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
}
