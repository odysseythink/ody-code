# 4.3.5 Part 1 — TurnAgent trait & event/error types

**Scope:** 定义 `TurnFlow` 所需的最小 Agent 宿主接口（`TurnAgent` 及其子系统访问 trait）、turn 级事件枚举（与 TS `AgentEvent` 对齐）以及错误分类类型。本 part 不实现状态机，只钉死契约，使后续 `turn.md` 和 `adapter.md` 能并行开发。

---

## Task 1: Scaffold `turn` module and define `TurnContext`

**Depends on:** none（复用 4.3.1 `context::types::{ContextMessage, PromptOrigin}` 与 4.3.4 `agent_loop::types::LoopRecordedEvent`）

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/lib.rs:10`（新增 `pub mod turn;`）
- Create: `rust-ody/crates/agent-rs/src/turn/mod.rs`
- Create: `rust-ody/crates/agent-rs/src/turn/types.rs`

### Steps

- [ ] Write the failing test.

在 `rust-ody/crates/agent-rs/src/turn/types.rs` 末尾加入：

```rust
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
            self.calls.lock().unwrap().push("append_user_message".into());
        }
        fn append_message(&self, _message: ContextMessage) {}
        fn messages(&self) -> Vec<Message> {
            vec![Message { role: Role::User, name: None, content: vec![], tool_calls: vec![], tool_call_id: None, partial: None }]
        }
        fn append_loop_event(&self, _event: LoopRecordedEvent) {}
        fn has_open_steps(&self) -> bool { false }
        fn clear(&self) {}
    }

    #[test]
    fn turn_context_trait_is_callable() {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let ctx: Arc<dyn TurnContext> = Arc::new(DummyContext { calls: calls.clone() });
        ctx.append_user_message(vec![ContentPart::Text { text: "hi".into() }], USER_PROMPT_ORIGIN);
        assert_eq!(calls.lock().unwrap().as_slice(), &["append_user_message"]);
    }
}
```

- [ ] Run it and verify it FAILS.

```bash
cd rust-ody && cargo test -p agent-rs turn_context_trait_is_callable --no-run
```

预期失败：未定义 `TurnContext`、`USER_PROMPT_ORIGIN`、`LoopRecordedEvent` 等符号。

- [ ] Write the minimal implementation.

`rust-ody/crates/agent-rs/src/lib.rs`：

```rust
pub mod agent_loop;
pub mod config;
pub mod context;
pub mod permission;
pub mod records;
pub mod skill;
pub mod tool;
pub mod turn;
pub mod usage;

pub use records::*;
```

`rust-ody/crates/agent-rs/src/turn/mod.rs`：

```rust
pub mod canonical_args;
pub mod error;
pub mod kosong_llm;
pub mod remote_kosong_llm;
pub mod telemetry;
pub mod tool_dedup;
pub mod turn_flow;
pub mod types;

pub use types::*;
```

`rust-ody/crates/agent-rs/src/turn/types.rs` 顶部骨架：

```rust
use kosong_rs::message::{ContentPart, Message};

pub use crate::context::types::{ContextMessage, PromptOrigin, USER_PROMPT_ORIGIN};
pub use crate::agent_loop::types::LoopRecordedEvent;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoopControl {
    pub max_steps_per_turn: Option<u32>,
    pub max_retries_per_step: Option<u32>,
    pub reserved_context_size: Option<i64>,
}

#[async_trait::async_trait]
pub trait TurnContext: Send + Sync {
    fn append_user_message(&self, content: Vec<ContentPart>, origin: PromptOrigin);
    fn append_message(&self, message: ContextMessage);
    fn messages(&self) -> Vec<Message>;
    fn append_loop_event(&self, event: LoopRecordedEvent);
    fn has_open_steps(&self) -> bool;
    fn clear(&self);
}
```

- [ ] Run it and verify it PASSES.

```bash
cd rust-ody && cargo test -p agent-rs turn_context_trait_is_callable
```

预期：测试通过。

- [ ] Commit.

```bash
git add rust-ody/crates/agent-rs/src/lib.rs rust-ody/crates/agent-rs/src/turn rust-ody/crates/agent-rs/src/turn/types.rs
git commit -m "feat(agent-rs): scaffold turn module and TurnContext trait"
```

---

## Task 2: Define remaining subsystem access traits

**Depends on:** Task 1

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/turn/types.rs`

### Steps

- [ ] Write the failing test.

在 `rust-ody/crates/agent-rs/src/turn/types.rs` 的 `#[cfg(test)]` 模块中追加：

