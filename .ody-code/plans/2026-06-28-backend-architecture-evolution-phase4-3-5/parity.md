# 4.3.5 Part 4 — TurnFlow L3 fixtures、golden binary、TS runner、对照测试

**Scope:** 在 Part 2（`TurnFlow` 状态机）与 Part 3（`KosongLLM` / `ToolCallDeduplicator` / telemetry）完成后，建立 **TS↔Rust TurnFlow 层 L3 对照测试**：共享 JSON fixture、Rust golden binary、TS runner、snapshot 归一化、parity assertion，以及 npm/CI 脚本。本 part 的所有 fixture 必须先在 TS 侧自跑通，再与 Rust golden binary 逐字段比对。

---

## Local File Structure

| File / Directory | Responsibility |
|---|---|
| `packages/integration-tests/src/parity/fixtures/turn/*.json` | 共享 L3 fixtures（prompt / steer / cancel / goal continuation / tool-call dedup） |
| `packages/integration-tests/src/parity/turn-fixture.ts` | fixture JSON 的 TS schema 与类型守卫 |
| `packages/integration-tests/src/parity/turn-l3-driver.ts` | TS runner：读取 fixture，驱动 `AgentTestContext` 的 `TurnFlow`，输出 snapshot |
| `packages/integration-tests/src/parity/normalize-turn.ts` | TurnFlow snapshot 归一化（UUID / 时间戳 / duration / turnId） |
| `packages/integration-tests/test/parity/turn-l3.test.ts` | TS runner 自测：每个 fixture 能跑完且不抛错 |
| `packages/integration-tests/test/parity/turn-l3-parity.test.ts` | TS vs Rust 逐字段 parity 测试 |
| `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs` | 可复用的 `FixtureAgent` / `FixtureLlm` / `FixtureTool`（实现 `TurnAgent`） |
| `rust-ody/crates/agent-rs/src/bin/turn_l3.rs` | Rust golden binary：读 fixture，跑 `TurnFlow`，输出 snapshot JSON |
| `rust-ody/crates/agent-rs/tests/turn_l3_fixture.rs` | cargo 测试：调用 `turn_l3` binary 并断言输出结构 |
| `packages/integration-tests/package.json` | 新增 `test:parity:turn` / `test:parity:turn-l3` 脚本 |
| `.github/workflows/rust-host.yml` | 在 `parity` job 中加入 `test:parity:turn-l3` 步骤 |

---

## Local Dependency Overview

```text
Task 1: 提取/创建可复用的 FixtureAgent/FixtureLlm/FixtureTool
    │
    ├──▶ Task 3: Rust golden binary turn_l3
    │
Task 2: 定义 fixture schema + 初始 fixtures
    │
    ├──▶ Task 4: TS runner turn-l3-driver
    │
    └──▶ Task 5: snapshot 归一化 normalize-turn
              │
              ▼
Task 6: TS↔Rust parity 测试 turn-l3-parity.test.ts
    │
    ▼
Task 7: npm/CI 脚本与 job 步骤
    │
    ▼
Task 8: 全 workspace typecheck / cargo check
```

- **可并行**：Task 1 与 Task 2 无依赖；Task 3 依赖 Task 1；Task 4/5 依赖 Task 2。
- **硬前置**：Task 1 依赖 `types.md` Task 3 与 `turn.md` Task 7（`TurnAgent` trait 与 `TurnFlow` 实现已可用）。
- **跨文件依赖**：Task 3/6 依赖 `adapter.md` Task 6（`ToolCallDeduplicator` 已接入 `TurnFlow` 的 `prepareToolExecution`/`finalizeToolResult` hooks）。

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| TS `AgentTestContext` 与 Rust `FixtureAgent` 的 snapshot 字段不对齐 | snapshot 只保留两端都能稳定产出的 `events` / `records` / `turnResults`；其余字段在归一化中抹平或丢弃 |
| `turn.step.*` 事件中的 `uuid` / `stepId` 非确定性 | 归一化为 `<uuid>` / `<id>` 占位 |
| 时间戳 / duration 字段非确定性 | 归一化为 `0` 或 `<ts>` |
| goal continuation fixture 需要可变的 goal store | `FixtureAgent` 暴露 `__set_goal_status` mock tool；TS runner 通过 `SessionGoalStore` 的 `readState/writeState` 注入初始 active goal |

---

## Local Spec-Coverage Table

| Roadmap 4.3.5.6 条目 | 覆盖任务 | 状态 |
|---|---|---|
| 单 turn / 多 step fixture | Task 2, Task 3, Task 4, Task 6 | covered |
| steer buffer fixture | Task 2, Task 3, Task 4, Task 6 | covered |
| cancel fixture | Task 2, Task 3, Task 4, Task 6 | covered |
| goal continuation fixture | Task 2, Task 3, Task 4, Task 6 | covered |
| tool-call 去重 fixture（same_step / cross_step） | Task 2, Task 3, Task 4, Task 6 | covered |
| 事件序列比对（`turn.started`/`turn.step.*`/`assistant.delta`/`tool.call.*`/`tool.result`/`turn.ended`） | Task 5, Task 6 | covered |
| CI `parity` job 接入 | Task 7 | covered |

---

## Task 1: Extract reusable `FixtureAgent` / `FixtureLlm` / `FixtureTool` test doubles

**Depends on:** `types.md` Task 3, `turn.md` Task 7

**Files:**
- Create: `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs`
- Modify: `rust-ody/crates/agent-rs/src/turn/mod.rs`（新增 `pub mod fixture_agent;`）
- Modify: `rust-ody/crates/agent-rs/src/turn/turn_flow.rs`（将 inline `FakeAgent` 替换为 `use crate::turn::fixture_agent::*;`）
- Test: `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs`（模块内测试）

### Steps

- [ ] Write the failing test.

在 `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs` 末尾加入：

```rust
#[cfg(test)]
mod tests {
    use super::*;
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
        let id = flow.prompt(vec![ContentPart::Text { text: "hi".into() }], USER_PROMPT_ORIGIN);
        assert!(id.is_some());
        let _ = flow.wait_for_current_turn(None).await.unwrap();
        let events = agent.captures.lock().unwrap().events.clone();
        assert!(events.iter().any(|e| matches!(e, AgentEvent::TurnStarted { .. })));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::TurnEnded { .. })));
    }
}
```

- [ ] Run it and verify it FAILS.

```bash
cd rust-ody && cargo test -p agent-rs fixture_agent_runs_turn_flow --no-run
```

预期失败：`fixture_agent.rs` 不存在，`FixtureAgent` / `FixtureResponse` / `TurnFlow` 等符号未定义。

- [ ] Write the minimal implementation.

创建 `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs`（完整代码）：

