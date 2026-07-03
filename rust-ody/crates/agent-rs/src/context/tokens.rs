use kosong_rs::message::{ContentPart, Message, Role};

/// 字符启发式 token 估算。
/// - ASCII 字符：ceil(count / 4)
/// - 非 ASCII 字符：count
pub fn estimate_tokens(text: &str) -> i64 {
    let mut ascii_count = 0i64;
    let mut non_ascii_count = 0i64;
    for ch in text.chars() {
        if (ch as u32) <= 127 {
            ascii_count += 1;
        } else {
            non_ascii_count += 1;
        }
    }
    ((ascii_count + 3) / 4) + non_ascii_count
}

pub fn estimate_tokens_for_messages(messages: &[Message]) -> i64 {
    messages.iter().map(estimate_tokens_for_message).sum()
}

pub fn estimate_tokens_for_message(message: &Message) -> i64 {
    let mut total = estimate_tokens(role_token_text(message.role));
    total += estimate_tokens_for_content_parts(&message.content);
    for call in &message.tool_calls {
        total += estimate_tokens(&call.name);
        total += estimate_tokens(call.arguments.as_deref().unwrap_or("null"));
    }
    total
}

pub fn estimate_tokens_for_content_parts(parts: &[ContentPart]) -> i64 {
    parts.iter().map(estimate_tokens_for_content_part).sum()
}

pub fn estimate_tokens_for_content_part(part: &ContentPart) -> i64 {
    match part {
        ContentPart::Text { text } => estimate_tokens(text),
        ContentPart::Think { think, .. } => estimate_tokens(think),
        _ => 0,
    }
}

fn role_token_text(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

pub fn estimate_tokens_for_tools(tools: &[kosong_rs::provider::Tool]) -> i64 {
    tools
        .iter()
        .map(|t| {
            estimate_tokens(&t.name)
                + estimate_tokens(&t.description)
                + estimate_tokens(&serde_json::to_string(&t.parameters).unwrap_or_default())
        })
        .sum()
}
