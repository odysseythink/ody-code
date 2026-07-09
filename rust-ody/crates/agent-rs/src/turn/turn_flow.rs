use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use kosong_rs::message::{ContentPart, Message, Role};
use kosong_rs::provider::{AbortSignal, ModelCapability};
use kosong_rs::usage::TokenUsage;
use tokio::sync::oneshot;

use crate::agent_loop::errors::{is_abort_error, is_max_steps_exceeded_error};
use crate::agent_loop::events::{
    DefaultLoopEventDispatcher, LoopEvent, LoopEventDispatcher, LoopInterruptReason,
    LoopLiveOnlyEvent, LoopRecordedEvent,
};
use crate::agent_loop::llm::{LlmChatParams, LlmChatResponse};
use crate::agent_loop::run_turn::run_turn;
use crate::agent_loop::types::{
    AfterStepResult, AuthorizeToolExecutionResult, BeforeStepResult, ExecutableToolResult,
    FinalizeToolResultContext, LoopAfterStepContext, LoopHooks, LoopStepHookContext,
    LoopStoppedStepContext, LoopTurnStopReason, PrepareToolExecutionResult,
    ResolvedToolExecutionHookContext, RunTurnInput, ShouldContinueAfterStopResult,
    ToolExecutionHookContext, TurnResult,
};
use crate::context::types::{ContextMessage, PromptOrigin};
use crate::records::nested::{GoalStatus, UsageRecordScope};
use crate::records::AgentRecord;
use crate::turn::error::to_ody_error;
#[cfg(test)]
use crate::turn::fixture_agent::*;
use crate::turn::telemetry::{
    classify_api_error, current_turn_input_tokens, goal_failure_pause_reason, map_loop_event,
    summarize_turn_error, telemetry_tool_error_type, telemetry_tool_outcome, tool_input_record,
    tool_result_text,
};
use crate::turn::tool_dedup::{Dedup, ToolCallDeduplicator};
use crate::turn::types::{
    AgentEvent, GoalSnapshot, TurnAgent, TurnEndResult, TurnEndedEvent, TurnEndedReason,
    TurnErrorSummary,
};

#[derive(Clone)]
pub struct TurnFlow {
    agent: Arc<dyn TurnAgent>,
    inner: Arc<Mutex<TurnFlowInner>>,
    dedup: Arc<dyn Dedup>,
}

#[derive(Default)]
struct TurnFlowInner {
    steer_buffer: Vec<(Vec<ContentPart>, PromptOrigin)>,
    turn_id: i64,
    active_turn: Option<ActiveTurn>,
    result_rx: Option<oneshot::Receiver<TurnEndResult>>,
    current_step: u32,
    tool_call_started_at: HashMap<String, (String, i64)>,
    step_failure_by_turn: HashMap<i64, String>,
}

struct ActiveTurn {
    turn_id: i64,
    signal: AbortSignal,
}

struct PromptHookEndResult {
    event: TurnEndedEvent,
    blocked: bool,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn is_goal_over_budget(snapshot: &GoalSnapshot) -> bool {
    let token_reached = snapshot
        .budget_limits
        .token_budget
        .map_or(false, |b| snapshot.tokens_used >= b);
    let turn_reached = snapshot
        .budget_limits
        .turn_budget
        .map_or(false, |b| snapshot.turns_used >= b);
    let wall_reached = snapshot
        .budget_limits
        .wall_clock_budget_ms
        .map_or(false, |b| snapshot.wall_clock_ms >= b);
    token_reached || turn_reached || wall_reached
}

fn goal_continuation_origin() -> PromptOrigin {
    PromptOrigin::SystemTrigger {
        name: "goal_continuation".into(),
    }
}

fn goal_continuation_prompt() -> Vec<ContentPart> {
    const TEXT: &str = "Continue working toward the active goal. Keep the self-audit brief. Do not explore unrelated interpretations once the goal can be decided. If the objective is simple, already answered, impossible, unsafe, or contradictory, do not run another goal turn. Explain briefly if useful, then call UpdateGoal with `complete` or `blocked` in the same turn. Otherwise, weigh the objective and any completion criteria against the work done so far. Goal mode is iterative: do one coherent slice of work, then reassess. Call UpdateGoal with `complete` only when all required work is done, any stated validation has passed, and there is no useful next action. Do not mark complete after only producing a plan, summary, first pass, or partial result. If an external condition or required user input prevents progress, or the objective cannot be completed as stated, call UpdateGoal with `blocked`. Otherwise keep going — use the existing conversation context and your tools, and do not ask the user for input unless a real blocker prevents progress.";
    vec![ContentPart::Text { text: TEXT.into() }]
}

impl TurnFlow {
    pub fn new(agent: Arc<dyn TurnAgent>) -> Self {
        Self {
            agent,
            inner: Arc::new(Mutex::new(TurnFlowInner::default())),
            dedup: Arc::new(ToolCallDeduplicator::new()),
        }
    }

    pub fn prompt(&self, input: Vec<ContentPart>, origin: PromptOrigin) -> Option<i64> {
        self.agent.records().log_record(AgentRecord::TurnPrompt {
            time: None,
            input: input.clone(),
            origin: origin.clone(),
        });
        self.launch(input, origin)
    }

    pub fn steer(&self, input: Vec<ContentPart>, origin: PromptOrigin) -> Option<i64> {
        self.agent.records().log_record(AgentRecord::TurnSteer {
            time: None,
            input: input.clone(),
            origin: origin.clone(),
        });
        let mut inner = self.inner.lock().unwrap();
        if inner.active_turn.is_some() {
            inner.steer_buffer.push((input, origin));
            return None;
        }
        drop(inner);
        self.launch(input, origin)
    }