```rust
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use kosong_rs::message::{ContentPart, Message, Role, ToolCall};
use kosong_rs::provider::{
    AbortSignal, ChatProvider, FinishReason, GenerateOptions, ModelCapability, StreamedMessage,
    ThinkingEffort, Tool,
};
use kosong_rs::usage::TokenUsage;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value as JsonValue;

use crate::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse, LlmStreamTiming};
use crate::agent_loop::types::{
    AuthorizeToolExecutionResult, ExecutableTool, ExecutableToolContext, ExecutableToolErrorResult,
    ExecutableToolOutput, ExecutableToolResult, ExecutableToolSuccessResult,
    FinalizeToolResultContext, PrepareToolExecutionResult, ResolvedToolExecutionHookContext,
    RunnableToolExecution, ToolExecution, ToolExecutionHookContext,
};
use crate::context::types::{ContextMessage, PromptOrigin, USER_PROMPT_ORIGIN};
use crate::records::nested::{
    ExecutableToolOutput as NestedExecutableToolOutput, GoalBudgetLimits, GoalStatus,
    UsageRecordScope,
};
use crate::records::AgentRecord;
use crate::turn::types::*;
use crate::turn::TurnFlow;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FixtureResponse {
    #[serde(rename = "toolCalls", default)]
    pub tool_calls: Vec<ToolCall>,
    #[serde(rename = "finishReason", default, deserialize_with = "deserialize_finish_reason")]
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
    pub loop_events: Vec<crate::agent_loop::types::LoopRecordedEvent>,
    pub begin_turn_count: usize,
    pub end_turn_count: usize,
    pub telemetry_events: Vec<(String, JsonValue)>,
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
}

impl TurnAgent for FixtureAgent {
    fn context(&self) -> &dyn TurnContext { self }
    fn usage(&self) -> &dyn TurnUsage { self }
    fn config(&self) -> &dyn TurnConfig { self }
    fn tools(&self) -> &dyn TurnTools { self }
    fn permission(&self) -> &dyn TurnPermission { self }
    fn injection(&self) -> &dyn TurnInjection { self }
    fn full_compaction(&self) -> &dyn TurnFullCompaction { self }
    fn micro_compaction(&self) -> &dyn TurnMicroCompaction { self }
    fn split_plan_checkpoint(&self) -> &dyn TurnSplitPlanCheckpoint { self }
    fn normal_mode_task_checkpoint(&self) -> &dyn TurnNormalTaskCheckpoint { self }
    fn session_mode(&self) -> &dyn TurnSessionMode { self }
    fn goals(&self) -> Option<&dyn TurnGoal> { Some(self) }
    fn hooks(&self) -> Option<&dyn TurnHooks> { Some(self) }
    fn telemetry(&self) -> &dyn TurnTelemetry { self }
    fn log(&self) -> &dyn TurnLog { self }
    fn mcp(&self) -> Option<&dyn TurnMcp> { None }
    fn subagent_host(&self) -> Option<&dyn TurnSubagentHost> { None }
    fn records(&self) -> &dyn TurnRecords { self }
    fn event_emitter(&self) -> &dyn TurnEventEmitter { self }
    fn llm_resolver(&self) -> &dyn TurnLlmResolver { self }
    fn flush_deferred_context_switch(&self) {}
    fn agent_type(&self) -> &str { "main" }
    fn homedir(&self) -> Option<&str> { None }
    fn goal_runtime_enabled(&self) -> bool { true }
}

#[async_trait::async_trait]
impl TurnContext for FixtureAgent {
    fn append_user_message(&self, content: Vec<ContentPart>, origin: PromptOrigin) {
        self.captures.lock().unwrap().context_inputs.push((content.clone(), origin.clone()));
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
        self.history.lock().unwrap().push(message);
    }

    fn messages(&self) -> Vec<Message> {
        self.history.lock().unwrap().iter().map(|cm| cm.message.clone()).collect()
    }

    fn append_loop_event(&self, event: crate::agent_loop::types::LoopRecordedEvent) {
        self.captures.lock().unwrap().loop_events.push(event);
    }

    fn has_open_steps(&self) -> bool { false }
    fn clear(&self) { self.history.lock().unwrap().clear(); }
}

impl TurnUsage for FixtureAgent {
    fn begin_turn(&self) { self.captures.lock().unwrap().begin_turn_count += 1; }
    fn end_turn(&self) { self.captures.lock().unwrap().end_turn_count += 1; }
    fn record(&self, _model: &str, _usage: TokenUsage, _scope: UsageRecordScope) {}
}

impl TurnConfig for FixtureAgent {
    fn model(&self) -> String { "mock-model".into() }
    fn model_alias(&self) -> Option<String> { None }
    fn system_prompt(&self) -> String { "You are a fixture agent.".into() }
    fn thinking_level(&self) -> String { "off".into() }
    fn provider(&self) -> Box<dyn ChatProvider> { Box::new(NoopChatProvider) }
    fn model_capabilities(&self) -> ModelCapability { ModelCapability::unknown() }
    fn loop_control(&self) -> Option<LoopControl> { self.loop_control.clone() }
    fn has_model(&self) -> bool { true }
}

impl TurnTools for FixtureAgent {
    fn loop_tools(&self) -> Vec<Arc<dyn ExecutableTool>> { self.tools.lock().unwrap().clone() }
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
    fn reset_for_turn(&self) {}
    async fn before_step(&self, _signal: AbortSignal) {}
    async fn after_step(&self) {}
    async fn handle_overflow_error(&self, _signal: AbortSignal, _error: anyhow::Error) {}
}

impl TurnMicroCompaction for FixtureAgent {
    fn detect(&self) {}
}

#[async_trait::async_trait]
impl TurnSplitPlanCheckpoint for FixtureAgent {
    async fn before_step(&self, _signal: AbortSignal) {}
}

#[async_trait::async_trait]
impl TurnNormalTaskCheckpoint for FixtureAgent {
    async fn before_step(&self, _signal: AbortSignal) {}
}

impl TurnSessionMode for FixtureAgent {
    fn is_active(&self) -> bool { false }
    fn kind(&self) -> Option<String> { None }
}

#[async_trait::async_trait]
impl TurnGoal for FixtureAgent {
    fn get_goal(&self) -> Option<GoalSnapshot> {
        self.goal_status.lock().unwrap().clone()
    }

    async fn increment_turn(&self) {
        let mut g = self.goal_status.lock().unwrap();
        if let Some(ref mut s) = *g {
            s.turns_used += 1;
        }
    }

    async fn mark_blocked(&self, reason: &str) {
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

    async fn pause_on_interrupt(&self, _reason: &str) {}
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
        self.captures.lock().unwrap().context_inputs.push((input, USER_PROMPT_ORIGIN));
        Ok(self.hook_results.lock().unwrap().clone())
    }

    async fn trigger_stop_hook(
        &self,
        _signal: AbortSignal,
    ) -> Result<Option<StopHookBlock>, anyhow::Error> {
        Ok(self.stop_block.lock().unwrap().clone())
    }

    fn fire_and_forget_trigger(&self, _event: &str, _data: JsonValue) {}
}

impl TurnTelemetry for FixtureAgent {
    fn track(&self, event: &str, properties: JsonValue) {
        self.captures.lock().unwrap().telemetry_events.push((event.into(), properties));
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

impl TurnLlmResolver for FixtureAgent {
    fn refresh_llm(&self) {}
    fn llm(&self) -> Arc<dyn Llm> { self.llm.clone() }
}

pub struct FixtureLlm {
    responses: Vec<FixtureResponse>,
    index: Mutex<usize>,
}

impl FixtureLlm {
    pub fn new(responses: Vec<FixtureResponse>) -> Self {
        Self { responses, index: Mutex::new(0) }
    }
}

#[async_trait::async_trait]
impl Llm for FixtureLlm {
    fn system_prompt(&self) -> &str { "fixture" }
    fn model_name(&self) -> &str { "mock" }

    async fn chat(&self, _params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        let idx = {
            let mut guard = self.index.lock().unwrap();
            let i = *guard;
            *guard = guard.wrapping_add(1);
            i
        };
        let response = self.responses.get(idx)
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
    fn name(&self) -> &str { &self.def.name }
    fn description(&self) -> &str { &self.def.description }
    fn parameters(&self) -> JsonValue { self.def.parameters.clone() }

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
                execute: Box::new(|_ctx| Box::pin(async move {
                    Ok(ExecutableToolResult::Success(ExecutableToolSuccessResult {
                        output: ExecutableToolOutput::Text("ok".into()),
                        is_error: None,
                        stop_turn: None,
                        message: None,
                    }))
                })),
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
                    as Pin<Box<dyn Future<Output = Result<ExecutableToolResult, anyhow::Error>> + Send>>
            }),
        }))
    }
}

#[derive(Clone)]
struct NoopChatProvider;

#[async_trait::async_trait]
impl ChatProvider for NoopChatProvider {
    fn name(&self) -> &str { "noop" }
    fn model_name(&self) -> &str { "noop" }
    fn thinking_effort(&self) -> Option<ThinkingEffort> { None }

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
```

更新 `rust-ody/crates/agent-rs/src/turn/mod.rs`：

```rust
pub mod canonical_args;
pub mod error;
pub mod fixture_agent;
pub mod kosong_llm;
pub mod remote_kosong_llm;
pub mod telemetry;
pub mod tool_dedup;
pub mod turn_flow;
pub mod types;

pub use fixture_agent::*;
pub use telemetry::*;
pub use turn_flow::*;
pub use types::*;
```

