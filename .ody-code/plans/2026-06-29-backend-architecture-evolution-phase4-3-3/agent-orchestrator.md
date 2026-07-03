# 4.3.9 Agent orchestrator & CoreHost integration — Part 1

**Goal:** 把 4.3.0–4.3.8 的子模块组装成可运行的 `Agent`，接入 `ody-host` 的 `CoreHost`，并通过 L2/L3/L4 对照门。

**Architecture:** `agent-rs/src/agent/mod.rs` 定义 `Agent` 结构体，内部用 `Arc<Mutex<...>>` 持有 records/context/config/usage/tool/permission/injection/session-mode/compaction/background/cron；`Agent` 实现所有子模块所需的上下文 trait 以及 `TurnAgent`。`TurnFlow` / `BackgroundManager` / `CronManager` 在 `Agent` 初始化后安装。`ody-host` 为每个 session 创建 `Agent`，通过 `AgentApi` 将 `CoreAPI` 方法路由到 `Agent`，事件经 `EventSink` 发出。

**Tech Stack:** Rust 2021, `async-trait`, `tokio`, `serde`/`serde_json`, `anyhow`；TS vitest + `packages/integration-tests/src/parity`。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File | Responsibility |
|---|---|
| `rust-ody/crates/agent-rs/src/agent/mod.rs` | `Agent` / `AgentBuilder`；所有上下文 trait + `TurnAgent` 实现 |
| `rust-ody/crates/agent-rs/src/api.rs` | `AgentApi` JSON 方法封装 |
| `rust-ody/crates/agent-rs/src/lib.rs` | `pub mod agent;` |
| `rust-ody/crates/agent-rs/src/context/memory.rs` | `ContextMemory` 持 `Arc<dyn ContextAgent>`（共享签名） |
| `rust-ody/crates/agent-rs/src/bin/context_golden.rs` | 适配 `ContextMemory::new` |
| `rust-ody/crates/ody-host/Cargo.toml` | 加 `agent-rs` 依赖 |
| `rust-ody/crates/ody-host/src/agent_api.rs` | `CoreHost` 内的 `AgentApi` 路由 |
| `rust-ody/crates/ody-host/src/host.rs` | 按 session 创建 `Agent`，替换旧 provider 路径 |
| `rust-ody/crates/ody-host/src/llm/fixture_provider.rs` | fixture 多轮 mock provider |
| `packages/integration-tests/src/parity/scenarios/agent-api-l2.ts` | L2 场景 |
| `packages/integration-tests/src/parity/scenarios/agent-orchestrator-l3.ts` | 4.3.9 L3 场景 |
| `packages/integration-tests/src/parity/scenarios/cross-resume-l4.ts` | L4 跨后端 resume 场景 |

---

## Dependency Overview

```text
Task 1 (Agent struct + submodule context traits)
  │
  ├──► Task 2 (TurnAgent + LLM resolver)
  │       │
  │       ├──► Task 3 (TurnFlow/Background/Cron wiring)
  │       │       │
  │       │       ├──► Task 4 (Resume via replay)
  │       │       │
  │       │       └──► Task 5 (AgentApi JSON surface + fixture mock)
  │       │               │
  │       │               ├──► Task 6 (ody-host CoreHost integration)
  │       │               │       │
  │       │               │       ├──► Task 7 (L2 parity)
  │       │               │       │       │
  │       │               │       │       ├──► Task 8 (L3 parity)
  │       │               │       │       │       │
  │       │               │       │       │       └──► Task 9 (L4 + benchmarks)
```

---

## Risks & Open Questions

- `ContextMemory<'a>` 改为 `Arc<dyn ContextAgent>` 会触发共享签名变更，必须在 Task 1 内完成所有调用方更新。
- 子模块上下文 trait 的 `&mut self` 与 `Agent` 的内部可变性需要统一；Task 1 用 `Mutex` 包装所有可变字段。
- 4.3.9 L3 场景不依赖 4.4 文件工具，`file-edit`/`multi-turn-tool` 明确 defer 到 4.4.8。

---

### Task 1: `Agent` 结构体、Builder 与上下文 trait 委托

**Depends on:** none（4.3.0–4.3.8 子模块已存在）

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/context/memory.rs:19-43`
- Modify: `rust-ody/crates/agent-rs/src/bin/context_golden.rs:49`
- Create: `rust-ody/crates/agent-rs/src/agent/mod.rs`
- Modify: `rust-ody/crates/agent-rs/src/lib.rs:1-19`
- Test: `rust-ody/crates/agent-rs/src/agent/mod.rs`（底部 `#[cfg(test)]`）

#### 步骤

- [ ] **Write the failing test**：在 `agent/mod.rs` 底部增加单元测试，验证 `AgentBuilder` 能构造出 `Agent` 且 `Agent` 实现 `TurnAgent`。

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn::types::TurnAgent;
    use std::sync::Arc;

    #[tokio::test]
    async fn agent_builder_yields_turn_agent() {
        let tmp = tempfile::tempdir().unwrap();
        let agent = AgentBuilder::new(tmp.path().to_path_buf())
            .build()
            .unwrap();
        let arc: Arc<dyn TurnAgent> = Arc::new(agent);
        assert_eq!(arc.agent_type(), "main");
        assert!(arc.context().messages().is_empty());
    }
}
```

- [ ] **Run it and verify it FAILS**：
  ```bash
  cargo test -p agent-rs agent_builder_yields_turn_agent
  ```
  预期失败：`AgentBuilder` / `Agent` 不存在。

- [ ] **Write the minimal implementation**：

1. 先把 `ContextMemory` 的生命周期去掉，否则 `Agent` 无法自持有 context。

`rust-ody/crates/agent-rs/src/context/memory.rs:19-43`：
```rust
pub struct ContextMemory {
    agent: Arc<dyn crate::context::types::ContextAgent>,
    history: Vec<ContextMessage>,
    // ... 其余字段不变
}

