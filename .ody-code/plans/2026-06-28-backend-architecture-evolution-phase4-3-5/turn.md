# 4.3.5 Part 2 — TurnFlow 状态机核心

**Scope:** 在 Part 1 的 `TurnAgent` trait 之上，实现 `TurnFlow` 的状态机：turn 生命周期（prompt/steer/cancel/wait）、`runOneTurn`、`runStepLoop`（对接 `agent_loop::run_turn`）、`driveGoal` 自动 continuation、steer buffer、事件映射与错误分类。本 part 不依赖 KosongLLM / ToolCallDeduplicator 的具体实现，只使用 `agent_loop::llm::Llm` trait 与本地占位去重器。

---

## Local File Structure

| File / Directory | Responsibility |
|---|---|
| `rust-ody/crates/agent-rs/src/turn/types.rs` | 新增 `TurnEndResult`；扩展 `GoalSnapshot` 使其包含 budget 实时字段 |
| `rust-ody/crates/agent-rs/src/turn/turn_flow.rs` | `TurnFlow` 结构体与全部状态机方法 |
| `rust-ody/crates/agent-rs/src/turn/telemetry.rs` | `summarize_turn_error`、`classify_api_error`、`goal_failure_pause_reason`、工具 telemetry helper |
| `rust-ody/crates/agent-rs/src/turn/mod.rs` | re-export `TurnFlow`、`TurnEndResult`、telemetry helper |

---

## Local Dependency Overview

```text
Task 1: TurnFlow 骨架 + public API + FakeAgent
    │
    ▼
Task 2: telemetry/error helper + map_loop_event
    │
    ├──▶ Task 3: run_one_turn（调用 summarize_turn_error）
    │
    ├──▶ Task 4: run_step_loop（调用 map_loop_event）
    │
    ├──▶ Task 5: steer/cancel/wait（依赖 run_step_loop 提供的可结束 turn）
    │
    └──▶ Task 6: turn_worker + drive_goal（依赖 run_one_turn + run_step_loop）
              │
              ▼
Task 7: 模块接线 + 全 workspace typecheck
```

- **可并行**：Task 2 与 Task 1 在代码上可并行，但 Task 3-6 依赖两者。
- **硬前置**：Task 1 依赖 `types.md`（`TurnAgent` trait 已定义）。

---

## Local Spec-Coverage Table

| Roadmap 4.3.5.1 条目 | 覆盖任务 | 状态 |
|---|---|---|
| `prompt` / `steer` / `cancel` / `wait` 公开 API | Task 1, Task 5 | covered |
| `activeTurn` 生命周期（分配、同步释放、`resuming`） | Task 1, Task 3, Task 5, Task 6 | covered |
| `steerBuffer` 缓冲与同 frame 刷新 | Task 5 | covered |
| `turnWorker` / `runOneTurn` 单 turn 生命周期 | Task 3, Task 6 | covered |
| `driveGoal` 自动 continuation / budget / blocked | Task 6 | covered |
| LoopEvent → `AgentEvent` 映射 | Task 2, Task 4 | covered |

---

## Task 1: Scaffold `TurnFlow` public API + 扩展 `GoalSnapshot` + FakeAgent

**Depends on:** `types.md` Task 3

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/turn/types.rs:276-280`（扩展 `GoalSnapshot`）
- Create: `rust-ody/crates/agent-rs/src/turn/turn_flow.rs`
- Modify: `rust-ody/crates/agent-rs/src/turn/mod.rs`（re-export）

### Steps

- [ ] Write the failing test.

在 `rust-ody/crates/agent-rs/src/turn/turn_flow.rs` 末尾加入：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use kosong_rs::message::ContentPart;

    #[tokio::test]
    async fn prompt_allocates_monotonic_turn_id() {
        let agent = Arc::new(FakeAgent::new(FakeLlm::end_turn()));
        let flow = TurnFlow::new(agent.clone());
        let id1 = flow.prompt(vec![ContentPart::Text { text: "hi".into() }], USER_PROMPT_ORIGIN);
        assert!(id1.is_some());
        let _ = flow.wait_for_current_turn(None).await.unwrap();
        let id2 = flow.prompt(vec![ContentPart::Text { text: "again".into() }], USER_PROMPT_ORIGIN);
        assert!(id2.is_some());
        assert_eq!(id2.unwrap(), id1.unwrap() + 1);
    }

    #[tokio::test]
    async fn steer_buffers_while_active() {
        let agent = Arc::new(FakeAgent::new(FakeLlm::end_turn()));
        let flow = TurnFlow::new(agent.clone());
        let id = flow.prompt(vec![ContentPart::Text { text: "go".into() }], USER_PROMPT_ORIGIN).unwrap();
        assert!(flow.has_active_turn());
        let steer_id = flow.steer(vec![ContentPart::Text { text: "faster".into() }], USER_PROMPT_ORIGIN);
        assert_eq!(steer_id, None);
        let _ = flow.wait_for_current_turn(None).await.unwrap();
        let records = agent.records.lock().unwrap();
        assert!(records.iter().any(|r| matches!(r, AgentRecord::TurnSteer { .. })));
    }
}
```

- [ ] Run it and verify it FAILS.

```bash
cd rust-ody && cargo test -p agent-rs prompt_allocates_monotonic_turn_id --no-run
```

预期失败：`TurnFlow`、`FakeAgent`、`FakeLlm`、`TurnEndResult`、`USER_PROMPT_ORIGIN` 等未定义。

- [ ] Write the minimal implementation.

**Step 1a: 扩展 `GoalSnapshot`**

在 `rust-ody/crates/agent-rs/src/turn/types.rs` 中把 `GoalSnapshot` 替换为：

```rust
#[derive(Debug, Clone, Default)]
pub struct GoalSnapshot {
    pub status: GoalStatus,
    pub budget_limits: GoalBudgetLimits,
    pub tokens_used: i64,
    pub turns_used: i64,
    pub wall_clock_ms: i64,
}
```

**Step 1b: 新增 `TurnEndResult`**

在 `rust-ody/crates/agent-rs/src/turn/types.rs` 中 `GoalSnapshot` 之前加入：

```rust
use crate::agent_loop::types::LoopTurnStopReason;

#[derive(Debug, Clone, PartialEq)]
pub struct TurnEndResult {
    pub event: TurnEndedEvent,
    pub stop_reason: Option<LoopTurnStopReason>,
    pub blocked_by_user_prompt_hook: bool,
}
```

**Step 1c: 创建 `turn_flow.rs` 骨架**