    pub fn cancel(&self, turn_id: Option<i64>, reason: Option<String>) {
        self.agent.records().log_record(AgentRecord::TurnCancel {
            time: None,
            turn_id,
        });
        let current = self.current_id();
        if turn_id.is_some() && turn_id != Some(current) {
            return;
        }
        let mut inner = self.inner.lock().unwrap();
        if let Some(active) = inner.active_turn.take() {
            active.signal.abort();
        }
        drop(inner);
        if let Some(host) = self.agent.subagent_host() {
            host.cancel_all(&reason.unwrap_or_else(|| "user cancellation".into()));
        }
    }

    pub fn restore_prompt(&self) {
        let mut inner = self.inner.lock().unwrap();
        if inner.active_turn.is_some() {
            return;
        }
        let turn_id = inner.turn_id;
        inner.active_turn = Some(ActiveTurn {
            turn_id,
            signal: AbortSignal::new(),
        });
    }

    pub fn restore_steer(&self, input: Vec<ContentPart>, origin: PromptOrigin) {
        let mut inner = self.inner.lock().unwrap();
        if inner.active_turn.is_some() {
            inner.steer_buffer.push((input, origin));
            return;
        }
        let turn_id = inner.turn_id;
        inner.active_turn = Some(ActiveTurn {
            turn_id,
            signal: AbortSignal::new(),
        });
    }

    pub fn current_id(&self) -> i64 {
        self.inner.lock().unwrap().turn_id
    }

    pub fn has_active_turn(&self) -> bool {
        self.inner.lock().unwrap().active_turn.is_some()
    }

    pub async fn wait_for_current_turn(
        &self,
        external_signal: Option<AbortSignal>,
    ) -> Result<TurnEndResult, anyhow::Error> {
        let rx = {
            let mut inner = self.inner.lock().unwrap();
            if inner.active_turn.is_none() && inner.result_rx.is_none() {
                return Err(anyhow::anyhow!("No active turn"));
            }
            inner.result_rx.take()
        };
        let rx = match rx {
            Some(rx) => rx,
            None => return Err(anyhow::anyhow!("Turn result already consumed")),
        };
        if let Some(sig) = external_signal {
            tokio::select! {
                biased;
                _ = wait_abort(&sig) => {
                    self.cancel(Some(self.current_id()), Some("external signal aborted".into()));
                    Err(anyhow::anyhow!("external signal aborted"))
                }
                res = rx => res.map_err(|_| anyhow::anyhow!("turn worker dropped")),
            }
        } else {
            rx.await.map_err(|_| anyhow::anyhow!("turn worker dropped"))
        }
    }

    fn launch(&self, input: Vec<ContentPart>, origin: PromptOrigin) -> Option<i64> {
        let mut inner = self.inner.lock().unwrap();
        if inner.active_turn.is_some() {
            self.agent
                .event_emitter()
                .emit_event(AgentEvent::Error(TurnErrorSummary {
                    code: "turn.agent_busy".into(),
                    name: "TurnAgentBusy".into(),
                    message: format!(
                        "Cannot launch a new turn while another turn (ID {}) is active",
                        inner.turn_id
                    ),
                    retryable: false,
                    details: Some(serde_json::json!({ "turnId": inner.turn_id })),
                }));
            return None;
        }
        let turn_id = inner.turn_id + 1;
        inner.turn_id = turn_id;
        let signal = AbortSignal::new();
        let signal_for_worker = signal.clone();
        let (tx, rx) = oneshot::channel();
        let flow = self.clone();
        tokio::spawn(async move {
            let result = flow
                .turn_worker(turn_id, input, origin, signal_for_worker)
                .await;
            let _ = tx.send(result);
        });
        inner.active_turn = Some(ActiveTurn { turn_id, signal });
        inner.result_rx = Some(rx);
        Some(turn_id)
    }

    async fn turn_worker(
        &self,
        first_turn_id: i64,
        input: Vec<ContentPart>,
        origin: PromptOrigin,
        signal: AbortSignal,
    ) -> TurnEndResult {
        let owns_active_turn = || {
            let inner = self.inner.lock().unwrap();
            let current_id = inner.turn_id;
            inner
                .active_turn
                .as_ref()
                .map(|a| a.turn_id == current_id)
                .unwrap_or(false)
        };

        let initial_goal_status = self
            .agent
            .goals()
            .and_then(|g| g.get_goal())
            .map(|s| s.status);

        let result = if self.agent.goal_runtime_enabled()
            && initial_goal_status == Some(GoalStatus::Active)
        {
            self.drive_goal(first_turn_id, input, origin, signal.clone())
                .await
        } else {
            let end = self
                .run_one_turn(first_turn_id, input, origin, signal.clone(), true)
                .await;
            let resumed_from_paused_or_blocked = initial_goal_status == Some(GoalStatus::Paused)
                || initial_goal_status == Some(GoalStatus::Blocked);
            let current_goal_status = self
                .agent
                .goals()
                .and_then(|g| g.get_goal())
                .map(|s| s.status);
            if self.agent.goal_runtime_enabled()
                && resumed_from_paused_or_blocked
                && current_goal_status == Some(GoalStatus::Active)
                && end.event.reason != TurnEndedReason::Cancelled
                && end.event.reason != TurnEndedReason::Failed
            {
                let next_id = {
                    let mut inner = self.inner.lock().unwrap();
                    inner.turn_id += 1;
                    inner.turn_id
                };
                self.drive_goal(
                    next_id,
                    goal_continuation_prompt(),
                    goal_continuation_origin(),
                    signal.clone(),
                )
                .await
            } else {
                end
            }
        };

        if owns_active_turn() {
            self.inner.lock().unwrap().active_turn = None;
        }
        result
    }