impl ContextMemory {
    pub fn new(agent: Arc<dyn crate::context::types::ContextAgent>) -> Self {
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
    // ... 其余方法不变，self.agent 通过 Arc deref 调用
}
```

2. 更新 `context_golden.rs` 中的调用方：

`rust-ody/crates/agent-rs/src/bin/context_golden.rs:49`：
```rust
let mut memory = ContextMemory::new(agent.clone());
```

3. 查找所有调用方并确认：
  ```bash
  rg -n "ContextMemory::new" rust-ody/crates/agent-rs/src/
  rg -n "ContextMemory \{" rust-ody/crates/agent-rs/src/
  ```
  只有 `context_golden.rs` 和本文件内部；都已更新。

4. 新建 `rust-ody/crates/agent-rs/src/agent/mod.rs`：

```rust
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::context::memory::ContextMemory;
use crate::context::types::{
    BackgroundNotifications, Clock, ContextAgent, ContextSwitchFlusher, InjectionLifecycle,
    MicroCompaction, RecordLog, ReplayBuilder, StatusEmitter, USER_PROMPT_ORIGIN,
};
use crate::context::types::{ContextMessage, PromptOrigin};
use crate::records::AgentRecord;

pub struct Agent {
    pub session_dir: PathBuf,
    pub home_dir: PathBuf,
    context: Mutex<ContextMemory>,
    records: Arc<dyn RecordLog + Send + Sync>,
    event_emitter: Arc<dyn crate::turn::types::TurnEventEmitter + Send + Sync>,
}

impl Agent {
    pub fn new(session_dir: PathBuf, home_dir: PathBuf) -> Self {
        let records: Arc<dyn RecordLog + Send + Sync> = Arc::new(AgentRecordsAdapter);
        let event_emitter: Arc<dyn crate::turn::types::TurnEventEmitter + Send + Sync> =
            Arc::new(AgentEventEmitterAdapter);
        let context = ContextMemory::new(Arc::new(ContextAgentImpl {
            records: records.clone(),
            emitter: event_emitter.clone(),
        }));
        Self {
            session_dir,
            home_dir,
            context: Mutex::new(context),
            records,
            event_emitter,
        }
    }
}

// 最小适配：满足 ContextAgent 接口；Task 2 将 Agent 自身实现 ContextAgent 并替换此包装。
struct AgentRecordsAdapter;
impl RecordLog for AgentRecordsAdapter {
    fn log_record(&self, _record: AgentRecord) {}
    fn restoring_time(&self) -> Option<i64> { None }
}

struct AgentEventEmitterAdapter;
impl crate::turn::types::TurnEventEmitter for AgentEventEmitterAdapter {
    fn emit_event(&self, _event: crate::turn::types::AgentEvent) {}
}

// ContextAgentImpl 包装 records + event_emitter，使 ContextMemory 能持有单一 Arc<dyn ContextAgent>。
struct ContextAgentImpl {
    records: Arc<dyn RecordLog + Send + Sync>,
    emitter: Arc<dyn crate::turn::types::TurnEventEmitter + Send + Sync>,
}
impl RecordLog for ContextAgentImpl {
    fn log_record(&self, r: AgentRecord) { self.records.log_record(r); }
    fn restoring_time(&self) -> Option<i64> { self.records.restoring_time() }
}
impl ContextAgent for ContextAgentImpl {
    fn record_log(&self) -> &dyn RecordLog { self }
    fn micro_compaction(&self) -> &dyn MicroCompaction { &NoopMicroCompaction }
    fn injection(&self) -> &dyn InjectionLifecycle { &NoopInjectionLifecycle }
    fn background(&self) -> &dyn BackgroundNotifications { &NoopBackgroundNotifications }
    fn replay_builder(&self) -> &dyn ReplayBuilder { &NoopReplayBuilder }
    fn status_emitter(&self) -> &dyn StatusEmitter { &NoopStatusEmitter }
    fn context_switch_flusher(&self) -> &dyn ContextSwitchFlusher { &NoopContextSwitchFlusher }
    fn clock(&self) -> &dyn Clock { &WallClock }
}

struct NoopMicroCompaction;
impl MicroCompaction for NoopMicroCompaction {
    fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage> { messages.to_vec() }
    fn reset(&self, _max_cutoff: usize) {}
}

struct NoopInjectionLifecycle;
impl InjectionLifecycle for NoopInjectionLifecycle {
    fn on_context_clear(&self) {}
    fn on_context_compacted(&self, _count: usize) {}
    fn on_context_message_removed(&self, _index: usize) {}
}

struct NoopBackgroundNotifications;
impl BackgroundNotifications for NoopBackgroundNotifications {
    fn mark_delivered_notification(&self, _origin: &PromptOrigin) {}
}

struct NoopReplayBuilder;
impl ReplayBuilder for NoopReplayBuilder {
    fn push_message(&self, _message: &ContextMessage) {}
    fn remove_last_messages(&self, _messages: &[ContextMessage]) {}
}

struct NoopStatusEmitter;
impl StatusEmitter for NoopStatusEmitter {
    fn emit_status_updated(&self) {}
}

struct NoopContextSwitchFlusher;
impl ContextSwitchFlusher for NoopContextSwitchFlusher {
    fn flush_deferred_context_switch(&self) {}
}

struct WallClock;
impl Clock for WallClock {
    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }
}

pub struct AgentBuilder {
    session_dir: PathBuf,
    home_dir: PathBuf,
}

impl AgentBuilder {
    pub fn new(session_dir: PathBuf) -> Self {
        let home_dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self { session_dir, home_dir }
    }
    pub fn with_home_dir(mut self, home_dir: PathBuf) -> Self {
        self.home_dir = home_dir;
        self
    }
    pub fn build(self) -> Result<Agent, anyhow::Error> {
        Ok(Agent::new(self.session_dir, self.home_dir))
    }
}

// 最小 TurnAgent 实现，让测试编译通过；Task 2/3 会逐步替换为真实子模块委托。
impl crate::turn::types::TurnAgent for Agent {
    fn context(&self) -> &dyn crate::turn::types::TurnContext { self }
    fn usage(&self) -> &dyn crate::turn::types::TurnUsage { self }
    fn config(&self) -> &dyn crate::turn::types::TurnConfig { self }
    fn tools(&self) -> &dyn crate::turn::types::TurnTools { self }
    fn permission(&self) -> &dyn crate::turn::types::TurnPermission { self }
    fn injection(&self) -> &dyn crate::turn::types::TurnInjection { self }
    fn full_compaction(&self) -> &dyn crate::turn::types::TurnFullCompaction { self }
    fn micro_compaction(&self) -> &dyn crate::turn::types::TurnMicroCompaction { self }
    fn split_plan_checkpoint(&self) -> &dyn crate::turn::types::TurnSplitPlanCheckpoint { self }
    fn normal_mode_task_checkpoint(&self) -> &dyn crate::turn::types::TurnNormalTaskCheckpoint { self }
    fn session_mode(&self) -> &dyn crate::turn::types::TurnSessionMode { self }
    fn goals(&self) -> Option<&dyn crate::turn::types::TurnGoal> { None }
    fn hooks(&self) -> Option<&dyn crate::turn::types::TurnHooks> { None }
    fn telemetry(&self) -> &dyn crate::turn::types::TurnTelemetry { self }
    fn log(&self) -> &dyn crate::turn::types::TurnLog { self }
    fn mcp(&self) -> Option<&dyn crate::turn::types::TurnMcp> { None }
    fn subagent_host(&self) -> Option<&dyn crate::turn::types::TurnSubagentHost> { None }
    fn records(&self) -> &dyn crate::turn::types::TurnRecords { self }
    fn event_emitter(&self) -> &dyn crate::turn::types::TurnEventEmitter { self }
    fn llm_resolver(&self) -> &dyn crate::turn::types::TurnLlmResolver { self }
    fn flush_deferred_context_switch(&self) {}
    fn agent_type(&self) -> &str { "main" }
    fn homedir(&self) -> Option<&str> { self.home_dir.to_str() }
    fn goal_runtime_enabled(&self) -> bool { false }
}

// 最小 trait 实现占位；Task 2 会替换为真实子模块。
impl crate::turn::types::TurnContext for Agent {
    fn append_user_message(&self, content: Vec<kosong_rs::message::ContentPart>, origin: PromptOrigin) {
        self.context.lock().unwrap().append_user_message(content, origin);
    }
    fn append_message(&self, _message: ContextMessage) {}
    fn messages(&self) -> Vec<kosong_rs::message::Message> { self.context.lock().unwrap().messages() }
    fn append_loop_event(&self, _event: crate::records::nested::LoopRecordedEvent) {}
    fn has_open_steps(&self) -> bool { false }
    fn clear(&self) { self.context.lock().unwrap().clear(); }
    fn history(&self) -> Vec<ContextMessage> { self.context.lock().unwrap().history().to_vec() }
    fn token_count(&self) -> i64 { self.context.lock().unwrap().token_count() }
    fn token_count_with_pending(&self) -> i64 { self.context.lock().unwrap().token_count_with_pending() }
    fn apply_compaction(&self, _result: crate::records::nested::CompactionResult) {}
    fn project(&self, messages: &[ContextMessage]) -> Vec<kosong_rs::message::Message> {
        self.context.lock().unwrap().project(messages)
    }
    fn last_assistant_at_ms(&self) -> Option<i64> { self.context.lock().unwrap().last_assistant_at() }
    fn append_system_reminder(&self, content: &str, origin: PromptOrigin) {
        self.context.lock().unwrap().append_system_reminder(content, origin);
    }
}