更新 `rust-ody/crates/agent-rs/src/turn/turn_flow.rs`：

```rust
use crate::turn::fixture_agent::*;
```

并删除该文件 `#[cfg(test)]` 模块中的 inline `FakeAgent` / `FakeLlm` 定义（保留测试用例本身）。

- [ ] Run it and verify it PASSES.

```bash
cd rust-ody && cargo test -p agent-rs fixture_agent_runs_turn_flow
```

预期：测试通过；`turn_flow.rs` 中原有测试仍通过。

- [ ] Commit.

```bash
git add rust-ody/crates/agent-rs/src/turn/fixture_agent.rs rust-ody/crates/agent-rs/src/turn/mod.rs rust-ody/crates/agent-rs/src/turn/turn_flow.rs
git commit -m "feat(agent-rs): extract reusable FixtureAgent for turn parity tests"
```

---

## Task 2: Define TurnFlow L3 fixture schema and create initial fixtures

**Depends on:** none（只依赖已存在的 JSON / zod；不依赖未实现的代码）

**Files:**
- Create: `packages/integration-tests/src/parity/turn-fixture.ts`
- Create: `packages/integration-tests/src/parity/fixtures/turn/end-turn.json`
- Create: `packages/integration-tests/src/parity/fixtures/turn/single-tool-call.json`
- Create: `packages/integration-tests/src/parity/fixtures/turn/tool-not-found.json`
- Create: `packages/integration-tests/src/parity/fixtures/turn/steer-buffer.json`
- Create: `packages/integration-tests/src/parity/fixtures/turn/cancel-mid-step.json`
- Create: `packages/integration-tests/src/parity/fixtures/turn/same-step-dedup.json`
- Create: `packages/integration-tests/src/parity/fixtures/turn/cross-step-dedup.json`
- Create: `packages/integration-tests/src/parity/fixtures/turn/goal-continuation.json`
- Create: `packages/integration-tests/test/parity/turn-fixtures-valid.test.ts`

### Steps

- [ ] Write the failing test.

创建 `packages/integration-tests/test/parity/turn-fixtures-valid.test.ts`：

```ts
import { readdir, readFile } from 'node:fs/promises';
import { join } from 'pathe';
import { describe, expect, it } from 'vitest';

import { parseTurnFixture } from '../../src/parity/turn-fixture';

const fixturesDir = join(import.meta.dirname, '../../src/parity/fixtures/turn');

describe('turn fixtures are valid', () => {
  it.each([
    'end-turn.json',
    'single-tool-call.json',
    'tool-not-found.json',
    'steer-buffer.json',
    'cancel-mid-step.json',
    'same-step-dedup.json',
    'cross-step-dedup.json',
    'goal-continuation.json',
  ])('%s parses against schema', async (name) => {
    const raw = await readFile(join(fixturesDir, name), 'utf8');
    const parsed = parseTurnFixture(raw);
    expect(parsed.name).toBe(name.replace('.json', ''));
    expect(parsed.actions.length).toBeGreaterThan(0);
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
cd packages/integration-tests && pnpm vitest run test/parity/turn-fixtures-valid.test.ts
```

预期失败：`turn-fixture.ts` 与 fixtures 不存在，`parseTurnFixture` 未定义。

- [ ] Write the minimal implementation.

创建 `packages/integration-tests/src/parity/turn-fixture.ts`：

```ts
import { z } from 'zod';

export const ContentPartSchema = z.union([
  z.object({ type: z.literal('text'), text: z.string() }),
  z.object({
    type: z.literal('image_url'),
    imageUrl: z.object({ id: z.string().optional(), url: z.string().optional() }).passthrough(),
  }),
]);

export type FixtureContentPart = z.infer<typeof ContentPartSchema>;

export const PromptOriginSchema = z.union([
  z.object({ kind: z.literal('user') }),
  z.object({ kind: z.literal('system_trigger'), name: z.string() }),
  z.object({ kind: z.literal('hook_result'), event: z.string(), blocked: z.boolean().optional() }),
  z.object({ kind: z.string(), name: z.string().optional() }).passthrough(),
]);

export type FixturePromptOrigin = z.infer<typeof PromptOriginSchema>;

export const FixtureActionSchema = z.union([
  z.object({ op: z.literal('prompt'), input: z.array(ContentPartSchema), origin: PromptOriginSchema }),
  z.object({ op: z.literal('steer'), input: z.array(ContentPartSchema), origin: PromptOriginSchema }),
  z.object({ op: z.literal('cancel'), turnId: z.number().optional(), reason: z.string().optional() }),
  z.object({ op: z.literal('wait') }),
]);

export type FixtureAction = z.infer<typeof FixtureActionSchema>;

export const FixtureResponseSchema = z.object({
  toolCalls: z.array(z.any()).default([]),
  finishReason: z.string().optional(),
  rawFinishReason: z.string().optional(),
  usage: z.object({
    inputOther: z.number().default(0),
    output: z.number().default(0),
    inputCacheRead: z.number().default(0),
    inputCacheCreation: z.number().default(0),
  }),
});

export type FixtureResponse = z.infer<typeof FixtureResponseSchema>;

export const FixtureToolResultSchema = z.object({
  output: z.union([z.string(), z.array(ContentPartSchema)]),
  isError: z.boolean().optional(),
  stopTurn: z.boolean().optional(),
  message: z.string().optional(),
});

export const FixtureToolDefSchema = z.object({
  name: z.string(),
  description: z.string(),
  parameters: z.record(z.any()),
  result: FixtureToolResultSchema,
});

export type FixtureToolDef = z.infer<typeof FixtureToolDefSchema>;

export const TurnFixtureSchema = z.object({
  name: z.string(),
  initialGoal: z.object({
    status: z.enum(['active', 'paused', 'blocked', 'complete']),
    budget: z.object({
      tokenBudget: z.number().optional(),
      turnBudget: z.number().optional(),
      wallClockBudgetMs: z.number().optional(),
    }).default({}),
  }).optional(),
  loopControl: z.object({
    maxSteps: z.number().optional(),
    maxRetryAttempts: z.number().optional(),
  }).optional(),
  actions: z.array(FixtureActionSchema),
  responses: z.array(FixtureResponseSchema),
  tools: z.array(FixtureToolDefSchema).default([]),
});

export type TurnFixture = z.infer<typeof TurnFixtureSchema>;

export interface TurnL3Snapshot {
  readonly name: string;
  readonly turns: Array<{
    readonly turnId: number;
    readonly reason: string;
    readonly error?: unknown;
    readonly stopReason?: string;
  }>;
  readonly events: unknown[];
  readonly records: unknown[];
  readonly contextInputs: Array<{ text: string; originKind: string }>;
  readonly telemetry: Array<{ event: string; properties: unknown }>;
  readonly goalState?: { status: string; turnsUsed: number; tokensUsed: number };
}

export function parseTurnFixture(raw: string): TurnFixture {
  return TurnFixtureSchema.parse(JSON.parse(raw));
}
```

创建 fixture 文件（节选；每个文件都要完整写入）：

`packages/integration-tests/src/parity/fixtures/turn/end-turn.json`：

```json
{
  "name": "end-turn",
  "actions": [
    { "op": "prompt", "input": [{ "type": "text", "text": "Hello" }], "origin": { "kind": "user" } },
    { "op": "wait" }
  ],
  "responses": [
    {
      "toolCalls": [],
      "finishReason": "completed",
      "rawFinishReason": "stop",
      "usage": { "inputOther": 5, "output": 3, "inputCacheRead": 0, "inputCacheCreation": 0 }
    }
  ],
  "tools": []
}
```

`packages/integration-tests/src/parity/fixtures/turn/single-tool-call.json`：