    async fn run_one_turn(
        &self,
        turn_id: i64,
        input: Vec<ContentPart>,
        origin: PromptOrigin,
        signal: AbortSignal,
        standalone: bool,
    ) -> TurnEndResult {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.current_step = 0;
        }

        let telemetry_mode = if self.agent.session_mode().is_active() {
            "plan"
        } else {
            "agent"
        };
        self.agent.telemetry().track(
            "turn_started",
            serde_json::json!({ "mode": telemetry_mode }),
        );
        self.agent
            .full_compaction()
            .reset_for_turn(self.agent.clone());
        self.agent.usage().begin_turn();
        self.agent
            .event_emitter()
            .emit_event(AgentEvent::TurnStarted {
                turn_id,
                origin: origin.clone(),
            });
        self.agent
            .context()
            .append_user_message(input.clone(), origin.clone());

        let started_at = now_ms();
        let mut error_event: Option<AgentEvent> = None;

        let (ended, stop_reason, blocked_by_user_prompt_hook) = match self
            .apply_user_prompt_hook(turn_id, &input, &origin, &signal)
            .await
        {
            Some(hook_end) => (hook_end.event, None, hook_end.blocked),
            None => match self.run_step_loop(turn_id, &signal).await {
                Ok(stop) => {
                    let reason = if stop == LoopTurnStopReason::Aborted {
                        TurnEndedReason::Cancelled
                    } else {
                        TurnEndedReason::Completed
                    };
                    (
                        TurnEndedEvent {
                            turn_id,
                            reason,
                            error: None,
                        },
                        Some(stop),
                        false,
                    )
                }
                Err(err) => {
                    if is_abort_error(&err) || signal.is_aborted() {
                        (
                            TurnEndedEvent {
                                turn_id,
                                reason: TurnEndedReason::Cancelled,
                                error: None,
                            },
                            None,
                            false,
                        )
                    } else {
                        let summary = summarize_turn_error(&err, turn_id);
                        if let Some(hooks) = self.agent.hooks() {
                            hooks.fire_and_forget_trigger(
                                "StopFailure",
                                serde_json::json!({
                                    "matcherValue": summary.name,
                                    "inputData": {
                                        "errorType": summary.name,
                                        "errorMessage": summary.message,
                                    },
                                }),
                            );
                        }
                        let error_summary = summary.clone();
                        let ended = TurnEndedEvent {
                            turn_id,
                            reason: TurnEndedReason::Failed,
                            error: Some(summary),
                        };
                        error_event = Some(AgentEvent::Error(error_summary.clone()));
                        if self.should_track_api_error(turn_id) {
                            let classification = classify_api_error(&err, &error_summary);
                            let duration = now_ms() - started_at;
                            let mut props = serde_json::json!({
                                "error_type": classification.error_type,
                                "model": self.agent.config().model(),
                                "retryable": error_summary.retryable,
                                "duration_ms": duration,
                            });
                            if let Some(sc) = classification.status_code {
                                props["status_code"] = serde_json::json!(sc);
                            }
                            if let Some(tokens) = current_turn_input_tokens(
                                self.agent.usage().current_turn_usage().as_ref(),
                            ) {
                                props["input_tokens"] = serde_json::json!(tokens);
                            }
                            self.agent.telemetry().track("api_error", props);
                        }
                        (ended, None, false)
                    }
                }
            },
        };

        self.agent.flush_deferred_context_switch();
        if self.current_id() == turn_id {
            self.agent.usage().end_turn();
        }
        self.agent
            .event_emitter()
            .emit_event(AgentEvent::TurnEnded(ended.clone()));
        if standalone && self.current_id() == turn_id {
            self.inner.lock().unwrap().active_turn = None;
        }
        if let Some(ev) = error_event {
            self.agent.event_emitter().emit_event(ev);
        }
        if ended.reason != TurnEndedReason::Completed {
            let at_step = self.inner.lock().unwrap().current_step;
            self.agent.telemetry().track(
                "turn_interrupted",
                serde_json::json!({ "mode": telemetry_mode, "at_step": at_step }),
            );
        }

        TurnEndResult {
            event: ended,
            stop_reason,
            blocked_by_user_prompt_hook,
        }
    }