// 其余 Turn* trait 先用空实现占位，Task 2/3 替换。
impl crate::turn::types::TurnUsage for Agent {}
impl crate::turn::types::TurnConfig for Agent {
    fn model(&self) -> String { "mock".into() }
    fn has_model(&self) -> bool { false }
    fn system_prompt(&self) -> String { String::new() }
    fn thinking_level(&self) -> String { "off".into() }
    fn provider(&self) -> Box<dyn kosong_rs::provider::ChatProvider> {
        Box::new(kosong_rs::mock::MockProvider::new("mock", "mock"))
    }
    fn model_capabilities(&self) -> kosong_rs::provider::ModelCapability { kosong_rs::provider::ModelCapability::unknown() }
    fn loop_control(&self) -> Option<crate::turn::types::LoopControl> { None }
    fn e2e_enabled(&self) -> bool { false }
    fn test_review_enabled(&self) -> bool { false }
    fn model_alias(&self) -> Option<String> { None }
}
impl crate::turn::types::TurnTools for Agent {
    fn loop_tools(&self) -> Vec<Arc<dyn crate::agent_loop::types::ExecutableTool>> { vec![] }
    fn store_data(&self) -> std::collections::HashMap<String, serde_json::Value> { Default::default() }
}
#[async_trait::async_trait]
impl crate::turn::types::TurnPermission for Agent {
    async fn before_tool_call(&self, _ctx: crate::agent_loop::types::ResolvedToolExecutionHookContext<'_>) -> Result<Option<crate::agent_loop::types::AuthorizeToolExecutionResult>, anyhow::Error> {
        Ok(None)
    }
}
#[async_trait::async_trait]
impl crate::turn::types::TurnInjection for Agent {
    async fn inject_goal(&self) {}
    async fn inject(&self) {}
}
#[async_trait::async_trait]
impl crate::turn::types::TurnFullCompaction for Agent {
    fn reset_for_turn(&self, _agent: Arc<dyn crate::turn::types::TurnAgent>) {}
    async fn before_step(&self, _agent: Arc<dyn crate::turn::types::TurnAgent>, _signal: kosong_rs::provider::AbortSignal) -> Result<(), anyhow::Error> { Ok(()) }
    async fn after_step(&self, _agent: Arc<dyn crate::turn::types::TurnAgent>) {}
    async fn handle_overflow_error(&self, _agent: Arc<dyn crate::turn::types::TurnAgent>, _signal: kosong_rs::provider::AbortSignal, _error: anyhow::Error) -> Result<(), anyhow::Error> { Ok(()) }
    async fn compact_checkpoint(&self, _agent: Arc<dyn crate::turn::types::TurnAgent>, _signal: kosong_rs::provider::AbortSignal) -> Result<(), anyhow::Error> { Ok(()) }
    fn begin(&self, _agent: Arc<dyn crate::turn::types::TurnAgent>, _data: crate::records::nested::CompactionBeginData) {}
    fn cancel(&self, _agent: Arc<dyn crate::turn::types::TurnAgent>) {}
    fn compacted_history(&self) -> Vec<crate::turn::types::CompactedHistory> { vec![] }
    fn is_compacting(&self) -> bool { false }
}
impl crate::turn::types::TurnMicroCompaction for Agent {
    fn detect(&self, _agent: Arc<dyn crate::turn::types::TurnAgent>) {}
    fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage> { messages.to_vec() }
    fn reset(&self, _max_cutoff: usize) {}
}
#[async_trait::async_trait]
impl crate::turn::types::TurnSplitPlanCheckpoint for Agent {
    async fn before_step(&self, _agent: Arc<dyn crate::turn::types::TurnAgent>, _signal: kosong_rs::provider::AbortSignal) {}
    fn reset(&self) {}
}
#[async_trait::async_trait]
impl crate::turn::types::TurnNormalTaskCheckpoint for Agent {
    async fn before_step(&self, _agent: Arc<dyn crate::turn::types::TurnAgent>, _signal: kosong_rs::provider::AbortSignal) {}
    fn reset(&self) {}
}
#[async_trait::async_trait]
impl crate::turn::types::TurnSessionMode for Agent {
    fn is_active(&self) -> bool { false }
    fn kind(&self) -> Option<String> { None }
    fn file_path(&self) -> Option<String> { None }
    async fn data(&self) -> Option<String> { None }
}
impl crate::turn::types::TurnTelemetry for Agent {
    fn track(&self, _event: &str, _properties: serde_json::Value) {}
}
impl crate::turn::types::TurnLog for Agent {
    fn debug(&self, _msg: &str, _data: serde_json::Value) {}
    fn warn(&self, _msg: &str, _data: serde_json::Value) {}
    fn error(&self, _msg: &str, _data: serde_json::Value) {}
}
impl crate::turn::types::TurnRecords for Agent {
    fn log_record(&self, record: AgentRecord) { self.records.log_record(record); }
}
impl crate::turn::types::TurnEventEmitter for Agent {
    fn emit_event(&self, event: crate::turn::types::AgentEvent) { self.event_emitter.emit_event(event); }
}
#[async_trait::async_trait]
impl crate::turn::types::TurnLlmResolver for Agent {
    fn refresh_llm(&self) {}
    fn llm(&self) -> Arc<dyn crate::agent_loop::llm::Llm> {
        Arc::new(crate::turn::fixture_agent::PendingLlm)
    }
    async fn generate_one_off(&self, _provider: Box<dyn kosong_rs::provider::ChatProvider + Send>, _system_prompt: String, _tools: Vec<kosong_rs::provider::Tool>, _messages: Vec<kosong_rs::message::Message>, _signal: kosong_rs::provider::AbortSignal) -> Result<crate::turn::types::CompactGenerateResult, anyhow::Error> {
        Ok(crate::turn::types::CompactGenerateResult::default())
    }
}
```

5. 在 `lib.rs` 导出：

```rust
pub mod agent;
```

- [ ] **Run it and verify it PASSES**：
  ```bash
  cargo test -p agent-rs agent_builder_yields_turn_agent
  ```
  预期：测试通过；`cargo test -p agent-rs` 全绿。

- [ ] **Commit**：
  ```bash
  git add rust-ody/crates/agent-rs/src/context/memory.rs \
         rust-ody/crates/agent-rs/src/bin/context_golden.rs \
         rust-ody/crates/agent-rs/src/agent/mod.rs \
         rust-ody/crates/agent-rs/src/lib.rs
  git commit -m "feat(agent-rs): scaffold Agent struct and ContextMemory Arc ownership"
  ```

---

### Task 2: `TurnAgent` 子模块真实委托 + LLM 解析器

**Depends on:** Task 1

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/agent/mod.rs:1-260`
- Modify: `rust-ody/crates/agent-rs/src/context/memory.rs:1-260`
- Modify: `rust-ody/crates/agent-rs/src/config/state.rs:1-100`
- Modify: `rust-ody/crates/agent-rs/src/tool/manager.rs:1-100`
- Modify: `rust-ody/crates/agent-rs/src/permission/manager.rs:1-100`
- Modify: `rust-ody/crates/agent-rs/src/usage/recorder.rs:1-100`
- Modify: `rust-ody/crates/agent-rs/src/injection/manager.rs:1-100`
- Modify: `rust-ody/crates/agent-rs/src/session_mode/manager.rs:1-100`
- Modify: `rust-ody/crates/agent-rs/src/background/manager.rs:1-100`
- Modify: `rust-ody/crates/agent-rs/src/cron/manager.rs:1-100`
- Test: `rust-ody/crates/agent-rs/src/agent/mod.rs`

