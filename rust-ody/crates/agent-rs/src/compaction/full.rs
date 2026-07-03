use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::anyhow;
use futures_util::future::{BoxFuture, FutureExt, Shared};
use kosong_rs::errors::{is_retryable_generate_error, ChatProviderError};
use kosong_rs::message::{ContentPart, Message, Role};
use kosong_rs::provider::{AbortSignal, FinishReason, Tool};
use kosong_rs::usage::TokenUsage;

use crate::agent_loop::retry::retry_backoff_delays;
use crate::compaction::budget::{
    apply_completion_budget, resolve_completion_budget, CompletionBudgetConfig,
    DEFAULT_UNKNOWN_OUTPUT_FALLBACK,
};
use crate::compaction::instruction::compaction_instruction;
use crate::compaction::render_messages::render_messages_to_text;
use crate::compaction::strategy::CompactionStrategy;
use crate::compaction::types::CompactionBeginData;
use crate::context::projector::drop_orphan_tool_results;
use crate::context::tokens::{
    estimate_tokens, estimate_tokens_for_messages, estimate_tokens_for_tools,
};
use crate::records::nested::{CompactionResult, CompactionSource, UsageRecordScope};
use crate::records::AgentRecord;
use crate::turn::error::OdyError;
use crate::turn::types::{AgentEvent, CompactedHistory, TurnAgent, TurnFullCompaction};

pub const MAX_COMPACTION_RETRY_ATTEMPTS: u32 = 5;

#[derive(Debug, thiserror::Error)]
#[error("Compaction response was truncated before producing a complete summary.")]
struct CompactionTruncatedError;

#[derive(Debug, Clone)]
enum CompactionTelemetryTrigger {
    Auto,
    Manual,
    ManualWithPrompt,
}

pub struct FullCompaction {
    inner: Arc<Inner>,
}

struct Inner {
    strategy: Arc<dyn CompactionStrategy>,
    state: Mutex<FullCompactionState>,
}

#[derive(Default)]
struct FullCompactionState {
    compaction_count_in_turn: i64,
    compacting: Option<CompactingState>,
    compacted_history: Vec<CompactedHistory>,
}

struct CompactingState {
    abort_controller: AbortSignal,
    _started_at: i64,
    _telemetry_trigger: CompactionTelemetryTrigger,
    promise: Shared<BoxFuture<'static, ()>>,
    blocked_by_turn: Arc<AtomicBool>,
}

impl Clone for FullCompaction {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl FullCompaction {
    pub fn new(strategy: Arc<dyn CompactionStrategy>) -> Self {
        Self {
            inner: Arc::new(Inner {
                strategy,
                state: Mutex::new(FullCompactionState::default()),
            }),
        }
    }

    fn with_state<T>(&self, f: impl FnOnce(&mut FullCompactionState) -> T) -> T {
        f(&mut self.inner.state.lock().unwrap())
    }

    fn is_compacting(&self) -> bool {
        self.with_state(|state| state.compacting.is_some())
    }

    fn compacted_history(&self) -> Vec<CompactedHistory> {
        self.with_state(|state| state.compacted_history.clone())
    }

    fn get_signal(&self) -> AbortSignal {
        self.with_state(|state| {
            state
                .compacting
                .as_ref()
                .map(|c| c.abort_controller.clone())
                .unwrap_or_else(AbortSignal::new)
        })
    }
}

#[async_trait::async_trait]
impl TurnFullCompaction for FullCompaction {
    fn reset_for_turn(&self, _agent: Arc<dyn TurnAgent>) {
        self.with_state(|state| state.compaction_count_in_turn = 0);
    }

