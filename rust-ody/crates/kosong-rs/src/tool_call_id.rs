use crate::message::{Message, ToolCall};

pub struct ToolCallIdPolicy {
    pub normalize: Box<dyn Fn(&str) -> String>,
    pub max_length: Option<usize>,
}

impl ToolCallIdPolicy {
    pub fn new(normalize: impl Fn(&str) -> String + 'static, max_length: Option<usize>) -> Self {
        Self {
            normalize: Box::new(normalize),
            max_length,
        }
    }
}

const EMPTY_TOOL_CALL_ID: &str = "tool_call";
const TOOL_CALL_ID_SAFE_CHARS: &str =
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-";

pub fn sanitize_tool_call_id(id: &str, max_length: Option<usize>) -> String {
    let sanitized: String = id
        .chars()
        .map(|c| {
            if TOOL_CALL_ID_SAFE_CHARS.contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    match max_length {
        Some(n) => sanitized.chars().take(n).collect(),
        None => sanitized,
    }
}

pub fn sanitize_openai_responses_call_id(id: &str, max_length: Option<usize>) -> String {
    let call_id = id.split('|').next().unwrap_or(id);
    sanitize_tool_call_id(call_id, max_length)
}

pub fn normalize_tool_call_ids_for_provider(
    messages: &[Message],
    policy: &ToolCallIdPolicy,
) -> Vec<Message> {
    let raw_ids = collect_tool_call_ids(messages);
    if raw_ids.is_empty() {
        return messages.to_vec();
    }

    let mapped_ids = build_tool_call_id_map(&raw_ids, policy);
    let mut changed = false;
    let normalized: Vec<Message> = messages
        .iter()
        .map(|message| {
            let mut message_changed = false;
            let tool_calls: Vec<ToolCall> = message
                .tool_calls
                .iter()
                .map(|tc| {
                    if let Some(mapped) = mapped_ids.get(&tc.id) {
                        if mapped != &tc.id {
                            message_changed = true;
                            return ToolCall {
                                id: mapped.clone(),
                                ..tc.clone()
                            };
                        }
                    }
                    tc.clone()
                })
                .collect();

            let mapped_tool_call_id = message
                .tool_call_id
                .as_ref()
                .and_then(|id| mapped_ids.get(id))
                .cloned()
                .or_else(|| message.tool_call_id.clone());
            if mapped_tool_call_id != message.tool_call_id {
                message_changed = true;
            }

            if !message_changed {
                return message.clone();
            }
            changed = true;
            Message {
                tool_calls,
                tool_call_id: mapped_tool_call_id,
                ..message.clone()
            }
        })
        .collect();

    if changed {
        normalized
    } else {
        messages.to_vec()
    }
}

fn collect_tool_call_ids(messages: &[Message]) -> Vec<String> {
    let mut ids = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for message in messages {
        for tc in &message.tool_calls {
            if seen.insert(tc.id.clone()) {
                ids.push(tc.id.clone());
            }
        }
        if let Some(id) = &message.tool_call_id {
            if seen.insert(id.clone()) {
                ids.push(id.clone());
            }
        }
    }
    ids
}

fn build_tool_call_id_map(
    raw_ids: &[String],
    policy: &ToolCallIdPolicy,
) -> std::collections::HashMap<String, String> {
    let mut mapped_ids = std::collections::HashMap::new();
    let mut used_ids = std::collections::HashSet::new();

    for raw_id in raw_ids {
        let normalized = (policy.normalize)(raw_id);
        if normalized == *raw_id && !normalized.is_empty() {
            mapped_ids.insert(raw_id.clone(), normalized.clone());
            used_ids.insert(normalized);
        }
    }

    for raw_id in raw_ids {
        if mapped_ids.contains_key(raw_id) {
            continue;
        }
        let normalized = (policy.normalize)(raw_id);
        let unique = make_unique_tool_call_id(normalized, &used_ids, policy.max_length);
        mapped_ids.insert(raw_id.clone(), unique.clone());
        used_ids.insert(unique);
    }

    mapped_ids
}

fn make_unique_tool_call_id(
    normalized: String,
    used_ids: &std::collections::HashSet<String>,
    max_length: Option<usize>,
) -> String {
    let base = if normalized.is_empty() {
        EMPTY_TOOL_CALL_ID
    } else {
        &normalized
    };
    let candidate = truncate_tool_call_id(base, max_length, "");
    if !used_ids.contains(&candidate) {
        return candidate;
    }
    for i in 2.. {
        let suffix = format!("_{}", i);
        let suffixed = truncate_tool_call_id(base, max_length, &suffix);
        if !used_ids.contains(&suffixed) {
            return suffixed;
        }
    }
    unreachable!()
}

fn truncate_tool_call_id(base: &str, max_length: Option<usize>, suffix: &str) -> String {
    match max_length {
        None => format!("{}{}", base, suffix),
        Some(n) => {
            let base_len = n.saturating_sub(suffix.len());
            if base_len == 0 {
                panic!(
                    "Tool call id maxLength {} is too small for suffix {}.",
                    n, suffix
                );
            }
            let end = base_len.min(base.len());
            format!("{}{}", &base[..end], suffix)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ContentPart, Message, Role, ToolCall};

    #[test]
    fn sanitize_replaces_unsafe_chars() {
        assert_eq!(sanitize_tool_call_id("a|b/c", None), "a_b_c");
    }

    #[test]
    fn sanitize_truncates_to_max_length() {
        assert_eq!(sanitize_tool_call_id("abcdefghij", Some(5)), "abcde");
    }

    #[test]
    fn openai_responses_splits_pipe() {
        assert_eq!(sanitize_openai_responses_call_id("id|extra", Some(4)), "id");
    }

    #[test]
    fn normalize_renames_conflicting_ids() {
        let messages = vec![
            Message {
                role: Role::Assistant,
                name: None,
                content: vec![],
                tool_calls: vec![ToolCall {
                    call_type: "function".into(),
                    id: "a|b".into(),
                    name: "read".into(),
                    arguments: None,
                    extras: None,
                    stream_index: None,
                }],
                tool_call_id: None,
                partial: None,
            },
            Message {
                role: Role::Tool,
                name: None,
                content: vec![ContentPart::Text { text: "ok".into() }],
                tool_calls: vec![],
                tool_call_id: Some("a|b".into()),
                partial: None,
            },
        ];
        let policy = ToolCallIdPolicy::new(|id| sanitize_tool_call_id(id, Some(3)), Some(3));
        let normalized = normalize_tool_call_ids_for_provider(&messages, &policy);
        assert_eq!(normalized[0].tool_calls[0].id, "a_b");
        assert_eq!(normalized[1].tool_call_id, Some("a_b".into()));
    }

    #[test]
    fn normalize_suffixes_duplicates() {
        let messages = vec![make_tool_call_msg("a|b"), make_tool_call_msg("a/b")];
        let policy = ToolCallIdPolicy::new(|id| sanitize_tool_call_id(id, Some(3)), Some(3));
        let normalized = normalize_tool_call_ids_for_provider(&messages, &policy);
        assert_eq!(normalized[0].tool_calls[0].id, "a_b");
        assert_eq!(normalized[1].tool_calls[0].id, "a_2");
    }

    fn make_tool_call_msg(id: &str) -> Message {
        Message {
            role: Role::Assistant,
            name: None,
            content: vec![],
            tool_calls: vec![ToolCall {
                call_type: "function".into(),
                id: id.into(),
                name: "read".into(),
                arguments: None,
                extras: None,
                stream_index: None,
            }],
            tool_call_id: None,
            partial: None,
        }
    }
}