```json
{
  "name": "single-tool-call",
  "actions": [
    { "op": "prompt", "input": [{ "type": "text", "text": "Call Bash" }], "origin": { "kind": "user" } },
    { "op": "wait" }
  ],
  "responses": [
    {
      "toolCalls": [
        { "type": "function", "id": "call_bash_1", "name": "Bash", "arguments": "{\"command\":\"printf hello\",\"timeout\":60}" }
      ],
      "finishReason": "tool_calls",
      "rawFinishReason": "tool_calls",
      "usage": { "inputOther": 10, "output": 5, "inputCacheRead": 0, "inputCacheCreation": 0 }
    },
    {
      "toolCalls": [],
      "finishReason": "completed",
      "rawFinishReason": "stop",
      "usage": { "inputOther": 8, "output": 4, "inputCacheRead": 0, "inputCacheCreation": 0 }
    }
  ],
  "tools": [
    {
      "name": "Bash",
      "description": "Runs a shell command.",
      "parameters": { "type": "object", "properties": { "command": { "type": "string" }, "timeout": { "type": "number" } }, "required": ["command"] },
      "result": { "output": "hello" }
    }
  ]
}
```

`packages/integration-tests/src/parity/fixtures/turn/tool-not-found.json`：

```json
{
  "name": "tool-not-found",
  "actions": [
    { "op": "prompt", "input": [{ "type": "text", "text": "Call missing tool" }], "origin": { "kind": "user" } },
    { "op": "wait" }
  ],
  "responses": [
    {
      "toolCalls": [
        { "type": "function", "id": "call_missing_1", "name": "missing_tool", "arguments": "{}" }
      ],
      "finishReason": "tool_calls",
      "rawFinishReason": "tool_calls",
      "usage": { "inputOther": 6, "output": 4, "inputCacheRead": 0, "inputCacheCreation": 0 }
    },
    {
      "toolCalls": [],
      "finishReason": "completed",
      "rawFinishReason": "stop",
      "usage": { "inputOther": 5, "output": 3, "inputCacheRead": 0, "inputCacheCreation": 0 }
    }
  ],
  "tools": []
}
```

`packages/integration-tests/src/parity/fixtures/turn/steer-buffer.json`：

```json
{
  "name": "steer-buffer",
  "actions": [
    { "op": "prompt", "input": [{ "type": "text", "text": "first" }], "origin": { "kind": "user" } },
    { "op": "steer", "input": [{ "type": "text", "text": "second" }], "origin": { "kind": "user" } },
    { "op": "wait" },
    { "op": "prompt", "input": [{ "type": "text", "text": "third" }], "origin": { "kind": "user" } },
    { "op": "wait" }
  ],
  "responses": [
    {
      "toolCalls": [],
      "finishReason": "completed",
      "rawFinishReason": "stop",
      "usage": { "inputOther": 5, "output": 3, "inputCacheRead": 0, "inputCacheCreation": 0 }
    },
    {
      "toolCalls": [],
      "finishReason": "completed",
      "rawFinishReason": "stop",
      "usage": { "inputOther": 5, "output": 3, "inputCacheRead": 0, "inputCacheCreation": 0 }
    }
  ],
  "tools": []
}
```

`packages/integration-tests/src/parity/fixtures/turn/cancel-mid-step.json`：

```json
{
  "name": "cancel-mid-step",
  "actions": [
    { "op": "prompt", "input": [{ "type": "text", "text": "Long request" }], "origin": { "kind": "user" } },
    { "op": "cancel", "reason": "user cancellation" },
    { "op": "wait" }
  ],
  "responses": [
    {
      "toolCalls": [],
      "finishReason": "completed",
      "rawFinishReason": "stop",
      "usage": { "inputOther": 5, "output": 3, "inputCacheRead": 0, "inputCacheCreation": 0 }
    }
  ],
  "tools": []
}
```

`packages/integration-tests/src/parity/fixtures/turn/same-step-dedup.json`：

```json
{
  "name": "same-step-dedup",
  "actions": [
    { "op": "prompt", "input": [{ "type": "text", "text": "Run duplicates" }], "origin": { "kind": "user" } },
    { "op": "wait" }
  ],
  "responses": [
    {
      "toolCalls": [
        { "type": "function", "id": "call_dup_1", "name": "Bash", "arguments": "{\"command\":\"printf dup\",\"timeout\":60}" },
        { "type": "function", "id": "call_dup_2", "name": "Bash", "arguments": "{\"command\":\"printf dup\",\"timeout\":60}" }
      ],
      "finishReason": "tool_calls",
      "rawFinishReason": "tool_calls",
      "usage": { "inputOther": 12, "output": 6, "inputCacheRead": 0, "inputCacheCreation": 0 }
    },
    {
      "toolCalls": [],
      "finishReason": "completed",
      "rawFinishReason": "stop",
      "usage": { "inputOther": 8, "output": 4, "inputCacheRead": 0, "inputCacheCreation": 0 }
    }
  ],
  "tools": [
    {
      "name": "Bash",
      "description": "Runs a shell command.",
      "parameters": { "type": "object", "properties": { "command": { "type": "string" }, "timeout": { "type": "number" } }, "required": ["command"] },
      "result": { "output": "dup" }
    }
  ]
}
```

`packages/integration-tests/src/parity/fixtures/turn/cross-step-dedup.json`：

```json
{
  "name": "cross-step-dedup",
  "actions": [
    { "op": "prompt", "input": [{ "type": "text", "text": "Run duplicates across steps" }], "origin": { "kind": "user" } },
    { "op": "wait" }
  ],
  "responses": [
    {
      "toolCalls": [
        { "type": "function", "id": "call_dup_1", "name": "Bash", "arguments": "{\"command\":\"printf dup\",\"timeout\":60}" }
      ],
      "finishReason": "tool_calls",
      "rawFinishReason": "tool_calls",
      "usage": { "inputOther": 10, "output": 5, "inputCacheRead": 0, "inputCacheCreation": 0 }
    },
    {
      "toolCalls": [
        { "type": "function", "id": "call_dup_2", "name": "Bash", "arguments": "{\"command\":\"printf dup\",\"timeout\":60}" }
      ],
      "finishReason": "tool_calls",
      "rawFinishReason": "tool_calls",
      "usage": { "inputOther": 10, "output": 5, "inputCacheRead": 0, "inputCacheCreation": 0 }
    },
    {
      "toolCalls": [],
      "finishReason": "completed",
      "rawFinishReason": "stop",
      "usage": { "inputOther": 8, "output": 4, "inputCacheRead": 0, "inputCacheCreation": 0 }
    }
  ],
  "tools": [
    {
      "name": "Bash",
      "description": "Runs a shell command.",
      "parameters": { "type": "object", "properties": { "command": { "type": "string" }, "timeout": { "type": "number" } }, "required": ["command"] },
      "result": { "output": "dup" }
    }
  ]
}
```

`packages/integration-tests/src/parity/fixtures/turn/goal-continuation.json`：

```json
{
  "name": "goal-continuation",
  "initialGoal": { "status": "active", "budget": {} },
  "actions": [
    { "op": "prompt", "input": [{ "type": "text", "text": "Keep going" }], "origin": { "kind": "user" } },
    { "op": "wait" }
  ],
  "responses": [
    {
      "toolCalls": [
        { "type": "function", "id": "call_active_1", "name": "UpdateGoal", "arguments": "{\"status\":\"active\"}" }
      ],
      "finishReason": "tool_calls",
      "rawFinishReason": "tool_calls",
      "usage": { "inputOther": 10, "output": 5, "inputCacheRead": 0, "inputCacheCreation": 0 }
    },
    {
      "toolCalls": [
        { "type": "function", "id": "call_complete_1", "name": "UpdateGoal", "arguments": "{\"status\":\"complete\"}" }
      ],
      "finishReason": "tool_calls",
      "rawFinishReason": "tool_calls",
      "usage": { "inputOther": 10, "output": 5, "inputCacheRead": 0, "inputCacheCreation": 0 }
    },
    {
      "toolCalls": [],
      "finishReason": "completed",
      "rawFinishReason": "stop",
      "usage": { "inputOther": 8, "output": 4, "inputCacheRead": 0, "inputCacheCreation": 0 }
    }
  ],
  "tools": [
    {
      "name": "UpdateGoal",
      "description": "Updates the current goal status.",
      "parameters": { "type": "object", "properties": { "status": { "type": "string", "enum": ["active", "complete", "paused", "blocked"] } }, "required": ["status"] },
      "result": { "output": "ok" }
    }
  ]
}
```