```rust
use std::sync::{Arc, Mutex};

use kosong_rs::message::{ContentPart, Message};
use kosong_rs::provider::AbortSignal;
use tokio::sync::oneshot;

use crate::records::nested::PromptOrigin;
use crate::records::AgentRecord;
use crate::turn::types::{TurnAgent, TurnEndResult, USER_PROMPT_ORIGIN};

#[derive(Clone)]
pub struct TurnFlow {
    agent: Arc<dyn TurnAgent>,
    inner: Arc<Mutex<TurnFlowInner>>,
}

#[derive(Default)]
struct TurnFlowInner {
    steer_buffer: Vec<(Vec<ContentPart>, PromptOrigin)>,
    turn_id: i64,
    active_turn: Option<ActiveTurn>,
    current_step: u32,
}

struct ActiveTurn {
    signal: AbortSignal,
    result_rx: Option<oneshot::Receiver<TurnEndResult>>,
}

impl TurnFlow {
    pub fn new(agent: Arc<dyn TurnAgent>) -> Self {
        Self {
            agent,
            inner: Arc::new(Mutex::new(TurnFlowInner::default())),
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
        inner.turn_id += 1;
        inner.active_turn = Some(ActiveTurn {
            signal: AbortSignal::new(),
            result_rx: None,
        });
    }

    pub fn restore_steer(&self, input: Vec<ContentPart>, origin: PromptOrigin) {
        let mut inner = self.inner.lock().unwrap();
        if inner.active_turn.is_some() {
            inner.steer_buffer.push((input, origin));
            return;
        }
        inner.turn_id += 1;
        inner.active_turn = Some(ActiveTurn {
            signal: AbortSignal::new(),
            result_rx: None,
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
            match inner.active_turn.as_mut() {
                Some(active) => active.result_rx.take(),
                None => return Err(anyhow::anyhow!("No active turn")),
            }
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
            self.agent.event_emitter().emit_event(AgentEvent::Error(
                TurnErrorSummary {
                    code: "turn.agent_busy".into(),
                    name: "TurnAgentBusy".into(),
                    message: format!(
                        "Cannot launch a new turn while another turn (ID {}) is active",
                        inner.turn_id
                    ),
                    retryable: false,
                    details: Some(serde_json::json!({ "turnId": inner.turn_id })),
                },
            ));
            return None;
        }
        let turn_id = inner.turn_id + 1;
        inner.turn_id = turn_id;
        let signal = AbortSignal::new();
        let (tx, rx) = oneshot::channel();
        let flow = self.clone();
        tokio::spawn(async move {
            let result = flow
                .turn_worker(turn_id, input, origin, signal.clone())
                .await;
            let _ = tx.send(result);
        });
        inner.active_turn = Some(ActiveTurn {
            signal,
            result_rx: Some(rx),
        });
        Some(turn_id)
    }

    async fn turn_worker(
        &self,
        first_turn_id: i64,
        _input: Vec<ContentPart>,
        _origin: PromptOrigin,
        _signal: AbortSignal,
    ) -> TurnEndResult {
        // Task 6 将替换为真实实现；此处先返回 completed 使 Task 1 测试可结束。
        TurnEndResult {
            event: TurnEndedEvent {
                turn_id: first_turn_id,
                reason: TurnEndedReason::Completed,
                error: None,
            },
            stop_reason: None,
            blocked_by_user_prompt_hook: false,
        }
    }
}

async fn wait_abort(signal: &AbortSignal) {
    while !signal.is_aborted() {
        tokio::task::yield_now().await;
    }
}

use crate::turn::types::{AgentEvent, TurnEndedEvent, TurnEndedReason, TurnErrorSummary};
```

**Step 1d: 更新 `turn/mod.rs`**

```rust
pub mod canonical_args;
pub mod error;
pub mod kosong_llm;
pub mod remote_kosong_llm;
pub mod telemetry;
pub mod tool_dedup;
pub mod turn_flow;
pub mod types;

pub use telemetry::*;
pub use turn_flow::*;
pub use types::*;
```

**Step 1e: FakeAgent / FakeLlm（测试替身，完整实现）**

在 `turn_flow.rs` 的 `#[cfg(test)]` 模块中加入完整 `FakeAgent` 与 `FakeLlm`。由于 trait 数量较多，先给出完整代码；后续任务只通过字段调整行为。

