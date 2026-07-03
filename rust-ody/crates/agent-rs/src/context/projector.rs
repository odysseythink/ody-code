use kosong_rs::message::{ContentPart, Message, Role};

use crate::context::types::{ContextMessage, PromptOrigin};

/// 将内部 history 转换为 provider 可见的消息序列。
/// 过滤 partial/空 assistant 占位，并合并相邻的真实 user 消息。
pub fn project(history: &[ContextMessage]) -> Vec<Message> {
    let usable: Vec<&ContextMessage> = history
        .iter()
        .filter(|message| {
            let m = &message.message;
            !m.partial.unwrap_or(false)
                && !(m.role == Role::Assistant && m.content.is_empty() && m.tool_calls.is_empty())
        })
        .collect();
    merge_adjacent_user_messages(&usable)
        .into_iter()
        .map(strip_context_metadata)
        .collect()
}

/// 丢弃没有前导 assistant tool-call 的孤儿 tool result。
/// 只应在完整历史或从 0 开始的前缀上调用。
pub fn drop_orphan_tool_results(messages: Vec<Message>) -> Vec<Message> {
    let mut seen_call_ids = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(messages.len());
    for message in messages {
        if message.role == Role::Assistant {
            for call in &message.tool_calls {
                seen_call_ids.insert(call.id.clone());
            }
            out.push(message);
            continue;
        }
        if message.role == Role::Tool {
            if let Some(ref tool_call_id) = message.tool_call_id {
                if !seen_call_ids.contains(tool_call_id) {
                    continue;
                }
            }
        }
        out.push(message);
    }
    out
}

fn merge_adjacent_user_messages(history: &[&ContextMessage]) -> Vec<ContextMessage> {
    let mut out: Vec<ContextMessage> = Vec::with_capacity(history.len());
    for message in history {
        if let Some(previous) = out.last_mut() {
            if can_merge_user_message(message) && can_merge_user_message(previous) {
                *previous = merge_two_user_messages(previous, message);
                continue;
            }
        }
        out.push((*message).clone());
    }
    out
}

fn can_merge_user_message(message: &ContextMessage) -> bool {
    message.message.role == Role::User && message.origin == Some(PromptOrigin::User)
}

fn merge_two_user_messages(a: &ContextMessage, b: &ContextMessage) -> ContextMessage {
    let a_text = extract_text_only(&a.message);
    let b_text = extract_text_only(&b.message);
    let non_text_parts: Vec<ContentPart> = a
        .message
        .content
        .iter()
        .chain(b.message.content.iter())
        .filter(|p| !matches!(p, ContentPart::Text { .. }))
        .cloned()
        .collect();
    let merged_text = ContentPart::Text {
        text: format!("{}\n\n{}", a_text, b_text),
    };
    let mut content = vec![merged_text];
    content.extend(non_text_parts);
    ContextMessage {
        message: Message {
            role: Role::User,
            name: None,
            content,
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        },
        origin: a.origin.clone(),
        is_error: None,
    }
}

fn extract_text_only(message: &Message) -> String {
    message
        .content
        .iter()
        .filter_map(|p| match p {
            ContentPart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn strip_context_metadata(message: ContextMessage) -> Message {
    message.message
}
