use std::sync::{Arc, Mutex};

use kosong_rs::message::{ContentPart, Message, Role};

use crate::context::tokens::estimate_tokens_for_content_parts;
use crate::context::types::ContextMessage;
use crate::flags;
use crate::records::AgentRecord;
use crate::turn::types::TurnAgent;

#[derive(Debug, Clone)]
pub struct MicroCompactionConfig {
    pub keep_recent_messages: usize,
    pub min_content_tokens: i64,
    pub cache_missed_threshold_ms: i64,
    pub truncated_marker: String,
    pub min_context_usage_ratio: f64,
}

impl Default for MicroCompactionConfig {
    fn default() -> Self {
        Self {
            keep_recent_messages: 20,
            min_content_tokens: 100,
            cache_missed_threshold_ms: 60 * 60 * 1000,
            truncated_marker: "[Old tool result content cleared]".into(),
            min_context_usage_ratio: 0.5,
        }
    }
}

pub struct MicroCompaction {
    pub cutoff: Mutex<usize>,
    pub config: MicroCompactionConfig,
}

impl MicroCompaction {
    pub fn new(config: MicroCompactionConfig) -> Self {
        Self {
            cutoff: Mutex::new(0),
            config,
        }
    }

    pub fn reset(&self, max_cutoff: usize) {
        let mut cutoff = self.cutoff.lock().unwrap();
        *cutoff = (*cutoff).min(max_cutoff);
    }

    pub fn apply(&self, cutoff: usize) {
        *self.cutoff.lock().unwrap() = cutoff;
    }

    pub fn detect(&self, agent: Arc<dyn TurnAgent>) {
        if !flags::enabled("micro-compaction") {
            return;
        }
        let config = &self.config;
        let last_assistant_at = agent.context().last_assistant_at_ms();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let cache_age_ms = last_assistant_at.map(|t| now - t);
        let cache_missed = cache_age_ms
            .map(|age| age >= config.cache_missed_threshold_ms)
            .unwrap_or(false);
        if !cache_missed {
            return;
        }
        let max_context_tokens = agent.config().model_capabilities().max_context_tokens;
        let context_tokens = agent.context().token_count_with_pending();
        let context_usage_ratio = if max_context_tokens > 0 {
            context_tokens as f64 / max_context_tokens as f64
        } else {
            1.0
        };
        if context_usage_ratio < config.min_context_usage_ratio {
            return;
        }
        let history = agent.context().history();
        let previous_cutoff = *self.cutoff.lock().unwrap();
        let next_cutoff = history.len().saturating_sub(config.keep_recent_messages);
        self.apply(next_cutoff);
        if previous_cutoff != next_cutoff {
            let effect = self.measure_effect(&history, next_cutoff);
            agent.telemetry().track(
                "micro_compaction_applied",
                serde_json::json!({
                    "keep_recent_messages": config.keep_recent_messages,
                    "truncated_tool_result_count": effect.truncated_tool_result_count,
                    "before_tokens": effect.before_tokens,
                    "after_tokens": effect.after_tokens,
                    "cutoff": next_cutoff,
                    "message_count": history.len(),
                }),
            );
            agent
                .records()
                .log_record(AgentRecord::MicroCompactionApply {
                    time: None,
                    cutoff: next_cutoff as i64,
                });
        }
    }

    pub fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage> {
        if !flags::enabled("micro-compaction") {
            return messages.to_vec();
        }
        let config = &self.config;
        let cutoff = *self.cutoff.lock().unwrap();
        messages
            .iter()
            .enumerate()
            .map(|(i, msg)| {
                if i < cutoff
                    && msg.message.role == Role::Tool
                    && msg.message.tool_call_id.is_some()
                    && estimate_tokens_for_content_parts(&msg.message.content)
                        >= config.min_content_tokens
                {
                    ContextMessage {
                        message: Message {
                            role: Role::Tool,
                            name: msg.message.name.clone(),
                            content: vec![ContentPart::Text {
                                text: config.truncated_marker.clone(),
                            }],
                            tool_calls: vec![],
                            tool_call_id: msg.message.tool_call_id.clone(),
                            partial: msg.message.partial,
                        },
                        origin: msg.origin.clone(),
                        is_error: msg.is_error,
                    }
                } else {
                    msg.clone()
                }
            })
            .collect()
    }