```rust
#[cfg(test)]
mod fake {
    use super::*;
    use std::sync::{Arc, Mutex};
    use kosong_rs::message::{ContentPart, Message, Role, ToolCall};
    use kosong_rs::provider::{AbortSignal, ChatProvider, ModelCapability};
    use kosong_rs::usage::TokenUsage;
    use serde_json::Value as JsonValue;

    use crate::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse, LlmStreamTiming};
    use crate::agent_loop::types::{
        AuthorizeToolExecutionResult, ExecutableTool, ExecutableToolResult, FinalizeToolResultContext,
        PrepareToolExecutionResult, ResolvedToolExecutionHookContext, RunnableToolExecution,
        ToolExecutionHookContext,
    };
    use crate::context::types::{ContextMessage, PromptOrigin, USER_PROMPT_ORIGIN};
    use crate::records::nested::{ExecutableToolOutput, GoalBudgetLimits, GoalStatus, UsageRecordScope};
    use crate::records::AgentRecord;
    use crate::turn::types::*;

    #[derive(Default, Clone)]
    pub struct Captures {
        pub events: Vec<AgentEvent>,
        pub records: Vec<AgentRecord>,
        pub context_inputs: Vec<(Vec<ContentPart>, PromptOrigin)>,
        pub begin_turn_count: usize,
        pub end_turn_count: usize,
        pub full_compaction_reset: usize,
        pub goal_increment_turn: usize,
        pub goal_mark_blocked: Vec<String>,
        pub goal_pause_on_interrupt: Vec<String>,
        pub hook_user_prompt_submit: Vec<Vec<ContentPart>>,
        pub hook_stop_hook: usize,
        pub telemetry_events: Vec<(String, JsonValue)>,
    }

    pub struct FakeAgent {
        pub captures: Arc<Mutex<Captures>>,
        pub llm: Arc<dyn Llm>,
        pub has_model: bool,
        pub goal_status: Arc<Mutex<Option<GoalSnapshot>>>,
        pub goal_runtime_enabled: bool,
        pub hook_results: Arc<Mutex<Vec<HookResult>>>,
        pub stop_block: Arc<Mutex<Option<StopHookBlock>>>,
    }

    impl FakeAgent {
        pub fn new(llm: Arc<dyn Llm>) -> Self {
            Self {
                captures: Arc::new(Mutex::new(Captures::default())),
                llm,
                has_model: true,
                goal_status: Arc::new(Mutex::new(None)),
                goal_runtime_enabled: false,
                hook_results: Arc::new(Mutex::new(Vec::new())),
                stop_block: Arc::new(Mutex::new(None)),
            }
        }
    }

    impl TurnAgent for FakeAgent {
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
            self.goal_runtime_enabled
        }
    }

    #[async_trait::async_trait]
    impl TurnContext for FakeAgent {
        fn append_user_message(&self, content: Vec<ContentPart>, origin: PromptOrigin) {
            self.captures.lock().unwrap().context_inputs.push((content.clone(), origin.clone()));
        }
        fn append_message(&self, _message: ContextMessage) {}
        fn messages(&self) -> Vec<Message> {
            vec![Message {
                role: Role::User,
                name: None,
                content: vec![ContentPart::Text { text: "history".into() }],
                tool_calls: vec![],
                tool_call_id: None,
                partial: None,
            }]
        }
        fn append_loop_event(&self, _event: crate::agent_loop::types::LoopRecordedEvent) {}
        fn has_open_steps(&self) -> bool {
            false
        }
        fn clear(&self) {}
    }

    impl TurnUsage for FakeAgent {
        fn begin_turn(&self) {
            self.captures.lock().unwrap().begin_turn_count += 1;
        }
        fn end_turn(&self) {
            self.captures.lock().unwrap().end_turn_count += 1;
        }
        fn record(&self, _model: &str, _usage: TokenUsage, _scope: UsageRecordScope) {}
    }

    impl TurnConfig for FakeAgent {
        fn model(&self) -> String {
            "kimi-k2".into()
        }
        fn model_alias(&self) -> Option<String> {
            None
        }
        fn system_prompt(&self) -> String {
            "You are a helpful assistant.".into()
        }
        fn thinking_level(&self) -> String {
            "off".into()
        }
        fn provider(&self) -> Box<dyn ChatProvider> {
            panic!("FakeAgent::provider not needed in turn flow tests")
        }
        fn model_capabilities(&self) -> ModelCapability {
            ModelCapability::default()
        }
        fn loop_control(&self) -> Option<LoopControl> {
            None
        }
        fn has_model(&self) -> bool {
            self.has_model
        }
    }

    impl TurnTools for FakeAgent {
        fn loop_tools(&self) -> Vec<Arc<dyn ExecutableTool>> {
            vec![]
        }
    }

    #[async_trait::async_trait]
    impl TurnPermission for FakeAgent {
        async fn before_tool_call(
            &self,
            _ctx: ResolvedToolExecutionHookContext<'_>,
        ) -> Result<Option<AuthorizeToolExecutionResult>, anyhow::Error> {
            Ok(None)
        }
    }

    #[async_trait::async_trait]
    impl TurnInjection for FakeAgent {
        async fn inject_goal(&self) {}
        async fn inject(&self) {}
    }

    #[async_trait::async_trait]
    impl TurnFullCompaction for FakeAgent {
        fn reset_for_turn(&self) {
            self.captures.lock().unwrap().full_compaction_reset += 1;
        }
        async fn before_step(&self, _signal: AbortSignal) {}
        async fn after_step(&self) {}
        async fn handle_overflow_error(&self, _signal: AbortSignal, _error: anyhow::Error) {}
    }

    impl TurnMicroCompaction for FakeAgent {
        fn detect(&self) {}
    }

    #[async_trait::async_trait]
    impl TurnSplitPlanCheckpoint for FakeAgent {
        async fn before_step(&self, _signal: AbortSignal) {}
    }

    #[async_trait::async_trait]
    impl TurnNormalTaskCheckpoint for FakeAgent {
        async fn before_step(&self, _signal: AbortSignal) {}
    }

    impl TurnSessionMode for FakeAgent {
        fn is_active(&self) -> bool {
            false
        }
        fn kind(&self) -> Option<String> {
            None
        }
    }

    #[async_trait::async_trait]
    impl TurnGoal for FakeAgent {
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
            self.captures.lock().unwrap().goal_mark_blocked.push(reason.into());
            let mut g = self.goal_status.lock().unwrap();
            if let Some(ref mut s) = *g {
                s.status = GoalStatus::Blocked;
            }
        }
        async fn pause_on_interrupt(&self, reason: &str) {
            self.captures.lock().unwrap().goal_pause_on_interrupt.push(reason.into());
        }
        async fn pause_active_goal(&self, _actor: &str, _reason: &str) {}
        async fn record_token_usage(
            &self,
            _token_delta: i64,
            _agent_id: &str,
            _agent_type: &str,
            _source: &str,
        ) -> Option<GoalSnapshot> {
            self.goal_status.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl TurnHooks for FakeAgent {
        async fn trigger_user_prompt_submit(
            &self,
            input: Vec<ContentPart>,
            _signal: AbortSignal,
        ) -> Result<Vec<HookResult>, anyhow::Error> {
            self.captures.lock().unwrap().hook_user_prompt_submit.push(input);
            Ok(self.hook_results.lock().unwrap().clone())
        }
        async fn trigger_stop_hook(
            &self,
            _signal: AbortSignal,
        ) -> Result<Option<StopHookBlock>, anyhow::Error> {
            self.captures.lock().unwrap().hook_stop_hook += 1;
            Ok(self.stop_block.lock().unwrap().clone())
        }
        fn fire_and_forget_trigger(&self, _event: &str, _data: JsonValue) {}
    }

    impl TurnTelemetry for FakeAgent {
        fn track(&self, event: &str, properties: JsonValue) {
            self.captures.lock().unwrap().telemetry_events.push((event.into(), properties));
        }
    }

    impl TurnLog for FakeAgent {
        fn debug(&self, _msg: &str, _data: JsonValue) {}
        fn warn(&self, _msg: &str, _data: JsonValue) {}
        fn error(&self, _msg: &str, _data: JsonValue) {}
    }

    impl TurnRecords for FakeAgent {
        fn log_record(&self, record: AgentRecord) {
            self.captures.lock().unwrap().records.push(record);
        }
    }

    impl TurnEventEmitter for FakeAgent {
        fn emit_event(&self, event: AgentEvent) {
            self.captures.lock().unwrap().events.push(event);
        }
    }

    impl TurnLlmResolver for FakeAgent {
        fn refresh_llm(&self) {}
        fn llm(&self) -> Arc<dyn Llm> {
            self.llm.clone()
        }
    }

    impl TurnSubagentHost for FakeAgent {
        fn cancel_all(&self, _reason: &str) {}
    }

    pub struct FakeLlm {
        response: LlmChatResponse,
    }

    impl FakeLlm {
        pub fn end_turn() -> Arc<dyn Llm> {
            Arc::new(Self {
                response: LlmChatResponse {
                    tool_calls: vec![],
                    provider_finish_reason: Some(kosong_rs::provider::FinishReason::Stop),
                    raw_finish_reason: None,
                    usage: TokenUsage::default(),
                    stream_timing: Some(LlmStreamTiming {
                        first_token_latency_ms: 0,
                        stream_duration_ms: 0,
                    }),
                },
            })
        }
    }

    #[async_trait::async_trait]
    impl Llm for FakeLlm {
        fn system_prompt(&self) -> &str {
            ""
        }
        fn model_name(&self) -> &str {
            "fake"
        }
        async fn chat(&self, _params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
            Ok(self.response.clone())
        }
    }
}

#[cfg(test)]
use fake::*;
```

- [ ] Run it and verify it PASSES.

```bash
cd rust-ody && cargo test -p agent-rs prompt_allocates_monotonic_turn_id steer_buffers_while_active
```

预期：两个测试通过。

- [ ] Commit.

```bash
git add rust-ody/crates/agent-rs/src/turn/types.rs rust-ody/crates/agent-rs/src/turn/turn_flow.rs rust-ody/crates/agent-rs/src/turn/mod.rs
git commit -m "feat(agent-rs): scaffold TurnFlow public API and FakeAgent"
```

---

## Task 2: telemetry/error helper + `map_loop_event`

**Depends on:** Task 1

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/turn/types.rs:214-218`（为 `TurnUsage` 增加 `current_turn_usage`）
- Create: `rust-ody/crates/agent-rs/src/turn/telemetry.rs`
- Modify: `rust-ody/crates/agent-rs/src/turn/turn_flow.rs`（FakeAgent 追加 `current_turn_usage` 实现）

### Steps

- [ ] Write the failing test.

在 `rust-ody/crates/agent-rs/src/turn/telemetry.rs` 末尾加入：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use kosong_rs::usage::TokenUsage;

    #[test]
    fn summarize_turn_error_replaces_model_not_configured_message() {
        let err = anyhow::anyhow!(OdyError::new(
            "model.not_configured",
            "OdyError",
            "Model not set",
        ));
        let summary = summarize_turn_error(&err, 7);
        assert_eq!(summary.code, "model.not_configured");
        assert_eq!(summary.message, "LLM not set, send \"/login\" to login");
        assert_eq!(summary.details.as_ref().unwrap()["turnId"], 7);
    }

    #[test]
    fn classify_api_error_buckets_status_codes() {
        let err = anyhow::anyhow!("boom");
        let summary = TurnErrorSummary {
            code: "provider.api".into(),
            name: "APIStatusError".into(),
            message: "Bad Request".into(),
            retryable: false,
            details: Some(serde_json::json!({ "statusCode": 429 })),
        };
        let c = classify_api_error(&err, &summary);
        assert_eq!(c.error_type, "rate_limit");
        assert_eq!(c.status_code, Some(429));
    }

    #[test]
    fn map_loop_event_maps_step_begin() {
        use crate::agent_loop::events::LoopEvent;
        use crate::agent_loop::types::LoopRecordedEvent;
        let event = LoopEvent::Recorded(LoopRecordedEvent::StepBegin {
            uuid: "step-1".into(),
            turn_id: "42".into(),
            step: 3,
        });
        let mapped = map_loop_event(&event, 42).unwrap();
        match mapped {
            AgentEvent::TurnStepStarted { turn_id, step, step_id } => {
                assert_eq!(turn_id, 42);
                assert_eq!(step, 3);
                assert_eq!(step_id, "step-1");
            }
            _ => panic!("expected TurnStepStarted"),
        }
    }
}
```