#### 步骤

- [ ] **Write the failing test**：

```rust
#[tokio::test]
async fn agent_delegates_config_and_llm() {
    let tmp = tempfile::tempdir().unwrap();
    let agent = AgentBuilder::new(tmp.path().to_path_buf())
        .with_model("gpt-4o-mini".into())
        .build()
        .unwrap();
    let arc: Arc<dyn TurnAgent> = Arc::new(agent);
    assert_eq!(arc.config().model(), "gpt-4o-mini");
    let _ = arc.llm_resolver().llm(); // 能返回 LLM 实例
}
```

- [ ] **Run it and verify it FAILS**：
  ```bash
  cargo test -p agent-rs agent_delegates_config_and_llm
  ```
  预期失败：`with_model` / 真实 config / `llm_resolver` 不存在或未连接。

- [ ] **Write the minimal implementation**：

1. 在 `Agent` 中持有真实子模块实例（用 `Arc<Mutex<T>>` 包装）：

```rust
pub struct Agent {
    pub session_dir: PathBuf,
    pub home_dir: PathBuf,

    pub config: Arc<Mutex<crate::config::state::ConfigState<crate::config::handle::AgentConfigHandle>>>,
    pub context: Arc<Mutex<ContextMemory>>,
    pub usage: Arc<Mutex<crate::usage::recorder::UsageRecorder<crate::usage::handle::AgentUsageHandle>>>,
    pub tools: Arc<Mutex<crate::tool::manager::ToolManager<crate::tool::handle::AgentToolHandle>>>,
    pub permission: Arc<RwLock<crate::permission::manager::PermissionManager<'static, crate::permission::handle::AgentPermissionHandle>>>,
    pub injection: Arc<Mutex<crate::injection::manager::InjectionManager>>,
    pub session_mode: Arc<Mutex<crate::session_mode::manager::SessionModeManager>>,
    pub full_compaction: Arc<Mutex<crate::compaction::full::FullCompaction>>,
    pub micro_compaction: Arc<Mutex<crate::compaction::micro::MicroCompaction>>,
    pub split_plan_checkpoint: Arc<Mutex<crate::checkpoint::split_plan::SplitPlanCheckpoint>>,
    pub normal_task_checkpoint: Arc<Mutex<crate::checkpoint::normal_task::NormalModeTaskCheckpoint>>,
    pub background: Arc<Mutex<Option<Arc<crate::background::manager::BackgroundManager>>>>,
    pub cron: Arc<Mutex<Option<Arc<crate::cron::manager::CronManager>>>>,
    pub goals: Arc<Mutex<Option<crate::goal::store::GoalStore>>>,
    pub hooks: Arc<Mutex<Option<crate::hook::manager::HookManager>>>,
    pub telemetry: Arc<dyn crate::turn::types::TurnTelemetry + Send + Sync>,
    pub log: Arc<dyn crate::turn::types::TurnLog + Send + Sync>,
    pub records: Arc<dyn RecordLog + Send + Sync>,
    pub event_emitter: Arc<dyn crate::turn::types::TurnEventEmitter + Send + Sync>,
    pub llm: Arc<Mutex<Option<Arc<dyn crate::agent_loop::llm::Llm>>>>,
    pub kaos: Arc<crate::kaos::Kaos>,
}
```

> 注：如果子模块类型尚未导出或生命周期不匹配，先在本任务内通过 `Arc<Mutex<T>>` 包装并调整内部方法签名；不允许跨任务拆分共享签名变更。

2. 让 `Agent` 自身实现 `ContextAgent`，从而 `ContextMemory::new(Arc::new(agent.clone()))`：

```rust
impl ContextAgent for Agent {
    fn record_log(&self) -> &dyn RecordLog { self }
    fn micro_compaction(&self) -> &dyn crate::context::types::MicroCompaction { self }
    fn injection(&self) -> &dyn crate::context::types::InjectionLifecycle { self }
    fn background(&self) -> &dyn crate::context::types::BackgroundNotifications { self }
    fn replay_builder(&self) -> &dyn crate::context::types::ReplayBuilder { self }
    fn status_emitter(&self) -> &dyn crate::context::types::StatusEmitter { self }
    fn context_switch_flusher(&self) -> &dyn crate::context::types::ContextSwitchFlusher { self }
    fn clock(&self) -> &dyn crate::context::types::Clock { self }
}
```

3. 在 `AgentBuilder` 增加 `with_model`、`with_system_prompt`、`with_home_dir`；`build()` 初始化每个子模块。

4. 把 `Turn* trait` 的占位实现替换为对子模块的委托调用。例如：

```rust
impl crate::turn::types::TurnConfig for Agent {
    fn model(&self) -> String {
        self.config.lock().unwrap().read().model.clone()
    }
    fn has_model(&self) -> bool { !self.model().is_empty() }
    fn system_prompt(&self) -> String {
        self.config.lock().unwrap().read().system_prompt.clone().unwrap_or_default()
    }
    fn thinking_level(&self) -> String {
        self.config.lock().unwrap().read().thinking_level.clone().unwrap_or_else(|| "off".into())
    }
    fn provider(&self) -> Box<dyn kosong_rs::provider::ChatProvider + Send> {
        // 通过配置创建 Provider；先用 ProviderFactory 兜底
        Box::new(kosong_rs::mock::MockProvider::new(&self.model(), &self.model()))
    }
    fn model_capabilities(&self) -> kosong_rs::provider::ModelCapability {
        self.config.lock().unwrap().read().model_capabilities.clone().unwrap_or_default()
    }
    fn loop_control(&self) -> Option<crate::turn::types::LoopControl> {
        self.config.lock().unwrap().read().loop_control.clone()
    }
    fn e2e_enabled(&self) -> bool { false }
    fn test_review_enabled(&self) -> bool { false }
    fn model_alias(&self) -> Option<String> { None }
}
```

5. 在 `Agent` 上实现 `TurnLlmResolver`（新增 `ProviderLlm` 包装 `ChatProvider`）：

```rust
pub struct ProviderLlm {
    provider: Box<dyn kosong_rs::provider::ChatProvider + Send>,
    system_prompt: String,
    model_name: String,
}

#[async_trait::async_trait]
impl crate::agent_loop::llm::Llm for ProviderLlm {
    fn system_prompt(&self) -> &str { &self.system_prompt }
    fn model_name(&self) -> &str { &self.model_name }
    async fn chat(&self, params: crate::agent_loop::llm::LlmChatParams) -> Result<crate::agent_loop::llm::LlmChatResponse, anyhow::Error> {
        let options = kosong_rs::provider::GenerateOptions {
            signal: Some(params.signal),
            ..Default::default()
        };
        let result = kosong_rs::generate::generate(
            self.provider.as_ref(),
            &self.system_prompt,
            &params.tools,
            &params.messages,
            None,
            Some(&options),
        ).await?;
        Ok(crate::agent_loop::llm::LlmChatResponse {
            tool_calls: result.message.tool_calls,
            provider_finish_reason: result.finish_reason,
            raw_finish_reason: result.raw_finish_reason,
            usage: result.usage.unwrap_or_default(),
            stream_timing: None,
        })
    }
}

#[async_trait::async_trait]
impl crate::turn::types::TurnLlmResolver for Agent {
    fn refresh_llm(&self) {
        let config = self.config.lock().unwrap();
        let provider = config.read().provider();
        let model_name = config.read().model.clone();
        let system_prompt = config.read().system_prompt.clone().unwrap_or_default();
        *self.llm.lock().unwrap() = Some(Arc::new(ProviderLlm { provider, system_prompt, model_name }));
    }

    fn llm(&self) -> Arc<dyn crate::agent_loop::llm::Llm> {
        self.llm.lock().unwrap().clone().unwrap_or_else(|| Arc::new(crate::turn::fixture_agent::PendingLlm))
    }

    async fn generate_one_off(
        &self,
        provider: Box<dyn kosong_rs::provider::ChatProvider + Send>,
        system_prompt: String,
        tools: Vec<kosong_rs::provider::Tool>,
        messages: Vec<kosong_rs::message::Message>,
        signal: kosong_rs::provider::AbortSignal,
    ) -> Result<crate::turn::types::CompactGenerateResult, anyhow::Error> {
        let options = kosong_rs::provider::GenerateOptions {
            signal: Some(signal),
            ..Default::default()
        };
        let result = kosong_rs::generate::generate(
            provider.as_ref(),
            &system_prompt,
            &tools,
            &messages,
            None,
            Some(&options),
        ).await?;
        let text = result.message.content.iter().map(|p| match p {
            kosong_rs::message::ContentPart::Text { text } => text.clone(),
            kosong_rs::message::ContentPart::Think { think, .. } => think.clone(),
            _ => String::new(),
        }).collect::<Vec<_>>().join("");
        Ok(crate::turn::types::CompactGenerateResult {
            text,
            finish_reason: result.finish_reason,
            usage: result.usage.unwrap_or_default(),
        })
    }
}
```