    fn measure_effect(&self, messages: &[ContextMessage], cutoff: usize) -> MeasureEffect {
        let config = &self.config;
        let marker_tokens = estimate_tokens_for_content_parts(&[ContentPart::Text {
            text: config.truncated_marker.clone(),
        }]);
        let mut truncated_tool_result_count = 0i64;
        let mut before_tokens = 0i64;
        let mut after_tokens = 0i64;
        for (i, msg) in messages.iter().enumerate() {
            if i >= cutoff {
                break;
            }
            if msg.message.role != Role::Tool || msg.message.tool_call_id.is_none() {
                continue;
            }
            let content_tokens = estimate_tokens_for_content_parts(&msg.message.content);
            if content_tokens < config.min_content_tokens {
                continue;
            }
            truncated_tool_result_count += 1;
            before_tokens += content_tokens;
            after_tokens += marker_tokens;
        }
        MeasureEffect {
            truncated_tool_result_count,
            before_tokens,
            after_tokens,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct MeasureEffect {
    truncated_tool_result_count: i64,
    before_tokens: i64,
    after_tokens: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::types::{ContextMessage, PromptOrigin};

    fn tool_msg(text: &str, id: &str) -> ContextMessage {
        ContextMessage {
            message: Message {
                role: Role::Tool,
                name: None,
                content: vec![ContentPart::Text { text: text.into() }],
                tool_calls: vec![],
                tool_call_id: Some(id.into()),
                partial: None,
            },
            origin: None,
            is_error: None,
        }
    }

    fn user_msg(text: &str) -> ContextMessage {
        ContextMessage {
            message: Message {
                role: Role::User,
                name: None,
                content: vec![ContentPart::Text { text: text.into() }],
                tool_calls: vec![],
                tool_call_id: None,
                partial: None,
            },
            origin: Some(PromptOrigin::User),
            is_error: None,
        }
    }

    #[test]
    fn compact_truncates_old_tool_results_above_cutoff() {
        std::env::set_var("ODY_CODE_EXPERIMENTAL_MICRO_COMPACTION", "true");
        let config = MicroCompactionConfig {
            keep_recent_messages: 2,
            min_content_tokens: 1,
            cache_missed_threshold_ms: 0,
            truncated_marker: "[truncated]".into(),
            min_context_usage_ratio: 0.0,
        };
        let mc = MicroCompaction::new(config);
        mc.apply(2);
        let messages = vec![
            tool_msg("long tool result one", "tc1"),
            user_msg("u1"),
            tool_msg("long tool result two", "tc2"),
        ];
        let compacted = mc.compact(&messages);
        assert_eq!(compacted.len(), 3);
        assert_eq!(
            compacted[0].message.content,
            vec![ContentPart::Text {
                text: "[truncated]".into()
            }]
        );
        assert_eq!(compacted[0].message.role, Role::Tool);
        assert_eq!(
            compacted[1].message.content,
            vec![ContentPart::Text { text: "u1".into() }]
        );
        assert_eq!(
            compacted[2].message.content,
            vec![ContentPart::Text {
                text: "long tool result two".into()
            }]
        );
    }

    #[test]
    fn apply_and_reset_bound_cutoff() {
        let config = MicroCompactionConfig::default();
        let mc = MicroCompaction::new(config);
        mc.apply(5);
        assert_eq!(*mc.cutoff.lock().unwrap(), 5);
        mc.reset(3);
        assert_eq!(*mc.cutoff.lock().unwrap(), 3);
        mc.reset(10);
        assert_eq!(*mc.cutoff.lock().unwrap(), 3);
    }
}