- [ ] Run it and verify it PASSES.

```bash
cd packages/integration-tests && pnpm vitest run test/parity/turn-fixtures-valid.test.ts
```

预期：8 个 fixture 全部通过 schema 校验。

- [ ] Commit.

```bash
git add packages/integration-tests/src/parity/turn-fixture.ts packages/integration-tests/src/parity/fixtures/turn packages/integration-tests/test/parity/turn-fixtures-valid.test.ts
git commit -m "test(integration-tests): add TurnFlow L3 fixture schema and fixtures"
```

---

## Task 3: Rust golden binary `turn_l3`

**Depends on:** Task 1, Task 2, `adapter.md` Task 6（`ToolCallDeduplicator` 已接入 `TurnFlow` hooks）

**Files:**
- Create: `rust-ody/crates/agent-rs/src/bin/turn_l3.rs`
- Modify: `rust-ody/crates/agent-rs/Cargo.toml`（新增 `[[bin]] name = "turn_l3"`）
- Create: `rust-ody/crates/agent-rs/tests/turn_l3_fixture.rs`

### Steps

- [ ] Write the failing test.

创建 `rust-ody/crates/agent-rs/tests/turn_l3_fixture.rs`：

```rust
use std::process::Command;

#[test]
fn turn_l3_binary_runs_end_turn_fixture() {
    let binary = env!("CARGO_BIN_EXE_turn_l3");
    let fixture = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../packages/integration-tests/src/parity/fixtures/turn/end-turn.json"
    );
    let output = Command::new(binary)
        .arg(fixture)
        .output()
        .expect("failed to run turn_l3 binary");

    assert!(
        output.status.success(),
        "turn_l3 failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let snapshot: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("binary output is not valid JSON");

    assert_eq!(snapshot["name"], "end-turn");
    let turns = snapshot["turns"].as_array().unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0]["reason"], "completed");
    let events = snapshot["events"].as_array().unwrap();
    assert!(events.iter().any(|e| e["type"] == "turn.started"));
    assert!(events.iter().any(|e| e["type"] == "turn.ended"));
}
```

- [ ] Run it and verify it FAILS.

```bash
cd rust-ody && cargo test -p agent-rs turn_l3_binary_runs_end_turn_fixture --no-run
```

预期失败：`turn_l3` binary 未定义，`CARGO_BIN_EXE_turn_l3` 不存在。

- [ ] Write the minimal implementation.

在 `rust-ody/crates/agent-rs/Cargo.toml` 末尾追加：

```toml
[[bin]]
name = "turn_l3"
path = "src/bin/turn_l3.rs"
```

创建 `rust-ody/crates/agent-rs/src/bin/turn_l3.rs`：

```rust
use std::env;
use std::fs::File;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agent_rs::records::nested::{GoalBudgetLimits, GoalStatus};
use agent_rs::records::AgentRecord;
use agent_rs::turn::fixture_agent::{
    FixtureAgent, FixtureResponse, FixtureTool, FixtureToolDef,
};
use agent_rs::turn::types::{AgentEvent, LoopControl, TurnEndResult, USER_PROMPT_ORIGIN};
use agent_rs::turn::TurnFlow;
use anyhow::{Context, Error};
use kosong_rs::message::ContentPart;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Deserialize)]
struct Fixture {
    name: String,
    #[serde(default)]
    initial_goal: Option<FixtureInitialGoal>,
    #[serde(default)]
    loop_control: Option<FixtureLoopControl>,
    actions: Vec<FixtureAction>,
    responses: Vec<FixtureResponse>,
    #[serde(default)]
    tools: Vec<FixtureToolDef>,
}

#[derive(Debug, Deserialize)]
struct FixtureInitialGoal {
    status: String,
    #[serde(default)]
    budget: FixtureBudget,
}

#[derive(Debug, Default, Deserialize)]
struct FixtureBudget {
    #[serde(rename = "tokenBudget")]
    token_budget: Option<i64>,
    #[serde(rename = "turnBudget")]
    turn_budget: Option<i64>,
    #[serde(rename = "wallClockBudgetMs")]
    wall_clock_budget_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct FixtureLoopControl {
    max_steps: Option<u32>,
    max_retry_attempts: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op")]
enum FixtureAction {
    #[serde(rename = "prompt")]
    Prompt { input: Vec<ContentPart>, origin: JsonValue },
    #[serde(rename = "steer")]
    Steer { input: Vec<ContentPart>, origin: JsonValue },
    #[serde(rename = "cancel")]
    Cancel { turn_id: Option<i64>, reason: Option<String> },
    #[serde(rename = "wait")]
    Wait,
}

#[derive(Debug, Serialize)]
struct Snapshot {
    name: String,
    turns: Vec<TurnSummary>,
    events: Vec<JsonValue>,
    records: Vec<JsonValue>,
    context_inputs: Vec<ContextInputSummary>,
    telemetry: Vec<TelemetrySummary>,
    goal_state: Option<GoalStateSummary>,
}

#[derive(Debug, Serialize)]
struct TurnSummary {
    turn_id: i64,
    reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct ContextInputSummary {
    text: String,
    origin_kind: String,
}

#[derive(Debug, Serialize)]
struct TelemetrySummary {
    event: String,
    properties: JsonValue,
}

#[derive(Debug, Serialize)]
struct GoalStateSummary {
    status: String,
    turns_used: i64,
    tokens_used: i64,
}

fn parse_origin(origin: JsonValue) -> agent_rs::records::nested::PromptOrigin {
    use agent_rs::records::nested::PromptOrigin;
    match origin.get("kind").and_then(|v| v.as_str()) {
        Some("user") => PromptOrigin::User,
        Some("system_trigger") => PromptOrigin::SystemTrigger {
            name: origin.get("name").and_then(|v| v.as_str()).unwrap_or("").into(),
        },
        Some("hook_result") => PromptOrigin::HookResult {
            event: origin.get("event").and_then(|v| v.as_str()).unwrap_or("").into(),
            blocked: origin.get("blocked").and_then(|v| v.as_bool()),
        },
        _ => PromptOrigin::User,
    }
}

fn content_text(parts: &[ContentPart]) -> String {
    parts.iter().filter_map(|p| match p {
        ContentPart::Text { text } => Some(text.as_str()),
        _ => None,
    }).collect::<Vec<_>>().join("")
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let args: Vec<String> = env::args().collect();
    let fixture_path = args
        .get(1)
        .map(PathBuf::from)
        .context("usage: turn_l3 <fixture.json>")?;

    let file = File::open(&fixture_path)
        .with_context(|| format!("failed to open {}", fixture_path.display()))?;
    let fixture: Fixture = serde_json::from_reader(file)
        .with_context(|| format!("failed to parse {}", fixture_path.display()))?;

    let agent = Arc::new(FixtureAgent::new(fixture.responses, vec![]));

    if let Some(ctrl) = fixture.loop_control {
        agent.loop_control = Some(LoopControl {
            max_steps_per_turn: ctrl.max_steps,
            max_retries_per_step: ctrl.max_retry_attempts,
            reserved_context_size: None,
        });
    }

    let tools: Vec<_> = fixture
        .tools
        .into_iter()
        .map(|def| Arc::new(FixtureTool::new(def, agent.clone())) as Arc<_>)
        .collect();
    agent.tools.lock().unwrap().extend(tools);

    if let Some(goal) = fixture.initial_goal {
        let status = match goal.status.as_str() {
            "active" => GoalStatus::Active,
            "paused" => GoalStatus::Paused,
            "blocked" => GoalStatus::Blocked,
            "complete" => GoalStatus::Complete,
            _ => GoalStatus::Active,
        };
        agent.set_goal(
            status,
            GoalBudgetLimits {
                token_budget: goal.budget.token_budget,
                turn_budget: goal.budget.turn_budget,
                wall_clock_budget_ms: goal.budget.wall_clock_budget_ms,
            },
        );
    }

    let flow = TurnFlow::new(agent.clone());
    let mut turns: Vec<TurnSummary> = Vec::new();

    for action in fixture.actions {
        match action {
            FixtureAction::Prompt { input, origin } => {
                flow.prompt(input, parse_origin(origin));
            }
            FixtureAction::Steer { input, origin } => {
                flow.steer(input, parse_origin(origin));
            }
            FixtureAction::Cancel { turn_id, reason } => {
                flow.cancel(turn_id, reason);
            }
            FixtureAction::Wait => {
                if let Ok(end) = flow.wait_for_current_turn(None).await {
                    turns.push(turn_summary(&end));
                }
            }
        }
    }

    let captures = agent.captures.lock().unwrap();
    let snapshot = Snapshot {
        name: fixture.name,
        turns,
        events: captures
            .events
            .iter()
            .map(|e| serde_json::to_value(e).unwrap())
            .collect(),
        records: captures
            .records
            .iter()
            .map(|r| serde_json::to_value(r).unwrap())
            .collect(),
        context_inputs: captures
            .context_inputs
            .iter()
            .map(|(parts, origin)| ContextInputSummary {
                text: content_text(parts),
                origin_kind: format!("{:?}", origin).to_lowercase(),
            })
            .collect(),
        telemetry: captures
            .telemetry_events
            .iter()
            .map(|(event, props)| TelemetrySummary {
                event: event.clone(),
                properties: props.clone(),
            })
            .collect(),
        goal_state: agent.goal_status.lock().unwrap().as_ref().map(|g| GoalStateSummary {
            status: format!("{:?}", g.status).to_lowercase(),
            turns_used: g.turns_used,
            tokens_used: g.tokens_used,
        }),
    };

    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    Ok(())
}

fn turn_summary(end: &TurnEndResult) -> TurnSummary {
    TurnSummary {
        turn_id: end.event.turn_id,
        reason: format!("{:?}", end.event.reason).to_lowercase(),
        error: end.event.error.as_ref().map(|e| serde_json::to_value(e).unwrap()),
        stop_reason: end.stop_reason.map(|s| format!("{:?}", s).to_lowercase()),
    }
}
```