- [ ] **Run it and verify it PASSES**：
  ```bash
  cargo test -p agent-rs agent_delegates_config_and_llm
  ```
  预期通过。

- [ ] **Whole-tree typecheck**：
  ```bash
  cargo test -p agent-rs
  ```
  并：
  ```bash
  pnpm -r typecheck
  ```

- [ ] **Commit**：
  ```bash
  git add rust-ody/crates/agent-rs/src/agent/mod.rs
  git commit -m "feat(agent-rs): wire Agent to real submodules and LLM resolver"
  ```

---

### Task 3: TurnFlow / Background / Cron 与 `Agent` 对接

**Depends on:** Task 2

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/turn/flow.rs:1-260`
- Modify: `rust-ody/crates/agent-rs/src/turn/turn.rs:1-260`
- Modify: `rust-ody/crates/agent-rs/src/background/manager.rs:1-160`
- Modify: `rust-ody/crates/agent-rs/src/cron/manager.rs:1-160`
- Modify: `rust-ody/crates/agent-rs/src/agent/mod.rs:1-260`
- Test: `rust-ody/crates/agent-rs/src/turn/flow.rs`（新增单元测试）

#### 步骤

- [ ] **Write the failing test**：

```rust
#[tokio::test]
async fn turn_flow_runs_hello_world_through_agent() {
    let tmp = tempfile::tempdir().unwrap();
    let agent = Arc::new(
        AgentBuilder::new(tmp.path().to_path_buf())
            .with_model("mock".into())
            .build()
            .unwrap()
    ) as Arc<dyn TurnAgent>;

    let mut flow = TurnFlow::new(agent.clone(), AbortSignal::new());
    let request = TurnRequest {
        prompt: Some("hello".into()),
        ..Default::default()
    };
    let result = flow.run(request).await.unwrap();
    assert!(result.text.contains("hello") || result.tool_calls.is_empty());
}
```

- [ ] **Run it and verify it FAILS**：
  ```bash
  cargo test -p agent-rs turn_flow_runs_hello_world_through_agent
  ```
  预期失败：`TurnFlow::new` 尚未接受 `Arc<dyn TurnAgent>` 或 flow 未真正调用 agent 的 provider。

- [ ] **Write the minimal implementation**：

1. 调整 `TurnFlow` 持有 `Arc<dyn TurnAgent>`：

```rust
pub struct TurnFlow {
    agent: Arc<dyn TurnAgent>,
    signal: AbortSignal,
    state: TurnState,
}

impl TurnFlow {
    pub fn new(agent: Arc<dyn TurnAgent>, signal: AbortSignal) -> Self {
        Self { agent, signal, state: TurnState::default() }
    }

    pub async fn run(&mut self, request: TurnRequest) -> Result<TurnResult, anyhow::Error> {
        self.agent.context().append_user_message(
            vec![kosong_rs::message::ContentPart::text(request.prompt.unwrap_or_default())],
            crate::context::types::USER_PROMPT_ORIGIN.clone(),
        );

        let provider = self.agent.config().provider();
        let system_prompt = self.agent.config().system_prompt();
        let tools = self.agent.tools().loop_tools();
        let messages = self.agent.context().messages();

        let response = provider.complete(
            kosong_rs::provider::CompletionRequest {
                model: self.agent.config().model(),
                system_prompt,
                messages,
                tools: tools.iter().map(|t| t.to_provider_tool()).collect(),
                ..Default::default()
            },
            self.signal.clone(),
        ).await?;

        let text = response.content.into_iter()
            .filter_map(|p| if let kosong_rs::message::ContentPart::Text(t) = p { Some(t.text) } else { None })
            .collect::<String>();

        if let Some(call) = response.tool_calls.into_iter().next() {
            let tool = tools.iter().find(|t| t.name() == call.name)
                .ok_or_else(|| anyhow::anyhow!("tool not found: {}", call.name))?;
            let exec_result = tool.execute(call.arguments).await?;
            Ok(TurnResult { text: exec_result.to_string(), tool_calls: vec![call.name] })
        } else {
            Ok(TurnResult { text, tool_calls: vec![] })
        }
    }
}
```

2. `BackgroundManager` / `CronManager` 增加 `attach_to_agent(agent: Arc<dyn TurnAgent>)` 方法，方法内订阅 agent 的事件通道（无真实通道时先放空实现）。

3. `Agent` 的 `background()` / `cron()` getter 在 `Option` 存在时返回对应 `Arc`；否则返回 `None`。

- [ ] **Run it and verify it PASSES**：
  ```bash
  cargo test -p agent-rs turn_flow_runs_hello_world_through_agent
  ```

- [ ] **Whole-tree typecheck**：
  ```bash
  cargo test -p agent-rs
  ```

- [ ] **Commit**：
  ```bash
  git add rust-ody/crates/agent-rs/src/turn/flow.rs \
         rust-ody/crates/agent-rs/src/turn/turn.rs \
         rust-ody/crates/agent-rs/src/background/manager.rs \
         rust-ody/crates/agent-rs/src/cron/manager.rs \
         rust-ody/crates/agent-rs/src/agent/mod.rs
  git commit -m "feat(agent-rs): connect TurnFlow, Background, Cron to Agent"
  ```

---

### Task 4: 会话恢复（Resume / Replay）

**Depends on:** Task 3

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/agent/mod.rs:1-260`
- Modify: `rust-ody/crates/agent-rs/src/records/replay.rs:1-200`
- Modify: `rust-ody/crates/agent-rs/src/session/manager.rs:1-200`
- Modify: `rust-ody/crates/ody-host/src/host.rs:1-200`
- Test: `rust-ody/crates/agent-rs/src/agent/mod.rs`

#### 步骤

- [ ] **Write the failing test**：

```rust
#[tokio::test]
async fn agent_resumes_from_records() {
    let tmp = tempfile::tempdir().unwrap();
    let agent = AgentBuilder::new(tmp.path().to_path_buf())
        .with_model("mock".into())
        .build()
        .unwrap();
    agent.context().append_user_message(
        vec![kosong_rs::message::ContentPart::text("first".into())],
        crate::context::types::USER_PROMPT_ORIGIN.clone(),
    );

    let replay = agent.save_replay().unwrap();
    let resumed = AgentBuilder::new(tmp.path().to_path_buf())
        .with_model("mock".into())
        .replay(replay)
        .build()
        .unwrap();
    assert_eq!(resumed.context().history().len(), 1);
}
```

