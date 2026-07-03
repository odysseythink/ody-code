# Phase A: 类型、事件、LLM trait、工具访问与调度器

本部分把 TS `packages/agent-core/src/loop/` 的公共契约迁移到 `agent-rs::agent_loop`，保持字段名/语义与 TS 逐字段一致。所有代码先测试后实现；本部分结束后 `cargo check -p agent-rs --tests` 必须绿。

---

## Phase A 任务依赖图

```text
Task 1 (scaffold + deps)
    │
    ├──▶ Task 2 (loop types + LLM trait)
    │
    ├──▶ Task 3 (loop events + dispatcher)
    │
    └──▶ Task 4 (tool access + scheduler)
```

Task 2/3/4 都依赖 Task 1，但彼此独立，可并行开发；建议按 2 → 3 → 4 顺序合入，因为 Task 4 的测试会用到 Task 2 的 `ExecutableToolResult`。

---

### Task 1: 添加依赖并搭建 `agent_loop` 模块骨架

**Depends on:** none（硬前置为 4.3.0 已落地的 `agent-rs` crate 与 `kosong-rs` 类型）

**Files:**
- Modify: `rust-ody/crates/agent-rs/Cargo.toml`
- Modify: `rust-ody/crates/agent-rs/src/lib.rs`
- Create: `rust-ody/crates/agent-rs/src/agent_loop/mod.rs`
- Create: `rust-ody/crates/agent-rs/src/agent_loop/types.rs`（仅 `TurnResult` 与 `LoopTurnStopReason` 占位，供本 task 编译测试）
- Test: `rust-ody/crates/agent-rs/tests/loop_scaffold.rs`

#### 步骤

- [ ] 写失败测试 `tests/loop_scaffold.rs`：

```rust
use agent_rs::agent_loop::types::{LoopTurnStopReason, TurnResult};
use kosong_rs::usage::TokenUsage;
use serde_json;

#[test]
fn turn_result_serializes_like_ts() {
    let result = TurnResult {
        stop_reason: LoopTurnStopReason::EndTurn,
        steps: 1,
        usage: TokenUsage {
            input_other: 10,
            output: 5,
            input_cache_read: 0,
            input_cache_creation: 0,
        },
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("\"stopReason\":\"end_turn\""), "{}", json);
    assert!(json.contains("\"steps\":1"), "{}", json);
}
```

