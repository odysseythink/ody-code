use kosong_rs::message::{ContentPart, Message};
use kosong_rs::provider::{AbortSignal, ChatProvider, ModelCapability};
use kosong_rs::usage::TokenUsage;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub use crate::context::types::{ContextMessage, PromptOrigin, USER_PROMPT_ORIGIN};
pub use crate::records::nested::LoopRecordedEvent;

use crate::agent_loop::types::{
    AuthorizeToolExecutionResult, ExecutableTool, LoopTurnStopReason,
    ResolvedToolExecutionHookContext,
};
use crate::records::nested::{GoalBudgetLimits, GoalStatus, UsageRecordScope};
use crate::records::AgentRecord;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoopControl {
    pub max_steps_per_turn: Option<u32>,
    pub max_retries_per_step: Option<u32>,
    pub reserved_context_size: Option<i64>,
    pub split_plan_compaction_ratio: Option<f64>,
    pub normal_task_compaction_ratio: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct CompactedHistory {
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompactGenerateResult {
    pub text: String,
    pub finish_reason: Option<kosong_rs::provider::FinishReason>,
    pub usage: kosong_rs::usage::TokenUsage,
}

#[async_trait::async_trait]
pub trait TurnContext: Send + Sync {
    fn append_user_message(&self, content: Vec<ContentPart>, origin: PromptOrigin);
    fn append_message(&self, message: ContextMessage);
    fn messages(&self) -> Vec<Message>;
    fn append_loop_event(&self, event: LoopRecordedEvent);
    fn has_open_steps(&self) -> bool;
    fn clear(&self);
    fn history(&self) -> Vec<ContextMessage>;
    fn token_count(&self) -> i64;
    fn token_count_with_pending(&self) -> i64;
    fn apply_compaction(&self, result: crate::records::nested::CompactionResult);
    fn project(&self, messages: &[ContextMessage]) -> Vec<Message>;
    fn last_assistant_at_ms(&self) -> Option<i64>;
    fn append_system_reminder(&self, content: &str, origin: PromptOrigin);
}

pub trait TurnUsage: Send + Sync {
    fn begin_turn(&self);
    fn end_turn(&self);
    fn record(&self, model: &str, usage: TokenUsage, scope: UsageRecordScope);
    fn current_turn_usage(&self) -> Option<TokenUsage>;
}

pub trait TurnConfig: Send + Sync {
    fn model(&self) -> String;
    fn model_alias(&self) -> Option<String>;
    fn system_prompt(&self) -> String;
    fn thinking_level(&self) -> String;
    fn provider(&self) -> Box<dyn ChatProvider>;
    fn model_capabilities(&self) -> ModelCapability;
    fn loop_control(&self) -> Option<LoopControl>;
    fn has_model(&self) -> bool;
    fn e2e_enabled(&self) -> bool;
    fn test_review_enabled(&self) -> bool;
}

pub trait TurnTools: Send + Sync {
    fn loop_tools(&self) -> Vec<Arc<dyn ExecutableTool>>;
    fn store_data(&self) -> std::collections::HashMap<String, serde_json::Value>;
}

#[async_trait::async_trait]
pub trait TurnPermission: Send + Sync {
    async fn before_tool_call(
        &self,
        ctx: ResolvedToolExecutionHookContext<'_>,
    ) -> Result<Option<AuthorizeToolExecutionResult>, anyhow::Error>;
}

#[async_trait::async_trait]
pub trait TurnInjection: Send + Sync {
    async fn inject_goal(&self);
    async fn inject(&self);
}

#[async_trait::async_trait]
pub trait TurnFullCompaction: Send + Sync {
    fn reset_for_turn(&self, agent: std::sync::Arc<dyn TurnAgent>);
    async fn before_step(
        &self,
        agent: std::sync::Arc<dyn TurnAgent>,
        signal: AbortSignal,
    ) -> Result<(), anyhow::Error>;
    async fn after_step(&self, agent: std::sync::Arc<dyn TurnAgent>);
    async fn handle_overflow_error(
        &self,
        agent: std::sync::Arc<dyn TurnAgent>,
        signal: AbortSignal,
        error: anyhow::Error,
    ) -> Result<(), anyhow::Error>;
    async fn compact_checkpoint(
        &self,
        agent: std::sync::Arc<dyn TurnAgent>,
        signal: AbortSignal,
    ) -> Result<(), anyhow::Error>;
    fn begin(
        &self,
        agent: std::sync::Arc<dyn TurnAgent>,
        data: crate::records::nested::CompactionBeginData,
    );
    fn cancel(&self, agent: std::sync::Arc<dyn TurnAgent>);
    fn compacted_history(&self) -> Vec<CompactedHistory>;
    fn is_compacting(&self) -> bool;
}

pub trait TurnMicroCompaction: Send + Sync {
    fn detect(&self, agent: std::sync::Arc<dyn TurnAgent>);
    fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage>;
    fn reset(&self, max_cutoff: usize);
}

#[async_trait::async_trait]
pub trait TurnSplitPlanCheckpoint: Send + Sync {
    async fn before_step(&self, agent: std::sync::Arc<dyn TurnAgent>, signal: AbortSignal);
    fn reset(&self);
}

#[async_trait::async_trait]
pub trait TurnNormalTaskCheckpoint: Send + Sync {
    async fn before_step(&self, agent: std::sync::Arc<dyn TurnAgent>, signal: AbortSignal);
    fn reset(&self);
}

#[async_trait::async_trait]
pub trait TurnSessionMode: Send + Sync {
    fn is_active(&self) -> bool;
    fn kind(&self) -> Option<String>;
    fn file_path(&self) -> Option<String>;
    async fn data(&self) -> Option<String>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct TurnEndResult {
    pub event: TurnEndedEvent,
    pub stop_reason: Option<LoopTurnStopReason>,
    pub blocked_by_user_prompt_hook: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GoalSnapshot {
    pub status: GoalStatus,
    pub budget_limits: GoalBudgetLimits,
    pub tokens_used: i64,
    pub turns_used: i64,
    pub wall_clock_ms: i64,
}

#[async_trait::async_trait]
pub trait TurnGoal: Send + Sync {
    fn get_goal(&self) -> Option<GoalSnapshot>;
    async fn increment_turn(&self);
    async fn mark_blocked(&self, reason: &str);
    async fn pause_on_interrupt(&self, reason: &str);
    async fn pause_active_goal(&self, actor: &str, reason: &str);
    async fn record_token_usage(
        &self,
        token_delta: i64,
        agent_id: &str,
        agent_type: &str,
        source: &str,
    ) -> Option<GoalSnapshot>;
}

#[async_trait::async_trait]
pub trait TurnHooks: Send + Sync {
    async fn trigger_user_prompt_submit(
        &self,
        input: Vec<ContentPart>,
        signal: AbortSignal,
    ) -> Result<Vec<HookResult>, anyhow::Error>;
    async fn trigger_stop_hook(
        &self,
        signal: AbortSignal,
    ) -> Result<Option<StopHookBlock>, anyhow::Error>;
    fn fire_and_forget_trigger(&self, event: &str, data: serde_json::Value);
    async fn trigger(
        &self,
        event: &str,
        data: serde_json::Value,
        signal: AbortSignal,
    ) -> Result<(), anyhow::Error>;
}

#[derive(Debug, Clone)]
pub struct HookResult {
    pub event: String,
    pub text: Option<String>,
    pub message: Option<serde_json::Value>,
    pub blocked: bool,
}

#[derive(Debug, Clone)]
pub struct StopHookBlock {
    pub reason: String,
}

pub trait TurnTelemetry: Send + Sync {
    fn track(&self, event: &str, properties: serde_json::Value);
}

pub trait TurnLog: Send + Sync {
    fn debug(&self, msg: &str, data: serde_json::Value);
    fn warn(&self, msg: &str, data: serde_json::Value);
    fn error(&self, msg: &str, data: serde_json::Value);
}

#[async_trait::async_trait]
pub trait TurnMcp: Send + Sync {
    async fn wait_for_initial_load(&self, signal: AbortSignal) -> Result<(), anyhow::Error>;
}

pub trait TurnSubagentHost: Send + Sync {
    fn cancel_all(&self, reason: &str);
}

pub trait TurnRecords: Send + Sync {
    fn log_record(&self, record: AgentRecord);
}

pub trait TurnEventEmitter: Send + Sync {
    fn emit_event(&self, event: AgentEvent);
}

#[async_trait::async_trait]
pub trait TurnLlmResolver: Send + Sync {
    fn refresh_llm(&self);
    fn llm(&self) -> Arc<dyn crate::agent_loop::llm::Llm>;
    async fn generate_one_off(
        &self,
        provider: Box<dyn ChatProvider + Send>,
        system_prompt: String,
        tools: Vec<kosong_rs::provider::Tool>,
        messages: Vec<Message>,
        signal: AbortSignal,
    ) -> Result<CompactGenerateResult, anyhow::Error>;
}

pub trait TurnAgent: Send + Sync {
    fn context(&self) -> &dyn TurnContext;
    fn usage(&self) -> &dyn TurnUsage;
    fn config(&self) -> &dyn TurnConfig;
    fn tools(&self) -> &dyn TurnTools;
    fn permission(&self) -> &dyn TurnPermission;
    fn injection(&self) -> &dyn TurnInjection;
    fn full_compaction(&self) -> &dyn TurnFullCompaction;
    fn micro_compaction(&self) -> &dyn TurnMicroCompaction;
    fn split_plan_checkpoint(&self) -> &dyn TurnSplitPlanCheckpoint;
    fn normal_mode_task_checkpoint(&self) -> &dyn TurnNormalTaskCheckpoint;
    fn session_mode(&self) -> &dyn TurnSessionMode;
    fn goals(&self) -> Option<&dyn TurnGoal>;
    fn hooks(&self) -> Option<&dyn TurnHooks>;
    fn telemetry(&self) -> &dyn TurnTelemetry;
    fn log(&self) -> &dyn TurnLog;
    fn mcp(&self) -> Option<&dyn TurnMcp>;
    fn subagent_host(&self) -> Option<&dyn TurnSubagentHost>;
    fn records(&self) -> &dyn TurnRecords;
    fn event_emitter(&self) -> &dyn TurnEventEmitter;
    fn llm_resolver(&self) -> &dyn TurnLlmResolver;
    fn flush_deferred_context_switch(&self);
    fn agent_type(&self) -> &str;
    fn homedir(&self) -> Option<&str>;
    fn goal_runtime_enabled(&self) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnEndedReason {
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnEndedEvent {
    pub turn_id: i64,
    pub reason: TurnEndedReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<TurnErrorSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnErrorSummary {
    pub code: String,
    pub name: String,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepRetryingEvent {
    #[serde(rename = "turnId")]
    pub turn_id: i64,
    pub step: u32,
    #[serde(rename = "stepUuid")]
    pub step_uuid: String,
    #[serde(rename = "failedAttempt")]
    pub failed_attempt: u32,
    #[serde(rename = "nextAttempt")]
    pub next_attempt: u32,
    #[serde(rename = "maxAttempts")]
    pub max_attempts: u32,
    #[serde(rename = "delayMs")]
    pub delay_ms: u64,
    #[serde(rename = "errorName")]
    pub error_name: String,
    #[serde(rename = "errorMessage")]
    pub error_message: String,
    #[serde(rename = "statusCode", skip_serializing_if = "Option::is_none")]
    pub status_code: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentEvent {
    #[serde(rename = "turn.started", rename_all = "camelCase")]
    TurnStarted { turn_id: i64, origin: PromptOrigin },
    #[serde(rename = "background.task.started")]
    BackgroundTaskStarted {
        info: crate::background::types::BackgroundTaskInfo,
    },
    #[serde(rename = "background.task.terminated")]
    BackgroundTaskTerminated {
        info: crate::background::types::BackgroundTaskInfo,
    },
    #[serde(rename = "cron.fired")]
    CronFired {
        origin: PromptOrigin,
        prompt: String,
    },
    #[serde(rename = "turn.ended")]
    TurnEnded(TurnEndedEvent),
    #[serde(rename = "turn.step.started", rename_all = "camelCase")]
    TurnStepStarted {
        turn_id: i64,
        step: u32,
        step_id: String,
    },
    #[serde(rename = "turn.step.completed", rename_all = "camelCase")]
    TurnStepCompleted {
        turn_id: i64,
        step: u32,
        step_id: String,
        usage: TokenUsage,
        #[serde(skip_serializing_if = "Option::is_none")]
        finish_reason: Option<String>,
        #[serde(
            rename = "llmFirstTokenLatencyMs",
            skip_serializing_if = "Option::is_none"
        )]
        llm_first_token_latency_ms: Option<i64>,
        #[serde(
            rename = "llmStreamDurationMs",
            skip_serializing_if = "Option::is_none"
        )]
        llm_stream_duration_ms: Option<i64>,
        #[serde(
            rename = "providerFinishReason",
            skip_serializing_if = "Option::is_none"
        )]
        provider_finish_reason: Option<String>,
        #[serde(rename = "rawFinishReason", skip_serializing_if = "Option::is_none")]
        raw_finish_reason: Option<String>,
    },
    #[serde(rename = "turn.step.retrying")]
    TurnStepRetrying(StepRetryingEvent),
    #[serde(rename = "turn.step.interrupted", rename_all = "camelCase")]
    TurnStepInterrupted {
        turn_id: i64,
        step: u32,
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    #[serde(rename = "assistant.delta", rename_all = "camelCase")]
    AssistantDelta { turn_id: i64, delta: String },
    #[serde(rename = "thinking.delta", rename_all = "camelCase")]
    ThinkingDelta { turn_id: i64, delta: String },
    #[serde(rename = "tool.call.started", rename_all = "camelCase")]
    ToolCallStarted {
        turn_id: i64,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        name: String,
        args: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        display: Option<serde_json::Value>,
    },
    #[serde(rename = "tool.result", rename_all = "camelCase")]
    ToolResult {
        turn_id: i64,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        output: crate::records::nested::ExecutableToolOutput,
        #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    #[serde(rename = "tool.call.delta", rename_all = "camelCase")]
    ToolCallDelta {
        turn_id: i64,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(rename = "argumentsPart", skip_serializing_if = "Option::is_none")]
        arguments_part: Option<String>,
    },
    #[serde(rename = "tool.progress", rename_all = "camelCase")]
    ToolProgress {
        turn_id: i64,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        update: crate::records::nested::ToolUpdate,
    },
    #[serde(rename = "hook.result", rename_all = "camelCase")]
    HookResult {
        turn_id: i64,
        #[serde(rename = "hookEvent")]
        hook_event: String,
        content: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        blocked: Option<bool>,
    },
    #[serde(rename = "error")]
    Error(TurnErrorSummary),
    #[serde(rename = "compaction.started", rename_all = "camelCase")]
    CompactionStarted {
        trigger: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        instruction: Option<String>,
    },
    #[serde(rename = "compaction.cancelled")]
    CompactionCancelled,
    #[serde(rename = "compaction.blocked", rename_all = "camelCase")]
    CompactionBlocked { turn_id: i64 },
    #[serde(rename = "compaction.completed", rename_all = "camelCase")]
    CompactionCompleted {
        result: crate::records::nested::CompactionResult,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use kosong_rs::message::{ContentPart, Message, Role};
    use std::sync::{Arc, Mutex};

    struct DummyContext {
        calls: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl TurnContext for DummyContext {
        fn append_user_message(&self, _content: Vec<ContentPart>, _origin: PromptOrigin) {
            self.calls
                .lock()
                .unwrap()
                .push("append_user_message".into());
        }
        fn append_message(&self, _message: ContextMessage) {}
        fn messages(&self) -> Vec<Message> {
            vec![Message {
                role: Role::User,
                name: None,
                content: vec![],
                tool_calls: vec![],
                tool_call_id: None,
                partial: None,
            }]
        }
        fn append_loop_event(&self, _event: LoopRecordedEvent) {}
        fn has_open_steps(&self) -> bool {
            false
        }
        fn clear(&self) {}
        fn history(&self) -> Vec<ContextMessage> {
            vec![]
        }
        fn token_count(&self) -> i64 {
            0
        }
        fn token_count_with_pending(&self) -> i64 {
            0
        }
        fn apply_compaction(&self, _result: crate::records::nested::CompactionResult) {}
        fn project(&self, messages: &[ContextMessage]) -> Vec<Message> {
            messages.iter().map(|cm| cm.message.clone()).collect()
        }
        fn last_assistant_at_ms(&self) -> Option<i64> {
            None
        }
        fn append_system_reminder(&self, _content: &str, _origin: PromptOrigin) {}
    }

    #[test]
    fn turn_context_trait_is_callable() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let ctx: Arc<dyn TurnContext> = Arc::new(DummyContext {
            calls: calls.clone(),
        });
        ctx.append_user_message(
            vec![ContentPart::Text { text: "hi".into() }],
            USER_PROMPT_ORIGIN,
        );
        assert_eq!(calls.lock().unwrap().as_slice(), &["append_user_message"]);
    }

    #[test]
    fn all_subsystem_traits_are_implementable() {
        struct Dummy;
        impl TurnUsage for Dummy {
            fn begin_turn(&self) {}
            fn end_turn(&self) {}
            fn record(
                &self,
                _model: &str,
                _usage: kosong_rs::usage::TokenUsage,
                _scope: crate::records::nested::UsageRecordScope,
            ) {
            }
            fn current_turn_usage(&self) -> Option<TokenUsage> {
                None
            }
        }
        impl TurnConfig for Dummy {
            fn model(&self) -> String {
                "m".into()
            }
            fn model_alias(&self) -> Option<String> {
                Some("alias".into())
            }
            fn system_prompt(&self) -> String {
                "".into()
            }
            fn thinking_level(&self) -> String {
                "off".into()
            }
            fn provider(&self) -> Box<dyn kosong_rs::provider::ChatProvider> {
                panic!("noop")
            }
            fn model_capabilities(&self) -> kosong_rs::provider::ModelCapability {
                kosong_rs::provider::ModelCapability::unknown()
            }
            fn loop_control(&self) -> Option<LoopControl> {
                None
            }
            fn has_model(&self) -> bool {
                true
            }
            fn e2e_enabled(&self) -> bool {
                false
            }
            fn test_review_enabled(&self) -> bool {
                false
            }
        }
        impl TurnTools for Dummy {
            fn loop_tools(
                &self,
            ) -> Vec<std::sync::Arc<dyn crate::agent_loop::types::ExecutableTool>> {
                vec![]
            }
            fn store_data(&self) -> std::collections::HashMap<String, serde_json::Value> {
                std::collections::HashMap::new()
            }
        }
        impl TurnEventEmitter for Dummy {
            fn emit_event(&self, _event: AgentEvent) {}
        }

        let _: Box<dyn TurnUsage> = Box::new(Dummy);
        let _: Box<dyn TurnConfig> = Box::new(Dummy);
        let _: Box<dyn TurnTools> = Box::new(Dummy);
        let _: Box<dyn TurnEventEmitter> = Box::new(Dummy);
    }

    #[test]
    fn agent_event_round_trips_json() {
        let event = AgentEvent::TurnStarted {
            turn_id: 7,
            origin: USER_PROMPT_ORIGIN,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"turn.started\""));
        assert!(json.contains("\"turnId\":7"));
        let back: AgentEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn turn_error_summary_serializes_expected_shape() {
        let err = TurnErrorSummary {
            code: "model.not_configured".into(),
            name: "OdyError".into(),
            message: "LLM not set".into(),
            retryable: false,
            details: Some(serde_json::json!({ "turnId": 3 })),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"code\":\"model.not_configured\""));
        assert!(json.contains("\"retryable\":false"));
    }

    #[test]
    fn agent_event_background_and_cron_round_trip() {
        use crate::background::types::{
            BackgroundTaskId, BackgroundTaskInfo, BackgroundTaskStatus,
        };
        use chrono::Utc;

        let ts = Utc::now();
        let info = BackgroundTaskInfo {
            id: BackgroundTaskId::new("bt-123"),
            kind: crate::background::types::BackgroundTaskKind::Process,
            description: "echo hello".to_string(),
            status: BackgroundTaskStatus::Running,
            started_at: ts,
            finished_at: None,
            stop_reason: None,
            command: Some("echo hello".to_string()),
            pid: Some(1234),
            exit_code: None,
            output_snapshot: None,
            question_count: None,
            tool_call_id: None,
            agent_id: None,
            subagent_type: None,
            terminal_notification_suppressed: None,
            timeout_ms: None,
        };
        let event = AgentEvent::BackgroundTaskStarted { info: info.clone() };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"background.task.started\""));
        assert!(json.contains("\"taskId\":\"bt-123\""));
        let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentEvent::BackgroundTaskStarted { info: parsed_info } => {
                assert_eq!(parsed_info.id, info.id);
                assert_eq!(parsed_info.command, info.command);
            }
            _ => panic!("expected BackgroundTaskStarted, got {:?}", parsed),
        }

        let event = AgentEvent::CronFired {
            origin: PromptOrigin::User,
            prompt: "check status".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"cron.fired\""));
        assert!(json.contains("\"prompt\":\"check status\""));
        let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentEvent::CronFired { prompt, .. } => {
                assert_eq!(prompt, "check status");
            }
            _ => panic!("expected CronFired, got {:?}", parsed),
        }
    }

    #[test]
    fn compaction_surface_is_implementable() {
        use kosong_rs::message::Message;
        use kosong_rs::provider::Tool;

        struct Dummy;
        #[async_trait::async_trait]
        impl TurnContext for Dummy {
            fn append_user_message(&self, _content: Vec<ContentPart>, _origin: PromptOrigin) {}
            fn append_message(&self, _message: ContextMessage) {}
            fn messages(&self) -> Vec<Message> {
                vec![]
            }
            fn history(&self) -> Vec<ContextMessage> {
                vec![]
            }
            fn token_count(&self) -> i64 {
                0
            }
            fn token_count_with_pending(&self) -> i64 {
                0
            }
            fn apply_compaction(&self, _result: crate::records::nested::CompactionResult) {}
            fn project(&self, messages: &[ContextMessage]) -> Vec<Message> {
                messages.iter().map(|m| m.message.clone()).collect()
            }
            fn append_loop_event(&self, _event: LoopRecordedEvent) {}
            fn has_open_steps(&self) -> bool {
                false
            }
            fn clear(&self) {}
            fn last_assistant_at_ms(&self) -> Option<i64> {
                None
            }
            fn append_system_reminder(&self, _content: &str, _origin: PromptOrigin) {}
        }
        impl TurnTools for Dummy {
            fn loop_tools(&self) -> Vec<Arc<dyn ExecutableTool>> {
                vec![]
            }
            fn store_data(&self) -> std::collections::HashMap<String, serde_json::Value> {
                std::collections::HashMap::new()
            }
        }
        #[async_trait::async_trait]
        impl TurnSessionMode for Dummy {
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
        impl TurnFullCompaction for Dummy {
            fn reset_for_turn(&self, _agent: Arc<dyn TurnAgent>) {}
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
        impl TurnMicroCompaction for Dummy {
            fn detect(&self, _agent: Arc<dyn TurnAgent>) {}
            fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage> {
                messages.to_vec()
            }
            fn reset(&self, _max_cutoff: usize) {}
        }
        #[async_trait::async_trait]
        impl TurnSplitPlanCheckpoint for Dummy {
            async fn before_step(&self, _agent: Arc<dyn TurnAgent>, _signal: AbortSignal) {}
            fn reset(&self) {}
        }
        #[async_trait::async_trait]
        impl TurnNormalTaskCheckpoint for Dummy {
            async fn before_step(&self, _agent: Arc<dyn TurnAgent>, _signal: AbortSignal) {}
            fn reset(&self) {}
        }
        #[async_trait::async_trait]
        impl TurnLlmResolver for Dummy {
            fn refresh_llm(&self) {}
            fn llm(&self) -> Arc<dyn crate::agent_loop::llm::Llm> {
                panic!("noop")
            }
            async fn generate_one_off(
                &self,
                _provider: Box<dyn ChatProvider + Send>,
                _system_prompt: String,
                _tools: Vec<Tool>,
                _messages: Vec<Message>,
                _signal: AbortSignal,
            ) -> Result<CompactGenerateResult, anyhow::Error> {
                Ok(CompactGenerateResult::default())
            }
        }

        let _: Arc<dyn TurnContext> = Arc::new(Dummy);
        let _: Arc<dyn TurnTools> = Arc::new(Dummy);
        let _: Arc<dyn TurnSessionMode> = Arc::new(Dummy);
    }

    #[test]
    fn compaction_event_round_trips_json() {
        let event = AgentEvent::CompactionStarted {
            trigger: "auto".into(),
            instruction: Some("focus on code".into()),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"compaction.started\""));
        assert!(json.contains("\"trigger\":\"auto\""));

        let event2 = AgentEvent::CompactionCompleted {
            result: crate::records::nested::CompactionResult {
                summary: "summary".into(),
                compacted_count: 3,
                tokens_before: 100,
                tokens_after: 20,
            },
        };
        let json2 = serde_json::to_string(&event2).unwrap();
        let back: AgentEvent = serde_json::from_str(&json2).unwrap();
        assert_eq!(back, event2);
    }

    #[test]
    fn compaction_blocked_round_trips_json() {
        let event = AgentEvent::CompactionBlocked { turn_id: 42 };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"compaction.blocked\""));
        let back: AgentEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }
}