- [ ] Run it and verify it FAILS.

```bash
cd rust-ody && cargo test -p agent-rs summarize_turn_error_replaces_model_not_configured_message --no-run
```

预期失败：`telemetry.rs` 不存在，`TurnUsage` 无 `current_turn_usage`，`OdyError` 未定义。

- [ ] Write the minimal implementation.

**Step 2a: 扩展 `TurnUsage` trait**

在 `rust-ody/crates/agent-rs/src/turn/types.rs` 中：

```rust
pub trait TurnUsage: Send + Sync {
    fn begin_turn(&self);
    fn end_turn(&self);
    fn record(&self, model: &str, usage: TokenUsage, scope: UsageRecordScope);
    fn current_turn_usage(&self) -> Option<TokenUsage>;
}
```

**Step 2b: 创建 `telemetry.rs`**

```rust
use std::fmt;

use kosong_rs::message::ContentPart;
use kosong_rs::usage::TokenUsage;
use serde_json::Value as JsonValue;

use crate::agent_loop::events::{LoopEvent, LoopInterruptReason, LoopLiveOnlyEvent, LoopRecordedEvent};
use crate::agent_loop::types::LoopTurnStopReason;
use crate::records::nested::{ExecutableToolOutput, ExecutableToolResult, PromptOrigin};
use crate::turn::types::{AgentEvent, StepRetryingEvent, TurnEndedEvent, TurnEndedReason, TurnErrorSummary};

#[derive(Debug, Clone)]
pub struct OdyError {
    pub code: String,
    pub name: String,
    pub message: String,
    pub retryable: bool,
    pub details: Option<JsonValue>,
}

impl OdyError {
    pub fn new(code: impl Into<String>, name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            name: name.into(),
            message: message.into(),
            retryable: false,
            details: None,
        }
    }

    pub fn with_details(mut self, details: JsonValue) -> Self {
        self.details = Some(details);
        self
    }
}

impl fmt::Display for OdyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.message)
    }
}

impl std::error::Error for OdyError {}

pub fn to_ody_error(error: &anyhow::Error) -> Option<OdyError> {
    error.downcast_ref::<OdyError>().cloned()
}

pub fn summarize_turn_error(error: &anyhow::Error, turn_id: i64) -> TurnErrorSummary {
    const LLM_NOT_SET_MESSAGE: &str = "LLM not set, send \"/login\" to login";
    let mut summary = if let Some(ody) = to_ody_error(error) {
        let mut details = ody.details.clone().unwrap_or_else(|| JsonValue::Object(Default::default()));
        if let Some(obj) = details.as_object_mut() {
            obj.insert("turnId".into(), turn_id.into());
        }
        TurnErrorSummary {
            code: ody.code,
            name: ody.name,
            message: ody.message,
            retryable: ody.retryable,
            details: Some(details),
        }
    } else {
        TurnErrorSummary {
            code: "runtime.error".into(),
            name: error.to_string(),
            message: error.to_string(),
            retryable: false,
            details: Some(serde_json::json!({ "turnId": turn_id })),
        }
    };
    if summary.code == "model.not_configured" {
        summary.message = LLM_NOT_SET_MESSAGE.into();
    }
    summary
}

pub fn goal_failure_pause_reason(error: Option<&TurnErrorSummary>) -> Option<&'static str> {
    const GOAL_RATE_LIMIT_PAUSE_REASON: &str = "Paused after provider rate limit";
    if let Some(e) = error {
        if e.code == "provider.rate_limit" {
            return Some(GOAL_RATE_LIMIT_PAUSE_REASON);
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApiErrorClassification {
    pub error_type: String,
    pub status_code: Option<i32>,
}

pub fn classify_api_error(error: &anyhow::Error, summary: &TurnErrorSummary) -> ApiErrorClassification {
    let status_code = api_status_code(error).or_else(|| summary_status_code(summary));
    if let Some(sc) = status_code {
        if sc == 429 {
            return ApiErrorClassification { error_type: "rate_limit".into(), status_code: Some(sc) };
        }
        if sc == 401 || sc == 403 {
            return ApiErrorClassification { error_type: "auth".into(), status_code: Some(sc) };
        }
        if sc >= 500 {
            return ApiErrorClassification { error_type: "5xx_server".into(), status_code: Some(sc) };
        }
        if is_context_overflow_status_error(sc, &summary.message) {
            return ApiErrorClassification { error_type: "context_overflow".into(), status_code: Some(sc) };
        }
        if sc >= 400 {
            return ApiErrorClassification { error_type: "4xx_client".into(), status_code: Some(sc) };
        }
        return ApiErrorClassification { error_type: "api".into(), status_code: Some(sc) };
    }
    if summary.code == "provider.rate_limit" {
        return ApiErrorClassification { error_type: "rate_limit".into(), status_code: None };
    }
    if summary.code == "provider.auth_error" {
        return ApiErrorClassification { error_type: "auth".into(), status_code: None };
    }
    if summary.code == "context_overflow" {
        return ApiErrorClassification { error_type: "context_overflow".into(), status_code: None };
    }
    if is_api_connection_error(error, summary) {
        return ApiErrorClassification { error_type: "network".into(), status_code: None };
    }
    if is_api_timeout_error(error, summary) {
        return ApiErrorClassification { error_type: "timeout".into(), status_code: None };
    }
    if is_api_empty_response_error(error, summary) {
        return ApiErrorClassification { error_type: "empty_response".into(), status_code: None };
    }
    ApiErrorClassification { error_type: "other".into(), status_code: None }
}

fn api_status_code(error: &anyhow::Error) -> Option<i32> {
    if let Some(ody) = to_ody_error(error) {
        return status_code_from_value(&ody.details);
    }
    None
}

fn summary_status_code(summary: &TurnErrorSummary) -> Option<i32> {
    summary.details.as_ref().and_then(status_code_from_value)
}

fn status_code_from_value(value: &Option<JsonValue>) -> Option<i32> {
    value.as_ref()?.get("statusCode")?.as_i64()?.try_into().ok()
}

fn is_context_overflow_status_error(status_code: i32, message: &str) -> bool {
    status_code == 413 || (status_code == 400 && message.to_lowercase().contains("context"))
}

fn is_api_connection_error(error: &anyhow::Error, summary: &TurnErrorSummary) -> bool {
    error.to_string().to_lowercase().contains("connection") || summary.name == "APIConnectionError"
}

fn is_api_timeout_error(error: &anyhow::Error, summary: &TurnErrorSummary) -> bool {
    let text = error.to_string().to_lowercase();
    text.contains("timeout") || summary.name == "APITimeoutError" || summary.name == "TimeoutError"
}

fn is_api_empty_response_error(error: &anyhow::Error, summary: &TurnErrorSummary) -> bool {
    error.to_string().to_lowercase().contains("empty response") || summary.name == "APIEmptyResponseError"
}

pub fn current_turn_input_tokens(usage: Option<&TokenUsage>) -> Option<i64> {
    usage.map(|u| u.input_total())
}

pub fn telemetry_tool_outcome(result: &ExecutableToolResult) -> &'static str {
    match result {
        ExecutableToolResult::Success(s) if s.is_error != Some(true) => "success",
        ExecutableToolResult::Success(s) => {
            let text = tool_output_text(&s.output).to_lowercase();
            if text.contains("aborted") || text.contains("cancelled") || text.contains("manually interrupted") {
                "cancelled"
            } else {
                "error"
            }
        }
        ExecutableToolResult::Error(_) => {
            let text = tool_result_text(result).to_lowercase();
            if text.contains("aborted") || text.contains("cancelled") || text.contains("manually interrupted") {
                "cancelled"
            } else {
                "error"
            }
        }
    }
}

pub fn telemetry_tool_error_type(result: &ExecutableToolResult) -> &'static str {
    let text = tool_result_text(result);
    if text.starts_with("Tool \"") && text.contains("\" not found") {
        return "ToolNotFound";
    }
    if text.starts_with("Invalid args for tool \"") {
        return "ToolInputError";
    }
    if text.contains("prepareToolExecution hook failed") || text.contains("finalizeToolResult hook failed") {
        return "HookError";
    }
    if text.contains("blocked") {
        return "ToolBlocked";
    }
    "ToolError"
}

pub fn tool_result_text(result: &ExecutableToolResult) -> String {
    match result {
        ExecutableToolResult::Success(s) => tool_output_text(&s.output),
        ExecutableToolResult::Error(e) => tool_output_text(&e.output),
    }
}

pub fn tool_output_text(output: &ExecutableToolOutput) -> String {
    match output {
        ExecutableToolOutput::Text(s) => s.clone(),
        ExecutableToolOutput::Parts(parts) => parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

pub fn tool_input_record(args: &JsonValue) -> JsonValue {
    if args.is_object() {
        args.clone()
    } else {
        JsonValue::Object(Default::default())
    }
}

pub fn map_loop_event(event: &LoopEvent, turn_id: i64) -> Option<AgentEvent> {
    match event {
        LoopEvent::Recorded(LoopRecordedEvent::StepBegin { uuid, step, .. }) => Some(AgentEvent::TurnStepStarted {
            turn_id,
            step: *step as u32,
            step_id: uuid.clone(),
        }),
        LoopEvent::Recorded(LoopRecordedEvent::StepEnd {
            uuid,
            step,
            usage,
            finish_reason,
            llm_first_token_latency_ms,
            llm_stream_duration_ms,
            provider_finish_reason,
            raw_finish_reason,
            ..
        }) => Some(AgentEvent::TurnStepCompleted {
            turn_id,
            step: *step as u32,
            step_id: uuid.clone(),
            usage: usage.clone().unwrap_or_default(),
            finish_reason: finish_reason.clone(),
            llm_first_token_latency_ms: *llm_first_token_latency_ms,
            llm_stream_duration_ms: *llm_stream_duration_ms,
            provider_finish_reason: provider_finish_reason.clone(),
            raw_finish_reason: raw_finish_reason.clone(),
        }),
        LoopEvent::Recorded(LoopRecordedEvent::ContentPartEvent { .. }) => None,
        LoopEvent::Recorded(LoopRecordedEvent::ToolCallEvent {
            tool_call_id,
            name,
            args,
            description,
            display,
            ..
        }) => Some(AgentEvent::ToolCallStarted {
            turn_id,
            tool_call_id: tool_call_id.clone(),
            name: name.clone(),
            args: args.clone(),
            description: description.clone(),
            display: display.clone(),
        }),
        LoopEvent::Recorded(LoopRecordedEvent::ToolResultEvent { tool_call_id, result, .. }) => {
            let is_error = match result {
                ExecutableToolResult::Success(s) => s.is_error,
                ExecutableToolResult::Error(_) => Some(true),
            };
            Some(AgentEvent::ToolResult {
                turn_id,
                tool_call_id: tool_call_id.clone(),
                output: match result {
                    ExecutableToolResult::Success(s) => s.output.clone(),
                    ExecutableToolResult::Error(e) => e.output.clone(),
                },
                is_error,
            })
        }
        LoopEvent::Live(LoopLiveOnlyEvent::TurnInterrupted { reason, active_step, message, .. }) => {
            let step = active_step?;
            let reason_str = match reason {
                LoopInterruptReason::Aborted => "aborted",
                LoopInterruptReason::MaxSteps => "max_steps",
                LoopInterruptReason::Error => "error",
            };
            Some(AgentEvent::TurnStepInterrupted {
                turn_id,
                step: *step,
                reason: reason_str.into(),
                message: message.clone(),
            })
        }
        LoopEvent::Live(LoopLiveOnlyEvent::StepRetrying(e)) => Some(AgentEvent::TurnStepRetrying(StepRetryingEvent {
            turn_id,
            step: e.step,
            step_uuid: e.step_uuid.clone(),
            failed_attempt: e.failed_attempt,
            next_attempt: e.next_attempt,
            max_attempts: e.max_attempts,
            delay_ms: e.delay_ms,
            error_name: e.error_name.clone(),
            error_message: e.error_message.clone(),
            status_code: e.status_code,
        })),
        LoopEvent::Live(LoopLiveOnlyEvent::TextDelta { delta }) => Some(AgentEvent::AssistantDelta {
            turn_id,
            delta: delta.clone(),
        }),
        LoopEvent::Live(LoopLiveOnlyEvent::ThinkingDelta { delta }) => Some(AgentEvent::ThinkingDelta {
            turn_id,
            delta: delta.clone(),
        }),
        LoopEvent::Live(LoopLiveOnlyEvent::ToolCallDelta { tool_call_id, name, arguments_part }) => {
            Some(AgentEvent::ToolCallDelta {
                turn_id,
                tool_call_id: tool_call_id.clone(),
                name: name.clone(),
                arguments_part: arguments_part.clone(),
            })
        }
        LoopEvent::Live(LoopLiveOnlyEvent::ToolProgress { tool_call_id, update }) => Some(AgentEvent::ToolProgress {
            turn_id,
            tool_call_id: tool_call_id.clone(),
            update: update.clone(),
        }),
    }
}
```