运行并确认失败：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo check -p agent-rs --tests
```

预期失败：`error[E0433]: failed to resolve: use of undeclared crate or module `agent_loop``。

- [ ] 最小实现：

`rust-ody/crates/agent-rs/Cargo.toml` 在 `[dependencies]` 追加：

```toml
jsonschema = "0.29"
regex = "1"
tracing = "0.1"
```

`rust-ody/crates/agent-rs/src/lib.rs` 在 `pub mod usage;` 后追加：

```rust
pub mod agent_loop;
```

`rust-ody/crates/agent-rs/src/agent_loop/mod.rs`：

```rust
pub mod events;
pub mod llm;
pub mod tool_access;
pub mod tool_scheduler;
pub mod types;
```

`rust-ody/crates/agent-rs/src/agent_loop/types.rs`（Task 1 最小占位，后续 Task 2 扩展）：

```rust
use kosong_rs::usage::TokenUsage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopTurnStopReason {
    EndTurn,
    MaxTokens,
    ToolUse,
    Filtered,
    Paused,
    Unknown,
    Aborted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnResult {
    pub stop_reason: LoopTurnStopReason,
    pub steps: u32,
    pub usage: TokenUsage,
}
```

- [ ] 运行测试并确认通过：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo test -p agent-rs --test loop_scaffold
```

预期：`test turn_result_serializes_like_ts ... ok`。

- [ ] 提交：`feat(agent-rs): scaffold agent_loop module with deps and TurnResult`

---

### Task 2: 迁移 loop 类型与 `Llm` trait

**Depends on:** Task 1

**Files:**
- Create/Complete: `rust-ody/crates/agent-rs/src/agent_loop/types.rs`
- Create: `rust-ody/crates/agent-rs/src/agent_loop/llm.rs`
- Test: `rust-ody/crates/agent-rs/tests/loop_types.rs`
- Test: `rust-ody/crates/agent-rs/tests/loop_llm.rs`

#### 设计约束

- `ExecutableTool` 是 trait，工具实现者只需实现 `resolve_execution`；返回的 `ToolExecution` 既可以是立即 error，也可以是 `RunnableToolExecution`（含 async `execute`）。
- `LoopHooks` 用 trait object 表示，避免把每个 hook 都写成独立的泛型参数；`LoopHooks` 本身是一个 struct of `Option<Box<dyn ...>>`。
- 所有回调签名与 TS 一致：prepare/authorize/finalize 是 async 且可能失败；失败由调用方按 TS 语义捕获并转成 error result。
- `Llm::chat` 接收 `LlmChatParams`，回调用 `Box<dyn Fn(...)+Send>`；`on_text_part`/`on_think_part` 是 async，用 `Box<dyn Fn(...) -> Pin<Box<dyn Future<...>>> + Send>`。

#### 步骤

- [ ] 写失败测试 `tests/loop_types.rs` 与 `tests/loop_llm.rs`：

`tests/loop_types.rs`：

```rust
use agent_rs::agent_loop::types::{
    ExecutableTool, ExecutableToolContext, ExecutableToolResult, LoopHooks, LoopStepHookContext,
    ToolExecution, TurnResult,
};
use agent_rs::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse, LlmFactoryConfig};
use kosong_rs::message::ContentPart;
use kosong_rs::provider::{AbortSignal, ModelCapability, Tool};
use kosong_rs::usage::TokenUsage;
use serde_json::json;
use std::future::Future;
use std::pin::Pin;

// 一个必须保留的工具：名字含 "read_file" 但不应被当成内置 Read
struct EchoTool;

#[async_trait::async_trait]
impl ExecutableTool for EchoTool {
    async fn resolve_execution(
        &self,
        input: serde_json::Value,
    ) -> Result<ToolExecution, anyhow::Error> {
        Ok(ToolExecution::Runnable(agent_rs::agent_loop::types::RunnableToolExecution {
            approval_rule: "auto".to_string(),
            execute: Box::new(move |ctx: ExecutableToolContext| {
                Box::pin(async move {
                    Ok(ExecutableToolResult::Success(
                        agent_rs::agent_loop::types::ExecutableToolSuccessResult {
                            output: agent_rs::agent_loop::types::ExecutableToolOutput::Text(
                                format!("echo:{}:{}", ctx.tool_call_id, input["x"].as_str().unwrap_or("?")),
                            ),
                            message: None,
                            stop_turn: None,
                        },
                    ))
                })
            }),
            accesses: agent_rs::agent_loop::tool_access::ToolAccesses::none(),
            description: Some("echo".to_string()),
            display: None,
            stop_batch_after_this: None,
        }))
    }

    fn name(&self) -> &str { "echo" }
    fn description(&self) -> &str { "echo tool" }
    fn parameters(&self) -> &serde_json::Value { &json!({"type":"object"}) }
}

#[tokio::test]
async fn executable_tool_runs_and_returns_text_output() {
    let tool = EchoTool;
    let exec = tool.resolve_execution(json!({"x":"hi"})).await.unwrap();
    match exec {
        ToolExecution::Runnable(r) => {
            let ctx = ExecutableToolContext {
                turn_id: "t1".into(),
                tool_call_id: "tc1".into(),
                metadata: None,
                signal: AbortSignal::new(),
                on_update: None,
            };
            let result = (r.execute)(ctx).await.unwrap();
            let text = match result {
                ExecutableToolResult::Success(s) => match s.output {
                    agent_rs::agent_loop::types::ExecutableToolOutput::Text(t) => t,
                    _ => panic!("expected text"),
                },
                ExecutableToolResult::Error(_) => panic!("expected success"),
            };
            assert_eq!(text, "echo:tc1:hi");
        }
        _ => panic!("expected runnable"),
    }
}

#[test]
fn loop_hooks_default_all_none() {
    let hooks = LoopHooks::default();
    assert!(hooks.before_step.is_none());
    assert!(hooks.after_step.is_none());
    assert!(hooks.prepare_tool_execution.is_none());
    assert!(hooks.authorize_tool_execution.is_none());
    assert!(hooks.finalize_tool_result.is_none());
    assert!(hooks.should_continue_after_stop.is_none());
}
```

`tests/loop_llm.rs`：

```rust
use agent_rs::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse, LlmFactoryConfig, ToolCallDelta};
use agent_rs::agent_loop::types::LoopStepStopReason;
use kosong_rs::message::{ContentPart, Message, Role};
use kosong_rs::provider::{AbortSignal, FinishReason, ModelCapability, Tool};
use kosong_rs::usage::TokenUsage;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct MockLlm {
    response: LlmChatResponse,
}

#[async_trait::async_trait]
impl Llm for MockLlm {
    fn system_prompt(&self) -> &str { "sys" }
    fn model_name(&self) -> &str { "mock" }
    fn capability(&self) -> Option<&ModelCapability> { None }

    async fn chat(&self, _params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        Ok(self.response.clone())
    }
}

#[tokio::test]
async fn mock_llm_chat_returns_tool_call_response() {
    let response = LlmChatResponse {
        tool_calls: vec![kosong_rs::message::ToolCall {
            id: "tc1".into(),
            name: "echo".into(),
            arguments: Some("{\"x\":\"hi\"}".into()),
        }],
        provider_finish_reason: Some(FinishReason::ToolCalls),
        raw_finish_reason: Some("tool_calls".into()),
        usage: TokenUsage { input_other: 3, output: 2, input_cache_read: 0, input_cache_creation: 0 },
        stream_timing: None,
    };
    let llm = MockLlm { response: response.clone() };
    let params = LlmChatParams {
        messages: vec![Message { role: Role::User, content: vec![ContentPart::Text { text: "go".into() }], name: None }],
        tools: vec![],
        signal: AbortSignal::new(),
        request_log_context: None,
        on_text_delta: None,
        on_think_delta: None,
        on_tool_call_delta: None,
        on_text_part: None,
        on_think_part: None,
    };
    let got = llm.chat(params).await.unwrap();
    assert_eq!(got.tool_calls.len(), 1);
    assert_eq!(got.tool_calls[0].id, "tc1");
    assert_eq!(got.usage.output, 2);
}
```

运行并确认失败（类型未实现）：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo test -p agent-rs --test loop_types --test loop_llm
```

- [ ] 实现完整 `types.rs` 与 `llm.rs`：

`rust-ody/crates/agent-rs/src/agent_loop/types.rs`：

```rust
use std::future::Future;
use std::pin::Pin;

use kosong_rs::message::{ContentPart, ToolCall};
use kosong_rs::provider::{AbortSignal, ModelCapability, Tool};
use kosong_rs::usage::TokenUsage;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::agent_loop::tool_access::ToolAccesses;

pub type ExecutableToolOutput = crate::records::nested::ExecutableToolOutput;
pub type ExecutableToolSuccessResult = crate::records::nested::ExecutableToolSuccessResult;
pub type ExecutableToolErrorResult = crate::records::nested::ExecutableToolErrorResult;
pub type ExecutableToolResult = crate::records::nested::ExecutableToolResult;
pub type ToolUpdate = crate::records::nested::ToolUpdate;

pub struct ExecutableToolContext {
    pub turn_id: String,
    pub tool_call_id: String,
    pub metadata: Option<JsonValue>,
    pub signal: AbortSignal,
    pub on_update: Option<Box<dyn Fn(ToolUpdate) + Send + Sync>>,
}

pub struct RunnableToolExecution {
    pub is_error: Option<bool>,
    pub accesses: Option<ToolAccesses>,
    pub display: Option<JsonValue>,
    pub description: Option<String>,
    pub stop_batch_after_this: Option<bool>,
    pub approval_rule: String,
    pub matches_rule: Option<Box<dyn Fn(&str) -> bool + Send + Sync>>,
    pub execute: Box<
        dyn Fn(ExecutableToolContext) -> Pin<Box<dyn Future<Output = Result<ExecutableToolResult, anyhow::Error>> + Send>>
            + Send
            + Sync,
    >,
}

pub enum ToolExecution {
    Runnable(RunnableToolExecution),
    Error(ExecutableToolErrorResult),
}

#[async_trait::async_trait]
pub trait ExecutableTool: Tool + Send + Sync {
    async fn resolve_execution(&self, input: JsonValue) -> Result<ToolExecution, anyhow::Error>;
}

pub type LoopMessageBuilder = Box<
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<Vec<kosong_rs::message::Message>, anyhow::Error>> + Send>>
        + Send
        + Sync,
>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopStepStopReason {
    EndTurn,
    MaxTokens,
    ToolUse,
    Filtered,
    Paused,
    Unknown,
}

pub type LoopTerminalStepStopReason = LoopStepStopReason;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopTurnStopReason {
    EndTurn,
    MaxTokens,
    Filtered,
    Paused,
    Unknown,
    Aborted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnResult {
    pub stop_reason: LoopTurnStopReason,
    pub steps: u32,
    pub usage: TokenUsage,
}

pub struct LoopStepHookContext<'a> {
    pub turn_id: &'a str,
    pub step_number: u32,
    pub signal: AbortSignal,
    pub llm: &'a dyn crate::agent_loop::llm::Llm,
}

pub struct ToolExecutionHookContext<'a> {
    pub turn_id: &'a str,
    pub step_number: u32,
    pub signal: AbortSignal,
    pub llm: &'a dyn crate::agent_loop::llm::Llm,
    pub tool_call: &'a ToolCall,
    pub tool: Option<&'a dyn ExecutableTool>,
    pub args: JsonValue,
}

