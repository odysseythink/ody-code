use kosong_rs::message::{Message, Role};

use crate::context::tokens::estimate_tokens_for_message;
use crate::records::nested::CompactionSource;

use super::types::CompactionConfig;

pub trait CompactionStrategy: Send + Sync {
    fn should_compact(&self, used_size: i64) -> bool;
    fn should_block(&self, used_size: i64) -> bool;
    fn compute_compact_count(&self, messages: &[Message], source: CompactionSource) -> usize;
    fn reduce_compact_on_overflow(&self, messages: &[Message]) -> usize;
    fn check_after_step(&self) -> bool;
    fn max_compaction_per_turn(&self) -> i64;
}

pub struct DefaultCompactionStrategy {
    max_size_provider: Box<dyn Fn() -> i64 + Send + Sync>,
    config: CompactionConfig,
}

impl DefaultCompactionStrategy {
    pub fn new<F>(max_size_provider: F, config: Option<CompactionConfig>) -> Self
    where
        F: Fn() -> i64 + Send + Sync + 'static,
    {
        Self {
            max_size_provider: Box::new(max_size_provider),
            config: config.unwrap_or_default(),
        }
    }

    fn max_size(&self) -> i64 {
        (self.max_size_provider)()
    }

    fn should_use_reserved_context(&self, used_size: i64) -> bool {
        let reserved = self.config.reserved_context_size;
        reserved > 0 && reserved < self.max_size() && used_size + reserved >= self.max_size()
    }
}

impl CompactionStrategy for DefaultCompactionStrategy {
    fn should_compact(&self, used_size: i64) -> bool {
        if self.max_size() <= 0 {
            return false;
        }
        used_size as f64 >= self.max_size() as f64 * self.config.trigger_ratio
            || self.should_use_reserved_context(used_size)
    }

    fn should_block(&self, used_size: i64) -> bool {
        if self.max_size() <= 0 {
            return false;
        }
        used_size as f64 >= self.max_size() as f64 * self.config.block_ratio
            || self.should_use_reserved_context(used_size)
    }

    fn compute_compact_count(&self, messages: &[Message], source: CompactionSource) -> usize {
        if messages.is_empty() {
            return 0;
        }

        if source == CompactionSource::Manual {
            for i in (1..messages.len()).rev() {
                if can_split_after(messages, i - 1) {
                    return i;
                }
            }
            return 0;
        }

        let mut recent_messages = 1usize;
        let mut recent_user_messages = 0usize;
        let mut recent_size = 0i64;
        let mut best_n: Option<usize> = None;

        while recent_messages < messages.len() {
            let split_index = messages.len() - recent_messages - 1;
            let m2 = &messages[messages.len() - recent_messages];

            if m2.role == Role::User {
                recent_user_messages += 1;
            }
            recent_size += estimate_tokens_for_message(m2);

            if can_split_after(messages, split_index) {
                best_n = Some(split_index + 1);
            }

            let reaches_max = recent_messages >= self.config.max_recent_messages
                || recent_user_messages >= self.config.max_recent_user_messages
                || recent_size as f64 >= self.max_size() as f64 * self.config.max_recent_size_ratio;

            if reaches_max && best_n.is_some() {
                break;
            }
            recent_messages += 1;
        }

        best_n.unwrap_or(0)
    }

    fn reduce_compact_on_overflow(&self, messages: &[Message]) -> usize {
        let min_reduced_size =
            (self.max_size() as f64 * self.config.min_overflow_reduction_ratio).ceil() as i64;
        let min_reduced_size = min_reduced_size.max(1);
        let mut reduced_size = 0i64;
        let mut best_n: Option<usize> = None;

        for i in (1..messages.len().saturating_sub(1)).rev() {
            reduced_size += estimate_tokens_for_message(&messages[i + 1]);
            if can_split_after(messages, i) {
                best_n = Some(i + 1);
                if reduced_size >= min_reduced_size {
                    return i + 1;
                }
            }
        }

        best_n.unwrap_or(messages.len())
    }

    fn check_after_step(&self) -> bool {
        self.config.trigger_ratio != self.config.block_ratio
    }

    fn max_compaction_per_turn(&self) -> i64 {
        self.config.max_compaction_per_turn
    }
}

fn can_split_after(messages: &[Message], index: usize) -> bool {
    let Some(m) = messages.get(index) else {
        return false;
    };
    if m.role == Role::User {
        return false;
    }
    if m.role == Role::Assistant && !m.tool_calls.is_empty() {
        return false;
    }
    if messages.get(index + 1).map(|m| m.role) == Some(Role::Tool) {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use kosong_rs::message::{ContentPart, Message, Role, ToolCall};

    fn make_message(role: Role, text: &str) -> Message {
        Message {
            role,
            name: None,
            content: vec![ContentPart::Text { text: text.into() }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        }
    }

    fn make_asst_with_tool(text: &str, tool_call_id: &str) -> Message {
        Message {
            role: Role::Assistant,
            name: None,
            content: vec![ContentPart::Text { text: text.into() }],
            tool_calls: vec![ToolCall {
                call_type: "function".into(),
                id: tool_call_id.into(),
                name: "read".into(),
                arguments: Some("{}".into()),
                extras: None,
                stream_index: None,
            }],
            tool_call_id: None,
            partial: None,
        }
    }

    fn strategy_with_max(max: i64) -> DefaultCompactionStrategy {
        DefaultCompactionStrategy::new(move || max, None)
    }

    #[test]
    fn should_compact_when_crossing_trigger_ratio() {
        let s = strategy_with_max(100);
        assert!(!s.should_compact(80));
        assert!(s.should_compact(86));
    }

    #[test]
    fn manual_compaction_prefers_latest_safe_split() {
        let s = strategy_with_max(10_000);
        let messages = vec![
            make_message(Role::User, "u1"),
            make_message(Role::Assistant, "a1"),
            make_message(Role::User, "u2"),
            make_message(Role::Assistant, "a2"),
        ];
        // split after a1 -> compact first 2 messages
        assert_eq!(
            s.compute_compact_count(&messages, CompactionSource::Manual),
            2
        );
    }

    #[test]
    fn auto_preserves_at_least_one_recent_message() {
        let messages = vec![
            make_message(Role::User, "u1"),
            make_message(Role::Assistant, "a1"),
            make_message(Role::User, "u2"),
            make_message(Role::Assistant, "a2"),
            make_message(Role::User, "u3"),
        ];
        let s = strategy_with_max(10_000);
        // can split after a1 (index 1), preserving u2/a2/u3
        assert_eq!(
            s.compute_compact_count(&messages, CompactionSource::Auto),
            2
        );
    }

    #[test]
    fn cannot_split_after_asst_with_tool_calls() {
        let messages = vec![
            make_message(Role::User, "u1"),
            make_asst_with_tool("a1", "tc1"),
            make_message(Role::Tool, "r1"),
        ];
        let s = strategy_with_max(10_000);
        assert_eq!(
            s.compute_compact_count(&messages, CompactionSource::Auto),
            0
        );
    }

    #[test]
    fn reduce_compact_on_overflow_finds_shortest_safe_prefix() {
        let messages = vec![
            make_message(Role::User, "u1"),
            make_message(Role::Assistant, "a1"),
            make_message(Role::User, "u2"),
            make_message(Role::Assistant, "a2"),
        ];
        let s = strategy_with_max(10_000);
        // 从后往前，先找到 split after a1 -> return 2
        assert_eq!(s.reduce_compact_on_overflow(&messages), 2);
    }
}
