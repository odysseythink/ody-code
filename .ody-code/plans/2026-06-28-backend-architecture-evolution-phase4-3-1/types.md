# Part 1 — 类型、常量与 Host trait

**Goal:** 建立 `agent-rs/src/context/` 模块骨架，复用 `records` 中已存在的 `PromptOrigin`/`ContextMessage`，补齐 `USER_PROMPT_ORIGIN` 常量、`AgentContextData` 以及 `ContextMemory` 所需的最小 host trait 集合，使后续 projector/memory 实现有稳定的协作接口。

**Architecture:** `types.rs` 作为 context 层与外界的类型契约：它从 `agent_rs::records::nested` re-export 已有类型，新增 `USER_PROMPT_ORIGIN` 常量和 `AgentContextData`，并定义一组小型 trait（`RecordLog`、`MicroCompaction`、`InjectionLifecycle`、`BackgroundNotifications`、`ReplayBuilder`、`StatusEmitter`、`ContextSwitchFlusher`、`Clock`），最终组合为 `ContextAgent`。`ContextMemory` 只依赖 `ContextAgent`，不依赖真实 `Agent`，当前可用 test double 完成对照，未来由 4.3.6/4.3.7/4.3.9 的真实子系统实现这些 trait。

**Tech Stack:** Rust 2021, `serde`/`serde_json`。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File | Responsibility |
|---|---|
| `rust-ody/crates/agent-rs/src/lib.rs` | 新增 `pub mod context;` |
| `rust-ody/crates/agent-rs/src/context/mod.rs` | context 模块入口 |
| `rust-ody/crates/agent-rs/src/context/types.rs` | 常量、辅助结构体、host trait |
| `rust-ody/crates/agent-rs/tests/context_types.rs` | Task 1 类型/常量编译 + round-trip 测试 |
| `rust-ody/crates/agent-rs/tests/context_host_traits.rs` | Task 2 host trait test double 行为测试 |

---

## Dependency Overview

```text
[Task 1: module scaffold + constants/types]
        │
        ▼
[Task 2: host traits]
        │
        ▼
[projector.md / tokens.md / notification.md / memory.md]
```

- `types.md` 的两个任务顺序不可交换：Task 2 在 `types.rs` 中追加 trait，依赖 Task 1 建立的模块结构。
- 无共享签名变更；本部分只新增符号。

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| `RecordLog` trait 抽象与真实 `AgentRecords` 的泛型签名不匹配 | `RecordLog` 故意隐藏泛型；4.3.9 用内部可变 wrapper 实现 |
| `ReplayBuilder::remove_last_messages` 入参形式与 TS `Set<ContextMessage>` 不一致 | 采用 `&[ContextMessage]`，语义等价（去重由调用方保证），避免引入 `Hash` 约束 |
| `MicroCompaction` trait 提前定义会约束 4.3.6 的实现 | trait 方法直接对应 TS `MicroCompaction` 的 `compact`/`reset(maxCutoff)`，接口已锁定 |

---

### Task 1: context 模块骨架与类型常量

**Depends on:** none

**Files:**
- Create: `rust-ody/crates/agent-rs/src/context/mod.rs`
- Create: `rust-ody/crates/agent-rs/src/context/types.rs`
- Modify: `rust-ody/crates/agent-rs/src/lib.rs:1-3`
- Create: `rust-ody/crates/agent-rs/tests/context_types.rs`

- [ ] **Write the failing test**

  ```rust
  // rust-ody/crates/agent-rs/tests/context_types.rs
  use agent_rs::context::{AgentContextData, USER_PROMPT_ORIGIN};
  use agent_rs::records::nested::{ContextMessage, PromptOrigin};

  #[test]
  fn user_prompt_origin_constant_matches_user_variant() {
    assert_eq!(USER_PROMPT_ORIGIN, PromptOrigin::User);
  }

  #[test]
  fn agent_context_data_serializes_with_camel_case_token_count() {
    let data = AgentContextData {
      history: vec![ContextMessage {
        message: kosong_rs::message::Message::user(vec![kosong_rs::message::ContentPart::Text {
          text: "hi".into(),
        }]),
        origin: Some(PromptOrigin::User),
        is_error: None,
      }],
      token_count: 42,
    };
    let json = serde_json::to_string(&data).unwrap();
    assert!(json.contains("\"tokenCount\":42"), "got {}", json);
  }
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  cd rust-ody && cargo test -p agent-rs --test context_types
  ```

  Expected failure: `error[E0433]: failed to resolve: use of undeclared crate or module `context`` / `cannot find value `USER_PROMPT_ORIGIN` in module `agent_rs::context``.