pub struct ResolvedToolExecutionHookContext<'a> {
    pub turn_id: &'a str,
    pub step_number: u32,
    pub signal: AbortSignal,
    pub llm: &'a dyn crate::agent_loop::llm::Llm,
    pub tool_call: &'a ToolCall,
    pub tool: Option<&'a dyn ExecutableTool>,
    pub args: JsonValue,
    pub execution: &'a RunnableToolExecution,
}

#[derive(Debug, Clone, Default)]
pub struct AuthorizeToolExecutionResult {
    pub block: Option<bool>,
    pub reason: Option<String>,
    pub synthetic_result: Option<ExecutableToolResult>,
    pub execution_metadata: Option<JsonValue>,
}

#[derive(Debug, Clone, Default)]
pub struct PrepareToolExecutionResult {
    pub block: Option<bool>,
    pub reason: Option<String>,
    pub synthetic_result: Option<ExecutableToolResult>,
    pub execution_metadata: Option<JsonValue>,
    pub updated_args: Option<JsonValue>,
}

pub struct FinalizeToolResultContext<'a> {
    pub turn_id: &'a str,
    pub step_number: u32,
    pub signal: AbortSignal,
    pub llm: &'a dyn crate::agent_loop::llm::Llm,
    pub tool_call: &'a ToolCall,
    pub tool: Option<&'a dyn ExecutableTool>,
    pub args: JsonValue,
    pub result: ExecutableToolResult,
}