    async fn apply_user_prompt_hook(
        &self,
        turn_id: i64,
        input: &[ContentPart],
        origin: &PromptOrigin,
        signal: &AbortSignal,
    ) -> Option<PromptHookEndResult> {
        if !matches!(origin, PromptOrigin::User) {
            return None;
        }
        signal.throw_if_aborted().ok()?;
        let hooks = self.agent.hooks()?;
        let results = hooks
            .trigger_user_prompt_submit(input.to_vec(), signal.clone())
            .await
            .ok()?;
        signal.throw_if_aborted().ok()?;

        if let Some(block) = results.iter().find(|r| r.blocked) {
            let text = block.text.clone().unwrap_or_default();
            self.agent.context().append_message(ContextMessage {
                message: Message {
                    role: Role::Assistant,
                    name: None,
                    content: vec![ContentPart::Text { text: text.clone() }],
                    tool_calls: vec![],
                    tool_call_id: None,
                    partial: None,
                },
                origin: Some(PromptOrigin::HookResult {
                    event: block.event.clone(),
                    blocked: Some(true),
                }),
                is_error: None,
            });
            self.agent
                .event_emitter()
                .emit_event(AgentEvent::HookResult {
                    turn_id,
                    hook_event: block.event.clone(),
                    content: block.message.clone().unwrap_or(serde_json::Value::Null),
                    blocked: Some(true),
                });
            return Some(PromptHookEndResult {
                event: TurnEndedEvent {
                    turn_id,
                    reason: TurnEndedReason::Completed,
                    error: None,
                },
                blocked: true,
            });
        }

        if let Some(text_result) = results.iter().find(|r| r.text.is_some()) {
            self.agent.context().append_user_message(
                vec![ContentPart::Text {
                    text: text_result.text.clone().unwrap(),
                }],
                PromptOrigin::HookResult {
                    event: text_result.event.clone(),
                    blocked: None,
                },
            );
            self.agent
                .event_emitter()
                .emit_event(AgentEvent::HookResult {
                    turn_id,
                    hook_event: text_result.event.clone(),
                    content: text_result
                        .message
                        .clone()
                        .unwrap_or(serde_json::Value::Null),
                    blocked: None,
                });
        }
        None
    }

    fn should_track_api_error(&self, turn_id: i64) -> bool {
        self.inner
            .lock()
            .unwrap()
            .step_failure_by_turn
            .get(&turn_id)
            .map(|r| r == "error")
            .unwrap_or(false)
    }

    async fn run_step_loop(
        &self,
        turn_id: i64,
        signal: &AbortSignal,
    ) -> Result<LoopTurnStopReason, anyhow::Error> {
        let stop_hook_used = Arc::new(Mutex::new(false));
        let stop_for_goal_budget = Arc::new(Mutex::new(false));
        let dedup = self.dedup.clone();

        if let Some(mcp) = self.agent.mcp() {
            mcp.wait_for_initial_load(signal.clone()).await?;
        }
        self.agent.injection().inject_goal().await;
        self.agent.llm_resolver().refresh_llm();

        loop {
            signal.throw_if_aborted()?;
            let model = self.agent.config().model();

            let result = run_turn(RunTurnInput {
                turn_id: turn_id.to_string(),
                signal: signal.clone(),
                llm: Box::new(SharedLlm(self.agent.llm_resolver().llm())),
                build_messages: {
                    let agent = self.agent.clone();
                    Arc::new(move || {
                        let agent = agent.clone();
                        Box::pin(async move { Ok(agent.context().messages()) })
                    })
                },
                dispatch_event: self.build_dispatch_event(turn_id),
                tools: Some(self.agent.tools().loop_tools()),
                hooks: Some(LoopHooks {
                    before_step: Some(Box::new(BeforeStepHookImpl {
                        flow: self.clone(),
                        dedup: dedup.clone(),
                    })),
                    after_step: Some(Box::new(AfterStepHookImpl {
                        flow: self.clone(),
                        dedup: dedup.clone(),
                        model: model.clone(),
                        stop_for_goal_budget: stop_for_goal_budget.clone(),
                    })),
                    prepare_tool_execution: Some(Box::new(PrepareToolExecutionHookImpl {
                        dedup: dedup.clone(),
                    })),
                    authorize_tool_execution: Some(Box::new(AuthorizeToolExecutionHookImpl {
                        agent: self.agent.clone(),
                    })),
                    finalize_tool_result: Some(Box::new(FinalizeToolResultHookImpl {
                        agent: self.agent.clone(),
                        dedup: dedup.clone(),
                    })),
                    should_continue_after_stop: Some(Box::new(ShouldContinueAfterStopHookImpl {
                        flow: self.clone(),
                        stop_hook_used: stop_hook_used.clone(),
                    })),
                }),
                max_steps: self
                    .agent
                    .config()
                    .loop_control()
                    .and_then(|c| c.max_steps_per_turn),
                max_retry_attempts: self
                    .agent
                    .config()
                    .loop_control()
                    .and_then(|c| c.max_retries_per_step),
                record_step_usage: Some({
                    let agent = self.agent.clone();
                    let stop = stop_for_goal_budget.clone();
                    Arc::new(move |usage: TokenUsage| {
                        let agent = agent.clone();
                        let stop = stop.clone();
                        Box::pin(async move {
                            if let Some(goal) = agent.goals() {
                                if let Some(snapshot) = goal.get_goal() {
                                    if snapshot.status == GoalStatus::Active {
                                        let updated = goal
                                            .record_token_usage(
                                                usage.grand_total(),
                                                agent.agent_type(),
                                                agent.agent_type(),
                                                "agent_step",
                                            )
                                            .await;
                                        let check = updated.as_ref().unwrap_or(&snapshot);
                                        if is_goal_over_budget(check) {
                                            *stop.lock().unwrap() = true;
                                        }
                                    }
                                }
                            }
                            Ok(None)
                        })
                    })
                }),
            })
            .await;

            match result {
                Ok(TurnResult { stop_reason, .. }) => return Ok(stop_reason),
                Err(err) => {
                    if is_context_overflow(&err) {
                        self.agent
                            .full_compaction()
                            .handle_overflow_error(self.agent.clone(), signal.clone(), err)
                            .await?;
                        continue;
                    }
                    if is_max_steps_exceeded_error(&err) {
                        self.agent.log().warn(
                            "turn hit max steps",
                            serde_json::json!({ "turnId": turn_id, "error": err.to_string() }),
                        );
                    }
                    return Err(err);
                }
            }
        }
    }