**Step 2c: FakeAgent 追加 `current_turn_usage`**

在 `turn_flow.rs` 的 `fake` 模块 `impl TurnUsage for FakeAgent` 中追加：

```rust
fn current_turn_usage(&self) -> Option<TokenUsage> {
    None
}
```

- [ ] Run it and verify it PASSES.

```bash
cd rust-ody && cargo test -p agent-rs summarize_turn_error_replaces_model_not_configured_message classify_api_error_buckets_status_codes map_loop_event_maps_step_begin
```

预期：三个测试均通过。

- [ ] Commit.

```bash
git add rust-ody/crates/agent-rs/src/turn/types.rs rust-ody/crates/agent-rs/src/turn/telemetry.rs rust-ody/crates/agent-rs/src/turn/turn_flow.rs
git commit -m "feat(agent-rs): add turn telemetry helpers and loop event mapping"
```

---

## Task 3: `run_one_turn` 生命周期 + user prompt hook

**Depends on:** Task 2

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/turn/turn_flow.rs`

### Steps

- [ ] Write the failing test.

在 `turn_flow.rs` 的 `#[cfg(test)] mod tests` 中追加：

```rust
#[tokio::test]
async fn run_one_turn_emits_start_and_end() {
    let agent = Arc::new(FakeAgent::new(FakeLlm::end_turn()));
    let flow = TurnFlow::new(agent.clone());
    let id = flow.prompt(vec![ContentPart::Text { text: "hello".into() }], USER_PROMPT_ORIGIN).unwrap();
    let result = flow.wait_for_current_turn(None).await.unwrap();
    assert_eq!(result.event.turn_id, id);
    assert_eq!(result.event.reason, TurnEndedReason::Completed);

    let events = agent.captures.lock().unwrap().events.clone();
    assert!(events.iter().any(|e| matches!(e, AgentEvent::TurnStarted { .. })));
    assert!(events.iter().any(|e| matches!(e, AgentEvent::TurnEnded(TurnEndedEvent { reason: TurnEndedReason::Completed, .. }))));

    let ctx = agent.captures.lock().unwrap().context_inputs.clone();
    assert_eq!(ctx.len(), 1);
    assert_eq!(ctx[0].1, USER_PROMPT_ORIGIN);

    assert_eq!(agent.captures.lock().unwrap().begin_turn_count, 1);
    assert_eq!(agent.captures.lock().unwrap().end_turn_count, 1);
    assert_eq!(agent.captures.lock().unwrap().full_compaction_reset, 1);
}

#[tokio::test]
async fn run_one_turn_blocks_on_user_prompt_hook() {
    let agent = Arc::new(FakeAgent::new(FakeLlm::end_turn()));
    agent.hook_results.lock().unwrap().push(HookResult {
        event: "UserPromptSubmit".into(),
        text: Some("blocked by policy".into()),
        message: Some(serde_json::json!({ "reason": "policy" })),
        blocked: true,
    });
    let flow = TurnFlow::new(agent.clone());
    let id = flow.prompt(vec![ContentPart::Text { text: "x".into() }], USER_PROMPT_ORIGIN).unwrap();
    let result = flow.wait_for_current_turn(None).await.unwrap();
    assert_eq!(result.event.turn_id, id);
    assert_eq!(result.event.reason, TurnEndedReason::Completed);
    assert!(result.blocked_by_user_prompt_hook);

    let events = agent.captures.lock().unwrap().events.clone();
    assert!(events.iter().any(|e| matches!(e, AgentEvent::HookResult { blocked: Some(true), .. })));
}
```