pub struct LoopAfterStepContext<'a> {
    pub turn_id: &'a str,
    pub step_number: u32,
    pub signal: AbortSignal,
    pub llm: &'a dyn crate::agent_loop::llm::Llm,
    pub usage: TokenUsage,
    pub stop_reason: LoopStepStopReason,
}

pub struct LoopStoppedStepContext<'a> {
    pub turn_id: &'a str,
    pub step_number: u32,
    pub signal: AbortSignal,
    pub llm: &'a dyn crate::agent_loop::llm::Llm,
    pub usage: TokenUsage,
    pub stop_reason: LoopTerminalStepStopReason,
}

#[derive(Debug, Clone, Default)]
pub struct BeforeStepResult {
    pub block: Option<bool>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AfterStepResult {
    pub stop_turn: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct RecordStepUsageResult {
    pub stop_turn: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct ShouldContinueAfterStopResult {
    pub continue_: bool,
}

#[async_trait::async_trait]
pub trait BeforeStepHook: Send + Sync {
    async fn before_step(&self, ctx: LoopStepHookContext<'_>) -> Result<Option<BeforeStepResult>, anyhow::Error>;
}

#[async_trait::async_trait]
pub trait AfterStepHook: Send + Sync {
    async fn after_step(&self, ctx: LoopAfterStepContext<'_>) -> Result<Option<AfterStepResult>, anyhow::Error>;
}

#[async_trait::async_trait]
pub trait PrepareToolExecutionHook: Send + Sync {
    async fn prepare_tool_execution(
        &self,
        ctx: ToolExecutionHookContext<'_>,
    ) -> Result<Option<PrepareToolExecutionResult>, anyhow::Error>;
}

#[async_trait::async_trait]
pub trait AuthorizeToolExecutionHook: Send + Sync {
    async fn authorize_tool_execution(
        &self,
        ctx: ResolvedToolExecutionHookContext<'_>,
    ) -> Result<Option<AuthorizeToolExecutionResult>, anyhow::Error>;
}

#[async_trait::async_trait]
pub trait FinalizeToolResultHook: Send + Sync {
    async fn finalize_tool_result(
        &self,
        ctx: FinalizeToolResultContext<'_>,
    ) -> Result<Option<ExecutableToolResult>, anyhow::Error>;
}

#[async_trait::async_trait]
pub trait ShouldContinueAfterStopHook: Send + Sync {
    async fn should_continue_after_stop(
        &self,
        ctx: LoopStoppedStepContext<'_>,
    ) -> Result<Option<ShouldContinueAfterStopResult>, anyhow::Error>;
}

#[derive(Default)]
pub struct LoopHooks {
    pub before_step: Option<Box<dyn BeforeStepHook>>,
    pub after_step: Option<Box<dyn AfterStepHook>>,
    pub prepare_tool_execution: Option<Box<dyn PrepareToolExecutionHook>>,
    pub authorize_tool_execution: Option<Box<dyn AuthorizeToolExecutionHook>>,
    pub finalize_tool_result: Option<Box<dyn FinalizeToolResultHook>>,
    pub should_continue_after_stop: Option<Box<dyn ShouldContinueAfterStopHook>>,
}

pub struct RunTurnInput {
    pub turn_id: String,
    pub signal: AbortSignal,
    pub llm: Box<dyn crate::agent_loop::llm::Llm>,
    pub build_messages: LoopMessageBuilder,
    pub dispatch_event: Box<dyn crate::agent_loop::events::LoopEventDispatcher>,
    pub tools: Option<Vec<Box<dyn ExecutableTool>>>,
    pub hooks: Option<LoopHooks>,
    pub max_steps: Option<u32>,
    pub max_retry_attempts: Option<u32>,
    pub record_step_usage: Option<
        Box<
            dyn Fn(TokenUsage) -> Pin<Box<dyn Future<Output = Result<Option<RecordStepUsageResult>, anyhow::Error>> + Send>>
                + Send
                + Sync,
        >,
    >,
}
```

`rust-ody/crates/agent-rs/src/agent_loop/llm.rs`：

```rust
use std::future::Future;
use std::pin::Pin;

use kosong_rs::message::{ContentPart, FinishReason, Message, TextPart, ThinkPart, Tool, ToolCall};
use kosong_rs::provider::{AbortSignal, ModelCapability};
use kosong_rs::usage::TokenUsage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default)]
pub struct ToolCallDelta {
    pub tool_call_id: String,
    pub name: Option<String>,
    pub arguments_part: Option<String>,
}

#[derive(Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct LlmRequestLogContext {
    pub turn_id: Option<String>,
    pub step: Option<u32>,
    pub step_uuid: Option<String>,
    pub attempt: Option<u32>,
    pub max_attempts: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct LlmStreamTiming {
    pub first_token_latency_ms: u64,
    pub stream_duration_ms: u64,
}

pub type TextDeltaCallback = Box<dyn Fn(String) + Send + Sync>;
pub type ThinkDeltaCallback = Box<dyn Fn(String) + Send + Sync>;
pub type ToolCallDeltaCallback = Box<dyn Fn(ToolCallDelta) + Send + Sync>;
pub type TextPartCallback = Box<dyn Fn(TextPart) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;
pub type ThinkPartCallback = Box<dyn Fn(ThinkPart) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

pub struct LlmChatParams {
    pub messages: Vec<Message>,
    pub tools: Vec<Tool>,
    pub signal: AbortSignal,
    pub request_log_context: Option<LlmRequestLogContext>,
    pub on_text_delta: Option<TextDeltaCallback>,
    pub on_think_delta: Option<ThinkDeltaCallback>,
    pub on_tool_call_delta: Option<ToolCallDeltaCallback>,
    pub on_text_part: Option<TextPartCallback>,
    pub on_think_part: Option<ThinkPartCallback>,
}

#[derive(Debug, Clone, Default)]
pub struct LlmChatResponse {
    pub tool_calls: Vec<ToolCall>,
    pub provider_finish_reason: Option<FinishReason>,
    pub raw_finish_reason: Option<String>,
    pub usage: TokenUsage,
    pub stream_timing: Option<LlmStreamTiming>,
}

#[derive(Debug, Clone, Default)]
pub struct LlmFactoryConfig {
    pub model_name: String,
    pub system_prompt: String,
    pub capability: Option<ModelCapability>,
    pub provider: Option<kosong_rs::provider::ProviderConfig>,
}

#[async_trait::async_trait]
pub trait Llm: Send + Sync {
    fn system_prompt(&self) -> &str;
    fn model_name(&self) -> &str;
    fn capability(&self) -> Option<&ModelCapability> { None }
    fn is_retryable_error(&self, _error: &dyn std::error::Error) -> bool { false }
    async fn chat(&self, params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error>;
}
```

- [ ] 运行测试：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo test -p agent-rs --test loop_types --test loop_llm
```

- [ ] 提交：`feat(agent-rs): loop types and Llm trait`

---

### Task 3: 迁移 loop 事件与统一 dispatcher

**Depends on:** Task 2（`Llm` trait 与 `LoopStepHookContext` 等类型已在 Task 2 定义；Task 3 只依赖这些签名存在）

**Files:**
- Create: `rust-ody/crates/agent-rs/src/agent_loop/events.rs`
- Test: `rust-ody/crates/agent-rs/tests/loop_events.rs`

#### 步骤

- [ ] 写失败测试 `tests/loop_events.rs`：

```rust
use agent_rs::agent_loop::events::{
    DefaultLoopEventDispatcher, LoopEventDispatcher, LoopInterruptReason, LoopLiveOnlyEvent,
    LoopRecordedEvent,
};
use agent_rs::records::nested::{ExecutableToolResult, ExecutableToolSuccessResult, ExecutableToolOutput};
use kosong_rs::usage::TokenUsage;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn dispatcher_appends_recorded_and_emits_live() {
    let recorded: Arc<Mutex<Vec<LoopRecordedEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let live: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let r = recorded.clone();
    let l = live.clone();
    let dispatcher = DefaultLoopEventDispatcher::new(
        move |event: LoopRecordedEvent| {
            let r = r.clone();
            async move {
                r.lock().unwrap().push(event);
                Ok(()) as Result<(), anyhow::Error>
            }
        },
        Some(Box::new(move |event| {
            if let agent_rs::agent_loop::events::LoopEvent::Live(LoopLiveOnlyEvent::TextDelta { delta }) = event {
                l.lock().unwrap().push(delta);
            }
        })),
    );

    dispatcher.dispatch_live(LoopLiveOnlyEvent::TextDelta { delta: "hello".into() });
    dispatcher.dispatch_recorded(LoopRecordedEvent::StepBegin {
        uuid: "s1".into(),
        turn_id: "t1".into(),
        step: 1,
    }).await.unwrap();

    assert_eq!(live.lock().unwrap().as_slice(), &["hello"]);
    assert_eq!(recorded.lock().unwrap().len(), 1);
}

#[test]
fn live_event_serializes_like_ts() {
    let event = LoopLiveOnlyEvent::TurnInterrupted {
        reason: LoopInterruptReason::MaxSteps,
        attempted_steps: 3,
        active_step: Some(2),
        message: Some("too many".into()),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"turn.interrupted\""), "{}", json);
    assert!(json.contains("\"attemptedSteps\":3"), "{}", json);
}
```

运行并确认失败。

- [ ] 实现 `events.rs`：

```rust
use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

use crate::records::nested::{ExecutableToolResult, ToolUpdate};

pub use crate::records::nested::LoopRecordedEvent;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepRetryingEvent {
    #[serde(rename = "turnId")]
    pub turn_id: String,
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
#[serde(rename_all = "camelCase", tag = "type")]
pub enum LoopLiveOnlyEvent {
    #[serde(rename = "turn.interrupted")]
    TurnInterrupted {
        reason: LoopInterruptReason,
        attempted_steps: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        active_step: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    #[serde(rename = "step.retrying")]
    StepRetrying(StepRetryingEvent),
    #[serde(rename = "text.delta")]
    TextDelta { delta: String },
    #[serde(rename = "thinking.delta")]
    ThinkingDelta { delta: String },
    #[serde(rename = "tool.call.delta")]
    ToolCallDelta {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(rename = "argumentsPart", skip_serializing_if = "Option::is_none")]
        arguments_part: Option<String>,
    },
    #[serde(rename = "tool.progress")]
    ToolProgress {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        update: ToolUpdate,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopInterruptReason {
    Aborted,
    MaxSteps,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LoopEvent {
    Recorded(LoopRecordedEvent),
    Live(LoopLiveOnlyEvent),
}

pub type LiveEventEmitter = Box<dyn Fn(LoopEvent) + Send + Sync>;

#[async_trait::async_trait]
pub trait LoopEventDispatcher: Send + Sync {
    async fn dispatch_recorded(&self, event: LoopRecordedEvent) -> Result<(), anyhow::Error>;
    fn dispatch_live(&self, event: LoopLiveOnlyEvent);
}

type AppendRecordFn = Box<
    dyn Fn(LoopRecordedEvent) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>> + Send + Sync,
>;

pub struct DefaultLoopEventDispatcher {
    append_record: AppendRecordFn,
    emit_live: Option<LiveEventEmitter>,
}

impl DefaultLoopEventDispatcher {
    pub fn new<F, Fut>(append_record: F, emit_live: Option<LiveEventEmitter>) -> Self
    where
        F: Fn(LoopRecordedEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), anyhow::Error>> + Send + 'static,
    {
        Self {
            append_record: Box::new(move |event| Box::pin(append_record(event))),
            emit_live,
        }
    }
}

#[async_trait::async_trait]
impl LoopEventDispatcher for DefaultLoopEventDispatcher {
    async fn dispatch_recorded(&self, event: LoopRecordedEvent) -> Result<(), anyhow::Error> {
        (self.append_record)(event.clone()).await?;
        if let Some(emit) = &self.emit_live {
            emit(LoopEvent::Recorded(event));
        }
        Ok(())
    }

    fn dispatch_live(&self, event: LoopLiveOnlyEvent) {
        if let Some(emit) = &self.emit_live {
            emit(LoopEvent::Live(event));
        }
    }
}
```

注意：测试里 `dispatch_live` 返回的是 `()`，而 trait 要求 `async fn dispatch_live` 返回 `()`；测试中 `.await` 合法。

- [ ] 运行测试：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo test -p agent-rs --test loop_events
```

- [ ] 提交：`feat(agent-rs): loop events and dispatcher`

---

### Task 4: 迁移 `tool_access` 与 `tool_scheduler`

**Depends on:** Task 2（`ToolAccesses`、`ExecutableToolResult` 等签名已存在）

**Files:**
- Create: `rust-ody/crates/agent-rs/src/agent_loop/tool_access.rs`
- Create: `rust-ody/crates/agent-rs/src/agent_loop/tool_scheduler.rs`
- Test: `rust-ody/crates/agent-rs/tests/tool_access.rs`
- Test: `rust-ody/crates/agent-rs/tests/tool_scheduler.rs`

#### 步骤

- [ ] 写失败测试 `tests/tool_access.rs` 与 `tests/tool_scheduler.rs`：

`tests/tool_access.rs`：

```rust
use agent_rs::agent_loop::tool_access::{ToolAccesses, ToolFileAccessOperation};

#[test]
fn read_and_search_do_not_conflict() {
    let a = ToolAccesses::read_file("/tmp/foo.txt");
    let b = ToolAccesses::search_tree("/tmp");
    assert!(!ToolAccesses::conflict(&a, &b));
}

#[test]
fn write_conflicts_with_recursive_read_under_same_tree() {
    let a = ToolAccesses::write_tree("/tmp");
    let b = ToolAccesses::read_file("/tmp/foo.txt");
    assert!(ToolAccesses::conflict(&a, &b));
}

#[test]
fn all_conflicts_with_everything() {
    let a = ToolAccesses::all();
    let b = ToolAccesses::read_file("/tmp/foo.txt");
    assert!(ToolAccesses::conflict(&a, &b));
    assert!(ToolAccesses::conflict(&b, &a));
}
```

`tests/tool_scheduler.rs`：

```rust
use agent_rs::agent_loop::tool_scheduler::{ToolCallTask, ToolScheduler};
use agent_rs::agent_loop::tool_access::ToolAccesses;
use std::time::{Duration, Instant};

#[tokio::test]
async fn scheduler_runs_non_conflicting_tasks_in_parallel() {
    let mut scheduler = ToolScheduler::new();
    let start = Instant::now();

    let t1 = scheduler.add(ToolCallTask {
        accesses: ToolAccesses::read_file("/tmp/a.txt"),
        start: Box::new(|| Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok("a".to_string()) as Result<String, anyhow::Error>
        })),
    }).await.unwrap();

    let t2 = scheduler.add(ToolCallTask {
        accesses: ToolAccesses::read_file("/tmp/b.txt"),
        start: Box::new(|| Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok("b".to_string())
        })),
    }).await.unwrap();