    fn flush_steer_buffer(&self) -> bool {
        let mut inner = self.inner.lock().unwrap();
        let buffer: Vec<_> = std::mem::take(&mut inner.steer_buffer);
        drop(inner);
        if buffer.is_empty() {
            return false;
        }
        for (content, origin) in buffer {
            self.agent.context().append_user_message(content, origin);
        }
        true
    }

    fn build_dispatch_event(&self, turn_id: i64) -> Arc<dyn LoopEventDispatcher> {
        let agent = self.agent.clone();
        let agent_for_live = agent.clone();
        let inner = self.inner.clone();
        Arc::new(DefaultLoopEventDispatcher::new(
            move |event: LoopRecordedEvent| {
                let agent = agent.clone();
                async move {
                    agent.context().append_loop_event(event);
                    Ok(())
                }
            },
            Some(Box::new(move |event: LoopEvent| {
                match &event {
                    LoopEvent::Recorded(LoopRecordedEvent::ToolCallEvent {
                        tool_call_id,
                        name,
                        ..
                    }) => {
                        let now = now_ms();
                        inner
                            .lock()
                            .unwrap()
                            .tool_call_started_at
                            .insert(tool_call_id.clone(), (name.clone(), now));
                    }
                    LoopEvent::Recorded(LoopRecordedEvent::ToolResultEvent {
                        tool_call_id,
                        result,
                        ..
                    }) => {
                        let maybe = inner
                            .lock()
                            .unwrap()
                            .tool_call_started_at
                            .remove(tool_call_id);
                        if let Some((name, started)) = maybe {
                            let duration = now_ms() - started;
                            let outcome = telemetry_tool_outcome(result);
                            let mut props = serde_json::json!({
                                "tool_name": name,
                                "outcome": outcome,
                                "duration_ms": duration,
                                "dup_type": "normal",
                            });
                            if outcome == "error" {
                                props["error_type"] =
                                    serde_json::json!(telemetry_tool_error_type(result));
                            }
                            agent_for_live.telemetry().track("tool_call", props);
                        }
                    }
                    LoopEvent::Live(LoopLiveOnlyEvent::TurnInterrupted {
                        reason: LoopInterruptReason::Error,
                        active_step: Some(_),
                        ..
                    }) => {
                        inner
                            .lock()
                            .unwrap()
                            .step_failure_by_turn
                            .insert(turn_id, "error".into());
                    }
                    _ => {}
                }
                if let Some(mapped) = map_loop_event(&event, turn_id) {
                    agent_for_live.event_emitter().emit_event(mapped);
                }
            })),
        ))
    }

    async fn drive_goal(
        &self,
        first_turn_id: i64,
        input: Vec<ContentPart>,
        origin: PromptOrigin,
        signal: AbortSignal,
    ) -> TurnEndResult {
        let mut turn_id = first_turn_id;
        let mut turn_input = input;
        let mut turn_origin = origin;

        loop {
            let goal_before = self.agent.goals().and_then(|g| g.get_goal());
            if let Some(ref snapshot) = goal_before {
                if snapshot.status == GoalStatus::Active && is_goal_over_budget(snapshot) {
                    if let Some(goal) = self.agent.goals() {
                        goal.mark_blocked("A configured budget was reached").await;
                    }
                    let ended = self
                        .end_goal_turn_without_model(turn_id, turn_input, turn_origin)
                        .await;
                    return TurnEndResult {
                        event: ended,
                        stop_reason: None,
                        blocked_by_user_prompt_hook: false,
                    };
                }
            }

            if let Some(goal) = self.agent.goals() {
                goal.increment_turn().await;
            }

            let end = self
                .run_one_turn(
                    turn_id,
                    turn_input.clone(),
                    turn_origin.clone(),
                    signal.clone(),
                    false,
                )
                .await;

            if end.event.reason == TurnEndedReason::Cancelled {
                if let Some(goal) = self.agent.goals() {
                    goal.pause_on_interrupt("Paused after interruption").await;
                }
                return end;
            }
            if end.event.reason == TurnEndedReason::Failed {
                if let Some(reason) = goal_failure_pause_reason(end.event.error.as_ref()) {
                    if let Some(goal) = self.agent.goals() {
                        goal.pause_active_goal("runtime", reason).await;
                    }
                    return end;
                }
                let reason = format!(
                    "Runtime error: {}",
                    end.event
                        .error
                        .as_ref()
                        .map(|e| e.message.clone())
                        .unwrap_or_else(|| "unknown".into())
                );
                if let Some(goal) = self.agent.goals() {
                    goal.mark_blocked(&reason).await;
                }
                return end;
            }
            if end.blocked_by_user_prompt_hook {
                if let Some(goal) = self.agent.goals() {
                    goal.mark_blocked("Blocked by UserPromptSubmit hook").await;
                }
                return end;
            }

            let goal = self.agent.goals().and_then(|g| g.get_goal());
            if goal.is_none() || goal.as_ref().unwrap().status != GoalStatus::Active {
                return end;
            }
            if let Some(ref snapshot) = goal {
                if is_goal_over_budget(snapshot) {
                    if let Some(goal) = self.agent.goals() {
                        goal.mark_blocked("A configured budget was reached").await;
                    }
                    return end;
                }
            }

            turn_id = {
                let mut inner = self.inner.lock().unwrap();
                inner.turn_id += 1;
                inner.turn_id
            };
            turn_input = goal_continuation_prompt();
            turn_origin = goal_continuation_origin();
        }
    }