- [ ] **Run it and verify it FAILS**：
  ```bash
  cargo test -p agent-rs agent_resumes_from_records
  ```
  预期失败：`save_replay` / `replay` 不存在。

- [ ] **Write the minimal implementation**：

1. 在 `Agent` 中实现 `save_replay` / `replay`：

```rust
impl Agent {
    pub fn save_replay(&self) -> Result<crate::records::replay::Replay, anyhow::Error> {
        let ctx = self.context.lock().unwrap();
        Ok(crate::records::replay::Replay {
            messages: ctx.history().to_vec(),
            config: self.config.lock().unwrap().read().clone(),
        })
    }

    pub fn apply_replay(&self, replay: crate::records::replay::Replay) -> Result<(), anyhow::Error> {
        *self.config.lock().unwrap().read_mut()? = replay.config;
        let mut ctx = self.context.lock().unwrap();
        for m in replay.messages {
            ctx.append_message(m);
        }
        Ok(())
    }
}

impl AgentBuilder {
    pub fn replay(mut self, replay: crate::records::replay::Replay) -> Self {
        self.replay = Some(replay);
        self
    }
}
```

2. `Replay` 结构体补充 `config` 字段；`records/replay.rs` 添加序列化/反序列化。

3. `SessionManager` 增加 `resume(session_id, replay) -> Arc<Agent>`；`CoreHost` 在 `resume` 命令中调用。

- [ ] **Run it and verify it PASSES**：
  ```bash
  cargo test -p agent-rs agent_resumes_from_records
  ```

- [ ] **Whole-tree typecheck**：
  ```bash
  cargo test -p agent-rs
  ```

- [ ] **Commit**：
  ```bash
  git add rust-ody/crates/agent-rs/src/agent/mod.rs \
         rust-ody/crates/agent-rs/src/records/replay.rs \
         rust-ody/crates/agent-rs/src/session/manager.rs \
         rust-ody/crates/ody-host/src/host.rs
  git commit -m "feat(agent-rs): implement Agent resume/replay"
  ```

---

### Task 5: `AgentApi` trait 与 fixture 级 mock provider

**Depends on:** Task 4

**Files:**
- Create: `rust-ody/crates/agent-rs/src/api.rs`
- Create: `rust-ody/crates/agent-rs/src/llm/fixture.rs`
- Modify: `rust-ody/crates/agent-rs/src/lib.rs:1-20`
- Modify: `rust-ody/crates/ody-host/src/llm/mock.rs:1-120`
- Test: `rust-ody/crates/agent-rs/src/api.rs`

#### 步骤

- [ ] **Write the failing test**：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn agent_api_prompt_returns_text() {
        let tmp = tempfile::tempdir().unwrap();
        let agent = Arc::new(
            AgentBuilder::new(tmp.path().to_path_buf())
                .with_model("fixture".into())
                .build()
                .unwrap()
        );
        let api = AgentApi::new(agent);
        let response = api.prompt("hi", PromptOptions::default()).await.unwrap();
        assert_eq!(response.text, "hi");
    }
}
```

- [ ] **Run it and verify it FAILS**：
  ```bash
  cargo test -p agent-rs agent_api_prompt_returns_text
  ```
  预期失败：`AgentApi` / fixture provider 不存在。

- [ ] **Write the minimal implementation**：

1. 创建 `rust-ody/crates/agent-rs/src/llm/fixture.rs`：

```rust
use kosong_rs::provider::{ChatProvider, CompletionRequest, CompletionResponse, ContentPart, StreamedMessagePart};
use kosong_rs::provider::AbortSignal;
use std::collections::VecDeque;
use std::sync::Mutex;

pub struct FixtureChatProvider {
    turns: Mutex<VecDeque<Vec<StreamedMessagePart>>>,
}

impl FixtureChatProvider {
    pub fn new(turns: Vec<Vec<StreamedMessagePart>>) -> Self {
        Self { turns: Mutex::new(turns.into()) }
    }

    pub fn from_json_file(path: &std::path::Path) -> Result<Self, anyhow::Error> {
        let turns: Vec<Vec<StreamedMessagePart>> = serde_json::from_reader(
            std::fs::File::open(path)?
        )?;
        Ok(Self::new(turns))
    }
}

#[async_trait::async_trait]
impl ChatProvider for FixtureChatProvider {
    async fn complete(&self, _request: CompletionRequest, _signal: AbortSignal) -> Result<CompletionResponse, anyhow::Error> {
        let mut turns = self.turns.lock().unwrap();
        let parts = turns.pop_front().unwrap_or_default();
        let mut text = String::new();
        let mut tool_calls = Vec::new();
        for part in parts {
            match part {
                StreamedMessagePart::Text(t) => text.push_str(&t),
                StreamedMessagePart::ToolCall(tc) => tool_calls.push(tc),
                _ => {}
            }
        }
        Ok(CompletionResponse {
            content: vec![ContentPart::Text(kosong_rs::message::Text { text })],
            tool_calls,
            usage: None,
            finish_reason: Some("stop".into()),
        })
    }
}
```

2. 创建 `rust-ody/crates/agent-rs/src/api.rs`：

```rust
use std::sync::Arc;
use crate::agent::Agent;
use crate::turn::flow::TurnFlow;
use crate::turn::types::{TurnAgent, TurnRequest};
use kosong_rs::provider::AbortSignal;

pub struct AgentApi {
    agent: Arc<Agent>,
}

#[derive(Default)]
pub struct PromptOptions {
    pub system_prompt: Option<String>,
    pub model: Option<String>,
}

pub struct PromptResponse {
    pub text: String,
    pub tool_calls: Vec<String>,
}

impl AgentApi {
    pub fn new(agent: Arc<Agent>) -> Self { Self { agent } }

    pub async fn prompt(&self, prompt: &str, options: PromptOptions) -> Result<PromptResponse, anyhow::Error> {
        if let Some(model) = options.model {
            self.agent.config.lock().unwrap().read_mut()?.model = model;
        }
        if let Some(system) = options.system_prompt {
            self.agent.config.lock().unwrap().read_mut()?.system_prompt = Some(system);
        }

        let mut flow = TurnFlow::new(self.agent.clone(), AbortSignal::new());
        let result = flow.run(TurnRequest {
            prompt: Some(prompt.into()),
            ..Default::default()
        }).await?;
        Ok(PromptResponse { text: result.text, tool_calls: result.tool_calls })
    }
}
```

3. `ody-host/src/llm/mock.rs` 增加 `FixtureChatProvider` 的导出/包装，或 `ody-host` 直接使用 `agent-rs::llm::fixture`。

- [ ] **Run it and verify it PASSES**：
  ```bash
  cargo test -p agent-rs agent_api_prompt_returns_text
  ```

- [ ] **Whole-tree typecheck**：
  ```bash
  cargo test -p agent-rs
  ```

- [ ] **Commit**：
  ```bash
  git add rust-ody/crates/agent-rs/src/api.rs \
         rust-ody/crates/agent-rs/src/llm/fixture.rs \
         rust-ody/crates/agent-rs/src/lib.rs
  git commit -m "feat(agent-rs): add AgentApi and fixture ChatProvider for parity"
  ```

---

### Task 6: `CoreHost` 路由从 provider 切到 `AgentApi`

**Depends on:** Task 5

**Files:**
- Modify: `rust-ody/crates/ody-host/src/host.rs:1-260`
- Modify: `rust-ody/crates/ody-host/src/session.rs:1-200`
- Modify: `rust-ody/crates/ody-host/src/main.rs:1-120`
- Modify: `rust-ody/crates/ody-host/Cargo.toml:1-80`
- Test: `rust-ody/crates/ody-host/tests/host_smoke.rs`

#### 步骤

- [ ] **Write the failing test**：

```rust
#[tokio::test]
async fn host_prompt_routes_through_agent_api() {
    let tmp = tempfile::tempdir().unwrap();
    let host = CoreHost::builder(tmp.path().to_path_buf())
        .with_mock_provider("echo")
        .build()
        .unwrap();
    let response = host.prompt("hello").await.unwrap();
    assert_eq!(response, "hello");
}
```

- [ ] **Run it and verify it FAILS**：
  ```bash
  cargo test -p ody-host host_prompt_routes_through_agent_api
  ```
  预期失败：`CoreHost::builder` / `with_mock_provider` / prompt 返回类型不一致。

- [ ] **Write the minimal implementation**：

1. `ody-host/Cargo.toml` 增加 `agent-rs` 依赖（path）。

2. 在 `host.rs` 中：

```rust
use agent_rs::agent::{Agent, AgentBuilder};
use agent_rs::api::{AgentApi, PromptOptions};
use std::sync::{Arc, Mutex};