    let (r1, r2) = tokio::join!(t1, t2);
    assert_eq!(r1.unwrap(), "a");
    assert_eq!(r2.unwrap(), "b");
    assert!(start.elapsed() < Duration::from_millis(90), "parallel expected");
}

#[tokio::test]
async fn scheduler_serializes_conflicting_writes() {
    let mut scheduler = ToolScheduler::new();
    let start = Instant::now();

    let t1 = scheduler.add(ToolCallTask {
        accesses: ToolAccesses::write_file("/tmp/x.txt"),
        start: Box::new(|| Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok("first".to_string()) as Result<String, anyhow::Error>
        })),
    }).await.unwrap();

    let t2 = scheduler.add(ToolCallTask {
        accesses: ToolAccesses::write_file("/tmp/x.txt"),
        start: Box::new(|| Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok("second".to_string())
        })),
    }).await.unwrap();

    let (r1, r2) = tokio::join!(t1, t2);
    assert_eq!(r1.unwrap(), "first");
    assert_eq!(r2.unwrap(), "second");
    assert!(start.elapsed() >= Duration::from_millis(95), "serial expected");
}
```

运行并确认失败。

- [ ] 实现 `tool_access.rs` 与 `tool_scheduler.rs`：

`rust-ody/crates/agent-rs/src/agent_loop/tool_access.rs`：

```rust
use serde::{Deserialize, Serialize};

