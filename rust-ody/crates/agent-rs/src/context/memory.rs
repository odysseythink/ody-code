use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use kosong_rs::message::{ContentPart, Message, Role, ToolCall};

use crate::context::projector::{drop_orphan_tool_results, project};
use crate::context::tokens::estimate_tokens_for_message;
use crate::context::types::{AgentContextData, ContextAgent, ContextMessage, PromptOrigin};
use crate::records::nested::{
    CompactionResult, ExecutableToolOutput, ExecutableToolResult, LoopRecordedEvent,
};
use crate::records::AgentRecord;

pub const TOOL_ERROR_STATUS: &str = "<system>ERROR: Tool execution failed.</system>";
pub const TOOL_EMPTY_STATUS: &str = "<system>Tool output is empty.</system>";
pub const TOOL_EMPTY_ERROR_STATUS: &str =
    "<system>ERROR: Tool execution failed. Tool output is empty.</system>";
pub const TOOL_OUTPUT_EMPTY_TEXT: &str = "Tool output is empty.";

pub struct ContextMemory {
    agent: Arc<dyn ContextAgent>,
    history: Vec<ContextMessage>,
    token_count: i64,
    token_count_covered_message_count: usize,
    /// Maps an open step UUID to the index of the placeholder assistant message in `history`.
    open_steps: HashMap<String, usize>,
    pending_tool_result_ids: HashSet<String>,
    deferred_messages: Vec<ContextMessage>,
    last_assistant_at: Option<i64>,
}

impl ContextMemory {
    pub fn new(agent: Arc<dyn ContextAgent>) -> Self {
        Self {
            agent,
            history: Vec::new(),
            token_count: 0,
            token_count_covered_message_count: 0,
            open_steps: HashMap::new(),
            pending_tool_result_ids: HashSet::new(),
            deferred_messages: Vec::new(),
            last_assistant_at: None,
        }
    }

    pub fn append_user_message(&mut self, content: Vec<ContentPart>, origin: PromptOrigin) {
        self.append_message(ContextMessage {
            message: Message {
                role: Role::User,
                name: None,
                content,
                tool_calls: vec![],
                tool_call_id: None,
                partial: None,
            },
            origin: Some(origin),
            is_error: None,
        });
    }

    pub fn append_system_reminder(&mut self, content: &str, origin: PromptOrigin) {
        let text = format!("<system-reminder>\n{}\n</system-reminder>", content);
        self.append_message(ContextMessage {
            message: Message::user_text(text),
            origin: Some(origin),
            is_error: None,
        });
    }

    pub fn clear(&mut self) {
        self.agent
            .record_log()
            .log_record(AgentRecord::ContextClear { time: None });
        self.history.clear();
        self.token_count = 0;
        self.token_count_covered_message_count = 0;
        self.open_steps.clear();
        self.pending_tool_result_ids.clear();
        self.deferred_messages.clear();
        self.last_assistant_at = None;
        self.agent.micro_compaction().reset(0);
        self.agent.injection().on_context_clear();
        self.agent.status_emitter().emit_status_updated();
    }

    pub fn data(&self) -> AgentContextData {
        AgentContextData {
            history: self.history.clone(),
            token_count: self.token_count,
        }
    }

    pub fn token_count(&self) -> i64 {
        self.token_count
    }

    pub fn token_count_with_pending(&self) -> i64 {
        let pending = &self.history[self.token_count_covered_message_count..];
        self.token_count
            + pending
                .iter()
                .map(|m| estimate_tokens_for_message(&m.message))
                .sum::<i64>()
    }

    pub fn token_count_covered_message_count(&self) -> usize {
        self.token_count_covered_message_count
    }

    pub fn history(&self) -> &[ContextMessage] {
        &self.history
    }

    pub fn messages(&self) -> Vec<Message> {
        let compacted = self.agent.micro_compaction().compact(&self.history);
        drop_orphan_tool_results(project(&compacted))
    }

    pub fn append_message(&mut self, message: ContextMessage) {
        self.agent
            .record_log()
            .log_record(AgentRecord::ContextAppendMessage {
                time: None,
                message: message.clone(),
            });
        if self.has_open_tool_exchange() {
            self.deferred_messages.push(message);
            return;
        }
        self.push_history(vec![message]);
    }