    fn begin(&self, agent: Arc<dyn TurnAgent>, data: CompactionBeginData) {
        if self.is_compacting() {
            return;
        }
        if data.source == CompactionSource::Manual {
            self.with_state(|state| state.compaction_count_in_turn = 0);
        } else {
            self.with_state(|state| state.compaction_count_in_turn += 1);
        }
        if self.with_state(|state| state.compaction_count_in_turn)
            > self.inner.strategy.max_compaction_per_turn()
        {
            return;
        }

        let history: Vec<Message> = agent
            .context()
            .history()
            .iter()
            .map(|cm| cm.message.clone())
            .collect();
        let compacted_count = self
            .inner
            .strategy
            .compute_compact_count(&history, data.source);
        if compacted_count == 0 {
            return;
        }

        agent
            .records()
            .log_record(AgentRecord::FullCompactionBegin {
                time: None,
                data: data.clone(),
            });

        let abort_controller = AbortSignal::new();
        let started_at = now_ms();
        let _telemetry_trigger = telemetry_trigger(&data.source, data.instruction.as_deref());
        let trigger_str = format!("{:?}", data.source).to_lowercase();
        let instruction = data.instruction.clone();

        agent
            .event_emitter()
            .emit_event(AgentEvent::CompactionStarted {
                trigger: trigger_str,
                instruction,
            });

        let this = self.clone();
        let agent_for_worker = agent.clone();
        let fut: BoxFuture<'static, ()> = async move {
            this.compaction_worker(agent_for_worker, data, compacted_count)
                .await;
        }
        .boxed();
        let promise = fut.shared();

        self.with_state(|state| {
            state.compacting = Some(CompactingState {
                abort_controller,
                _started_at: started_at,
                _telemetry_trigger,
                promise,
                blocked_by_turn: Arc::new(AtomicBool::new(false)),
            });
        });
    }

    fn cancel(&self, agent: Arc<dyn TurnAgent>) {
        self.mark_canceled(agent);
    }

    async fn before_step(
        &self,
        agent: Arc<dyn TurnAgent>,
        signal: AbortSignal,
    ) -> Result<(), anyhow::Error> {
        self.check_auto_compaction(agent.clone(), true)?;
        if self
            .inner
            .strategy
            .should_block(agent.context().token_count_with_pending())
        {
            self.block(agent.clone(), signal).await;
        }
        Ok(())
    }

    async fn after_step(&self, agent: Arc<dyn TurnAgent>) {
        if self.inner.strategy.check_after_step() {
            let _ = self.check_auto_compaction(agent, false);
        }
    }

    async fn handle_overflow_error(
        &self,
        agent: Arc<dyn TurnAgent>,
        signal: AbortSignal,
        error: anyhow::Error,
    ) -> Result<(), anyhow::Error> {
        match self.begin_auto_compaction(agent.clone(), true) {
            Ok(()) if self.is_compacting() => {
                self.block(agent, signal).await;
                Ok(())
            }
            Ok(()) => Err(error),
            Err(e) => Err(e),
        }
    }

    fn compacted_history(&self) -> Vec<CompactedHistory> {
        self.compacted_history()
    }

    fn is_compacting(&self) -> bool {
        self.is_compacting()
    }

    async fn compact_checkpoint(
        &self,
        agent: Arc<dyn TurnAgent>,
        signal: AbortSignal,
    ) -> Result<(), anyhow::Error> {
        if self.is_compacting() {
            self.block(agent, signal).await;
            return Ok(());
        }
        match self.begin_auto_compaction(agent.clone(), false) {
            Ok(()) if self.is_compacting() => {
                self.block(agent, signal).await;
                Ok(())
            }
            Ok(()) => Ok(()),
            Err(e) => Err(e),
        }
    }
}

impl FullCompaction {
    fn check_auto_compaction(
        &self,
        agent: Arc<dyn TurnAgent>,
        throw_on_limit: bool,
    ) -> Result<(), anyhow::Error> {
        if self.is_compacting() {
            return Ok(());
        }
        if !self
            .inner
            .strategy
            .should_compact(agent.context().token_count_with_pending())
        {
            return Ok(());
        }
        self.begin_auto_compaction(agent, throw_on_limit)
    }

    fn begin_auto_compaction(
        &self,
        agent: Arc<dyn TurnAgent>,
        throw_on_limit: bool,
    ) -> Result<(), anyhow::Error> {
        if self.is_compacting() {
            return Ok(());
        }
        let max = self.inner.strategy.max_compaction_per_turn();
        let count = self.with_state(|state| state.compaction_count_in_turn);
        if count >= max {
            if throw_on_limit {
                return Err(anyhow!(OdyError::new(
                    "context_overflow",
                    "ContextOverflow",
                    format!("Compaction limit exceeded ({})", max),
                )
                .with_details(serde_json::json!({ "maxCompactions": max }))));
            }
            return Ok(());
        }
        self.begin(
            agent,
            CompactionBeginData {
                source: CompactionSource::Auto,
                instruction: None,
            },
        );
        Ok(())
    }