pub type ToolFileAccessOperation = &'static str;
pub const FILE_READ: &str = "read";
pub const FILE_WRITE: &str = "write";
pub const FILE_READWRITE: &str = "readwrite";
pub const FILE_SEARCH: &str = "search";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ToolResourceAccess {
    #[serde(rename = "file")]
    File {
        operation: String,
        path: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        recursive: Option<bool>,
    },
    #[serde(rename = "all")]
    All,
}

pub type ToolAccesses = Vec<ToolResourceAccess>;

impl ToolAccesses {
    pub fn none() -> Self {
        Vec::new()
    }

    pub fn all() -> Self {
        vec![ToolResourceAccess::All]
    }

    pub fn file(operation: &str, path: &str, recursive: Option<bool>) -> Self {
        vec![ToolResourceAccess::File {
            operation: operation.to_string(),
            path: path.to_string(),
            recursive,
        }]
    }

    pub fn read_file(path: &str) -> Self {
        Self::file(FILE_READ, path, None)
    }

    pub fn read_tree(path: &str) -> Self {
        Self::file(FILE_READ, path, Some(true))
    }

    pub fn write_file(path: &str) -> Self {
        Self::file(FILE_WRITE, path, None)
    }

    pub fn write_tree(path: &str) -> Self {
        Self::file(FILE_WRITE, path, Some(true))
    }

