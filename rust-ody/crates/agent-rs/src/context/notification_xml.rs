use serde_json::{Map, Value};

/// 渲染后台/cron 通知 XML，与 TS `renderNotificationXml` 逐字节对齐。
pub fn render_notification_xml(data: &Map<String, Value>) -> String {
    let id = string_attr(data.get("id"), "unknown");
    let category = string_attr(data.get("category"), "unknown");
    let ty = string_attr(data.get("type"), "unknown");
    let source_kind = string_attr(data.get("source_kind"), "unknown");
    let source_id = string_attr(data.get("source_id"), "unknown");
    let agent_id = optional_string_attr(data.get("agent_id"));
    let title = as_str(data.get("title"));
    let severity = as_str(data.get("severity"));
    let body = as_str(data.get("body"));

    let agent_id_attr = agent_id
        .as_ref()
        .map(|s| format!(" agent_id=\"{}\"", s))
        .unwrap_or_default();

    let mut lines: Vec<String> = vec![format!(
        "<notification id=\"{}\" category=\"{}\" type=\"{}\" source_kind=\"{}\" source_id=\"{}\"{}>",
        id, category, ty, source_kind, source_id, agent_id_attr
    )];

    if !title.is_empty() {
        lines.push(format!("Title: {}", title));
    }
    if !severity.is_empty() {
        lines.push(format!("Severity: {}", severity));
    }
    if !body.is_empty() {
        lines.push(body.to_string());
    }

    if source_kind == "background_task" {
        if let Some(Value::String(tail_raw)) = data.get("tail_output") {
            if !tail_raw.is_empty() {
                let truncated = truncate_tail_output(tail_raw, 20, 3000);
                lines.push("<task-notification>".into());
                lines.push(truncated);
                lines.push("</task-notification>".into());
            }
        }
    }

    lines.push("</notification>".into());
    lines.join("\n")
}

fn truncate_tail_output(raw: &str, max_lines: usize, max_chars: usize) -> String {
    let all_lines: Vec<&str> = raw.split('\n').collect();
    let tail_lines = if all_lines.len() > max_lines {
        &all_lines[all_lines.len() - max_lines..]
    } else {
        &all_lines[..]
    };
    let mut result = tail_lines.join("\n");
    if result.chars().count() > max_chars {
        result = result
            .chars()
            .rev()
            .take(max_chars)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
    }
    result
}

fn string_attr(value: Option<&Value>, fallback: &str) -> String {
    match value {
        Some(Value::String(s)) if !s.is_empty() => escape_xml_attr(s),
        _ => fallback.into(),
    }
}

fn optional_string_attr(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(s)) if !s.is_empty() => Some(escape_xml_attr(s)),
        _ => None,
    }
}

fn as_str(value: Option<&Value>) -> &str {
    match value {
        Some(Value::String(s)) => s.as_str(),
        _ => "",
    }
}

fn escape_xml_attr(input: &str) -> String {
    input.replace('&', "&amp;").replace('"', "&quot;")
}