```rust
#[test]
fn all_subsystem_traits_are_implementable() {
    struct Dummy;
    impl TurnUsage for Dummy {
        fn begin_turn(&self) {}
        fn end_turn(&self) {}
        fn record(&self, _model: &str, _usage: kosong_rs::usage::TokenUsage, _scope: crate::records::nested::UsageRecordScope) {}
    }
    impl TurnConfig for Dummy {
        fn model(&self) -> String { "m".into() }
        fn model_alias(&self) -> Option<String> { Some("alias".into()) }
        fn system_prompt(&self) -> String { "".into() }
        fn thinking_level(&self) -> String { "off".into() }
        fn provider(&self) -> Box<dyn kosong_rs::provider::ChatProvider> { panic!("noop") }
        fn model_capabilities(&self) -> kosong_rs::provider::ModelCapability { kosong_rs::provider::ModelCapability::unknown() }
        fn loop_control(&self) -> Option<LoopControl> { None }
        fn has_model(&self) -> bool { true }
    }
    impl TurnTools for Dummy {
        fn loop_tools(&self) -> Vec<std::sync::Arc<dyn crate::agent_loop::types::ExecutableTool>> { vec![] }
    }
    impl TurnEventEmitter for Dummy {
        fn emit_event(&self, _event: AgentEvent) {}
    }

    let _: Box<dyn TurnUsage> = Box::new(Dummy);
    let _: Box<dyn TurnConfig> = Box::new(Dummy);
    let _: Box<dyn TurnTools> = Box::new(Dummy);
    let _: Box<dyn TurnEventEmitter> = Box::new(Dummy);
}
```

- [ ] Run it and verify it FAILS.

```bash
cd rust-ody && cargo test -p agent-rs all_subsystem_traits_are_implementable --no-run
```

预期失败：`TurnUsage`、`TurnConfig`、`TurnTools`、`TurnEventEmitter`、`AgentEvent` 未定义。

- [ ] Write the minimal implementation.

在 `rust-ody/crates/agent-rs/src/turn/types.rs` 中追加以下内容（保留 Task 1 的 `TurnContext`）：

```rust
use kosong_rs::message::{ContentPart, Message, ToolCall};
use kosong_rs::provider::{AbortSignal, ChatProvider, ModelCapability};
use kosong_rs::usage::TokenUsage;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::agent_loop::types::{
    AuthorizeToolExecutionResult, ExecutableTool, ExecutableToolResult, FinalizeToolResultContext,
    PrepareToolExecutionResult, ResolvedToolExecutionHookContext, RunnableToolExecution,
    ToolExecutionHookContext,
};
use crate::records::nested::{UsageRecordScope, GoalStatus, GoalBudgetLimits};
use crate::records::AgentRecord;

pub trait TurnUsage: Send + Sync {
    fn begin_turn(&self);
    fn end_turn(&self);
    fn record(&self, model: &str, usage: TokenUsage, scope: UsageRecordScope);
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
}

pub trait TurnTools: Send + Sync {
    fn loop_tools(&self) -> Vec<Arc<dyn ExecutableTool>>;
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
    fn reset_for_turn(&self);
    async fn before_step(&self, signal: AbortSignal);
    async fn after_step(&self);
    async fn handle_overflow_error(&self, signal: AbortSignal, error: anyhow::Error);
}

pub trait TurnMicroCompaction: Send + Sync {
    fn detect(&self);
}

#[async_trait::async_trait]
pub trait TurnSplitPlanCheckpoint: Send + Sync {
    async fn before_step(&self, signal: AbortSignal);
}

#[async_trait::async_trait]
pub trait TurnNormalTaskCheckpoint: Send + Sync {
    async fn before_step(&self, signal: AbortSignal);
}

pub trait TurnSessionMode: Send + Sync {
    fn is_active(&self) -> bool;
    fn kind(&self) -> Option<String>;
}

#[derive(Debug, Clone, Default)]
pub struct GoalSnapshot {
    pub status: GoalStatus,
    pub budget: GoalBudgetLimits,
}

#[async_trait::async_trait]
pub trait TurnGoal: Send + Sync {
    fn get_goal(&self) -> Option<GoalSnapshot>;
    async fn increment_turn(&self);
    async fn mark_blocked(&self, reason: &str);
    async fn pause_on_interrupt(&self, reason: &str);
    async fn pause_active_goal(&self, actor: &str, reason: &str);
    async fn record_token_usage(&self, token_delta: i64, agent_id: &str, agent_type: &str, source: &str) -> Option<GoalSnapshot>;
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
    async fn wait_for_initial_load(&self, signal: AbortSignal);
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
}
```

- [ ] Run it and verify it PASSES.

```bash
cd rust-ody && cargo test -p agent-rs all_subsystem_traits_are_implementable --no-run
```

预期：编译通过（测试不运行实体逻辑，只验证 trait 可实现）。

- [ ] Commit.

```bash
git add rust-ody/crates/agent-rs/src/turn/types.rs
git commit -m "feat(agent-rs): add TurnFlow subsystem access traits"
```

---

## Task 3: Define `TurnAgent` aggregate trait + events + errors