- [ ] **Write the minimal implementation**

  1. Add `pub mod context;` to `rust-ody/crates/agent-rs/src/lib.rs`:

     ```rust
     pub mod context;
     pub mod records;

     pub use records::*;
     ```

  2. Create `rust-ody/crates/agent-rs/src/context/mod.rs`:

     ```rust
     pub mod memory;
     pub mod notification_xml;
     pub mod projector;
     pub mod tokens;
     pub mod types;

     pub use types::*;
     ```

  3. Create `rust-ody/crates/agent-rs/src/context/types.rs`（Task 1 范围：常量 + 结构体，trait 留 Task 2）：

     ```rust
     use kosong_rs::message::ContentPart;
     use serde::{Deserialize, Serialize};

     pub use crate::records::nested::{ContextMessage, PromptOrigin};

     /// 与 TS `USER_PROMPT_ORIGIN` 对应的只读常量。
     pub const USER_PROMPT_ORIGIN: PromptOrigin = PromptOrigin::User;

     /// 用户消息落盘/重放的轻量记录形状。
     #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
     pub struct UserMessageRecord {
       pub content: Vec<ContentPart>,
       pub origin: PromptOrigin,
     }

     /// system-reminder 落盘/重放的轻量记录形状。
     #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
     pub struct SystemReminderRecord {
       pub content: String,
       pub origin: PromptOrigin,
     }

     /// `ContextMemory::data()` 的返回值；与 TS `AgentContextData` 对齐。
     #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
     #[serde(rename_all = "camelCase")]
     pub struct AgentContextData {
       pub history: Vec<ContextMessage>,
       pub token_count: i64,
     }
     ```

- [ ] **Run it and verify it PASSES**

  ```bash
  cd rust-ody && cargo test -p agent-rs --test context_types
  ```

  Expected: `test result: ok. 2 passed; 0 failed`.

- [ ] **Commit**

  ```bash
  git add rust-ody/crates/agent-rs/src/lib.rs \
         rust-ody/crates/agent-rs/src/context/mod.rs \
         rust-ody/crates/agent-rs/src/context/types.rs \
         rust-ody/crates/agent-rs/tests/context_types.rs
  git commit -m "feat(agent-rs): context module scaffold and types/constants"
  ```

---

### Task 2: 定义 ContextMemory 所需 host trait

