use kosong_rs::message::{ContentPart, Message, ToolCall};
use serde_json::Value;

pub fn render_messages_to_text(messages: &[Message]) -> String {
    messages
        .iter()
        .enumerate()
        .map(|(index, message)| render_message_to_text(message, index))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_message_to_text(message: &Message, index: usize) -> String {
    let mut header = vec![
        format!("message {}", index + 1),
        format!("role={:?}", message.role).to_lowercase(),
    ];
    if let Some(name) = &message.name {
        header.push(format!(
            "name={}",
            serde_json::to_string(name).unwrap_or_default()
        ));
    }
    if let Some(tool_call_id) = &message.tool_call_id {
        header.push(format!(
            "toolCallId={}",
            serde_json::to_string(tool_call_id).unwrap_or_default()
        ));
    }
    if message.partial == Some(true) {
        header.push("partial=true".into());
    }

    let mut lines = vec![format!("--- {} ---", header.join(" "))];
    if message.content.is_empty() {
        lines.push("[empty content]".into());
    } else {
        lines.extend(message.content.iter().map(render_content_part_to_text));
    }

    if !message.tool_calls.is_empty() {
        lines.push("tool calls:".into());
        for tool_call in &message.tool_calls {
            lines.push(render_tool_call_to_text(tool_call));
        }
    }

    lines.join("\n")
}

fn render_content_part_to_text(part: &ContentPart) -> String {
    match part {
        ContentPart::Text { text } => render_block("text", text),
        ContentPart::Think { think, .. } => render_block("think", think),
        ContentPart::ImageUrl { image_url } => {
            render_media_part("image_url", &image_url.url, image_url.id.as_deref())
        }
        ContentPart::AudioUrl { audio_url } => {
            render_media_part("audio_url", &audio_url.url, audio_url.id.as_deref())
        }
        ContentPart::VideoUrl { video_url } => {
            render_media_part("video_url", &video_url.url, video_url.id.as_deref())
        }
    }
}

fn render_tool_call_to_text(tool_call: &ToolCall) -> String {
    let mut lines = vec![
        format!("- {}: {}", tool_call.id, tool_call.name),
        render_block(
            "arguments",
            &render_tool_call_arguments(tool_call.arguments.as_deref()),
        ),
    ];
    if let Some(extras) = &tool_call.extras {
        lines.push(render_block("extras", &stringify_jsonish(extras)));
    }
    lines.join("\n")
}

fn render_tool_call_arguments(args: Option<&str>) -> String {
    match args {
        None => "null".into(),
        Some(args) => match serde_json::from_str::<Value>(args) {
            Ok(value) => stringify_jsonish(&value),
            Err(_) => args.into(),
        },
    }
}

fn render_media_part(kind: &str, url: &str, id: Option<&str>) -> String {
    match id {
        None => format!("{}: {}", kind, url),
        Some(id) => format!("{}: {} (id={})", kind, url, id),
    }
}

fn render_block(label: &str, value: &str) -> String {
    format!("{}:\n{}", label, indent_block(value))
}

fn indent_block(value: &str) -> String {
    if value.is_empty() {
        return "  ".into();
    }
    value
        .split('\n')
        .map(|line| format!("  {}", line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn stringify_jsonish(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kosong_rs::message::{ContentPart, Message, Role, ToolCall};

    #[test]
    fn renders_text_message() {
        let messages = vec![Message {
            role: Role::User,
            name: None,
            content: vec![ContentPart::Text {
                text: "hello".into(),
            }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        }];
        let text = render_messages_to_text(&messages);
        assert!(text.contains("message 1"));
        assert!(text.contains("role=user"));
        assert!(text.contains("text:"));
        assert!(text.contains("  hello"));
    }

    #[test]
    fn renders_tool_call() {
        let messages = vec![Message {
            role: Role::Assistant,
            name: None,
            content: vec![],
            tool_calls: vec![ToolCall {
                call_type: "function".into(),
                id: "call-1".into(),
                name: "read".into(),
                arguments: Some(r#"{"path":"/a"}"#.into()),
                extras: None,
                stream_index: None,
            }],
            tool_call_id: None,
            partial: None,
        }];
        let text = render_messages_to_text(&messages);
        assert!(text.contains("tool calls:"));
        assert!(text.contains("- call-1: read"));
        assert!(text.contains("arguments:"));
        assert!(text.contains("\"path\":"));
    }
}