- [ ] Run it and verify it PASSES.

```bash
cd rust-ody && cargo test -p agent-rs turn_l3_binary_runs_end_turn_fixture
```

预期：测试通过；binary 成功运行 `end-turn.json` 并输出合法 JSON。

- [ ] Commit.

```bash
git add rust-ody/crates/agent-rs/src/bin/turn_l3.rs rust-ody/crates/agent-rs/Cargo.toml rust-ody/crates/agent-rs/tests/turn_l3_fixture.rs
git commit -m "feat(agent-rs): add turn_l3 golden binary for TurnFlow L3 parity"
```

---

## Task 4: TS runner `turn-l3-driver.ts`

**Depends on:** Task 2

**Files:**
- Create: `packages/integration-tests/src/parity/turn-l3-driver.ts`
- Modify: `packages/agent-core/test/agent/harness/agent.ts`（新增 `records` public getter）
- Test: `packages/integration-tests/test/parity/turn-l3.test.ts`

### Steps

- [ ] Write the failing test.

创建 `packages/integration-tests/test/parity/turn-l3.test.ts`：

```ts
import { join } from 'pathe';
import { describe, expect, it } from 'vitest';

import { runTurnL3Fixture } from '../../src/parity/turn-l3-driver';

const fixturesDir = join(import.meta.dirname, '../../src/parity/fixtures/turn');

describe('turn L3 TS runner', () => {
  it.each([
    'end-turn.json',
    'single-tool-call.json',
    'tool-not-found.json',
    'steer-buffer.json',
    'cancel-mid-step.json',
    'same-step-dedup.json',
    'cross-step-dedup.json',
    'goal-continuation.json',
  ])('%s produces a snapshot', async (name) => {
    const snapshot = await runTurnL3Fixture(join(fixturesDir, name));
    expect(snapshot.name).toBe(name.replace('.json', ''));
    expect(snapshot.turns.length).toBeGreaterThan(0);
    expect(snapshot.events.some((e) => e.type === 'turn.started')).toBe(true);
    expect(snapshot.events.some((e) => e.type === 'turn.ended')).toBe(true);
  }, 60000);
});
```

- [ ] Run it and verify it FAILS.

```bash
cd packages/integration-tests && pnpm vitest run test/parity/turn-l3.test.ts
```

预期失败：`turn-l3-driver.ts` 与 `runTurnL3Fixture` 不存在。

- [ ] Write the minimal implementation.

修改 `packages/agent-core/test/agent/harness/agent.ts`，在 `AgentTestContext` 中新增 public getter：

```ts
get records(): AgentRecord[] {
  return this.recordHistory;
}
```

创建 `packages/integration-tests/src/parity/turn-l3-driver.ts`：

```ts
import { randomUUID } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import type { ContentPart, ToolCall } from '@odysseythink/kosong';

import { testAgent } from '../../../agent-core/test/agent/harness/agent';
import type { TestAgentContext } from '../../../agent-core/test/agent/harness/agent';
import { recordingTelemetry } from '../../../agent-core/test/fixtures/telemetry';
import {
  SessionGoalStore,
  type SessionGoalState,
  type GoalActor,
} from '../../../agent-core/src/session/goal';
import {
  parseTurnFixture,
  type FixtureAction,
  type FixtureResponse,
  type TurnFixture,
  type TurnL3Snapshot,
} from './turn-fixture';

function toToolCall(raw: unknown): ToolCall {
  const r = raw as { id: string; name: string; arguments: string };
  return {
    type: 'function',
    id: r.id,
    name: r.name,
    arguments: r.arguments,
  };
}

function toContentParts(raw: unknown): ContentPart[] {
  return raw as ContentPart[];
}

function buildParts(response: FixtureResponse): (ContentPart | ToolCall)[] {
  const parts: (ContentPart | ToolCall)[] = [];
  if (response.toolCalls.length > 0) {
    parts.push(...response.toolCalls.map(toToolCall));
  }
  return parts;
}

function buildInitialGoal(fixture: TurnFixture): SessionGoalState | undefined {
  if (!fixture.initialGoal) return undefined;
  const now = new Date().toISOString();
  return {
    goalId: randomUUID(),
    objective: 'fixture goal',
    status: fixture.initialGoal.status,
    createdAt: now,
    updatedAt: now,
    startedBy: 'user' as GoalActor,
    updatedBy: 'user' as GoalActor,
    turnsUsed: 0,
    tokensUsed: 0,
    wallClockMs: 0,
    budgetLimits: fixture.initialGoal.budget,
  };
}

export async function runTurnL3Fixture(fixturePath: string): Promise<TurnL3Snapshot> {
  const raw = await readFile(fixturePath, 'utf8');
  const fixture = parseTurnFixture(raw);

  const telemetryRecords: Array<{ event: string; properties?: unknown }> = [];
  let goalState: SessionGoalState | undefined = buildInitialGoal(fixture);

  const goalStore =
    goalState !== undefined
      ? new SessionGoalStore({
          readState: () => goalState,
          writeState: async (s) => {
            goalState = s as SessionGoalState | undefined;
          },
        })
      : undefined;

  const ctx: TestAgentContext = testAgent({
    initialConfig: {
      providers: {},
      loopControl: fixture.loopControl
        ? {
            maxStepsPerTurn: fixture.loopControl.maxSteps,
            maxRetriesPerStep: fixture.loopControl.maxRetryAttempts,
          }
        : undefined,
    },
    goals: goalStore,
    telemetry: recordingTelemetry(telemetryRecords),
  });

  const toolNames = fixture.tools.map((t) => t.name);
  ctx.configure({ tools: toolNames });
  if (toolNames.includes('Bash') || toolNames.includes('UpdateGoal')) {
    await ctx.rpc.setPermission({ mode: 'yolo' });
  }

  for (const response of fixture.responses) {
    ctx.mockNextResponse(...buildParts(response));
  }

  const turns: TurnL3Snapshot['turns'] = [];
  let lastEventIndex = 0;

  for (const action of fixture.actions) {
    switch (action.op) {
      case 'prompt': {
        await ctx.rpc.prompt({ input: toContentParts(action.input) });
        break;
      }
      case 'steer': {
        await ctx.rpc.steer({ input: toContentParts(action.input) });
        break;
      }
      case 'cancel': {
        await ctx.rpc.cancel({ turnId: action.turnId, reason: action.reason });
        break;
      }
      case 'wait': {
        await ctx.untilTurnEnd();
        const slice = ctx.allEvents.slice(lastEventIndex);
        lastEventIndex = ctx.allEvents.length;
        const ended = slice.find(
          (e) => e.type === '[rpc]' && e.event === 'turn.ended',
        );
        if (ended) {
          turns.push({
            turnId: (ended.args as { turnId: number }).turnId,
            reason: (ended.args as { reason: string }).reason,
            error: (ended.args as { error?: unknown }).error,
          });
        }
        break;
      }
    }
  }

  const events = ctx.allEvents.filter((e) => e.type === '[rpc]').map((e) => ({
    type: e.event,
    ...(e.args as Record<string, unknown>),
  }));

  const records = ctx.records.map((r) => JSON.parse(JSON.stringify(r)) as unknown);

  const contextInputs = ctx.agent.context
    .data()
    .history.filter((m) => m.role === 'user')
    .map((m) => ({
      text: m.content
        .filter((p): p is Extract<typeof p, { type: 'text' }> => p.type === 'text')
        .map((p) => p.text)
        .join(''),
      originKind: m.origin?.kind ?? 'unknown',
    }));

  const goalSnapshot = ctx.agent.goals?.getGoal().goal;

  return {
    name: fixture.name,
    turns,
    events,
    records,
    contextInputs,
    telemetry: telemetryRecords.map((t) => ({ event: t.event, properties: t.properties })),
    goalState: goalSnapshot
      ? {
          status: goalSnapshot.status,
          turnsUsed: goalSnapshot.turnsUsed,
          tokensUsed: goalSnapshot.tokensUsed,
        }
      : undefined,
  };
}
```