    pub async fn block(&self, agent: Arc<dyn TurnAgent>, signal: AbortSignal) {
        let promise = self.with_state(|state| state.compacting.as_ref().map(|c| c.promise.clone()));
        if let Some(promise) = promise {
            self.with_state(|state| {
                if let Some(ref mut c) = state.compacting {
                    c.blocked_by_turn.store(true, Ordering::SeqCst);
                }
            });
            agent
                .event_emitter()
                .emit_event(AgentEvent::CompactionBlocked { turn_id: 0 });
            let _ = signal.throw_if_aborted();
            promise.await;
        }
    }

    fn mark_canceled(&self, agent: Arc<dyn TurnAgent>) {
        let was_active = self.with_state(|state| {
            if state.compacting.is_none() {
                return false;
            }
            if let Some(ref c) = state.compacting {
                c.abort_controller.abort();
            }
            state.compacting = None;
            true
        });
        if was_active {
            agent
                .records()
                .log_record(AgentRecord::FullCompactionCancel { time: None });
            agent
                .event_emitter()
                .emit_event(AgentEvent::CompactionCancelled);
        }
    }

    async fn compaction_worker(
        &self,
        agent: Arc<dyn TurnAgent>,
        data: CompactionBeginData,
        initial_compact_count: usize,
    ) {
        let started_at = now_ms();
        let original_history = agent.context().history();
        let original_messages: Vec<Message> = original_history
            .iter()
            .map(|cm| cm.message.clone())
            .collect();
        let tokens_before = estimate_tokens_for_messages(&original_messages);
        let mut retry_count = 0u32;

        if let Err(error) = self.trigger_pre_compact(&agent, &data, tokens_before).await {
            self.handle_worker_error(agent, error, started_at, tokens_before, retry_count)
                .await;
            return;
        }

        let model = agent.config().model();
        let system_prompt = agent.config().system_prompt();
        let loop_tools: Vec<Tool> = agent
            .tools()
            .loop_tools()
            .iter()
            .map(|t| Tool {
                name: t.name().into(),
                description: t.description().into(),
                parameters: t.parameters(),
            })
            .collect();
        let capability = agent.config().model_capabilities();
        let reserved_context_size = agent
            .config()
            .loop_control()
            .and_then(|c| c.reserved_context_size);
        let mut budget = resolve_completion_budget(reserved_context_size);

        let _delays = retry_backoff_delays(MAX_COMPACTION_RETRY_ATTEMPTS);
        let summary: String;
        let mut compacted_count = initial_compact_count;
        let used_usage: Option<TokenUsage>;

        loop {
            let messages_to_compact: Vec<_> = original_history
                .iter()
                .take(compacted_count)
                .cloned()
                .collect();
            let projected = agent.context().project(&messages_to_compact);
            let mut messages = drop_orphan_tool_results(projected);
            messages.push(Message {
                role: Role::User,
                name: None,
                content: vec![ContentPart::Text {
                    text: compaction_instruction(data.instruction.as_deref().unwrap_or("")),
                }],
                tool_calls: vec![],
                tool_call_id: None,
                partial: None,
            });

            let estimated_input_tokens = estimate_tokens(&system_prompt)
                + estimate_tokens_for_messages(&messages)
                + estimate_tokens_for_tools(&loop_tools);
            let effective_budget = budget.clone().unwrap_or(CompletionBudgetConfig {
                hard_cap: None,
                fallback: Some(DEFAULT_UNKNOWN_OUTPUT_FALLBACK),
            });
            let provider = apply_completion_budget(
                agent.config().provider(),
                budget.as_ref(),
                &capability,
                Some(estimated_input_tokens),
            );

            match agent
                .llm_resolver()
                .generate_one_off(
                    provider,
                    system_prompt.clone(),
                    loop_tools.clone(),
                    messages.clone(),
                    self.get_signal(),
                )
                .await
            {
                Ok(result) => {
                    if result.finish_reason == Some(FinishReason::Truncated) {
                        if let Err(error) = self
                            .handle_generate_error(
                                &agent,
                                &messages_to_compact,
                                &mut compacted_count,
                                &mut budget,
                                &effective_budget,
                                &capability,
                                estimated_input_tokens,
                                anyhow!(CompactionTruncatedError),
                            )
                            .await
                        {
                            self.handle_worker_error(
                                agent,
                                error,
                                started_at,
                                tokens_before,
                                retry_count,
                            )
                            .await;
                            return;
                        }
                        retry_count = 0;
                        continue;
                    }
                    used_usage = Some(result.usage);
                    summary = result.text;
                    break;
                }
                Err(error) => {
                    if let Err(error) = self
                        .handle_generate_error(
                            &agent,
                            &messages_to_compact,
                            &mut compacted_count,
                            &mut budget,
                            &effective_budget,
                            &capability,
                            estimated_input_tokens,
                            error,
                        )
                        .await
                    {
                        self.handle_worker_error(
                            agent,
                            error,
                            started_at,
                            tokens_before,
                            retry_count,
                        )
                        .await;
                        return;
                    }
                    retry_count = 0;
                    continue;
                }
            }
        }

        let todo_list_text = Self::extract_todo_list(&summary);
        let final_summary = if let Some(todo_text) = todo_list_text {
            format!("{}\n\nTODO List\n{}", summary, todo_text)
        } else {
            summary
        };

        let tokens_after = estimate_tokens(&final_summary);
        if let Some(usage) = used_usage {
            agent.usage().record(&model, usage, UsageRecordScope::Turn);
        }

        let result = CompactionResult {
            summary: final_summary,
            compacted_count: compacted_count as i64,
            tokens_before,
            tokens_after,
        };

        agent.context().apply_compaction(result.clone());
        agent
            .records()
            .log_record(AgentRecord::ContextApplyCompaction {
                time: None,
                result: result.clone(),
            });
        agent
            .event_emitter()
            .emit_event(AgentEvent::CompactionCompleted { result });

        self.with_state(|state| {
            let history_text = render_messages_to_text(
                &agent
                    .context()
                    .history()
                    .iter()
                    .map(|cm| cm.message.clone())
                    .collect::<Vec<_>>(),
            );
            state.compacting = None;
            state
                .compacted_history
                .push(CompactedHistory { text: history_text });
        });
        agent
            .records()
            .log_record(AgentRecord::FullCompactionComplete { time: None });
    }