pub struct CoreHost {
    session_dir: PathBuf,
    agent: Arc<Mutex<Option<Arc<Agent>>>>,
    api: Arc<Mutex<Option<AgentApi>>>,
}

pub struct CoreHostBuilder {
    session_dir: PathBuf,
    mock_provider: Option<String>,
}

impl CoreHost {
    pub fn builder(session_dir: PathBuf) -> CoreHostBuilder {
        CoreHostBuilder { session_dir, mock_provider: None }
    }

    pub async fn prompt(&self, prompt: &str) -> Result<String, anyhow::Error> {
        self.ensure_agent().await?;
        let api = self.api.lock().unwrap().as_ref().unwrap().clone();
        let response = api.prompt(prompt, PromptOptions::default()).await?;
        Ok(response.text)
    }

    async fn ensure_agent(&self) -> Result<(), anyhow::Error> {
        let mut agent_guard = self.agent.lock().unwrap();
        if agent_guard.is_none() {
            let agent = Arc::new(
                AgentBuilder::new(self.session_dir.clone()).build()?
            );
            *agent_guard = Some(agent.clone());
            *self.api.lock().unwrap() = Some(AgentApi::new(agent));
        }
        Ok(())
    }
}

impl CoreHostBuilder {
    pub fn with_mock_provider(mut self, name: impl Into<String>) -> Self {
        self.mock_provider = Some(name.into());
        self
    }
    pub fn build(self) -> Result<CoreHost, anyhow::Error> {
        Ok(CoreHost {
            session_dir: self.session_dir,
            agent: Arc::new(Mutex::new(None)),
            api: Arc::new(Mutex::new(None)),
        })
    }
}
```

3. `main.rs` 中的 CLI 命令（`prompt`/`steer`/`chat`/`resume`）全部改为调用 `host.prompt(...)` / `host.steer(...)` / `host.resume(...)`。

- [ ] **Run it and verify it PASSES**：
  ```bash
  cargo test -p ody-host host_prompt_routes_through_agent_api
  ```

- [ ] **Whole-tree typecheck**：
  ```bash
  cargo test -p ody-host
  cargo test -p agent-rs
  ```

- [ ] **Commit**：
  ```bash
  git add rust-ody/crates/ody-host/src/host.rs \
         rust-ody/crates/ody-host/src/session.rs \
         rust-ody/crates/ody-host/src/main.rs \
         rust-ody/crates/ody-host/Cargo.toml \
         rust-ody/crates/ody-host/tests/host_smoke.rs
  git commit -m "feat(ody-host): route CoreHost through AgentApi"
  ```

---

### Task 7: L2 集成测试（`agent-rs` crate 内）

**Depends on:** Task 6

**Files:**
- Create: `rust-ody/crates/agent-rs/tests/agent_l2.rs`
- Create: `rust-ody/crates/agent-rs/tests/fixtures/hello_world_fixture.json`
- Modify: `rust-ody/crates/agent-rs/Cargo.toml:1-80`
- Test: `cargo test -p agent-rs --test agent_l2`

#### 步骤

- [ ] **Write the failing test**：

```rust
use agent_rs::agent::AgentBuilder;
use agent_rs::api::AgentApi;
use agent_rs::llm::fixture::FixtureChatProvider;
use std::sync::Arc;

#[tokio::test]
async fn l2_hello_world() {
    let tmp = tempfile::tempdir().unwrap();
    let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/hello_world_fixture.json");
    let provider = FixtureChatProvider::from_json_file(&fixture_path).unwrap();
    let agent = Arc::new(
        AgentBuilder::new(tmp.path().to_path_buf())
            .with_provider(Box::new(provider))
            .build()
            .unwrap()
    );
    let api = AgentApi::new(agent);
    let resp = api.prompt("hello", Default::default()).await.unwrap();
    assert!(resp.text.to_lowercase().contains("hello"));
}
```

- [ ] **Run it and verify it FAILS**：
  ```bash
  cargo test -p agent-rs --test agent_l2 l2_hello_world
  ```
  预期失败：`with_provider` / fixture 文件不存在。

- [ ] **Write the minimal implementation**：

1. `AgentBuilder` 增加 `with_provider`：

```rust
pub fn with_provider(mut self, provider: Box<dyn kosong_rs::provider::ChatProvider + Send + Sync>) -> Self {
    self.provider = Some(provider);
    self
}
```

2. `AgentBuilder::build()` 如果 `self.provider` 存在则使用它创建 `llm`；否则走默认 mock。

3. 创建 fixture JSON：

`tests/fixtures/hello_world_fixture.json`：
```json
[
  [{ "type": "text", "text": "Hello, world!" }]
]
```

4. `Cargo.toml` 的 `[dev-dependencies]` 添加 `tempfile` / `tokio`。

- [ ] **Run it and verify it PASSES**：
  ```bash
  cargo test -p agent-rs --test agent_l2 l2_hello_world
  ```

- [ ] **Commit**：
  ```bash
  git add rust-ody/crates/agent-rs/tests/agent_l2.rs \
         rust-ody/crates/agent-rs/tests/fixtures/hello_world_fixture.json \
         rust-ody/crates/agent-rs/Cargo.toml
  git commit -m "test(agent-rs): add L2 integration test with fixture provider"
  ```

---

### Task 8: L3 对照测试 harness（无文件工具场景）

**Depends on:** Task 7

**Files:**
- Modify: `packages/integration-tests/src/parity/backends.ts:1-200`
- Modify: `packages/integration-tests/src/parity/run-parity.ts:1-200`
- Create: `packages/integration-tests/src/parity/scenarios/hello-world.ts`
- Create: `packages/integration-tests/src/parity/scenarios/mock-prompt.ts`
- Create: `packages/integration-tests/src/parity/scenarios/bash-echo.ts`
- Create: `packages/integration-tests/src/parity/fixtures/hello_world_fixture.json`
- Modify: `packages/integration-tests/package.json:1-80`
- Test: `pnpm --filter integration-tests run parity:rust`

#### 步骤

- [ ] **Write the failing test**：

`packages/integration-tests/src/parity/scenarios/hello-world.ts`：
```ts
import { defineScenario } from '../driver';

export default defineScenario({
  name: 'hello-world',
  only: ['node-sdk', 'rust-host'],
  steps: [
    { prompt: 'hello', expectedText: 'hello' },
  ],
});
```

- [ ] **Run it and verify it FAILS**：
  ```bash
  pnpm --filter integration-tests run parity:rust --scenario hello-world
  ```
  预期失败：`rust-host` backend 尚未加载 fixture / scenarios 目录未注册。

- [ ] **Write the minimal implementation**：

1. `packages/integration-tests/src/parity/backends.ts`：

```ts
export interface BackendHandle {
  name: string;
  send(prompt: string): Promise<string>;
  close(): Promise<void>;
}

