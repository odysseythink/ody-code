use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use kosong_rs::generate::StreamedMessage;
use kosong_rs::message::{ContentPart, Message, Role, ToolCall};
use kosong_rs::provider::{
    AbortSignal, ChatProvider, FinishReason, GenerateOptions, ModelCapability, ThinkingEffort, Tool,
};
use kosong_rs::usage::TokenUsage;
use serde::{Deserialize, Deserializer};
use serde_json::Value as JsonValue;

use crate::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse, LlmStreamTiming};
use crate::agent_loop::types::{
    AuthorizeToolExecutionResult, ExecutableTool, ExecutableToolErrorResult, ExecutableToolOutput,
    ExecutableToolResult, ExecutableToolSuccessResult, ResolvedToolExecutionHookContext,
    RunnableToolExecution, ToolExecution,
};
use crate::background::manager::BackgroundManager;
use crate::context::types::{ContextMessage, PromptOrigin};
use crate::cron::clock::ClockSources;
use crate::cron::manager::CronManager;
#[cfg(test)]
use crate::cron::manager::CronManagerOptions;
use crate::records::nested::{GoalBudgetLimits, GoalStatus, LoopRecordedEvent, UsageRecordScope};
use crate::records::AgentRecord;
use crate::turn::types::*;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FixtureResponse {
    #[serde(rename = "toolCalls", default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(
        rename = "finishReason",
        default,
        deserialize_with = "deserialize_finish_reason"
    )]
    pub finish_reason: Option<FinishReason>,
    #[serde(rename = "rawFinishReason", default)]
    pub raw_finish_reason: Option<String>,
    pub usage: TokenUsage,
}