    async fn trigger_pre_compact(
        &self,
        agent: &Arc<dyn TurnAgent>,
        data: &CompactionBeginData,
        tokens_before: i64,
    ) -> Result<(), anyhow::Error> {
        if let Some(hooks) = agent.hooks() {
            let event = match data.source {
                CompactionSource::Manual => "pre_compact_manual",
                CompactionSource::Auto => "pre_compact_auto",
            };
            hooks
                .trigger(
                    event,
                    serde_json::json!({ "tokens": tokens_before }),
                    AbortSignal::new(),
                )
                .await?;
        }
        Ok(())
    }

    async fn handle_generate_error(
        &self,
        agent: &Arc<dyn TurnAgent>,
        _messages: &[crate::context::types::ContextMessage],
        compacted_count: &mut usize,
        _budget: &mut Option<CompletionBudgetConfig>,
        _effective_budget: &CompletionBudgetConfig,
        _capability: &kosong_rs::provider::ModelCapability,
        _estimated_input_tokens: i64,
        error: anyhow::Error,
    ) -> Result<(), anyhow::Error> {
        if let Some(chat_error) = error.downcast_ref::<ChatProviderError>() {
            if !is_retryable_generate_error(chat_error) {
                return Err(error);
            }
        }

        let history: Vec<Message> = agent
            .context()
            .history()
            .iter()
            .map(|cm| cm.message.clone())
            .collect();
        let reduced = self.inner.strategy.reduce_compact_on_overflow(&history);
        if reduced > *compacted_count {
            *compacted_count = reduced;
            return Ok(());
        }

        Err(error)
    }

    async fn handle_worker_error(
        &self,
        agent: Arc<dyn TurnAgent>,
        error: anyhow::Error,
        _started_at: i64,
        _tokens_before: i64,
        _retry_count: u32,
    ) {
        self.with_state(|state| {
            state.compacting = None;
        });
        agent.log().warn(
            "compaction_failed",
            serde_json::json!({ "error": format!("{}", error) }),
        );
    }