- [ ] Run it and verify it FAILS.

```bash
cd rust-ody && cargo test -p agent-rs run_one_turn_emits_start_and_end --no-run
```

预期失败：`run_one_turn`、`apply_user_prompt_hook`、`PromptHookEndResult`、`now_ms` 等未定义。

- [ ] Write the minimal implementation.

**Step 3a: 替换 `turn_flow.rs` 顶部 imports 为完整版本**

```rust
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use kosong_rs::message::{ContentPart, Message, Role, ToolCall};
use kosong_rs::provider::{AbortSignal, FinishReason, ModelCapability};
use kosong_rs::usage::TokenUsage;
use serde_json::Value as JsonValue;

use crate::agent_loop::errors::{create_max_steps_exceeded_error, is_abort_error, is_max_steps_exceeded_error};
use crate::agent_loop::events::{DefaultLoopEventDispatcher, LoopEvent, LoopEventDispatcher, LoopLiveOnlyEvent, LoopRecordedEvent};
use crate::agent_loop::run_turn::run_turn;
use crate::agent_loop::types::{
    AuthorizeToolExecutionResult, BeforeStepResult, ExecutableTool, ExecutableToolResult,
    FinalizeToolResultContext, LoopAfterStepContext, LoopHooks, LoopMessageBuilder,
    LoopStepHookContext, LoopStepStopReason, LoopStoppedStepContext, LoopTurnStopReason,
    PrepareToolExecutionResult, RecordStepUsageResult, ResolvedToolExecutionHookContext,
    RunnableToolExecution, RunTurnInput, ShouldContinueAfterStopResult, ToolExecutionHookContext,
    TurnResult,
};
use crate::context::types::{ContextMessage, PromptOrigin, USER_PROMPT_ORIGIN};
use crate::records::nested::{ExecutableToolOutput, GoalBudgetLimits, GoalStatus, UsageRecordScope};
use crate::records::AgentRecord;
use crate::turn::telemetry::{
    classify_api_error, current_turn_input_tokens, goal_failure_pause_reason, is_api_connection_error,
    is_api_empty_response_error, is_api_timeout_error, is_context_overflow_status_error,
    map_loop_event, summarize_turn_error, telemetry_tool_error_type, telemetry_tool_outcome,
    tool_input_record, tool_output_text, tool_result_text, to_ody_error, OdyError,
};
use crate::turn::types::{
    AgentEvent, GoalSnapshot, HookResult, LoopControl, StopHookBlock, TurnAgent, TurnEndResult,
    TurnEndedEvent, TurnEndedReason, TurnErrorSummary, TurnGoal,
};
```

**Step 3b: 扩展 `TurnFlowInner`**

```rust
#[derive(Default)]
struct TurnFlowInner {
    steer_buffer: Vec<(Vec<ContentPart>, PromptOrigin)>,
    turn_id: i64,
    active_turn: Option<ActiveTurn>,
    current_step: u32,
    tool_call_started_at: HashMap<String, (String, i64)>,
    step_failure_by_turn: HashMap<i64, String>,
}
```

**Step 3c: 新增辅助类型与方法**

在 `impl TurnFlow` 之前加入：

```rust
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
```

**Step 3d: 在 `impl TurnFlow` 中替换 `turn_worker` stub 并新增 `run_one_turn` 等方法**

保留 `launch`/`prompt`/`steer`/`cancel`/`wait`/`current_id`/`has_active_turn` 不变；把 `turn_worker` 改为：

```rust
async fn turn_worker(
    &self,
    first_turn_id: i64,
    input: Vec<ContentPart>,
    origin: PromptOrigin,
    signal: AbortSignal,
) -> TurnEndResult {
    // Task 6 将替换为真实实现。
    self.run_one_turn(first_turn_id, input, origin, signal, true).await
}
```

并追加：

```rust
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

    let telemetry_mode = if self.agent.session_mode().is_active() { "plan" } else { "agent" };
    self.agent.telemetry().track("turn_started", serde_json::json!({ "mode": telemetry_mode }));
    self.agent.full_compaction().reset_for_turn();
    self.agent.usage().begin_turn();
    self.agent.event_emitter().emit_event(AgentEvent::TurnStarted {
        turn_id,
        origin: origin.clone(),
    });
    self.agent.context().append_user_message(input.clone(), origin.clone());

    let started_at = now_ms();
    let mut error_event: Option<AgentEvent> = None;

    let (ended, stop_reason, blocked_by_user_prompt_hook) =
        match self.apply_user_prompt_hook(turn_id, &input, &origin, &signal).await {
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
                            if let Some(tokens) = current_turn_input_tokens(self.agent.usage().current_turn_usage().as_ref()) {
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
    self.agent.event_emitter().emit_event(AgentEvent::TurnEnded(ended.clone()));
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
        self.agent.event_emitter().emit_event(AgentEvent::HookResult {
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
        self.agent.event_emitter().emit_event(AgentEvent::HookResult {
            turn_id,
            hook_event: text_result.event.clone(),
            content: text_result.message.clone().unwrap_or(serde_json::Value::Null),
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
    _turn_id: i64,
    _signal: &AbortSignal,
) -> Result<LoopTurnStopReason, anyhow::Error> {
    // Task 4 将替换为真实实现；此处 stub 使单 turn 正常结束。
    Ok(LoopTurnStopReason::EndTurn)
}
```

- [ ] Run it and verify it PASSES.

```bash
cd rust-ody && cargo test -p agent-rs run_one_turn_emits_start_and_end run_one_turn_blocks_on_user_prompt_hook
```