    pub fn read_write_file(path: &str) -> Self {
        Self::file(FILE_READWRITE, path, None)
    }

    pub fn read_write_tree(path: &str) -> Self {
        Self::file(FILE_READWRITE, path, Some(true))
    }

    pub fn search_tree(path: &str) -> Self {
        Self::file(FILE_SEARCH, path, Some(true))
    }

    pub fn conflict(left: &Self, right: &Self) -> bool {
        left.iter().any(|l| right.iter().any(|r| resource_conflict(l, r)))
    }
}

fn resource_conflict(left: &ToolResourceAccess, right: &ToolResourceAccess) -> bool {
    match (left, right) {
        (ToolResourceAccess::All, _) | (_, ToolResourceAccess::All) => true,
        (
            ToolResourceAccess::File { operation: lo, path: lp, recursive: lr },
            ToolResourceAccess::File { operation: ro, path: rp, recursive: rr },
        ) => file_operations_conflict(lo, ro) && file_accesses_overlap(lp, *lr, rp, *rr),
    }
}

fn file_operations_conflict(left: &str, right: &str) -> bool {
    file_operation_writes(left) || file_operation_writes(right)
}

fn file_operation_writes(operation: &str) -> bool {
    matches!(operation, FILE_WRITE | FILE_READWRITE)
}

fn file_accesses_overlap(left: &str, left_recursive: Option<bool>, right: &str, right_recursive: Option<bool>) -> bool {
    let lp = normalize_path(left);
    let rp = normalize_path(right);
    if lp == rp {
        return true;
    }
    let lpfx = if lp.ends_with('/') { lp.clone() } else { format!("{}/", lp) };
    let rpfx = if rp.ends_with('/') { rp.clone() } else { format!("{}/", rp) };
    (left_recursive == Some(true) && rp.starts_with(&lpfx))
        || (right_recursive == Some(true) && lp.starts_with(&rpfx))
}