**Depends on:** Task 1

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/context/types.rs`
- Create: `rust-ody/crates/agent-rs/tests/context_host_traits.rs`

- [ ] **Write the failing test**

  ```rust
  // rust-ody/crates/agent-rs/tests/context_host_traits.rs
  use std::cell::RefCell;

  use agent_rs::context::*;
  use agent_rs::records::nested::{BackgroundTask, PromptOrigin};
  use agent_rs::records::AgentRecord;

  struct SpyRecordLog(RefCell<Vec<String>>);
  impl RecordLog for SpyRecordLog {
    fn log_record(&self, record: AgentRecord) {
      self.0.borrow_mut().push(record.record_type().to_string());
    }
  }

  struct SpyMicroCompaction(RefCell<Vec<usize>>);
  impl MicroCompaction for SpyMicroCompaction {
    fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage> {
      messages.to_vec()
    }
    fn reset(&self, max_cutoff: usize) {
      self.0.borrow_mut().push(max_cutoff);
    }
  }

  struct SpyInjection(RefCell<Vec<String>>);
  impl InjectionLifecycle for SpyInjection {
    fn on_context_clear(&self) { self.0.borrow_mut().push("clear".into()); }
    fn on_context_compacted(&self, compacted_count: usize) {
      self.0.borrow_mut().push(format!("compacted:{}", compacted_count));
    }
    fn on_context_message_removed(&self, index: usize) {
      self.0.borrow_mut().push(format!("removed:{}", index));
    }
  }

  struct NoopBackground;
  impl BackgroundNotifications for NoopBackground {
    fn mark_delivered_notification(&self, _origin: &BackgroundTask) {}
  }

  struct SpyReplay(RefCell<Vec<String>>);
  impl ReplayBuilder for SpyReplay {
    fn push_message(&self, _message: &ContextMessage) { self.0.borrow_mut().push("push".into()); }
    fn remove_last_messages(&self, _messages: &[ContextMessage]) { self.0.borrow_mut().push("remove".into()); }
  }

  struct NoopStatus;
  impl StatusEmitter for NoopStatus { fn emit_status_updated(&self) {} }

  struct NoopFlusher;
  impl ContextSwitchFlusher for NoopFlusher { fn flush_deferred_context_switch(&self) {} }

  struct FixedClock(i64);
  impl Clock for FixedClock { fn now_ms(&self) -> i64 { self.0 } }

  struct TestAgent {
    record_log: SpyRecordLog,
    micro_compaction: SpyMicroCompaction,
    injection: SpyInjection,
    background: NoopBackground,
    replay_builder: SpyReplay,
    status: NoopStatus,
    flusher: NoopFlusher,
    clock: FixedClock,
  }

  impl ContextAgent for TestAgent {
    fn record_log(&self) -> &dyn RecordLog { &self.record_log }
    fn micro_compaction(&self) -> &dyn MicroCompaction { &self.micro_compaction }
    fn injection(&self) -> &dyn InjectionLifecycle { &self.injection }
    fn background(&self) -> &dyn BackgroundNotifications { &self.background }
    fn replay_builder(&self) -> &dyn ReplayBuilder { &self.replay_builder }
    fn status_emitter(&self) -> &dyn StatusEmitter { &self.status }
    fn context_switch_flusher(&self) -> &dyn ContextSwitchFlusher { &self.flusher }
    fn clock(&self) -> &dyn Clock { &self.clock }
  }

  #[test]
  fn context_agent_trait_routes_to_all_subsystems() {
    let agent = TestAgent {
      record_log: SpyRecordLog(RefCell::new(vec![])),
      micro_compaction: SpyMicroCompaction(RefCell::new(vec![])),
      injection: SpyInjection(RefCell::new(vec![])),
      background: NoopBackground,
      replay_builder: SpyReplay(RefCell::new(vec![])),
      status: NoopStatus,
      flusher: NoopFlusher,
      clock: FixedClock(12345),
    };

    agent.record_log().log_record(AgentRecord::ContextClear { time: None });
    agent.injection().on_context_clear();
    agent.micro_compaction().reset(7);

    assert_eq!(agent.record_log.0.borrow().as_slice(), &["context.clear"]);
    assert_eq!(agent.injection.0.borrow().as_slice(), &["clear"]);
    assert_eq!(agent.micro_compaction.0.borrow().as_slice(), &[7]);
    assert_eq!(agent.clock().now_ms(), 12345);
  }
  ```

  Note: `AgentRecord::record_type()` already exists on the enum from 4.3.0; if it does not, add the helper method in `records/types.rs` as part of this task.

- [ ] **Run it and verify it FAILS**

  ```bash
  cd rust-ody && cargo test -p agent-rs --test context_host_traits
  ```

  Expected failure: trait `RecordLog`, `MicroCompaction`, `InjectionLifecycle`, `ContextAgent` 等未找到。

- [ ] **Write the minimal implementation**

  追加到 `rust-ody/crates/agent-rs/src/context/types.rs`：

  ```rust
  use crate::records::AgentRecord;
  use crate::records::nested::BackgroundTask;

  /// 记录日志抽象。隐藏 `AgentRecords` 的泛型，使 `ContextMemory` 可测可替换。
  pub trait RecordLog: Send + Sync {
    fn log_record(&self, record: AgentRecord);
  }

  /// Micro-compaction 最小接口。4.3.6 的真实实现与当前 test double 都满足此契约。
  pub trait MicroCompaction: Send + Sync {
    fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage>;
    fn reset(&self, max_cutoff: usize);
  }

  /// Injection 生命周期回调。4.3.7 的 `InjectionManager` 将实现此 trait。
  pub trait InjectionLifecycle: Send + Sync {
    fn on_context_clear(&self);
    fn on_context_compacted(&self, compacted_count: usize);
    fn on_context_message_removed(&self, index: usize);
  }

  /// 后台任务通知投递标记。4.3.8 的 `BackgroundManager` 将实现此 trait。
  pub trait BackgroundNotifications: Send + Sync {
    fn mark_delivered_notification(&self, origin: &BackgroundTask);
  }

  /// ReplayBuilder 最小接口。4.3.7 的真实实现负责维护 resume 校验所需消息序列。
  pub trait ReplayBuilder: Send + Sync {
    fn push_message(&self, message: &ContextMessage);
    fn remove_last_messages(&self, messages: &[ContextMessage]);
  }

  pub trait StatusEmitter: Send + Sync {
    fn emit_status_updated(&self);
  }

  pub trait ContextSwitchFlusher: Send + Sync {
    fn flush_deferred_context_switch(&self);
  }

  pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
  }

  /// `ContextMemory` 所需的全部 host 能力。真实 Agent 与 test double 都实现此 trait。
  pub trait ContextAgent: Send + Sync {
    fn record_log(&self) -> &dyn RecordLog;
    fn micro_compaction(&self) -> &dyn MicroCompaction;
    fn injection(&self) -> &dyn InjectionLifecycle;
    fn background(&self) -> &dyn BackgroundNotifications;
    fn replay_builder(&self) -> &dyn ReplayBuilder;
    fn status_emitter(&self) -> &dyn StatusEmitter;
    fn context_switch_flusher(&self) -> &dyn ContextSwitchFlusher;
    fn clock(&self) -> &dyn Clock;
  }
  ```

  若 `AgentRecord` 缺少 `record_type()` 方法，在 `rust-ody/crates/agent-rs/src/records/types.rs` 中添加：

  ```rust
  impl AgentRecord {
    pub fn record_type(&self) -> &'static str {
      match self {
        AgentRecord::Metadata { .. } => "metadata",
        AgentRecord::TurnPrompt { .. } => "turn.prompt",
        AgentRecord::TurnSteer { .. } => "turn.steer",
        AgentRecord::TurnCancel { .. } => "turn.cancel",
        AgentRecord::ConfigUpdate { .. } => "config.update",
        AgentRecord::PermissionSetMode { .. } => "permission.set_mode",
        AgentRecord::PermissionRecordApprovalResult { .. } => "permission.record_approval_result",
        AgentRecord::FullCompactionBegin { .. } => "full_compaction.begin",
        AgentRecord::SessionModeEnter { .. } => "session_mode.enter",
        AgentRecord::SessionModeCancel { .. } => "session_mode.cancel",
        AgentRecord::SessionModeExit { .. } => "session_mode.exit",
        AgentRecord::ToolsRegisterUserTool { .. } => "tools.register_user_tool",
        AgentRecord::ToolsUnregisterUserTool { .. } => "tools.unregister_user_tool",
        AgentRecord::ToolsSetActiveTools { .. } => "tools.set_active_tools",
        AgentRecord::UsageRecord { .. } => "usage.record",
        AgentRecord::FullCompactionCancel { .. } => "full_compaction.cancel",
        AgentRecord::FullCompactionComplete { .. } => "full_compaction.complete",
        AgentRecord::MicroCompactionApply { .. } => "micro_compaction.apply",
        AgentRecord::ContextAppendMessage { .. } => "context.append_message",
        AgentRecord::ContextAppendLoopEvent { .. } => "context.append_loop_event",
        AgentRecord::ContextClear { .. } => "context.clear",
        AgentRecord::ContextApplyCompaction { .. } => "context.apply_compaction",
        AgentRecord::ContextUndo { .. } => "context.undo",
        AgentRecord::ToolsUpdateStore { .. } => "tools.update_store",
        AgentRecord::GoalCreate { .. } => "goal.create",
        AgentRecord::GoalUpdate { .. } => "goal.update",
        AgentRecord::GoalAccountUsage { .. } => "goal.account_usage",
        AgentRecord::GoalContinuation { .. } => "goal.continuation",
        AgentRecord::GoalClear { .. } => "goal.clear",
      }
    }
  }
  ```

- [ ] **Run it and verify it PASSES**

  ```bash
  cd rust-ody && cargo test -p agent-rs --test context_host_traits
  ```

  Expected: `test result: ok. 1 passed; 0 failed`.

- [ ] **Commit**

  ```bash
  git add rust-ody/crates/agent-rs/src/context/types.rs \
         rust-ody/crates/agent-rs/tests/context_host_traits.rs
  git commit -m "feat(agent-rs): context host traits for ContextMemory"
  ```

---

## Local Self-Review

- [ ] 1. Spec-coverage（本 part）：Task 1 覆盖 4.3.1.1 的类型/常量迁移；Task 2 提供 ContextMemory 所需的 host trait，支撑 4.3.1.2。无 GAP。
- [ ] 2. Placeholder scan：所有代码均为真实 Rust 代码，无 TODO/TBD；`record_type()` 匹配所有 `AgentRecord` 变体，无 `_ =>` fallback。
- [ ] 3. No phantom tasks：Task 1 产出模块/常量/测试；Task 2 产出 trait/辅助方法/测试。
- [ ] 4. Dependency soundness：Task 2 仅依赖 Task 1 建立的 `types.rs` 与 `AgentRecord`。
- [ ] 5. Caller & build soundness：新增 `record_type()` 为 `impl AgentRecord` 新方法，不影响现有调用；新增 `context` 模块未改变现有 API。Task 2 结束时运行 `cargo check -p agent-rs --workspace --tests` 全绿。
- [ ] 6. Test-the-risk：Task 1 断言序列化字段名（`tokenCount`）防止与 TS 不一致；Task 2 用 spy 断言 `ContextAgent` 路由到正确子系统，避免 trait 方法挂错。
- [ ] 7. Type consistency：`ContextMessage`/`PromptOrigin` 直接复用 `records::nested` 定义；`AgentContextData` 字段名与 TS `tokenCount` 对齐；trait 方法签名与 TS 行为一致（`reset(max_cutoff)`、`compact` 返回 `Vec<ContextMessage>`）。