    pub fn undo(&mut self, count: i64) {
        if count <= 0 {
            return;
        }
        if self.history.is_empty() {
            return;
        }

        self.agent
            .record_log()
            .log_record(AgentRecord::ContextUndo { time: None, count });

        let target = count as usize;
        let mut removed_user_count = 0usize;
        let mut removed_messages: Vec<ContextMessage> = Vec::new();
        let mut stopped_at_boundary = false;

        for i in (0..self.history.len()).rev() {
            let message = &self.history[i];
            if message.origin.as_ref() == Some(&PromptOrigin::CompactionSummary) {
                stopped_at_boundary = true;
                break;
            }
            if matches!(message.origin, Some(PromptOrigin::Injection { .. })) {
                continue;
            }

            let message = self.history.remove(i);
            removed_messages.push(message.clone());
            self.agent.injection().on_context_message_removed(i);

            if i < self.token_count_covered_message_count {
                self.token_count_covered_message_count -= 1;
                self.token_count -= estimate_tokens_for_message(&message.message);
            }

            if is_real_user_prompt(&message) {
                removed_user_count += 1;
                if removed_user_count >= target {
                    break;
                }
            }
        }

        self.agent
            .replay_builder()
            .remove_last_messages(&removed_messages);
        self.open_steps.clear();
        self.pending_tool_result_ids.clear();
        self.deferred_messages.clear();
        self.agent.micro_compaction().reset(self.history.len());
        self.agent.status_emitter().emit_status_updated();

        let restoring = self.agent.record_log().restoring_time().is_some();
        if !restoring && (stopped_at_boundary || removed_user_count < target) {
            panic!("Nothing to undo in the active context.");
        }
    }

    pub fn apply_compaction(&mut self, summary: CompactionResult) {
        self.agent
            .record_log()
            .log_record(AgentRecord::ContextApplyCompaction {
                time: None,
                result: summary.clone(),
            });

        let compacted_count = summary.compacted_count as usize;
        let summary_message = ContextMessage {
            message: Message::assistant(
                vec![ContentPart::Text {
                    text: summary.summary.clone(),
                }],
                vec![],
            ),
            origin: Some(PromptOrigin::CompactionSummary),
            is_error: None,
        };

        let mut new_history = vec![summary_message];
        new_history.extend(self.history.iter().skip(compacted_count).cloned());
        self.history = new_history;

        self.open_steps.clear();
        self.flush_deferred_messages_if_tool_exchange_closed();
        self.token_count = summary.tokens_after;
        self.token_count_covered_message_count = self.history.len();
        self.agent.micro_compaction().reset(0);
        self.agent
            .injection()
            .on_context_compacted(summary.compacted_count as usize);
        self.agent.status_emitter().emit_status_updated();
    }

    pub fn append_loop_event(&mut self, event: LoopRecordedEvent) {
        self.agent
            .record_log()
            .log_record(AgentRecord::ContextAppendLoopEvent {
                time: None,
                event: event.clone(),
            });

        match event {
            LoopRecordedEvent::StepBegin { uuid, .. } => {
                let message = ContextMessage {
                    message: Message::assistant(vec![], vec![]),
                    origin: None,
                    is_error: None,
                };
                self.push_history(vec![message]);
                let idx = self.history.len() - 1;
                self.open_steps.insert(uuid, idx);
            }
            LoopRecordedEvent::StepEnd { uuid, usage, .. } => {
                let open_step_idx = self.open_steps.remove(&uuid);
                if let Some(ref usage) = usage {
                    let open_step_index = open_step_idx.unwrap_or(self.history.len());
                    self.token_count = usage.input_cache_read
                        + usage.input_cache_creation
                        + usage.input_other
                        + usage.output;
                    self.token_count_covered_message_count = open_step_index + 1;
                }
                self.flush_deferred_messages_if_tool_exchange_closed();
                self.agent
                    .context_switch_flusher()
                    .flush_deferred_context_switch();
            }
            LoopRecordedEvent::ContentPartEvent {
                step_uuid, part, ..
            } => {
                let idx = *self.open_steps.get(&step_uuid).unwrap_or_else(|| {
                    panic!("content.part for unknown step_uuid '{}'", step_uuid)
                });
                self.history[idx].message.content.push(part);
            }
            LoopRecordedEvent::ToolCallEvent {
                step_uuid,
                tool_call_id,
                name,
                args,
                ..
            } => {
                let idx = *self
                    .open_steps
                    .get(&step_uuid)
                    .unwrap_or_else(|| panic!("tool.call for unknown step_uuid '{}'", step_uuid));
                self.history[idx].message.tool_calls.push(ToolCall {
                    call_type: "function".into(),
                    id: tool_call_id.clone(),
                    name,
                    arguments: Some(args.to_string()),
                    extras: None,
                    stream_index: None,
                });
                self.pending_tool_result_ids.insert(tool_call_id);
            }
            LoopRecordedEvent::ToolResultEvent {
                tool_call_id,
                result,
                ..
            } => {
                let output = tool_result_output_for_model(&result);
                let message = tool_message(&tool_call_id, output);
                let is_error = match &result {
                    ExecutableToolResult::Success(r) => r.is_error.unwrap_or(false),
                    ExecutableToolResult::Error(r) => r.is_error,
                };
                self.push_history(vec![ContextMessage {
                    message,
                    origin: None,
                    is_error: Some(is_error),
                }]);
                self.pending_tool_result_ids.remove(&tool_call_id);
                self.flush_deferred_messages_if_tool_exchange_closed();
            }
        }
    }