预期：两个测试通过。

- [ ] Commit.

```bash
git add rust-ody/crates/agent-rs/src/turn/turn_flow.rs
git commit -m "feat(agent-rs): implement run_one_turn lifecycle and user prompt hook"

---

## Task 4: `run_step_loop` 与 `agent_loop::run_turn` 对接

**Depends on:** Task 2, Task 3

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/turn/turn_flow.rs`

### Steps

- [ ] Write the failing test.

在 `turn_flow.rs` 的 `#[cfg(test)] mod tests` 中追加：

```rust
#[tokio::test]
async fn run_step_loop_emits_step_events() {
    let agent = Arc::new(FakeAgent::new(FakeLlm::end_turn()));
    let flow = TurnFlow::new(agent.clone());
    let id = flow.prompt(vec![ContentPart::Text { text: "step".into() }], USER_PROMPT_ORIGIN).unwrap();
    let result = flow.wait_for_current_turn(None).await.unwrap();
    assert_eq!(result.stop_reason, Some(LoopTurnStopReason::EndTurn));
    let events = agent.captures.lock().unwrap().events.clone();
    assert!(events.iter().any(|e| matches!(e, AgentEvent::TurnStepStarted { .. })));
    assert!(events.iter().any(|e| matches!(e, AgentEvent::TurnStepCompleted { .. })));
    assert!(events.iter().any(|e| matches!(e, AgentEvent::TurnEnded(TurnEndedEvent { turn_id, .. }) if *turn_id == id)));
}
```

- [ ] Run it and verify it FAILS.

```bash
cd rust-ody && cargo test -p agent-rs run_step_loop_emits_step_events --no-run
```

预期失败：`LoopTurnStopReason` 在该 test 模块中未 import，`run_step_loop` 仍返回 stub EndTurn 但缺少事件发射，测试断言失败。

- [ ] Write the minimal implementation.

**Step 4a: 补充 imports**

在 `turn_flow.rs` 顶部 import 块追加：

```rust
use async_trait::async_trait;
use crate::agent_loop::events::LoopInterruptReason;
use crate::agent_loop::llm::{LlmChatParams, LlmChatResponse};
```

并把 `agent_loop::types` import 中的 `AfterStepResult` 加进去：

```rust
use crate::agent_loop::types::{
    AfterStepResult, AuthorizeToolExecutionResult, BeforeStepResult, ExecutableTool,
    ExecutableToolResult, FinalizeToolResultContext, LoopAfterStepContext, LoopHooks,
    LoopMessageBuilder, LoopStepHookContext, LoopStepStopReason, LoopStoppedStepContext,
    LoopTurnStopReason, PrepareToolExecutionResult, RecordStepUsageResult,
    ResolvedToolExecutionHookContext, RunnableToolExecution, RunTurnInput,
    ShouldContinueAfterStopResult, ToolExecutionHookContext, TurnResult,
};
```

**Step 4b: 替换 `run_step_loop` stub 为真实实现，并追加支持类型/方法**

把 `impl TurnFlow` 中的 `run_step_loop` stub 替换为：

```rust
async fn run_step_loop(
    &self,
    turn_id: i64,
    signal: &AbortSignal,
) -> Result<LoopTurnStopReason, anyhow::Error> {
    let stop_hook_used = Arc::new(Mutex::new(false));
    let stop_for_goal_budget = Arc::new(Mutex::new(false));
    let dedup: Arc<dyn Dedup> = Arc::new(NoopDedup);

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
            max_steps: self.agent.config().loop_control().and_then(|c| c.max_steps_per_turn),
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
                        .handle_overflow_error(signal.clone(), err)
                        .await;
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
```

在 `impl TurnFlow` 中追加：

```rust
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
                    let maybe = inner.lock().unwrap().tool_call_started_at.remove(tool_call_id);
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
                            props["error_type"] = serde_json::json!(telemetry_tool_error_type(result));
                        }
                        agent.telemetry().track("tool_call", props);
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
                agent.event_emitter().emit_event(mapped);
            }
        })),
    ))
}
```

**Step 4c: 在 `turn_flow.rs` 中追加支持类型（放在 `impl TurnFlow` 之后即可）**

```rust
#[async_trait]
trait Dedup: Send + Sync {
    fn begin_step(&self);
    fn end_step(&self);
    fn check_same_step(
        &self,
        _tool_call_id: &str,
        _tool_name: &str,
        _args: &JsonValue,
    ) -> Option<ExecutableToolResult> {
        None
    }
    async fn finalize_result(
        &self,
        _tool_call_id: &str,
        _tool_name: &str,
        _args: &JsonValue,
        result: ExecutableToolResult,
    ) -> ExecutableToolResult {
        result
    }
}

struct NoopDedup;
impl Dedup for NoopDedup {
    fn begin_step(&self) {}
    fn end_step(&self) {}
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
    fn is_retryable_error(&self, error: &dyn std::error::Error) -> bool {
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
        self.flow.agent.micro_compaction().detect();
        self.flow
            .agent
            .full_compaction()
            .before_step(ctx.signal.clone())
            .await;
        self.flow
            .agent
            .split_plan_checkpoint()
            .before_step(ctx.signal.clone())
            .await;
        self.flow
            .agent
            .normal_mode_task_checkpoint()
            .before_step(ctx.signal.clone())
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
        self.flow.agent.full_compaction().after_step().await;
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
        let mut used = self.stop_hook_used.lock().unwrap();
        if !*used {
            if let Some(block) = hooks.trigger_stop_hook(ctx.signal.clone()).await? {
                *used = true;
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
        if let Some(synthetic) = self
            .dedup
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
```

- [ ] Run it and verify it PASSES.

```bash
cd rust-ody && cargo test -p agent-rs run_step_loop_emits_step_events
```

预期：测试通过，输出中包含 `turn.step.started` 与 `turn.step.completed` 事件。

- [ ] Commit.

```bash
git add rust-ody/crates/agent-rs/src/turn/turn_flow.rs
git commit -m "feat(agent-rs): wire run_step_loop to agent_loop::run_turn"

---

## Task 5: `steer` buffer 同步刷新、`cancel`/`wait` 语义

**Depends on:** Task 4

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/turn/turn_flow.rs`（追加测试替身 `PendingLlm` 与两个测试）

### Steps

- [ ] Write the failing test.

在 `turn_flow.rs` 的 `fake` 模块中追加 `PendingLlm`：

```rust
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
```

在 `#[cfg(test)] mod tests` 中追加：

```rust
#[tokio::test]
async fn steer_buffer_flushes_in_before_step() {
    let agent = Arc::new(FakeAgent::new(FakeLlm::end_turn()));
    let flow = TurnFlow::new(agent.clone());
    flow.prompt(vec![ContentPart::Text { text: "go".into() }], USER_PROMPT_ORIGIN)
        .unwrap();
    flow.steer(
        vec![ContentPart::Text { text: "faster".into() }],
        USER_PROMPT_ORIGIN,
    );
    let _ = flow.wait_for_current_turn(None).await.unwrap();
    let ctx = agent.captures.lock().unwrap().context_inputs.clone();
    assert_eq!(ctx.len(), 2);
    assert_eq!(ctx[0].0, vec![ContentPart::Text { text: "go".into() }]);
    assert_eq!(ctx[1].0, vec![ContentPart::Text { text: "faster".into() }]);
}

#[tokio::test]
async fn cancel_turn_returns_cancelled_and_clears_active_turn() {
    let agent = Arc::new(FakeAgent::new(Arc::new(PendingLlm)));
    let flow = TurnFlow::new(agent.clone());
    let id = flow
        .prompt(vec![ContentPart::Text { text: "hang".into() }], USER_PROMPT_ORIGIN)
        .unwrap();
    tokio::task::yield_now().await;
    flow.cancel(Some(id), Some("user stop".into()));
    let result = flow.wait_for_current_turn(None).await.unwrap();
    assert_eq!(result.event.reason, TurnEndedReason::Cancelled);
    assert!(!flow.has_active_turn());
}
```