    async fn end_goal_turn_without_model(
        &self,
        turn_id: i64,
        input: Vec<ContentPart>,
        origin: PromptOrigin,
    ) -> TurnEndedEvent {
        self.agent.usage().begin_turn();
        self.agent
            .event_emitter()
            .emit_event(AgentEvent::TurnStarted {
                turn_id,
                origin: origin.clone(),
            });
        self.agent.context().append_user_message(input, origin);
        let ended = TurnEndedEvent {
            turn_id,
            reason: TurnEndedReason::Completed,
            error: None,
        };
        self.agent.usage().end_turn();
        self.agent
            .event_emitter()
            .emit_event(AgentEvent::TurnEnded(ended.clone()));
        ended
    }
}

async fn wait_abort(signal: &AbortSignal) {
    while !signal.is_aborted() {
        tokio::task::yield_now().await;
    }
}

struct SharedLlm(Arc<dyn crate::agent_loop::llm::Llm>);

#[async_trait]
impl crate::agent_loop::llm::Llm for SharedLlm {
    fn system_prompt(&self) -> &str {
        self.0.system_prompt()
    }
    fn model_name(&self) -> &str {
        self.0.model_name()
    }
    fn capability(&self) -> Option<&ModelCapability> {
        self.0.capability()
    }
    fn is_retryable_error(&self, error: &anyhow::Error) -> bool {
        self.0.is_retryable_error(error)
    }
    async fn chat(&self, params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        self.0.chat(params).await
    }
}

struct BeforeStepHookImpl {
    flow: TurnFlow,
    dedup: Arc<dyn Dedup>,
}

#[async_trait]
impl crate::agent_loop::types::BeforeStepHook for BeforeStepHookImpl {
    async fn before_step(
        &self,
        ctx: LoopStepHookContext<'_>,
    ) -> Result<Option<BeforeStepResult>, anyhow::Error> {
        self.flow.flush_steer_buffer();
        self.flow
            .agent
            .micro_compaction()
            .detect(self.flow.agent.clone());
        self.flow
            .agent
            .full_compaction()
            .before_step(self.flow.agent.clone(), ctx.signal.clone())
            .await?;
        self.flow
            .agent
            .split_plan_checkpoint()
            .before_step(self.flow.agent.clone(), ctx.signal.clone())
            .await;
        self.flow
            .agent
            .normal_mode_task_checkpoint()
            .before_step(self.flow.agent.clone(), ctx.signal.clone())
            .await;
        self.flow.agent.injection().inject().await;
        self.dedup.begin_step();
        Ok(None)
    }
}

struct AfterStepHookImpl {
    flow: TurnFlow,
    dedup: Arc<dyn Dedup>,
    model: String,
    stop_for_goal_budget: Arc<Mutex<bool>>,
}

#[async_trait]
impl crate::agent_loop::types::AfterStepHook for AfterStepHookImpl {
    async fn after_step(
        &self,
        ctx: LoopAfterStepContext<'_>,
    ) -> Result<Option<AfterStepResult>, anyhow::Error> {
        self.flow
            .agent
            .usage()
            .record(&self.model, ctx.usage, UsageRecordScope::Turn);
        self.flow
            .agent
            .full_compaction()
            .after_step(self.flow.agent.clone())
            .await;
        self.dedup.end_step();
        let stop = *self.stop_for_goal_budget.lock().unwrap();
        Ok(if stop {
            Some(AfterStepResult {
                stop_turn: Some(true),
            })
        } else {
            None
        })
    }
}

struct ShouldContinueAfterStopHookImpl {
    flow: TurnFlow,
    stop_hook_used: Arc<Mutex<bool>>,
}

#[async_trait]
impl crate::agent_loop::types::ShouldContinueAfterStopHook for ShouldContinueAfterStopHookImpl {
    async fn should_continue_after_stop(
        &self,
        ctx: LoopStoppedStepContext<'_>,
    ) -> Result<Option<ShouldContinueAfterStopResult>, anyhow::Error> {
        if self.flow.flush_steer_buffer() {
            return Ok(Some(ShouldContinueAfterStopResult { continue_: true }));
        }
        ctx.signal.throw_if_aborted()?;
        let hooks = match self.flow.agent.hooks() {
            Some(h) => h,
            None => return Ok(Some(ShouldContinueAfterStopResult { continue_: false })),
        };
        let should_trigger = {
            let used = *self.stop_hook_used.lock().unwrap();
            !used
        };
        if should_trigger {
            if let Some(block) = hooks.trigger_stop_hook(ctx.signal.clone()).await? {
                *self.stop_hook_used.lock().unwrap() = true;
                self.flow.agent.context().append_user_message(
                    vec![ContentPart::Text {
                        text: block.reason.clone(),
                    }],
                    PromptOrigin::SystemTrigger {
                        name: "stop_hook".into(),
                    },
                );
                return Ok(Some(ShouldContinueAfterStopResult { continue_: true }));
            }
        }
        Ok(Some(ShouldContinueAfterStopResult { continue_: false }))
    }
}

struct PrepareToolExecutionHookImpl {
    dedup: Arc<dyn Dedup>,
}