    pub fn has_open_steps(&self) -> bool {
        !self.open_steps.is_empty()
    }

    pub fn reset_runtime_state(&mut self) {
        self.open_steps.clear();
        self.pending_tool_result_ids.clear();
        self.deferred_messages.clear();
    }

    pub fn last_assistant_at(&self) -> Option<i64> {
        self.last_assistant_at
    }

    fn has_open_tool_exchange(&self) -> bool {
        !self.pending_tool_result_ids.is_empty()
    }

    fn push_history(&mut self, messages: Vec<ContextMessage>) {
        let start = self.history.len();
        let count = messages.len();
        self.history.extend(messages);
        for offset in 0..count {
            let message = &self.history[start + offset];
            if message.message.role == Role::Assistant {
                self.last_assistant_at = self
                    .agent
                    .record_log()
                    .restoring_time()
                    .or_else(|| Some(self.agent.clock().now_ms()));
            }
            if let Some(PromptOrigin::BackgroundTask { .. }) = message.origin {
                self.agent
                    .background()
                    .mark_delivered_notification(message.origin.as_ref().unwrap());
            }
            self.agent.replay_builder().push_message(message);
        }
    }

    fn flush_deferred_messages_if_tool_exchange_closed(&mut self) {
        if !self.pending_tool_result_ids.is_empty() || self.deferred_messages.is_empty() {
            return;
        }
        let messages = std::mem::take(&mut self.deferred_messages);
        self.push_history(messages);
    }
}

fn tool_message(tool_call_id: &str, output: ExecutableToolOutput) -> Message {
    let content = match output {
        ExecutableToolOutput::Text(text) => vec![ContentPart::Text { text }],
        ExecutableToolOutput::Parts(parts) => parts,
    };
    Message {
        role: Role::Tool,
        name: None,
        content,
        tool_calls: vec![],
        tool_call_id: Some(tool_call_id.into()),
        partial: None,
    }
}

fn tool_result_output_for_model(result: &ExecutableToolResult) -> ExecutableToolOutput {
    let (output, is_error) = match result {
        ExecutableToolResult::Success(r) => (&r.output, r.is_error.unwrap_or(false)),
        ExecutableToolResult::Error(r) => (&r.output, r.is_error),
    };

    match output {
        ExecutableToolOutput::Text(text) => {
            if is_error {
                if text.is_empty() {
                    return ExecutableToolOutput::Text(TOOL_EMPTY_ERROR_STATUS.into());
                }
                if text.trim_start().starts_with("<system>ERROR:") {
                    return ExecutableToolOutput::Text(text.clone());
                }
                return ExecutableToolOutput::Text(format!("{}\n{}", TOOL_ERROR_STATUS, text));
            }
            if is_empty_output_text(text) {
                return ExecutableToolOutput::Text(TOOL_EMPTY_STATUS.into());
            }
            ExecutableToolOutput::Text(text.clone())
        }
        ExecutableToolOutput::Parts(parts) => {
            if parts.is_empty() {
                let status = if is_error {
                    TOOL_EMPTY_ERROR_STATUS
                } else {
                    TOOL_EMPTY_STATUS
                };
                return ExecutableToolOutput::Text(status.into());
            }
            if is_error {
                let mut content = vec![ContentPart::Text {
                    text: TOOL_ERROR_STATUS.into(),
                }];
                content.extend(parts.clone());
                return ExecutableToolOutput::Parts(content);
            }
            ExecutableToolOutput::Parts(parts.clone())
        }
    }
}

fn is_empty_output_text(output: &str) -> bool {
    output.is_empty() || output.trim() == TOOL_OUTPUT_EMPTY_TEXT
}

fn is_real_user_prompt(message: &ContextMessage) -> bool {
    if message.message.role != Role::User {
        return false;
    }
    match &message.origin {
        None | Some(PromptOrigin::User) => true,
        Some(PromptOrigin::SkillActivation { trigger, .. }) => trigger == "user-slash",
        _ => false,
    }
}