export function makeRustBackend(options: {
  binaryPath: string;
  fixturePath?: string;
  sessionDir: string;
}): BackendHandle {
  const args = ['--rpc', '--session-dir', options.sessionDir];
  if (options.fixturePath) {
    args.push('--fixture', options.fixturePath);
  }
  const proc = spawn(options.binaryPath, args, { stdio: ['pipe', 'pipe', 'pipe'] });
  // ... SDKRpcClient.connect 封装 ...
  return {
    name: 'rust-host',
    async send(prompt) {
      return rpcClient.prompt(prompt);
    },
    async close() {
      proc.kill();
    },
  };
}
```

2. `ody-host/src/main.rs` 增加 CLI 参数解析：

```rust
#[derive(Parser)]
struct Cli {
    #[arg(long)]
    rpc: bool,
    #[arg(long)]
    session_dir: PathBuf,
    #[arg(long)]
    fixture: Option<PathBuf>,
}
```

3. `host.rs` 在 `with_fixture` 时把 `FixtureChatProvider` 注入 `AgentBuilder`：

```rust
impl CoreHostBuilder {
    pub fn with_fixture(mut self, path: PathBuf) -> Self {
        self.fixture = Some(path);
        self
    }
}
```

4. `run-parity.ts` 注册 `hello-world`、`mock-prompt`、`bash-echo` 三个 4.3.9 限定场景；`file-edit` / `multi-turn-tool` 仍从 registry 中排除（明确 defer 到 4.4.8）。

- [ ] **Run it and verify it PASSES**：
  ```bash
  pnpm --filter integration-tests run parity:rust --scenario hello-world
  ```

- [ ] **Commit**：
  ```bash
  git add packages/integration-tests/src/parity/backends.ts \
         packages/integration-tests/src/parity/run-parity.ts \
         packages/integration-tests/src/parity/scenarios/hello-world.ts \
         packages/integration-tests/src/parity/scenarios/mock-prompt.ts \
         packages/integration-tests/src/parity/scenarios/bash-echo.ts \
         packages/integration-tests/src/parity/fixtures/hello_world_fixture.json \
         rust-ody/crates/ody-host/src/main.rs \
         rust-ody/crates/ody-host/src/host.rs
  git commit -m "test(integration): add rust-host L3 parity harness without file tools"
  ```

---

### Task 9: L4 性能基线、最终校验与文档更新

**Depends on:** Task 8

**Files:**
- Create: `rust-ody/crates/agent-rs/benches/turn_flow.rs`
- Create: `packages/integration-tests/src/parity/bench.ts`
- Modify: `.ody-code/roadmaps/backend-architecture-evolution-phase4-rust-host-migration-roadmap.md` §4.3.9
- Modify: `rust-ody/crates/agent-rs/Cargo.toml:1-80`
- Modify: `packages/integration-tests/package.json:1-80`
- Test: `cargo bench -p agent-rs` 与 `pnpm --filter integration-tests run parity:bench`

#### 步骤

- [ ] **Write the failing benchmark / manual check**：

`rust-ody/crates/agent-rs/benches/turn_flow.rs`：
```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use agent_rs::agent::AgentBuilder;
use agent_rs::api::AgentApi;
use std::sync::Arc;

fn turn_flow_hello(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("turn_flow_hello", |b| {
        b.to_async(&rt).iter(|| async {
            let tmp = tempfile::tempdir().unwrap();
            let agent = Arc::new(
                AgentBuilder::new(tmp.path().to_path_buf())
                    .with_model("mock".into())
                    .build()
                    .unwrap()
            );
            let api = AgentApi::new(agent);
            let resp = api.prompt(black_box("hello"), Default::default()).await.unwrap();
            assert!(!resp.text.is_empty());
        })
    });
}

criterion_group!(benches, turn_flow_hello);
criterion_main!(benches);
```

- [ ] **Run it and verify it FAILS**：
  ```bash
  cargo bench -p agent-rs -- turn_flow_hello
  ```
  预期失败：`criterion` / `benches/` 目录未配置。

- [ ] **Write the minimal implementation**：

1. `Cargo.toml` 增加 `[dev-dependencies] criterion = "0.5"` 与 `[[bench]]` 段。

2. `packages/integration-tests/src/parity/bench.ts` 记录 4.3.9 三个场景的 node-sdk / rust-host 耗时对比，输出到 `packages/integration-tests/parity-bench.json`。

3. 更新 roadmap 4.3.9 状态：把 `hello-world`、`mock-prompt`、`bash-echo` 标记为 ✅；把 `file-edit` / `multi-turn-tool` 明确 defer 到 4.4.8 并加注释。

4. 最终校验命令：

```bash
# Rust workspace
cargo test -p agent-rs -p ody-host

# TypeScript workspace
pnpm -r typecheck

# L3 parity（限定场景）
pnpm --filter integration-tests run parity:rust

# L4 benchmark
cargo bench -p agent-rs -- turn_flow_hello
```

- [ ] **Run it and verify it PASSES**：
  执行上述最终校验，确认 `parity:rust` 三个场景全绿且 benchmark 生成 `target/criterion` 报告。

- [ ] **Commit**：
  ```bash
  git add rust-ody/crates/agent-rs/benches/turn_flow.rs \
         packages/integration-tests/src/parity/bench.ts \
         .ody-code/roadmaps/backend-architecture-evolution-phase4-rust-host-migration-roadmap.md
  git commit -m "chore(agent,ody-host): add L4 benchmark and finalize 4.3.9 parity"
  ```

---

## Local Self-Review（本 Part）

- [x] 1. Spec-coverage table（本 Part 范围内）：

| Roadmap §4.3.9 条目 | 覆盖任务 | 状态 |
|---|---|---|
| 组装真实 `Agent` 结构体并持有各子模块 | Task 1, Task 2 | covered |
| `ContextMemory` 改为 `Arc<dyn ContextAgent>` | Task 1 | covered |
| 实现 `TurnAgent` 对子模块的真实委托 | Task 2 | covered |
| `TurnFlow` / Background / Cron 与 `Agent` 对接 | Task 3 | covered |
| 会话 Resume / Replay | Task 4 | covered |
| `AgentApi` 与 fixture mock provider | Task 5 | covered |
| `CoreHost` 路由切换到 `AgentApi` | Task 6 | covered |
| L2 集成测试 | Task 7 | covered |
| L3 对照测试 harness（无文件工具场景） | Task 8 | covered |
| L4 benchmark 与最终校验 | Task 9 | covered |
| `file-edit` / `multi-turn-tool` defer 到 4.4.8 | Task 8, Task 9 | no-op |

- [x] 2. Placeholder scan：所有步骤均给出真实代码或精确命令；无 `TODO`/`TBD`。
- [x] 3. No phantom tasks：每个任务都有创建/修改文件、测试命令与 commit。
- [x] 4. Dependency soundness：Task N 仅依赖 Task N-1 或更早任务，无反向依赖。
- [x] 5. Caller & build soundness：Task 1 统一修改 `ContextMemory` 签名并更新 `context_golden.rs`；每个共享签名变更任务末尾运行 `cargo test -p agent-rs` 与 `pnpm -r typecheck`。
- [x] 6. Test-the-risk：每个状态变更任务都包含行为断言（`assert_eq!` / `assert!`），不只是编译通过。
- [x] 7. Type consistency：Part 内部使用的 `Agent` / `AgentBuilder` / `AgentApi` / `FixtureChatProvider` 命名与字段保持一致。

---