#[async_trait]
impl crate::agent_loop::types::PrepareToolExecutionHook for PrepareToolExecutionHookImpl {
    async fn prepare_tool_execution(
        &self,
        ctx: ToolExecutionHookContext<'_>,
    ) -> Result<Option<PrepareToolExecutionResult>, anyhow::Error> {
        if let Some(synthetic) =
            self.dedup
                .check_same_step(&ctx.tool_call.id, &ctx.tool_call.name, &ctx.args)
        {
            return Ok(Some(PrepareToolExecutionResult {
                synthetic_result: Some(synthetic),
                ..Default::default()
            }));
        }
        Ok(None)
    }
}

struct AuthorizeToolExecutionHookImpl {
    agent: Arc<dyn TurnAgent>,
}

#[async_trait]
impl crate::agent_loop::types::AuthorizeToolExecutionHook for AuthorizeToolExecutionHookImpl {
    async fn authorize_tool_execution(
        &self,
        ctx: ResolvedToolExecutionHookContext<'_>,
    ) -> Result<Option<AuthorizeToolExecutionResult>, anyhow::Error> {
        self.agent.permission().before_tool_call(ctx).await
    }
}

struct FinalizeToolResultHookImpl {
    agent: Arc<dyn TurnAgent>,
    dedup: Arc<dyn Dedup>,
}

#[async_trait]
impl crate::agent_loop::types::FinalizeToolResultHook for FinalizeToolResultHookImpl {
    async fn finalize_tool_result(
        &self,
        ctx: FinalizeToolResultContext<'_>,
    ) -> Result<Option<ExecutableToolResult>, anyhow::Error> {
        let final_result = self
            .dedup
            .finalize_result(
                &ctx.tool_call.id,
                &ctx.tool_call.name,
                &ctx.args,
                ctx.result.clone(),
            )
            .await;
        let is_error = matches!(&final_result, ExecutableToolResult::Error(_))
            || matches!(&final_result, ExecutableToolResult::Success(s) if s.is_error == Some(true));
        let event = if is_error {
            "PostToolUseFailure"
        } else {
            "PostToolUse"
        };
        let output_text = tool_result_text(&final_result);
        if let Some(hooks) = self.agent.hooks() {
            hooks.fire_and_forget_trigger(
                event,
                serde_json::json!({
                    "matcherValue": ctx.tool_call.name,
                    "inputData": {
                        "toolName": ctx.tool_call.name,
                        "toolInput": tool_input_record(&ctx.args),
                        "toolCallId": ctx.tool_call.id,
                        "error": if is_error { serde_json::json!(output_text) } else { serde_json::Value::Null },
                        "toolOutput": if is_error { serde_json::Value::Null } else { serde_json::json!(output_text.chars().take(2000).collect::<String>()) },
                    },
                }),
            );
        }
        Ok(Some(final_result))
    }
}