- [ ] Run it and verify it PASSES.

```bash
cd packages/integration-tests && pnpm vitest run test/parity/turn-l3.test.ts
```

预期：8 个 fixture 都能在 TS runner 下跑完并产生包含 `turn.started` / `turn.ended` 的 snapshot。

- [ ] Commit.

```bash
git add packages/integration-tests/src/parity/turn-l3-driver.ts packages/agent-core/test/agent/harness/agent.ts packages/integration-tests/test/parity/turn-l3.test.ts
git commit -m "test(integration-tests): add TS runner for TurnFlow L3 fixtures"
```

---

## Task 5: Snapshot normalization `normalize-turn.ts`

**Depends on:** Task 2（fixture/snapshot 形状已定）

**Files:**
- Create: `packages/integration-tests/src/parity/normalize-turn.ts`
- Create: `packages/integration-tests/test/parity/normalize-turn.test.ts`

### Steps

- [ ] Write the failing test.

创建 `packages/integration-tests/test/parity/normalize-turn.test.ts`：

```ts
import { describe, expect, it } from 'vitest';

import { normalizeTurnSnapshot } from '../../src/parity/normalize-turn';

describe('normalizeTurnSnapshot', () => {
  it('replaces uuids, step ids, and timestamps', () => {
    const input = {
      name: 'end-turn',
      turns: [{ turnId: 0, reason: 'completed' }],
      events: [
        {
          type: 'turn.step.started',
          turnId: 0,
          step: 1,
          stepId: '123e4567-e89b-12d3-a456-426614174000',
        },
        {
          type: 'turn.step.completed',
          turnId: 0,
          step: 1,
          stepId: '123e4567-e89b-12d3-a456-426614174001',
          usage: { inputOther: 5, output: 3, inputCacheRead: 0, inputCacheCreation: 0 },
          llmFirstTokenLatencyMs: 120,
          llmStreamDurationMs: 340,
        },
      ],
      records: [
        {
          type: 'turn.prompt',
          time: 1700000000000,
          input: [{ type: 'text', text: 'Hello' }],
          origin: { kind: 'user' },
        },
      ],
      contextInputs: [{ text: 'Hello', originKind: 'user' }],
      telemetry: [{ event: 'turn_started', properties: { mode: 'agent' } }],
    };

    const normalized = normalizeTurnSnapshot(input as never);

    expect(normalized.events[0].stepId).toBe('<id>');
    expect(normalized.events[1].stepId).toBe('<id>');
    expect(normalized.events[1].llmFirstTokenLatencyMs).toBe(0);
    expect(normalized.events[1].llmStreamDurationMs).toBe(0);
    expect(normalized.records[0].time).toBe('<time>');
  });

  it('does not replace deterministic turn ids or step numbers', () => {
    const input = {
      name: 'x',
      turns: [{ turnId: 2, reason: 'completed' }],
      events: [{ type: 'turn.started', turnId: 2 }],
      records: [],
      contextInputs: [],
      telemetry: [],
    };
    const normalized = normalizeTurnSnapshot(input as never);
    expect(normalized.turns[0].turnId).toBe(2);
    expect(normalized.events[0].turnId).toBe(2);
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
cd packages/integration-tests && pnpm vitest run test/parity/normalize-turn.test.ts
```

预期失败：`normalize-turn.ts` 不存在。

- [ ] Write the minimal implementation.

创建 `packages/integration-tests/src/parity/normalize-turn.ts`：

```ts
const UUID_RE = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi;

const TIME_FIELDS = new Set([
  'time',
  'createdAt',
  'updatedAt',
  'wallClockResumedAt',
]);

const DURATION_FIELDS = new Set([
  'duration_ms',
  'llmFirstTokenLatencyMs',
  'llmStreamDurationMs',
]);

const ID_FIELDS = new Set([
  'stepId',
  'stepUuid',
  'uuid',
  'toolCallId',
  'goalId',
]);

function normalizeString(value: string): string {
  return value.replace(UUID_RE, '<id>');
}

function normalizeValue(key: string | undefined, value: unknown): unknown {
  if (key !== undefined && TIME_FIELDS.has(key)) {
    return '<time>';
  }
  if (key !== undefined && DURATION_FIELDS.has(key)) {
    return 0;
  }
  if (key !== undefined && ID_FIELDS.has(key) && typeof value === 'string') {
    return normalizeString(value);
  }
  if (typeof value === 'string') {
    return normalizeString(value);
  }
  if (Array.isArray(value)) {
    return value.map((item) => normalizeValue(undefined, item));
  }
  if (value !== null && typeof value === 'object') {
    const out: Record<string, unknown> = {};
    for (const [k, v] of Object.entries(value)) {
      if (v !== undefined) {
        out[k] = normalizeValue(k, v);
      }
    }
    return out;
  }
  return value;
}

export function normalizeTurnSnapshot(snapshot: {
  readonly name: string;
  readonly turns: unknown;
  readonly events: unknown;
  readonly records: unknown;
  readonly contextInputs: unknown;
  readonly telemetry: unknown;
  readonly goalState?: unknown;
}) {
  return {
    name: snapshot.name,
    turns: normalizeValue(undefined, snapshot.turns),
    events: normalizeValue(undefined, snapshot.events),
    records: normalizeValue(undefined, snapshot.records),
    contextInputs: normalizeValue(undefined, snapshot.contextInputs),
    telemetry: normalizeValue(undefined, snapshot.telemetry),
    goalState: normalizeValue(undefined, snapshot.goalState),
  };
}
```

- [ ] Run it and verify it PASSES.

```bash
cd packages/integration-tests && pnpm vitest run test/parity/normalize-turn.test.ts
```

预期：两个测试均通过；UUID / 时间戳 / duration 被正确归一化，而 `turnId` / `step` 保持不变。

- [ ] Commit.

```bash
git add packages/integration-tests/src/parity/normalize-turn.ts packages/integration-tests/test/parity/normalize-turn.test.ts
git commit -m "test(integration-tests): add TurnFlow snapshot normalization"
```

---

## Task 6: TS↔Rust parity test

**Depends on:** Task 3, Task 4, Task 5

**Files:**
- Create: `packages/integration-tests/test/parity/turn-l3-parity.test.ts`

### Steps

- [ ] Write the failing test.