**Depends on:** Task 2

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/turn/types.rs`

### Steps

- [ ] Write the failing test.

在 `rust-ody/crates/agent-rs/src/turn/types.rs` 的 `#[cfg(test)]` 模块中追加：

```rust
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
```

- [ ] Run it and verify it FAILS.

```bash
cd rust-ody && cargo test -p agent-rs agent_event_round_trips_json --no-run
```

预期失败：`TurnAgent`、`AgentEvent`、`TurnErrorSummary` 未定义。

- [ ] Write the minimal实现.

在 `rust-ody/crates/agent-rs/src/turn/types.rs` 中追加：

```rust
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
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentEvent {
    #[serde(rename = "turn.started")]
    TurnStarted { turn_id: i64, origin: PromptOrigin },
    #[serde(rename = "turn.ended")]
    TurnEnded(TurnEndedEvent),
    #[serde(rename = "turn.step.started")]
    TurnStepStarted { turn_id: i64, step: u32, step_id: String },
    #[serde(rename = "turn.step.completed")]
    TurnStepCompleted {
        turn_id: i64,
        step: u32,
        step_id: String,
        usage: TokenUsage,
        #[serde(skip_serializing_if = "Option::is_none")]
        finish_reason: Option<String>,
        #[serde(rename = "llmFirstTokenLatencyMs", skip_serializing_if = "Option::is_none")]
        llm_first_token_latency_ms: Option<i64>,
        #[serde(rename = "llmStreamDurationMs", skip_serializing_if = "Option::is_none")]
        llm_stream_duration_ms: Option<i64>,
        #[serde(rename = "providerFinishReason", skip_serializing_if = "Option::is_none")]
        provider_finish_reason: Option<String>,
        #[serde(rename = "rawFinishReason", skip_serializing_if = "Option::is_none")]
        raw_finish_reason: Option<String>,
    },
    #[serde(rename = "turn.step.retrying")]
    TurnStepRetrying(StepRetryingEvent),
    #[serde(rename = "turn.step.interrupted")]
    TurnStepInterrupted {
        turn_id: i64,
        step: u32,
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    #[serde(rename = "assistant.delta")]
    AssistantDelta { turn_id: i64, delta: String },
    #[serde(rename = "thinking.delta")]
    ThinkingDelta { turn_id: i64, delta: String },
    #[serde(rename = "tool.call.started")]
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
    #[serde(rename = "tool.result")]
    ToolResult {
        turn_id: i64,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        output: crate::records::nested::ExecutableToolOutput,
        #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    #[serde(rename = "tool.call.delta")]
    ToolCallDelta {
        turn_id: i64,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(rename = "argumentsPart", skip_serializing_if = "Option::is_none")]
        arguments_part: Option<String>,
    },
    #[serde(rename = "tool.progress")]
    ToolProgress {
        turn_id: i64,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        update: crate::records::nested::ToolUpdate,
    },
    #[serde(rename = "hook.result")]
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
}
```

- [ ] Run it and verify it PASSES.

```bash
cd rust-ody && cargo test -p agent-rs agent_event_round_trips_json turn_error_summary_serializes_expected_shape
```

预期：两个测试均通过。

- [ ] Run whole-tree typecheck.

```bash
cd rust-ody && cargo check -p agent-rs --workspace --tests
```

预期：`agent-rs` 编译通过；由于只新增模块，其他 crate 不受影响。

- [ ] Commit.

```bash
git add rust-ody/crates/agent-rs/src/turn/types.rs
git commit -m "feat(agent-rs): define TurnAgent aggregate trait and turn events"
```

---

## Local Self-Review

- [x] 1. Spec-coverage：本 part 覆盖 roadmap 4.3.5 的接口契约层（4.3.5.1–4.3.5.6 所需的 `TurnAgent` / 事件类型），无 GAP。
- [x] 2. Placeholder scan：无 TODO/TBD；所有 trait 方法均带完整签名与默认/实现说明。
- [x] 3. No phantom tasks：3 个 task 均产出可验证代码/编译/测试变更。
- [x] 4. Dependency soundness：Task 1 → Task 2 → Task 3 单向；无 forward reference。
- [x] 5. Caller & build soundness：`lib.rs` 新增 `pub mod turn;` 在同一 task 内完成；Task 3 以 `cargo check -p agent-rs --workspace --tests` 收尾。
- [x] 6. Test-the-risk：Task 1 验证 `TurnContext` 方法调用；Task 3 验证事件 JSON 往返与错误摘要形状。
- [x] 7. Type 一致性：`AgentEvent` 字段名 / camelCase / 枚举 tag 与 TS `AgentEvent` 对齐；复用 `ContextMessage`、`PromptOrigin`、`LoopRecordedEvent`、`TokenUsage`、`ExecutableToolOutput`、`ToolUpdate` 等已有类型。