fn is_context_overflow(err: &anyhow::Error) -> bool {
    if let Some(ody) = to_ody_error(err) {
        ody.code == "context_overflow"
    } else {
        err.to_string().to_lowercase().contains("context overflow")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::types::USER_PROMPT_ORIGIN;
    use crate::records::nested::GoalBudgetLimits;
    use crate::turn::types::HookResult;
    use kosong_rs::message::ContentPart;

    #[tokio::test]
    async fn prompt_allocates_monotonic_turn_id() {
        let agent = Arc::new(FixtureAgent::new(vec![], vec![]));
        let flow = TurnFlow::new(agent.clone());
        let id1 = flow.prompt(
            vec![ContentPart::Text { text: "hi".into() }],
            USER_PROMPT_ORIGIN,
        );
        assert!(id1.is_some());
        let _ = flow.wait_for_current_turn(None).await.unwrap();
        let id2 = flow.prompt(
            vec![ContentPart::Text {
                text: "again".into(),
            }],
            USER_PROMPT_ORIGIN,
        );
        assert!(id2.is_some());
        assert_eq!(id2.unwrap(), id1.unwrap() + 1);
    }

    #[tokio::test]
    async fn steer_buffers_while_active() {
        let agent = Arc::new(FixtureAgent::new(vec![], vec![]));
        let flow = TurnFlow::new(agent.clone());
        let _id = flow
            .prompt(
                vec![ContentPart::Text { text: "go".into() }],
                USER_PROMPT_ORIGIN,
            )
            .unwrap();
        assert!(flow.has_active_turn());
        let steer_id = flow.steer(
            vec![ContentPart::Text {
                text: "faster".into(),
            }],
            USER_PROMPT_ORIGIN,
        );
        assert_eq!(steer_id, None);
        let _ = flow.wait_for_current_turn(None).await.unwrap();
        let records = agent.captures.lock().unwrap().records.clone();
        assert!(records
            .iter()
            .any(|r| matches!(r, AgentRecord::TurnSteer { .. })));
    }

    #[tokio::test]
    async fn run_one_turn_emits_start_and_end() {
        let agent = Arc::new(FixtureAgent::new(vec![], vec![]));
        let flow = TurnFlow::new(agent.clone());
        let id = flow
            .prompt(
                vec![ContentPart::Text {
                    text: "hello".into(),
                }],
                USER_PROMPT_ORIGIN,
            )
            .unwrap();
        let result = flow.wait_for_current_turn(None).await.unwrap();
        assert_eq!(result.event.turn_id, id);
        assert_eq!(result.event.reason, TurnEndedReason::Completed);

        let events = agent.captures.lock().unwrap().events.clone();
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::TurnStarted { .. })));
        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::TurnEnded(TurnEndedEvent {
                reason: TurnEndedReason::Completed,
                ..
            })
        )));

        let ctx = agent.captures.lock().unwrap().context_inputs.clone();
        assert_eq!(ctx.len(), 1);
        assert_eq!(ctx[0].1, USER_PROMPT_ORIGIN);

        assert_eq!(agent.captures.lock().unwrap().begin_turn_count, 1);
        assert_eq!(agent.captures.lock().unwrap().end_turn_count, 1);
        assert_eq!(agent.captures.lock().unwrap().full_compaction_reset, 1);
    }

    #[tokio::test]
    async fn run_one_turn_blocks_on_user_prompt_hook() {
        let agent = Arc::new(FixtureAgent::new(vec![], vec![]));
        agent.hook_results.lock().unwrap().push(HookResult {
            event: "UserPromptSubmit".into(),
            text: Some("blocked by policy".into()),
            message: Some(serde_json::json!({ "reason": "policy" })),
            blocked: true,
        });
        let flow = TurnFlow::new(agent.clone());
        let id = flow
            .prompt(
                vec![ContentPart::Text { text: "x".into() }],
                USER_PROMPT_ORIGIN,
            )
            .unwrap();
        let result = flow.wait_for_current_turn(None).await.unwrap();
        assert_eq!(result.event.turn_id, id);
        assert_eq!(result.event.reason, TurnEndedReason::Completed);
        assert!(result.blocked_by_user_prompt_hook);

        let events = agent.captures.lock().unwrap().events.clone();
        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::HookResult {
                blocked: Some(true),
                ..
            }
        )));
    }

    #[tokio::test]
    async fn run_step_loop_emits_step_events() {
        let agent = Arc::new(FixtureAgent::new(vec![], vec![]));
        let flow = TurnFlow::new(agent.clone());
        let id = flow
            .prompt(
                vec![ContentPart::Text {
                    text: "step".into(),
                }],
                USER_PROMPT_ORIGIN,
            )
            .unwrap();
        let result = flow.wait_for_current_turn(None).await.unwrap();
        assert_eq!(result.stop_reason, Some(LoopTurnStopReason::EndTurn));
        let events = agent.captures.lock().unwrap().events.clone();
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::TurnStepStarted { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::TurnStepCompleted { .. })));
        assert!(events.iter().any(
            |e| matches!(e, AgentEvent::TurnEnded(TurnEndedEvent { turn_id, .. }) if *turn_id == id)
        ));
    }

    #[tokio::test]
    async fn steer_buffer_flushes_in_before_step() {
        let agent = Arc::new(FixtureAgent::new(vec![], vec![]));
        let flow = TurnFlow::new(agent.clone());
        flow.prompt(
            vec![ContentPart::Text { text: "go".into() }],
            USER_PROMPT_ORIGIN,
        )
        .unwrap();
        flow.steer(
            vec![ContentPart::Text {
                text: "faster".into(),
            }],
            USER_PROMPT_ORIGIN,
        );
        let _ = flow.wait_for_current_turn(None).await.unwrap();
        let ctx = agent.captures.lock().unwrap().context_inputs.clone();
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx[0].0, vec![ContentPart::Text { text: "go".into() }]);
        assert_eq!(
            ctx[1].0,
            vec![ContentPart::Text {
                text: "faster".into()
            }]
        );
    }

    #[tokio::test]
    async fn cancel_turn_returns_cancelled_and_clears_active_turn() {
        let mut agent = FixtureAgent::new(vec![], vec![]);
        agent.llm = Arc::new(PendingLlm);
        let agent = Arc::new(agent);
        let flow = TurnFlow::new(agent.clone());
        let id = flow
            .prompt(
                vec![ContentPart::Text {
                    text: "hang".into(),
                }],
                USER_PROMPT_ORIGIN,
            )
            .unwrap();
        tokio::task::yield_now().await;
        flow.cancel(Some(id), Some("user stop".into()));
        let result = flow.wait_for_current_turn(None).await.unwrap();
        assert_eq!(result.event.reason, TurnEndedReason::Cancelled);
        assert!(!flow.has_active_turn());
    }

    #[tokio::test]
    async fn goal_continuation_runs_until_budget_then_blocks() {
        let agent = FixtureAgent::new(vec![], vec![]);
        *agent.goal_status.lock().unwrap() = Some(GoalSnapshot {
            status: GoalStatus::Active,
            budget_limits: GoalBudgetLimits {
                token_budget: None,
                turn_budget: Some(2),
                wall_clock_budget_ms: None,
            },
            tokens_used: 0,
            turns_used: 0,
            wall_clock_ms: 0,
        });
        let agent = Arc::new(agent);
        let flow = TurnFlow::new(agent.clone());
        let id = flow
            .prompt(
                vec![ContentPart::Text {
                    text: "goal".into(),
                }],
                USER_PROMPT_ORIGIN,
            )
            .unwrap();
        let result = flow.wait_for_current_turn(None).await.unwrap();
        assert_eq!(result.event.turn_id, id + 1);

        let events = agent.captures.lock().unwrap().events.clone();
        let started_count = events
            .iter()
            .filter(|e| matches!(e, AgentEvent::TurnStarted { .. }))
            .count();
        assert_eq!(started_count, 2);
        assert_eq!(agent.captures.lock().unwrap().goal_increment_turn, 2);
        assert!(agent
            .captures
            .lock()
            .unwrap()
            .goal_mark_blocked
            .iter()
            .any(|r| r == "A configured budget was reached"));
        assert_eq!(
            agent.goal_status.lock().unwrap().as_ref().unwrap().status,
            GoalStatus::Blocked
        );
    }
}