fn normalize_path(path: &str) -> String {
    let normalized = path.replace('\\', "/").replace("//", "/");
    let folded = normalized.to_lowercase();
    if folded.len() > 1 && folded.ends_with('/') {
        folded[..folded.len() - 1].to_string()
    } else {
        folded
    }
}
```

`rust-ody/crates/agent-rs/src/agent_loop/tool_scheduler.rs`：

```rust
use std::future::Future;
use std::pin::Pin;

use tokio::sync::oneshot;

use crate::agent_loop::tool_access::ToolAccesses;

pub struct ToolCallTask<Result> {
    pub accesses: ToolAccesses,
    pub start: Box<
        dyn FnOnce() -> Pin<Box<dyn Future<Output = Result> + Send>> + Send,
    >,
}

struct ScheduledTask<Result> {
    accesses: ToolAccesses,
    result_tx: Option<oneshot::Sender<Result>>,
    start: Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = Result> + Send>> + Send>,
}

pub struct ToolScheduler<Result> {
    active: Vec<ScheduledTask<Result>>,
    queued: Vec<ScheduledTask<Result>>,
}

impl<Result: Send + 'static> ToolScheduler<Result> {
    pub fn new() -> Self {
        Self { active: Vec::new(), queued: Vec::new() }
    }

    pub async fn add(&mut self, task: ToolCallTask<Result>) -> Result<oneshot::Receiver<Result>, anyhow::Error> {
        let (tx, rx) = oneshot::channel();
        let scheduled = ScheduledTask {
            accesses: task.accesses,
            result_tx: Some(tx),
            start: task.start,
        };
        if self.is_blocked(&scheduled, &self.queued) {
            self.queued.push(scheduled);
        } else {
            self.start_task(scheduled);
        }
        Ok(rx)
    }

    fn is_blocked(&self, task: &ScheduledTask<Result>, queued_before: &[ScheduledTask<Result>]) -> bool {
        self.conflicts_with_any(task, &self.active) || self.conflicts_with_any(task, queued_before)
    }

    fn conflicts_with_any(&self, task: &ScheduledTask<Result>, candidates: &[ScheduledTask<Result>]) -> bool {
        candidates.iter().any(|c| ToolAccesses::conflict(&task.accesses, &c.accesses))
    }

    fn start_task(&mut self, mut task: ScheduledTask<Result>) {
        self.active.push(task);
        let index = self.active.len() - 1;
        let start = self.active[index].start.take().unwrap();
        let tx = self.active[index].result_tx.take();
        let this = self as *mut Self;
        tokio::spawn(async move {
            let result = start().await;
            if let Some(tx) = tx {
                let _ = tx.send(result);
            }
            unsafe {
                (*this).finish(index);
            }
        });
    }

    fn finish(&mut self, index: usize) {
        if index < self.active.len() {
            self.active.swap_remove(index);
        }
        self.start_queued();
    }

    fn start_queued(&mut self) {
        let mut still_queued: Vec<ScheduledTask<Result>> = Vec::new();
        // Drain queued tasks one-by-one so that new starts can affect conflict checks
        while let Some(task) = self.queued.first() {
            if self.is_blocked(task, &still_queued) {
                still_queued.push(self.queued.remove(0));
            } else {
                let task = self.queued.remove(0);
                self.start_task(task);
            }
        }
        self.queued = still_queued;
    }
}
```

注意：`finish` 使用 raw pointer 是因为 `start_task` 在 `tokio::spawn` 闭包里需要调用 `finish`，而 `self` 被借用。这是 unsafe 的，但在本场景下安全，因为 scheduler 的生命周期大于所有 spawned task。若评审不通过，可改用 `Arc<Mutex<ToolSchedulerInner>>` 重构，但本计划先保持最小实现。

- [ ] 运行测试：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo test -p agent-rs --test tool_access --test tool_scheduler
```

- [ ] 运行全 workspace typecheck（因为修改了 `lib.rs` 共享签名）：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo check --workspace --tests
```

- [ ] 提交：`feat(agent-rs): tool access conflict detection and scheduler`

---

## Phase A 本地自审

- [ ] 1. Spec-coverage：Roadmap 4.3.4.1（loop 类型与事件）已映射到 Task 2/3；4.3.4.4 工具调度映射到 Task 4。
- [ ] 2. Placeholder scan：本部分代码不含 TODO/TBD；所有 trait 方法、事件字段、scheduler 逻辑均已给出。
- [ ] 3. No phantom tasks：每个 task 都产生文件变更与可运行测试；无 `--allow-empty`。
- [ ] 4. Dependency soundness：Task 2/3/4 均依赖 Task 1；Task 4 依赖 Task 2 的 `ToolAccesses` 签名。
- [ ] 5. Caller & build soundness：Task 1 修改了 `lib.rs` 共享签名，Task 4 结尾运行 `cargo check --workspace --tests`。
- [ ] 6. Test-the-risk：scheduler 并发/串行用时间断言；tool access 覆盖 read/search 不冲突、write/recursive-read 冲突、`all` 冲突。
- [ ] 7. Type consistency：复用 `records::nested::{ExecutableToolResult, ToolUpdate, LoopRecordedEvent}`；字段名与 TS 一致（camelCase/snake_case）。