    fn extract_todo_list(summary: &str) -> Option<String> {
        let marker = "## All User Messages";
        let idx = summary.find(marker)?;
        let todo_section = &summary[idx + marker.len()..];
        let trimmed = todo_section.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn telemetry_trigger(
    source: &CompactionSource,
    instruction: Option<&str>,
) -> CompactionTelemetryTrigger {
    match source {
        CompactionSource::Manual => {
            if instruction.filter(|s| !s.is_empty()).is_some() {
                CompactionTelemetryTrigger::ManualWithPrompt
            } else {
                CompactionTelemetryTrigger::Manual
            }
        }
        CompactionSource::Auto => CompactionTelemetryTrigger::Auto,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compaction::strategy::DefaultCompactionStrategy;
    use crate::context::types::ContextMessage;
    use crate::turn::fixture_agent::FixtureAgent;
    use crate::turn::types::CompactGenerateResult;

    fn make_history() -> Vec<ContextMessage> {
        vec![
            ContextMessage {
                message: Message {
                    role: Role::User,
                    name: None,
                    content: vec![ContentPart::Text { text: "u1".into() }],
                    tool_calls: vec![],
                    tool_call_id: None,
                    partial: None,
                },
                origin: Some(crate::turn::types::USER_PROMPT_ORIGIN),
                is_error: None,
            },
            ContextMessage {
                message: Message {
                    role: Role::Assistant,
                    name: None,
                    content: vec![ContentPart::Text { text: "a1".into() }],
                    tool_calls: vec![],
                    tool_call_id: None,
                    partial: None,
                },
                origin: None,
                is_error: None,
            },
            ContextMessage {
                message: Message {
                    role: Role::User,
                    name: None,
                    content: vec![ContentPart::Text { text: "u2".into() }],
                    tool_calls: vec![],
                    tool_call_id: None,
                    partial: None,
                },
                origin: Some(crate::turn::types::USER_PROMPT_ORIGIN),
                is_error: None,
            },
            ContextMessage {
                message: Message {
                    role: Role::Assistant,
                    name: None,
                    content: vec![ContentPart::Text { text: "a2".into() }],
                    tool_calls: vec![],
                    tool_call_id: None,
                    partial: None,
                },
                origin: None,
                is_error: None,
            },
        ]
    }

    fn agent_with_history(history: Vec<ContextMessage>) -> Arc<FixtureAgent> {
        let agent = Arc::new(FixtureAgent::new(vec![], vec![]));
        *agent.history.lock().unwrap() = history;
        agent
    }

    fn message_text(message: &Message) -> String {
        message
            .content
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn full_compaction_summarizes_prefix_and_applies_result() {
        let agent = agent_with_history(make_history());
        agent
            .generate_one_off_responses
            .lock()
            .unwrap()
            .push(CompactGenerateResult {
                text: "summary".into(),
                finish_reason: Some(FinishReason::Completed),
                usage: TokenUsage::default(),
            });

        let strategy = Arc::new(DefaultCompactionStrategy::new(|| 100, None));
        let compaction = FullCompaction::new(strategy);
        compaction.begin(
            agent.clone(),
            CompactionBeginData {
                source: CompactionSource::Manual,
                instruction: None,
            },
        );

        // Trigger the worker by calling block (which awaits the shared promise)
        let signal = AbortSignal::new();
        compaction.block(agent.clone(), signal).await;

        let history = agent.history.lock().unwrap().clone();
        assert_eq!(history.len(), 3); // summary + u2 + a2
        assert_eq!(history[0].message.role, Role::Assistant);
        assert!(message_text(&history[0].message).contains("summary"));

        let records = agent.captures.lock().unwrap().records.clone();
        assert!(records
            .iter()
            .any(|r| matches!(r, AgentRecord::FullCompactionBegin { .. })));
        assert!(records
            .iter()
            .any(|r| matches!(r, AgentRecord::FullCompactionComplete { .. })));
        assert!(records
            .iter()
            .any(|r| matches!(r, AgentRecord::ContextApplyCompaction { .. })));

        let events = agent.captures.lock().unwrap().events.clone();
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::CompactionStarted { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::CompactionCompleted { .. })));
    }
}