fn deserialize_finish_reason<'de, D>(deserializer: D) -> Result<Option<FinishReason>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<String> = Option::deserialize(deserializer)?;
    match s {
        None => Ok(None),
        Some(s) => {
            let value = serde_json::Value::String(s);
            FinishReason::deserialize(value)
                .map(Some)
                .map_err(serde::de::Error::custom)
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct FixtureToolDef {
    pub name: String,
    pub description: String,
    pub parameters: JsonValue,
    pub result: FixtureToolResult,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FixtureToolResult {
    pub output: FixtureOutput,
    #[serde(rename = "isError", default)]
    pub is_error: Option<bool>,
    #[serde(rename = "stopTurn", default)]
    pub stop_turn: Option<bool>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum FixtureOutput {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Clone, Default)]
pub struct Captures {
    pub events: Vec<AgentEvent>,
    pub records: Vec<AgentRecord>,
    pub context_inputs: Vec<(Vec<ContentPart>, PromptOrigin)>,
    pub loop_events: Vec<LoopRecordedEvent>,
    pub begin_turn_count: usize,
    pub end_turn_count: usize,
    pub full_compaction_reset: usize,
    pub goal_increment_turn: usize,
    pub goal_mark_blocked: Vec<String>,
    pub goal_pause_on_interrupt: Vec<String>,
    pub hook_user_prompt_submit: Vec<Vec<ContentPart>>,
    pub hook_stop_hook: usize,
    pub telemetry_events: Vec<(String, JsonValue)>,
    pub hook_triggers: Vec<(String, JsonValue)>,
    pub hook_fire_and_forget: Vec<(String, JsonValue)>,
    pub generate_one_off_calls: Vec<(String, Vec<Tool>, Vec<Message>)>,
}

#[derive(Clone)]
pub struct FixtureClock {
    state: Arc<Mutex<Option<i64>>>,
}

impl FixtureClock {
    pub fn new(state: Arc<Mutex<Option<i64>>>) -> Self {
        Self { state }
    }
}

impl ClockSources for FixtureClock {
    fn wall_now(&self) -> i64 {
        self.state
            .lock()
            .unwrap()
            .unwrap_or_else(|| crate::cron::clock::resolve_clock_sources(None).wall_now())
    }

    fn mono_now_ms(&self) -> u128 {
        crate::cron::clock::resolve_clock_sources(None).mono_now_ms()
    }
}

pub struct FixtureAgent {
    pub captures: Arc<Mutex<Captures>>,
    pub llm: Arc<dyn Llm>,
    pub tools: Arc<Mutex<Vec<Arc<dyn ExecutableTool>>>>,
    pub history: Arc<Mutex<Vec<ContextMessage>>>,
    pub goal_status: Arc<Mutex<Option<GoalSnapshot>>>,
    pub hook_results: Arc<Mutex<Vec<HookResult>>>,
    pub stop_block: Arc<Mutex<Option<StopHookBlock>>>,
    pub loop_control: Option<LoopControl>,
    pub background: Mutex<Option<Arc<BackgroundManager>>>,
    pub cron: Mutex<Option<Arc<CronManager>>>,
    clock_state: Arc<Mutex<Option<i64>>>,
    pub generate_one_off_responses: Arc<Mutex<Vec<CompactGenerateResult>>>,
    pub last_assistant_at_ms: Arc<Mutex<Option<i64>>>,
    pub e2e_enabled: Arc<Mutex<bool>>,
    pub test_review_enabled: Arc<Mutex<bool>>,
}

impl FixtureAgent {
    pub fn new(responses: Vec<FixtureResponse>, tools: Vec<Arc<dyn ExecutableTool>>) -> Self {
        Self {
            captures: Arc::new(Mutex::new(Captures::default())),
            llm: Arc::new(FixtureLlm::new(responses)),
            tools: Arc::new(Mutex::new(tools)),
            history: Arc::new(Mutex::new(Vec::new())),
            goal_status: Arc::new(Mutex::new(None)),
            hook_results: Arc::new(Mutex::new(Vec::new())),
            stop_block: Arc::new(Mutex::new(None)),
            loop_control: None,
            background: Mutex::new(None),
            cron: Mutex::new(None),
            clock_state: Arc::new(Mutex::new(None)),
            generate_one_off_responses: Arc::new(Mutex::new(Vec::new())),
            last_assistant_at_ms: Arc::new(Mutex::new(None)),
            e2e_enabled: Arc::new(Mutex::new(false)),
            test_review_enabled: Arc::new(Mutex::new(false)),
        }
    }

    pub fn set_goal(&self, status: GoalStatus, budget: GoalBudgetLimits) {
        *self.goal_status.lock().unwrap() = Some(GoalSnapshot {
            status,
            budget_limits: budget,
            tokens_used: 0,
            turns_used: 0,
            wall_clock_ms: 0,
        });
    }

    pub fn install_managers(&self, background: Arc<BackgroundManager>, cron: Arc<CronManager>) {
        *self.background.lock().unwrap() = Some(background);
        *self.cron.lock().unwrap() = Some(cron);
    }

    pub fn advance_clock_to(&self, epoch_ms: i64) {
        *self.clock_state.lock().unwrap() = Some(epoch_ms);
    }

    pub fn clock(&self) -> Arc<dyn ClockSources> {
        Arc::new(FixtureClock::new(self.clock_state.clone()))
    }
}

impl TurnAgent for FixtureAgent {
    fn context(&self) -> &dyn TurnContext {
        self
    }
    fn usage(&self) -> &dyn TurnUsage {
        self
    }
    fn config(&self) -> &dyn TurnConfig {
        self
    }
    fn tools(&self) -> &dyn TurnTools {
        self
    }
    fn permission(&self) -> &dyn TurnPermission {
        self
    }
    fn injection(&self) -> &dyn TurnInjection {
        self
    }
    fn full_compaction(&self) -> &dyn TurnFullCompaction {
        self
    }
    fn micro_compaction(&self) -> &dyn TurnMicroCompaction {
        self
    }
    fn split_plan_checkpoint(&self) -> &dyn TurnSplitPlanCheckpoint {
        self
    }
    fn normal_mode_task_checkpoint(&self) -> &dyn TurnNormalTaskCheckpoint {
        self
    }
    fn session_mode(&self) -> &dyn TurnSessionMode {
        self
    }
    fn goals(&self) -> Option<&dyn TurnGoal> {
        Some(self)
    }
    fn hooks(&self) -> Option<&dyn TurnHooks> {
        Some(self)
    }
    fn telemetry(&self) -> &dyn TurnTelemetry {
        self
    }
    fn log(&self) -> &dyn TurnLog {
        self
    }
    fn mcp(&self) -> Option<&dyn TurnMcp> {
        None
    }
    fn subagent_host(&self) -> Option<&dyn TurnSubagentHost> {
        None
    }
    fn records(&self) -> &dyn TurnRecords {
        self
    }
    fn event_emitter(&self) -> &dyn TurnEventEmitter {
        self
    }
    fn llm_resolver(&self) -> &dyn TurnLlmResolver {
        self
    }
    fn flush_deferred_context_switch(&self) {}
    fn agent_type(&self) -> &str {
        "main"
    }
    fn homedir(&self) -> Option<&str> {
        None
    }
    fn goal_runtime_enabled(&self) -> bool {
        true
    }
}

#[async_trait::async_trait]
impl TurnContext for FixtureAgent {
    fn append_user_message(&self, content: Vec<ContentPart>, origin: PromptOrigin) {
        self.captures
            .lock()
            .unwrap()
            .context_inputs
            .push((content.clone(), origin.clone()));
        let mut history = self.history.lock().unwrap();
        history.push(ContextMessage {
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

    fn append_message(&self, message: ContextMessage) {
        if message.message.role == Role::Assistant {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            *self.last_assistant_at_ms.lock().unwrap() = Some(now);
        }
        self.history.lock().unwrap().push(message);
    }

    fn messages(&self) -> Vec<Message> {
        self.history
            .lock()
            .unwrap()
            .iter()
            .map(|cm| cm.message.clone())
            .collect()
    }

    fn append_loop_event(&self, event: LoopRecordedEvent) {
        self.captures.lock().unwrap().loop_events.push(event);
    }

    fn has_open_steps(&self) -> bool {
        false
    }
    fn clear(&self) {
        self.history.lock().unwrap().clear();
    }

    fn history(&self) -> Vec<ContextMessage> {
        self.history.lock().unwrap().clone()
    }
    fn token_count(&self) -> i64 {
        0
    }
    fn token_count_with_pending(&self) -> i64 {
        0
    }
    fn apply_compaction(&self, result: crate::records::nested::CompactionResult) {
        let mut history = self.history.lock().unwrap();
        let compacted_count = result.compacted_count as usize;
        if compacted_count == 0 || compacted_count > history.len() {
            return;
        }
        // Replace the first compacted_count messages with a single summary
        let summary_message = ContextMessage {
            message: Message {
                role: Role::Assistant,
                name: None,
                content: vec![ContentPart::Text {
                    text: result.summary.clone(),
                }],
                tool_calls: vec![],
                tool_call_id: None,
                partial: None,
            },
            origin: Some(PromptOrigin::CompactionSummary),
            is_error: None,
        };
        history.drain(0..compacted_count);
        history.insert(0, summary_message);
    }
    fn project(&self, messages: &[ContextMessage]) -> Vec<Message> {
        messages.iter().map(|cm| cm.message.clone()).collect()
    }

    fn last_assistant_at_ms(&self) -> Option<i64> {
        *self.last_assistant_at_ms.lock().unwrap()
    }

    fn append_system_reminder(&self, content: &str, origin: PromptOrigin) {
        let reminder_text = format!("<system-reminder>\n{}\n</system-reminder>", content);
        self.append_message(ContextMessage {
            message: Message {
                role: Role::User,
                name: None,
                content: vec![ContentPart::Text {
                    text: reminder_text,
                }],
                tool_calls: vec![],
                tool_call_id: None,
                partial: None,
            },
            origin: Some(origin),
            is_error: None,
        });
    }
}

impl TurnUsage for FixtureAgent {
    fn begin_turn(&self) {
        self.captures.lock().unwrap().begin_turn_count += 1;
    }
    fn end_turn(&self) {
        self.captures.lock().unwrap().end_turn_count += 1;
    }
    fn record(&self, _model: &str, _usage: TokenUsage, _scope: UsageRecordScope) {}
    fn current_turn_usage(&self) -> Option<TokenUsage> {
        None
    }
}

impl TurnConfig for FixtureAgent {
    fn model(&self) -> String {
        "mock-model".into()
    }
    fn model_alias(&self) -> Option<String> {
        None
    }
    fn system_prompt(&self) -> String {
        "You are a fixture agent.".into()
    }
    fn thinking_level(&self) -> String {
        "off".into()
    }
    fn provider(&self) -> Box<dyn ChatProvider> {
        Box::new(NoopChatProvider)
    }
    fn model_capabilities(&self) -> ModelCapability {
        ModelCapability::unknown()
    }
    fn loop_control(&self) -> Option<LoopControl> {
        self.loop_control.clone()
    }
    fn has_model(&self) -> bool {
        true
    }
    fn e2e_enabled(&self) -> bool {
        *self.e2e_enabled.lock().unwrap()
    }
    fn test_review_enabled(&self) -> bool {
        *self.test_review_enabled.lock().unwrap()
    }
}

impl TurnTools for FixtureAgent {
    fn loop_tools(&self) -> Vec<Arc<dyn ExecutableTool>> {
        self.tools.lock().unwrap().clone()
    }
    fn store_data(&self) -> std::collections::HashMap<String, serde_json::Value> {
        std::collections::HashMap::new()
    }
}

#[async_trait::async_trait]
impl TurnPermission for FixtureAgent {
    async fn before_tool_call(
        &self,
        _ctx: ResolvedToolExecutionHookContext<'_>,
    ) -> Result<Option<AuthorizeToolExecutionResult>, anyhow::Error> {
        Ok(None)
    }
}

#[async_trait::async_trait]
impl TurnInjection for FixtureAgent {
    async fn inject_goal(&self) {}
    async fn inject(&self) {}
}

#[async_trait::async_trait]
impl TurnFullCompaction for FixtureAgent {
    fn reset_for_turn(&self, _agent: Arc<dyn TurnAgent>) {
        self.captures.lock().unwrap().full_compaction_reset += 1;
    }
    async fn before_step(
        &self,
        _agent: Arc<dyn TurnAgent>,
        _signal: AbortSignal,
    ) -> Result<(), anyhow::Error> {
        Ok(())
    }
    async fn after_step(&self, _agent: Arc<dyn TurnAgent>) {}
    async fn handle_overflow_error(
        &self,
        _agent: Arc<dyn TurnAgent>,
        _signal: AbortSignal,
        _error: anyhow::Error,
    ) -> Result<(), anyhow::Error> {
        Ok(())
    }
    async fn compact_checkpoint(
        &self,
        _agent: Arc<dyn TurnAgent>,
        _signal: AbortSignal,
    ) -> Result<(), anyhow::Error> {
        Ok(())
    }
    fn begin(
        &self,
        _agent: Arc<dyn TurnAgent>,
        _data: crate::records::nested::CompactionBeginData,
    ) {
    }
    fn cancel(&self, _agent: Arc<dyn TurnAgent>) {}
    fn compacted_history(&self) -> Vec<CompactedHistory> {
        vec![]
    }
    fn is_compacting(&self) -> bool {
        false
    }
}

impl TurnMicroCompaction for FixtureAgent {
    fn detect(&self, _agent: Arc<dyn TurnAgent>) {}
    fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage> {
        messages.to_vec()
    }
    fn reset(&self, _max_cutoff: usize) {}
}

#[async_trait::async_trait]
impl TurnSplitPlanCheckpoint for FixtureAgent {
    async fn before_step(&self, _agent: Arc<dyn TurnAgent>, _signal: AbortSignal) {}
    fn reset(&self) {}
}

#[async_trait::async_trait]
impl TurnNormalTaskCheckpoint for FixtureAgent {
    async fn before_step(&self, _agent: Arc<dyn TurnAgent>, _signal: AbortSignal) {}
    fn reset(&self) {}
}

#[async_trait::async_trait]
impl TurnSessionMode for FixtureAgent {
    fn is_active(&self) -> bool {
        false
    }
    fn kind(&self) -> Option<String> {
        None
    }
    fn file_path(&self) -> Option<String> {
        None
    }
    async fn data(&self) -> Option<String> {
        None
    }
}

#[async_trait::async_trait]
impl TurnGoal for FixtureAgent {
    fn get_goal(&self) -> Option<GoalSnapshot> {
        self.goal_status.lock().unwrap().clone()
    }

    async fn increment_turn(&self) {
        self.captures.lock().unwrap().goal_increment_turn += 1;
        let mut g = self.goal_status.lock().unwrap();
        if let Some(ref mut s) = *g {
            s.turns_used += 1;
        }
    }

    async fn mark_blocked(&self, reason: &str) {
        self.captures
            .lock()
            .unwrap()
            .goal_mark_blocked
            .push(reason.into());
        let mut g = self.goal_status.lock().unwrap();
        if let Some(ref mut s) = *g {
            s.status = GoalStatus::Blocked;
        }
        drop(g);
        self.captures.lock().unwrap().telemetry_events.push((
            "goal_mark_blocked".into(),
            serde_json::json!({ "reason": reason }),
        ));
    }

    async fn pause_on_interrupt(&self, reason: &str) {
        self.captures
            .lock()
            .unwrap()
            .goal_pause_on_interrupt
            .push(reason.into());
    }
    async fn pause_active_goal(&self, _actor: &str, _reason: &str) {}

    async fn record_token_usage(
        &self,
        token_delta: i64,
        _agent_id: &str,
        _agent_type: &str,
        _source: &str,
    ) -> Option<GoalSnapshot> {
        let mut g = self.goal_status.lock().unwrap();
        if let Some(ref mut s) = *g {
            s.tokens_used += token_delta;
        }
        g.clone()
    }
}

#[async_trait::async_trait]
impl TurnHooks for FixtureAgent {
    async fn trigger_user_prompt_submit(
        &self,
        input: Vec<ContentPart>,
        _signal: AbortSignal,
    ) -> Result<Vec<HookResult>, anyhow::Error> {
        self.captures
            .lock()
            .unwrap()
            .hook_user_prompt_submit
            .push(input);
        Ok(self.hook_results.lock().unwrap().clone())
    }

    async fn trigger_stop_hook(
        &self,
        _signal: AbortSignal,
    ) -> Result<Option<StopHookBlock>, anyhow::Error> {
        self.captures.lock().unwrap().hook_stop_hook += 1;
        Ok(self.stop_block.lock().unwrap().clone())
    }

    fn fire_and_forget_trigger(&self, event: &str, data: JsonValue) {
        self.captures
            .lock()
            .unwrap()
            .hook_fire_and_forget
            .push((event.into(), data));
    }
    async fn trigger(
        &self,
        event: &str,
        data: serde_json::Value,
        _signal: AbortSignal,
    ) -> Result<(), anyhow::Error> {
        self.captures
            .lock()
            .unwrap()
            .hook_triggers
            .push((event.into(), data));
        Ok(())
    }
}

impl TurnTelemetry for FixtureAgent {
    fn track(&self, event: &str, properties: JsonValue) {
        self.captures
            .lock()
            .unwrap()
            .telemetry_events
            .push((event.into(), properties));
    }
}

impl TurnLog for FixtureAgent {
    fn debug(&self, _msg: &str, _data: JsonValue) {}
    fn warn(&self, _msg: &str, _data: JsonValue) {}
    fn error(&self, _msg: &str, _data: JsonValue) {}
}

impl TurnRecords for FixtureAgent {
    fn log_record(&self, record: AgentRecord) {
        self.captures.lock().unwrap().records.push(record);
    }
}

impl TurnEventEmitter for FixtureAgent {
    fn emit_event(&self, event: AgentEvent) {
        self.captures.lock().unwrap().events.push(event);
    }
}

#[async_trait::async_trait]
impl TurnLlmResolver for FixtureAgent {
    fn refresh_llm(&self) {}
    fn llm(&self) -> Arc<dyn Llm> {
        self.llm.clone()
    }
    async fn generate_one_off(
        &self,
        _provider: Box<dyn ChatProvider + Send>,
        system_prompt: String,
        tools: Vec<kosong_rs::provider::Tool>,
        messages: Vec<Message>,
        _signal: AbortSignal,
    ) -> Result<CompactGenerateResult, anyhow::Error> {
        self.captures
            .lock()
            .unwrap()
            .generate_one_off_calls
            .push((system_prompt, tools, messages));
        let mut queue = self.generate_one_off_responses.lock().unwrap();
        if queue.is_empty() {
            return Err(anyhow::anyhow!("no generate_one_off response configured"));
        }
        Ok(queue.remove(0))
    }
}

pub struct FixtureLlm {
    responses: Vec<FixtureResponse>,
    index: Mutex<usize>,
}

impl FixtureLlm {
    pub fn new(responses: Vec<FixtureResponse>) -> Self {
        Self {
            responses,
            index: Mutex::new(0),
        }
    }

    pub fn end_turn() -> Arc<dyn Llm> {
        Arc::new(Self::new(vec![FixtureResponse {
            tool_calls: vec![],
            finish_reason: Some(FinishReason::Completed),
            raw_finish_reason: None,
            usage: TokenUsage::default(),
        }]))
    }
}

#[async_trait::async_trait]
impl Llm for FixtureLlm {
    fn system_prompt(&self) -> &str {
        "fixture"
    }
    fn model_name(&self) -> &str {
        "mock"
    }

    async fn chat(&self, _params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        let idx = {
            let mut guard = self.index.lock().unwrap();
            let i = *guard;
            *guard = guard.wrapping_add(1);
            i
        };
        let response = self
            .responses
            .get(idx)
            .or_else(|| self.responses.last())
            .cloned()
            .unwrap_or_default();
        Ok(LlmChatResponse {
            tool_calls: response.tool_calls,
            provider_finish_reason: response.finish_reason,
            raw_finish_reason: response.raw_finish_reason,
            usage: response.usage,
            stream_timing: Some(LlmStreamTiming::default()),
        })
    }
}

pub struct PendingLlm;

#[async_trait::async_trait]
impl Llm for PendingLlm {
    fn system_prompt(&self) -> &str {
        ""
    }
    fn model_name(&self) -> &str {
        "pending"
    }

    async fn chat(&self, params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        while !params.signal.is_aborted() {
            tokio::task::yield_now().await;
        }
        Err(anyhow::anyhow!("aborted"))
    }
}

pub struct FixtureTool {
    def: FixtureToolDef,
    agent: Arc<FixtureAgent>,
}

impl FixtureTool {
    pub fn new(def: FixtureToolDef, agent: Arc<FixtureAgent>) -> Self {
        Self { def, agent }
    }
}

#[async_trait::async_trait]
impl ExecutableTool for FixtureTool {
    fn name(&self) -> &str {
        &self.def.name
    }
    fn description(&self) -> &str {
        &self.def.description
    }
    fn parameters(&self) -> JsonValue {
        self.def.parameters.clone()
    }

    async fn resolve_execution(&self, input: JsonValue) -> Result<ToolExecution, anyhow::Error> {
        if self.def.name == "UpdateGoal" {
            if let Some(status) = input.get("status").and_then(|v| v.as_str()) {
                let status = match status {
                    "active" => GoalStatus::Active,
                    "paused" => GoalStatus::Paused,
                    "blocked" => GoalStatus::Blocked,
                    "complete" => GoalStatus::Complete,
                    _ => GoalStatus::Active,
                };
                *self.agent.goal_status.lock().unwrap() = Some(GoalSnapshot {
                    status,
                    budget_limits: GoalBudgetLimits::default(),
                    tokens_used: 0,
                    turns_used: 0,
                    wall_clock_ms: 0,
                });
            }
            return Ok(ToolExecution::Runnable(RunnableToolExecution {
                is_error: None,
                accesses: None,
                display: None,
                description: None,
                stop_batch_after_this: None,
                approval_rule: String::new(),
                matches_rule: None,
                execute: Box::new(|_ctx| {
                    Box::pin(async move {
                        Ok(ExecutableToolResult::Success(ExecutableToolSuccessResult {
                            output: ExecutableToolOutput::Text("ok".into()),
                            is_error: None,
                            stop_turn: None,
                            message: None,
                        }))
                    })
                }),
            }));
        }

        let output = match &self.def.result.output {
            FixtureOutput::Text(s) => ExecutableToolOutput::Text(s.clone()),
            FixtureOutput::Parts(p) => ExecutableToolOutput::Parts(p.clone()),
        };
        let result = if self.def.result.is_error == Some(true) {
            ExecutableToolResult::Error(ExecutableToolErrorResult {
                output,
                is_error: true,
                stop_turn: self.def.result.stop_turn,
                message: self.def.result.message.clone(),
            })
        } else {
            ExecutableToolResult::Success(ExecutableToolSuccessResult {
                output,
                is_error: self.def.result.is_error,
                stop_turn: self.def.result.stop_turn,
                message: self.def.result.message.clone(),
            })
        };
        let result_clone = result.clone();
        Ok(ToolExecution::Runnable(RunnableToolExecution {
            is_error: None,
            accesses: None,
            display: None,
            description: None,
            stop_batch_after_this: None,
            approval_rule: String::new(),
            matches_rule: None,
            execute: Box::new(move |_ctx| {
                let r = result_clone.clone();
                Box::pin(async move { Ok(r) })
                    as Pin<
                        Box<
                            dyn Future<Output = Result<ExecutableToolResult, anyhow::Error>> + Send,
                        >,
                    >
            }),
        }))
    }
}

#[derive(Clone)]
struct NoopChatProvider;

#[async_trait::async_trait]
impl ChatProvider for NoopChatProvider {
    fn name(&self) -> &str {
        "noop"
    }
    fn model_name(&self) -> &str {
        "noop"
    }
    fn thinking_effort(&self) -> Option<ThinkingEffort> {
        None
    }

    async fn generate(
        &self,
        _system_prompt: &str,
        _tools: &[Tool],
        _history: &[Message],
        _options: Option<GenerateOptions>,
    ) -> Result<StreamedMessage, kosong_rs::errors::ChatProviderError> {
        panic!("NoopChatProvider::generate should not be called in turn fixtures")
    }

    fn with_thinking(&self, _effort: ThinkingEffort) -> Box<dyn ChatProvider> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn::TurnFlow;
    use kosong_rs::message::ContentPart;

    #[tokio::test]
    async fn fixture_agent_runs_turn_flow() {
        let agent = Arc::new(FixtureAgent::new(
            vec![FixtureResponse {
                tool_calls: vec![],
                finish_reason: Some(FinishReason::Completed),
                raw_finish_reason: Some("stop".into()),
                usage: TokenUsage::default(),
            }],
            vec![],
        ));
        let flow = TurnFlow::new(agent.clone());
        let id = flow.prompt(
            vec![ContentPart::Text { text: "hi".into() }],
            USER_PROMPT_ORIGIN,
        );
        assert!(id.is_some());
        let _ = flow.wait_for_current_turn(None).await.unwrap();
        let events = agent.captures.lock().unwrap().events.clone();
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::TurnStarted { .. })));
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::TurnEnded { .. })));
    }

    #[tokio::test]
    async fn fixture_agent_captures_cron_fire_event() {
        let agent = Arc::new(FixtureAgent::new(
            vec![FixtureResponse {
                tool_calls: vec![],
                finish_reason: Some(FinishReason::Completed),
                raw_finish_reason: None,
                usage: TokenUsage::default(),
            }],
            vec![],
        ));
        let flow = Arc::new(TurnFlow::new(agent.clone()));
        let background = Arc::new(BackgroundManager::new(agent.clone(), flow.clone(), None));
        let cron = CronManager::new(
            agent.clone(),
            flow.clone(),
            None,
            CronManagerOptions {
                clocks: Some(agent.clock()),
                poll_interval_ms: Some(0),
            },
        );
        agent.install_managers(background, cron.clone());

        // Use a known id for deterministic jitter
        cron.store
            .lock()
            .unwrap()
            .adopt(crate::cron::task::CronTask {
                id: "00000000".to_string(),
                cron: "* * * * *".to_string(),
                prompt: "ping".to_string(),
                created_at: 0,
                recurring: Some(true),
                last_fired_at: None,
            });
        agent.advance_clock_to(60_000);
        cron.tick();

        let _ = flow.wait_for_current_turn(None).await;

        let events = agent.captures.lock().unwrap().events.clone();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, AgentEvent::CronFired { .. })),
            "expected CronFired event, got {:?}",
            events
        );
    }
}