- [ ] Run it and verify it FAILS.

```bash
cd rust-ody && cargo test -p agent-rs steer_buffer_flushes_in_before_step --no-run
```

预期失败：`PendingLlm` 未定义。

- [ ] Write the minimal implementation.

实现即上述 `PendingLlm` 与测试代码本身；`flush_steer_buffer`/`cancel`/`wait_for_current_turn` 已在 Task 1/4 完成。只需把 `PendingLlm` 和测试加入文件。

- [ ] Run it and verify it PASSES.

```bash
cd rust-ody && cargo test -p agent-rs steer_buffer_flushes_in_before_step cancel_turn_returns_cancelled_and_clears_active_turn
```

预期：两个测试通过。

- [ ] Commit.

```bash
git add rust-ody/crates/agent-rs/src/turn/turn_flow.rs
git commit -m "feat(agent-rs): verify steer buffer flush and cancel semantics"
```

---

## Task 6: `turn_worker` + `drive_goal` 自动 continuation

**Depends on:** Task 3, Task 4, Task 5

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/turn/turn_flow.rs`

### Steps

- [ ] Write the failing test.

在 `#[cfg(test)] mod tests` 中追加：

```rust
#[tokio::test]
async fn goal_continuation_runs_until_budget_then_blocks() {
    let agent = Arc::new(FakeAgent::new(FakeLlm::end_turn()));
    agent.goal_runtime_enabled = true;
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
    let flow = TurnFlow::new(agent.clone());
    let id = flow.prompt(vec![ContentPart::Text { text: "goal".into() }], USER_PROMPT_ORIGIN).unwrap();
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
    assert_eq!(agent.goal_status.lock().unwrap().as_ref().unwrap().status, GoalStatus::Blocked);
}
```

- [ ] Run it and verify it FAILS.

```bash
cd rust-ody && cargo test -p agent-rs goal_continuation_runs_until_budget_then_blocks --no-run
```

预期失败：`turn_worker` 仍是 Task 1 的 stub，不会驱动多 turn。

- [ ] Write the minimal implementation.

**Step 6a: 给 `ActiveTurn` 增加 `turn_id` 字段，并更新所有构造点**

把 `ActiveTurn` 改为：

```rust
struct ActiveTurn {
    turn_id: i64,
    signal: AbortSignal,
    result_rx: Option<oneshot::Receiver<TurnEndResult>>,
}
```

把 `restore_prompt` / `restore_steer` 中的 `ActiveTurn` 构造改为：

```rust
let turn_id = inner.turn_id;
inner.active_turn = Some(ActiveTurn {
    turn_id,
    signal: AbortSignal::new(),
    result_rx: None,
});
```

把 `launch` 中的 `inner.active_turn = Some(...)` 改为：

```rust
inner.active_turn = Some(ActiveTurn {
    turn_id,
    signal,
    result_rx: Some(rx),
});
```

**Step 6b: 替换 `turn_worker` stub 并新增 `drive_goal` / `end_goal_turn_without_model`**

把 `turn_worker` 替换为：

```rust
async fn turn_worker(
    &self,
    first_turn_id: i64,
    input: Vec<ContentPart>,
    origin: PromptOrigin,
    signal: AbortSignal,
) -> TurnEndResult {
    let owns_active_turn = || {
        self.inner
            .lock()
            .unwrap()
            .active_turn
            .as_ref()
            .map(|a| a.turn_id == self.current_id())
            .unwrap_or(false)
    };

    let initial_goal_status = self
        .agent
        .goals()
        .and_then(|g| g.get_goal())
        .map(|s| s.status);

    let result = if self.agent.goal_runtime_enabled() && initial_goal_status == Some(GoalStatus::Active) {
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
```

在 `impl TurnFlow` 中追加：

```rust
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
    self.agent.event_emitter().emit_event(AgentEvent::TurnStarted {
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
    self.agent.event_emitter().emit_event(AgentEvent::TurnEnded(ended.clone()));
    ended
}
```

- [ ] Run it and verify it PASSES.

```bash
cd rust-ody && cargo test -p agent-rs goal_continuation_runs_until_budget_then_blocks
```

预期：测试通过；验证 goal mode 运行 2 个 turn 后因 budget 被 block。

- [ ] Commit.

```bash
git add rust-ody/crates/agent-rs/src/turn/turn_flow.rs
git commit -m "feat(agent-rs): implement turn_worker and drive_goal continuation"
```

---

## Task 7: 模块接线 + 全 workspace typecheck

**Depends on:** Task 6

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/turn/mod.rs`（已在 Task 1 完成，本任务确认）

### Steps

- [ ] 确认 `turn/mod.rs` 内容：

```rust
pub mod canonical_args;
pub mod error;
pub mod kosong_llm;
pub mod remote_kosong_llm;
pub mod telemetry;
pub mod tool_dedup;
pub mod turn_flow;
pub mod types;

pub use telemetry::*;
pub use turn_flow::*;
pub use types::*;
```

- [ ] Run whole-tree typecheck（包含 tests）。

```bash
cd rust-ody && cargo check -p agent-rs --workspace --tests
```

预期：`agent-rs` 及 workspace 中依赖它的 crate 全部编译通过；无 `unused` 之外的新 warning。

- [ ] Run all `agent-rs` turn tests。

```bash
cd rust-ody && cargo test -p agent-rs -- turn_
```

预期：所有本 part 的 turn 相关测试通过。

- [ ] Commit。

```bash
git add rust-ody/crates/agent-rs/src/turn/mod.rs
git commit -m "feat(agent-rs): wire turn module exports and verify workspace build"

---

## Local Self-Review

- [x] 1. Spec-coverage table：本 part 6 个 4.3.5.1 条目均映射到 Task；无 GAP。
- [x] 2. Placeholder scan：无 TODO/TBD；`run_step_loop` / `turn_worker` 的临时 stub 在后续 Task 中被完整替换，最终代码无占位。
- [x] 3. No phantom tasks：7 个 task 均产出文件变更与可验证测试；无 `--allow-empty` 或 "already done in Task N"。
- [x] 4. Dependency soundness：Task 1 → Task 2 → Task 3/4 → Task 5/6 → Task 7，所有 `Depends on:` 均指向前序 task；无 forward reference。
- [x] 5. Caller & build soundness：Task 1 扩展 `GoalSnapshot` 与 `TurnUsage` 后，FakeAgent 与所有测试同步更新；Task 7 以 `cargo check -p agent-rs --workspace --tests` 收尾。
- [x] 6. Test-the-risk：每个状态突变（turn id 分配、steer buffer、cancel 清除 active turn、goal continuation、step 事件发射）均有行为断言；测试依赖的常量（如 `GOAL_CONTINUATION_PROMPT` 文本、`GOAL_RATE_LIMIT_PAUSE_REASON`）与 TS 源码逐字符一致。
- [x] 7. Type 一致性：跨 task 复用 `TurnAgent`、`LoopTurnStopReason`、`AgentEvent`、`TurnEndedEvent`、`GoalSnapshot`、`PromptOrigin` 等类型；字段名 / camelCase / 枚举 tag 与 TS `AgentEvent` 对齐。
```
```
```