创建 `packages/integration-tests/test/parity/turn-l3-parity.test.ts`：

```ts
import { execFile } from 'node:child_process';
import { existsSync } from 'node:fs';
import { promisify } from 'node:util';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import { assertParity } from '../../src/parity/assert-parity';
import { normalizeTurnSnapshot } from '../../src/parity/normalize-turn';
import { runTurnL3Fixture } from '../../src/parity/turn-l3-driver';

const execFileAsync = promisify(execFile);

const __dirname = dirname(fileURLToPath(import.meta.url));
const fixturesDir = join(__dirname, '../../src/parity/fixtures/turn');

const fixtures = [
  'end-turn.json',
  'single-tool-call.json',
  'tool-not-found.json',
  'steer-buffer.json',
  'cancel-mid-step.json',
  'same-step-dedup.json',
  'cross-step-dedup.json',
  'goal-continuation.json',
];

function findProjectRoot(): string {
  let current = __dirname;
  while (current !== dirname(current)) {
    if (existsSync(join(current, '.git'))) {
      return current;
    }
    current = dirname(current);
  }
  return process.cwd();
}

async function runRustFixture(fixtureName: string): Promise<unknown> {
  const root = findProjectRoot();
  const fixturePath = join(fixturesDir, fixtureName);
  const { stdout } = await execFileAsync(
    'cargo',
    ['run', '--quiet', '--bin', 'turn_l3', '--', fixturePath],
    { cwd: join(root, 'rust-ody') },
  );
  return JSON.parse(stdout) as unknown;
}

describe('turn L3 TS-vs-Rust parity', () => {
  it.each(fixtures)(
    '%s matches the Rust golden binary',
    async (fixtureName) => {
      const fixturePath = join(fixturesDir, fixtureName);

      const tsSnapshot = normalizeTurnSnapshot(await runTurnL3Fixture(fixturePath));
      const rustSnapshot = normalizeTurnSnapshot(
        (await runRustFixture(fixtureName)) as {
          readonly name: string;
          readonly turns: unknown;
          readonly events: unknown;
          readonly records: unknown;
          readonly contextInputs: unknown;
          readonly telemetry: unknown;
          readonly goalState?: unknown;
        },
      );

      const diff = assertParity(fixtureName, tsSnapshot as never, rustSnapshot as never);
      expect(diff).toBeNull();
    },
    120000,
  );
});
```

- [ ] Run it and verify it FAILS.

```bash
cd packages/integration-tests && pnpm vitest run test/parity/turn-l3-parity.test.ts
```

预期失败：第一次运行时 TS 与 Rust 的 snapshot 存在差异（例如字段名、事件数量、records 内容）。这是期望的：差异会逐字段打印，工程师据此修复实现直到绿。

- [ ] Write the minimal implementation.

本 task 的“实现”是**通过修复 Part 2/3 的 `TurnFlow` / `ToolCallDeduplicator` / 事件映射代码**，使 parity test 通过。本文件本身无需额外实现；当 `turn-l3-parity.test.ts` 全绿时，Task 6 完成。

- [ ] Run it and verify it PASSES.

```bash
cd packages/integration-tests && pnpm vitest run test/parity/turn-l3-parity.test.ts
```

预期：8 个 fixture 的 TS 与 Rust snapshot 在归一化后 `assertParity` 返回 `null`。

- [ ] Commit.

```bash
git add packages/integration-tests/test/parity/turn-l3-parity.test.ts
git commit -m "test(integration-tests): add TurnFlow L3 TS-vs-Rust parity test"
```

---

## Task 7: npm 脚本与 CI 集成

**Depends on:** Task 4, Task 6

**Files:**
- Modify: `package.json`
- Modify: `packages/integration-tests/package.json`
- Modify: `.github/workflows/ci.yml`

### Steps

- [ ] 在根目录 `package.json` 增加一个聚合命令。

```json
{
  "scripts": {
    "test:parity": "pnpm --filter integration-tests test:parity"
  }
}
```

- [ ] 在 `packages/integration-tests/package.json` 增加：

```json
{
  "scripts": {
    "test:parity": "vitest run test/parity/turn-l3-parity.test.ts"
  }
}
```

- [ ] 在 `.github/workflows/ci.yml` 的 Rust 矩阵 job 后追加一步，确保每次 CI 都运行 parity test。

新增 step（放置在 `cargo test` 之后，job 末尾）：

```yaml
      - name: Run TurnFlow L3 TS↔Rust parity tests
        run: pnpm test:parity
```

如果 CI 环境未安装 Rust toolchain，已在同一 job 中安装；该 step 会在 `pnpm install` 与 Rust build cache 之后运行。

- [ ] 本地验证脚本可用。

```bash
pnpm test:parity
```

预期：命令正确触发 `cd packages/integration-tests && pnpm vitest run test/parity/turn-l3-parity.test.ts`。

- [ ] Commit.

```bash
git add package.json packages/integration-tests/package.json .github/workflows/ci.yml
git commit -m "chore: wire TurnFlow L3 parity tests into npm and CI"
```

---

## Task 8: 全仓库类型检查与构建验证

**Depends on:** Task 2, Task 3, Task 4, Task 6, Task 7

**Files:**
- 覆盖范围：`rust-ody/crates/agent-rs`、`packages/integration-tests`、`packages/agent-core` 等所有被改动触及的包

### Steps

- [ ] 运行 Rust 侧类型检查。

```bash
cd rust-ody && cargo check --all-targets
```

预期：无编译错误。

- [ ] 运行 Rust 测试。

```bash
cd rust-ody && cargo test --all-targets
```

预期：现有测试通过；新增的 `turn_l3` binary 不直接参与测试，但 `cargo check` 已覆盖编译。

- [ ] 运行 TS 全仓库类型检查。

```bash
pnpm -r typecheck
```

预期：所有 workspace package 的 `typecheck` 脚本通过，没有因为新增类型或导入导致的 tsc 错误。

- [ ] 运行完整 parity 测试（作为构建验证）。

```bash
pnpm test:parity
```

预期：8 个 fixture 全部绿。

- [ ] 提交任何意外需要的修复（如果类型检查通过且测试绿，则无需额外提交）。

---

## Part Self-Review

- [ ] 1. Spec-coverage table：本 part 覆盖 roadmap 4.3.5.6 的 L3 fixture、golden binary、TS runner、TS↔Rust parity test，无 GAP。

| Roadmap 4.3.5 条目 | 覆盖任务 | 状态 |
|---|---|---|
| 4.3.5.6 L3 fixture（end-turn / single-tool-call / tool-not-found / steer-buffer / cancel-mid-step / same-step-dedup / cross-step-dedup / goal-continuation） | Task 2, Task 3, Task 4, Task 6, Task 7 | covered |

- [ ] 2. Placeholder scan：`parity.md` 中无 TODO/TBD；所有 fixture JSON、代码、命令均为完整可执行内容。
- [ ] 3. No phantom tasks：每个 task 都产出文件变更与可验证步骤；Task 8 的 typecheck 不产出空提交。
- [ ] 4. Dependency soundness：`parity.md` 内部 Task 1 → 2 → 3 → 4/5 → 6 → 7 → 8 均为单向；跨 part 依赖 `types.md`、`turn.md`、`adapter.md` 均已在 index 声明。
- [ ] 5. Caller & build soundness：Task 3 新增 `turn_l3` binary 与 Task 4 扩展 `AgentTestContext` 均在本 part 内完成；Task 8 以全 workspace `cargo check/test` 与 `pnpm -r typecheck` 收尾，非单包构建。
- [ ] 6. Test-the-risk：Task 6 的 parity test 对同一 fixture 同时驱动 TS 与 Rust，逐事件断言输出一致；Task 5 的归一化对时间/ID 字段进行白盒替换，避免时间相关抖动导致误报。
- [ ] 7. Type 一致性：TS driver 与 Rust golden binary 的 snapshot schema 字段名（`name`、`turns`、`events`、`records`、`contextInputs`、`telemetry`、`goalState`）一致；事件映射 `{ type: eventName, ...args }` 与 Rust `AgentEvent` 序列化对应。

---
