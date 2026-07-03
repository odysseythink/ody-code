# Part 5 — ContextMemory 状态机

**Goal:** 在 Rust 中实现与 TS `ContextMemory` 逐行为准的状态机：构造、追加 user/system 消息、清空、undo、apply compaction、loop 事件（step / content / tool-call / tool-result）、工具结果格式化、deferred 消息、投影输出与 token 记账。

**Architecture:** `memory.rs` 持有 `&dyn ContextAgent`（来自 `types.md` 的 host trait），内部维护 `history`、`token_count`、open-step 映射、pending tool result id 集合与 deferred 消息队列。所有对真实 `Agent` 的依赖都通过 trait 路由，L1 测试使用 test double 完成；真实 Agent 将在 4.3.9 实现这些 trait。

**Tech Stack:** Rust 2021, `kosong-rs::message`, `serde_json`。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File | Responsibility |
|---|---|
| `rust-ody/crates/agent-rs/src/context/memory.rs` | `ContextMemory` 结构体与全部方法 |
| `rust-ody/crates/agent-rs/src/context/mod.rs` | 追加 `pub mod memory;` 与 re-export |
| `rust-ody/crates/agent-rs/src/context/types.rs` | 为 `RecordLog` 追加 `restoring_time()` 方法 |
| `rust-ody/crates/agent-rs/tests/common/mod.rs` | `ContextMemory` L1 test double（Task 6 创建） |
| `rust-ody/crates/agent-rs/tests/context_memory_basic.rs` | Task 6 基本追加/清空/投影测试 |
| `rust-ody/crates/agent-rs/tests/context_memory_undo_compaction.rs` | Task 7 undo/compaction 测试 |
| `rust-ody/crates/agent-rs/tests/context_memory_loop_event.rs` | Task 8 loop 事件/工具结果/deferred 测试 |

---

## Dependency Overview

```text
[types.md Task 1-2]
        │
        ├──► [projector.md Task 3]
        │
        ├──► [tokens.md Task 4]
        │
        ├──► [notification.md Task 5]
        │
        ▼
[memory.md Task 6: ContextMemory 骨架 + 基本追加/清空/投影]
        │
        ▼
[memory.md Task 7: undo + apply_compaction]
        │
        ▼
[memory.md Task 8: loop 事件 + 工具结果 + deferred 消息]
```

- 三个 task 顺序不可交换：Task 7/8 依赖 Task 6 建立的 `ContextMemory` 类型与方法签名。
- Task 6 会修改 `types.rs` 中的 `RecordLog` trait（新增 `restoring_time()`），属于共享签名变更；同一 task 必须更新 `tests/context_host_traits.rs` 中的 test double 并跑全量 typecheck。

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| `RecordLog` 缺少 `restoring_time()`，导致 `push_history` 无法复刻 TS `records.restoring?.time ?? Date.now()` | Task 6 在 trait 中新增方法，所有 test double 同步实现 |
| `ContextMessage` 不带 `Hash`，无法放入 `HashSet` | pending tool result id 集合只存 `String`（tool_call_id），不存消息本身 |
| `undo` 中 `count` 为 `i64` 但索引为 `usize` | 仅当 `count > 0` 时进入循环，转换为 `usize` 并比较已移除数量 |
| tool result 格式化字符串与 TS 不完全一致 | 常量 `TOOL_ERROR_STATUS` / `TOOL_EMPTY_STATUS` / `TOOL_EMPTY_ERROR_STATUS` / `TOOL_OUTPUT_EMPTY_TEXT` 与 TS 逐字对齐，并单独测试 |
| open step 在 `HashMap` 中以值存储，step end 后查找索引可能找不到 | `ContextMessage` 已实现 `PartialEq`，用 `iter().position()` 查找；找不到时回退到 `history.len()` |
| deferred 消息刷新顺序 | 保持 FIFO：`push_history(...deferred_messages)` 一次性追加 |

---

### Task 6: ContextMemory 骨架、追加消息、清空与投影

**Depends on:** `types.md` Task 2, `projector.md` Task 3, `tokens.md` Task 4

**Files:**
- Create: `rust-ody/crates/agent-rs/src/context/memory.rs`
- Modify: `rust-ody/crates/agent-rs/src/context/mod.rs`
- Modify: `rust-ody/crates/agent-rs/src/context/types.rs`（为 `RecordLog` 追加 `restoring_time()`）
- Modify: `rust-ody/crates/agent-rs/tests/context_host_traits.rs`（同步 test double）
- Create: `rust-ody/crates/agent-rs/tests/common/mod.rs`
- Create: `rust-ody/crates/agent-rs/tests/context_memory_basic.rs`

- [ ] **Write the failing test**

  ```rust
  // rust-ody/crates/agent-rs/tests/common/mod.rs
  use std::sync::Mutex;

  use agent_rs::context::*;
  use agent_rs::records::nested::{BackgroundTask, ContextMessage, PromptOrigin};
  use agent_rs::records::AgentRecord;

  #[derive(Default)]
  pub struct SpyRecordLog(pub Mutex<Vec<AgentRecord>>);
  impl RecordLog for SpyRecordLog {
    fn log_record(&self, record: AgentRecord) {
      self.0.lock().unwrap().push(record);
    }
    fn restoring_time(&self) -> Option<i64> {
      None
    }
  }

  pub struct SpyMicroCompaction(pub Mutex<Vec<usize>>);
  impl MicroCompaction for SpyMicroCompaction {
    fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage> {
      messages.to_vec()
    }
    fn reset(&self, max_cutoff: usize) {
      self.0.lock().unwrap().push(max_cutoff);
    }
  }

  pub struct SpyInjection(pub Mutex<Vec<String>>);
  impl InjectionLifecycle for SpyInjection {
    fn on_context_clear(&self) {
      self.0.lock().unwrap().push("clear".into());
    }
    fn on_context_compacted(&self, compacted_count: usize) {
      self.0.lock().unwrap().push(format!("compacted:{}", compacted_count));
    }
    fn on_context_message_removed(&self, index: usize) {
      self.0.lock().unwrap().push(format!("removed:{}", index));
    }
  }

  pub struct NoopBackground;
  impl BackgroundNotifications for NoopBackground {
    fn mark_delivered_notification(&self, _origin: &BackgroundTask) {}
  }

  pub struct SpyReplay(pub Mutex<Vec<Vec<ContextMessage>>>);
  impl ReplayBuilder for SpyReplay {
    fn push_message(&self, message: &ContextMessage) {
      self.0.lock().unwrap().push(vec![message.clone()]);
    }
    fn remove_last_messages(&self, messages: &[ContextMessage]) {
      self.0.lock().unwrap().push(messages.to_vec());
    }
  }

  pub struct NoopStatus;
  impl StatusEmitter for NoopStatus {
    fn emit_status_updated(&self) {}
  }

  pub struct NoopFlusher;
  impl ContextSwitchFlusher for NoopFlusher {
    fn flush_deferred_context_switch(&self) {}
  }

  pub struct FixedClock(pub i64);
  impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
      self.0
    }
  }

  pub struct TestAgent {
    pub record_log: SpyRecordLog,
    pub micro_compaction: SpyMicroCompaction,
    pub injection: SpyInjection,
    pub background: NoopBackground,
    pub replay_builder: SpyReplay,
    pub status: NoopStatus,
    pub flusher: NoopFlusher,
    pub clock: FixedClock,
  }

  impl TestAgent {
    pub fn new() -> Self {
      Self {
        record_log: SpyRecordLog::default(),
        micro_compaction: SpyMicroCompaction(Mutex::new(Vec::new())),
        injection: SpyInjection(Mutex::new(Vec::new())),
        background: NoopBackground,
        replay_builder: SpyReplay(Mutex::new(Vec::new())),
        status: NoopStatus,
        flusher: NoopFlusher,
        clock: FixedClock(12345),
      }
    }
  }

  impl ContextAgent for TestAgent {
    fn record_log(&self) -> &dyn RecordLog {
      &self.record_log
    }
    fn micro_compaction(&self) -> &dyn MicroCompaction {
      &self.micro_compaction
    }
    fn injection(&self) -> &dyn InjectionLifecycle {
      &self.injection
    }
    fn background(&self) -> &dyn BackgroundNotifications {
      &self.background
    }
    fn replay_builder(&self) -> &dyn ReplayBuilder {
      &self.replay_builder
    }
    fn status_emitter(&self) -> &dyn StatusEmitter {
      &self.status
    }
    fn context_switch_flusher(&self) -> &dyn ContextSwitchFlusher {
      &self.flusher
    }
    fn clock(&self) -> &dyn Clock {
      &self.clock
    }
  }
  ```

  ```rust
  // rust-ody/crates/agent-rs/tests/context_memory_basic.rs
  mod common;

  use agent_rs::context::{ContextMemory, USER_PROMPT_ORIGIN};
  use agent_rs::records::nested::{ContextMessage, PromptOrigin};
  use agent_rs::records::AgentRecord;
  use common::TestAgent;
  use kosong_rs::message::{ContentPart, Message, Role};

  fn user_message(text: &str) -> ContextMessage {
    ContextMessage {
      message: Message::user_text(text),
      origin: Some(PromptOrigin::User),
      is_error: None,
    }
  }

  fn assistant_message(text: &str) -> ContextMessage {
    ContextMessage {
      message: Message::assistant(vec![ContentPart::Text { text: text.into() }], vec![]),
      origin: None,
      is_error: None,
    }
  }

  #[test]
  fn memory_starts_empty_and_exposes_data() {
    let agent = TestAgent::new();
    let memory = ContextMemory::new(&agent);
    assert!(memory.history().is_empty());
    assert_eq!(memory.token_count(), 0);
    assert_eq!(memory.last_assistant_at(), None);
    let data = memory.data();
    assert!(data.history.is_empty());
    assert_eq!(data.token_count, 0);
  }

  #[test]
  fn append_user_message_pushes_real_user_message_and_logs() {
    let agent = TestAgent::new();
    let mut memory = ContextMemory::new(&agent);
    memory.append_user_message(vec![ContentPart::Text { text: "hello".into() }], USER_PROMPT_ORIGIN);

    assert_eq!(memory.history().len(), 1);
    assert_eq!(memory.history()[0].message.role, Role::User);
    let records = agent.record_log.0.lock().unwrap();
    assert_eq!(records.len(), 1);
    assert!(matches!(records[0], AgentRecord::ContextAppendMessage { .. }));
  }

  #[test]
  fn append_system_reminder_wraps_content() {
    let agent = TestAgent::new();
    let mut memory = ContextMemory::new(&agent);
    memory.append_system_reminder("remember this", PromptOrigin::SystemTrigger { name: "host".into() });

    let history = memory.history();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].message.role, Role::User);
    let text = match &history[0].message.content[0] {
      ContentPart::Text { text } => text.as_str(),
      _ => panic!("expected text"),
    };
    assert_eq!(text, "<system-reminder>\nremember this\n</system-reminder>");
  }

  #[test]
  fn clear_resets_history_and_emits_callbacks() {
    let agent = TestAgent::new();
    let mut memory = ContextMemory::new(&agent);
    memory.append_user_message(vec![ContentPart::Text { text: "a".into() }], USER_PROMPT_ORIGIN);
    memory.clear();

    assert!(memory.history().is_empty());
    assert_eq!(memory.token_count(), 0);
    assert_eq!(memory.last_assistant_at(), None);
    let records = agent.record_log.0.lock().unwrap();
    assert!(records.iter().any(|r| matches!(r, AgentRecord::ContextClear { .. })));
    let resets = agent.micro_compaction.0.lock().unwrap();
    assert_eq!(resets.as_slice(), &[0]);
    let injections = agent.injection.0.lock().unwrap();
    assert_eq!(injections.as_slice(), &["clear".into()]);
  }

  #[test]
  fn messages_are_projected_and_healed() {
    let agent = TestAgent::new();
    let mut memory = ContextMemory::new(&agent);
    memory.append_user_message(vec![ContentPart::Text { text: "hi".into() }], USER_PROMPT_ORIGIN);
    memory.append_user_message(vec![ContentPart::Text { text: "there".into() }], USER_PROMPT_ORIGIN);

    let messages = memory.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].role, Role::User);
    assert_eq!(
      messages[0].content,
      vec![ContentPart::Text { text: "hi\n\nthere".into() }]
    );
  }

  #[test]
  fn last_assistant_at_updated_on_assistant_push() {
    let agent = TestAgent::new();
    let mut memory = ContextMemory::new(&agent);
    memory.append_message(assistant_message("ok"));
    assert_eq!(memory.last_assistant_at(), Some(12345));
  }
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  cd rust-ody && cargo test -p agent-rs --test context_memory_basic
  ```

  Expected failure: `error[E0433]: failed to resolve: use of undeclared crate or module `memory`` / `cannot find struct `ContextMemory` in module `agent_rs::context`.

- [ ] **Write the minimal implementation**

  1. 修改 `rust-ody/crates/agent-rs/src/context/types.rs`，在 `RecordLog` trait 中追加 `restoring_time()`：

     ```rust
     pub trait RecordLog: Send + Sync {
       fn log_record(&self, record: AgentRecord);
       /// 若当前处于 record 恢复流程，返回恢复时间戳；否则返回 `None`。
       fn restoring_time(&self) -> Option<i64>;
     }
     ```

  2. 同步更新 `rust-ody/crates/agent-rs/tests/context_host_traits.rs` 中的 `SpyRecordLog`：

     ```rust
     impl RecordLog for SpyRecordLog {
       fn log_record(&self, record: AgentRecord) {
         self.0.borrow_mut().push(record.record_type().to_string());
       }
       fn restoring_time(&self) -> Option<i64> {
         None
       }
     }
     ```

  3. 创建 `rust-ody/crates/agent-rs/src/context/memory.rs`：

     ```rust
     use std::collections::{HashMap, HashSet};

     use kosong_rs::message::{ContentPart, Message, Role, ToolCall};

     use crate::context::projector::{drop_orphan_tool_results, project};
     use crate::context::tokens::estimate_tokens_for_message;
     use crate::context::types::{AgentContextData, ContextAgent, ContextMessage, PromptOrigin};
     use crate::records::nested::{CompactionResult, LoopRecordedEvent};
     use crate::records::AgentRecord;

     pub const TOOL_ERROR_STATUS: &str = "<system>ERROR: Tool execution failed.</system>";
     pub const TOOL_EMPTY_STATUS: &str = "<system>Tool output is empty.</system>";
     pub const TOOL_EMPTY_ERROR_STATUS: &str =
       "<system>ERROR: Tool execution failed. Tool output is empty.</system>";
     pub const TOOL_OUTPUT_EMPTY_TEXT: &str = "Tool output is empty.";

     pub struct ContextMemory<'a> {
       agent: &'a dyn ContextAgent,
       history: Vec<ContextMessage>,
       token_count: i64,
       token_count_covered_message_count: usize,
       open_steps: HashMap<String, ContextMessage>,
       pending_tool_result_ids: HashSet<String>,
       deferred_messages: Vec<ContextMessage>,
       last_assistant_at: Option<i64>,
     }

     impl<'a> ContextMemory<'a> {
       pub fn new(agent: &'a dyn ContextAgent) -> Self {
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

       pub fn append_user_message(
         &mut self,
         content: Vec<ContentPart>,
         origin: PromptOrigin,
       ) {
         self.append_message(ContextMessage {
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

       pub fn append_system_reminder(&mut self, content: &str, origin: PromptOrigin) {
         let text = format!("<system-reminder>\n{}\n</system-reminder>", content);
         self.append_message(ContextMessage {
           message: Message::user_text(text),
           origin: Some(origin),
           is_error: None,
         });
       }

       pub fn clear(&mut self) {
         self.agent.record_log().log_record(AgentRecord::ContextClear { time: None });
         self.history.clear();
         self.token_count = 0;
         self.token_count_covered_message_count = 0;
         self.open_steps.clear();
         self.pending_tool_result_ids.clear();
         self.deferred_messages.clear();
         self.last_assistant_at = None;
         self.agent.micro_compaction().reset(0);
         self.agent.injection().on_context_clear();
         self.agent.status_emitter().emit_status_updated();
       }

       pub fn data(&self) -> AgentContextData {
         AgentContextData {
           history: self.history.clone(),
           token_count: self.token_count,
         }
       }

       pub fn token_count(&self) -> i64 {
         self.token_count
       }

       pub fn token_count_with_pending(&self) -> i64 {
         let pending = &self.history[self.token_count_covered_message_count..];
         self.token_count + pending.iter().map(|m| estimate_tokens_for_message(&m.message)).sum::<i64>()
       }

       pub fn history(&self) -> &[ContextMessage] {
         &self.history
       }

       pub fn messages(&self) -> Vec<Message> {
         let compacted = self.agent.micro_compaction().compact(&self.history);
         drop_orphan_tool_results(project(&compacted))
       }

       pub fn append_message(&mut self, message: ContextMessage) {
         self.agent
           .record_log()
           .log_record(AgentRecord::ContextAppendMessage {
             time: None,
             message: message.clone(),
           });
         if self.has_open_tool_exchange() {
           self.deferred_messages.push(message);
           return;
         }
         self.push_history(vec![message]);
       }

       pub fn has_open_steps(&self) -> bool {
         !self.open_steps.is_empty()
       }

       pub fn reset_runtime_state(&mut self) {
         self.open_steps.clear();
         self.pending_tool_result_ids.clear();
         self.deferred_messages.clear();
       }

       pub fn last_assistant_at(&self) -> Option<i64> {
         self.last_assistant_at
       }

       fn has_open_tool_exchange(&self) -> bool {
         !self.pending_tool_result_ids.is_empty()
       }

       fn push_history(&mut self, mut messages: Vec<ContextMessage>) {
         self.history.append(&mut messages);
         for message in &messages {
           if message.message.role == Role::Assistant {
             self.last_assistant_at = self
               .agent
               .record_log()
               .restoring_time()
               .or_else(|| Some(self.agent.clock().now_ms()));
           }
           if let Some(PromptOrigin::BackgroundTask(ref origin)) = message.origin {
             self.agent.background().mark_delivered_notification(origin);
           }
           self.agent.replay_builder().push_message(message);
         }
       }
     }
     ```

  4. 修改 `rust-ody/crates/agent-rs/src/context/mod.rs` 追加 re-export：

     ```rust
     pub mod memory;
     pub use memory::ContextMemory;
     ```

     完整 `mod.rs`：

     ```rust
     pub mod memory;
     pub mod notification_xml;
     pub mod projector;
     pub mod tokens;
     pub mod types;

     pub use memory::ContextMemory;
     pub use notification_xml::render_notification_xml;
     pub use projector::{drop_orphan_tool_results, project};
     pub use tokens::{
       estimate_tokens, estimate_tokens_for_content_part, estimate_tokens_for_content_parts,
       estimate_tokens_for_message, estimate_tokens_for_messages,
     };
     pub use types::*;
     ```

- [ ] **Run it and verify it PASSES**

  ```bash
  cd rust-ody && cargo test -p agent-rs --test context_memory_basic
  ```

  Expected: `test result: ok. 6 passed; 0 failed`.

- [ ] **Run whole-tree typecheck (shared-signature change)**

  ```bash
  cd rust-ody && cargo check -p agent-rs --workspace --tests
  ```

  Expected: workspace typecheck clean.

- [ ] **Commit**

  ```bash
  git add rust-ody/crates/agent-rs/src/context/types.rs \
         rust-ody/crates/agent-rs/src/context/memory.rs \
         rust-ody/crates/agent-rs/src/context/mod.rs \
         rust-ody/crates/agent-rs/tests/context_host_traits.rs \
         rust-ody/crates/agent-rs/tests/common/mod.rs \
         rust-ody/crates/agent-rs/tests/context_memory_basic.rs
  git commit -m "feat(agent-rs): ContextMemory scaffold, append, clear and projection"

---

### Task 7: `undo` 与 `apply_compaction`

**Depends on:** Task 6

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/context/memory.rs`（追加 `undo` / `apply_compaction` 及私有辅助函数）
- Create: `rust-ody/crates/agent-rs/tests/context_memory_undo_compaction.rs`

- [ ] **Write the failing test**

  ```rust
  // rust-ody/crates/agent-rs/tests/context_memory_undo_compaction.rs
  mod common;

  use agent_rs::context::{ContextMemory, USER_PROMPT_ORIGIN};
  use agent_rs::records::nested::{CompactionResult, ContextMessage, PromptOrigin};
  use common::TestAgent;
  use kosong_rs::message::{ContentPart, Message, Role};

  fn user_msg(text: &str) -> ContextMessage {
    ContextMessage {
      message: Message::user_text(text),
      origin: Some(PromptOrigin::User),
      is_error: None,
    }
  }

  fn user_msg_with_origin(text: &str, origin: PromptOrigin) -> ContextMessage {
    ContextMessage {
      message: Message::user_text(text),
      origin: Some(origin),
      is_error: None,
    }
  }

  #[test]
  fn undo_removes_real_user_prompts_and_updates_token_accounting() {
    let agent = TestAgent::new();
    let mut memory = ContextMemory::new(&agent);
    memory.append_user_message(vec![ContentPart::Text { text: "a".into() }], USER_PROMPT_ORIGIN);
    memory.append_user_message(vec![ContentPart::Text { text: "b".into() }], USER_PROMPT_ORIGIN);

    memory.undo(1);

    assert_eq!(memory.history().len(), 1);
    assert_eq!(memory.history()[0].message.role, Role::User);
    let text = match &memory.history()[0].message.content[0] {
      ContentPart::Text { text } => text.as_str(),
      _ => panic!("expected text"),
    };
    assert_eq!(text, "a");
    let injections = agent.injection.0.lock().unwrap();
    assert!(injections.iter().any(|s| s.starts_with("removed:")));
  }

  #[test]
  fn undo_skips_injection_and_stops_at_compaction_summary() {
    let agent = TestAgent::new();
    let mut memory = ContextMemory::new(&agent);
    memory.append_message(user_msg_with_origin(
      "u1",
      PromptOrigin::Injection { variant: "test".into() },
    ));
    memory.append_message(user_msg("u2"));
    memory.append_message(user_msg_with_origin(
      "summary",
      PromptOrigin::CompactionSummary,
    ));
    memory.append_message(user_msg("u3"));

    memory.undo(1);

    assert_eq!(memory.history().len(), 3);
    assert_eq!(memory.history()[memory.history().len() - 1].message.role, Role::User);
    let text = match &memory.history()[memory.history().len() - 1].message.content[0] {
      ContentPart::Text { text } => text.as_str(),
      _ => panic!("expected text"),
    };
    assert_eq!(text, "u2");
  }

  #[test]
  fn undo_with_non_positive_count_is_noop() {
    let agent = TestAgent::new();
    let mut memory = ContextMemory::new(&agent);
    memory.append_message(user_msg("x"));
    memory.undo(0);
    memory.undo(-1);
    assert_eq!(memory.history().len(), 1);
  }

  #[test]
  fn apply_compaction_prepends_summary_and_resets_counts() {
    let agent = TestAgent::new();
    let mut memory = ContextMemory::new(&agent);
    memory.append_message(user_msg("a"));
    memory.append_message(user_msg("b"));
    memory.append_message(user_msg("c"));

    memory.apply_compaction(CompactionResult {
      summary: "summary text".into(),
      compacted_count: 2,
      tokens_before: 10,
      tokens_after: 3,
    });

    let history = memory.history();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].message.role, Role::Assistant);
    assert_eq!(history[0].origin, Some(PromptOrigin::CompactionSummary));
    assert_eq!(memory.token_count(), 3);
    assert_eq!(memory.token_count_covered_message_count(), 2);
    let injections = agent.injection.0.lock().unwrap();
    assert!(injections.iter().any(|s| s == "compacted:2"));
  }

  #[test]
  fn apply_compaction_with_zero_compacted_count_keeps_history() {
    let agent = TestAgent::new();
    let mut memory = ContextMemory::new(&agent);
    memory.append_message(user_msg("a"));

    memory.apply_compaction(CompactionResult {
      summary: "summary".into(),
      compacted_count: 0,
      tokens_before: 1,
      tokens_after: 5,
    });

    assert_eq!(memory.history().len(), 2);
    assert_eq!(memory.token_count(), 5);
  }
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  cd rust-ody && cargo test -p agent-rs --test context_memory_undo_compaction
  ```

  Expected failure: `error[E0599]: no method named `undo` found for struct `ContextMemory`` / `no method named `apply_compaction` found`.

- [ ] **Write the minimal implementation**

  在 `rust-ody/crates/agent-rs/src/context/memory.rs` 的 `impl ContextMemory` 中追加：

  ```rust
  pub fn undo(&mut self, count: i64) {
    if count <= 0 {
      return;
    }
    if self.history.is_empty() {
      return;
    }

    self.agent.record_log().log_record(AgentRecord::ContextUndo {
      time: None,
      count,
    });

    let target = count as usize;
    let mut removed_user_count = 0usize;
    let mut removed_messages: Vec<ContextMessage> = Vec::new();
    let mut stopped_at_boundary = false;

    for i in (0..self.history.len()).rev() {
      let message = self.history.remove(i);
      removed_messages.push(message.clone());
      self.agent.injection().on_context_message_removed(i);

      if i < self.token_count_covered_message_count {
        self.token_count_covered_message_count -= 1;
        self.token_count -= estimate_tokens_for_message(&message.message);
      }

      if is_real_user_prompt(&message) {
        removed_user_count += 1;
        if removed_user_count >= target {
          break;
        }
      }
    }

    self.agent.replay_builder().remove_last_messages(&removed_messages);
    self.open_steps.clear();
    self.pending_tool_result_ids.clear();
    self.deferred_messages.clear();
    self.agent.micro_compaction().reset(self.history.len());
    self.agent.status_emitter().emit_status_updated();

    let restoring = self.agent.record_log().restoring_time().is_some();
    if !restoring && (stopped_at_boundary || removed_user_count < target) {
      panic!("Nothing to undo in the active context.");
    }
  }

  pub fn apply_compaction(&mut self, summary: CompactionResult) {
    self.agent
      .record_log()
      .log_record(AgentRecord::ContextApplyCompaction {
        time: None,
        result: summary.clone(),
      });

    let compacted_count = summary.compacted_count as usize;
    let summary_message = ContextMessage {
      message: Message::assistant(
        vec![ContentPart::Text {
          text: summary.summary.clone(),
        }],
        vec![],
      ),
      origin: Some(PromptOrigin::CompactionSummary),
      is_error: None,
    };

    let mut new_history = vec![summary_message];
    new_history.extend(self.history.iter().skip(compacted_count).cloned());
    self.history = new_history;

    self.open_steps.clear();
    self.flush_deferred_messages_if_tool_exchange_closed();
    self.token_count = summary.tokens_after;
    self.token_count_covered_message_count = self.history.len();
    self.agent.micro_compaction().reset(0);
    self.agent
      .injection()
      .on_context_compacted(summary.compacted_count as usize);
    self.agent.status_emitter().emit_status_updated();
  }

  pub fn token_count_covered_message_count(&self) -> usize {
    self.token_count_covered_message_count
  }
  ```

  并在文件末尾追加辅助函数：

  ```rust
  fn is_real_user_prompt(message: &ContextMessage) -> bool {
    if message.message.role != Role::User {
      return false;
    }
    match &message.origin {
      None | Some(PromptOrigin::User) => true,
      Some(PromptOrigin::SkillActivation { trigger, .. }) => trigger == "user-slash",
      _ => false,
    }
  }
  ```

  将 `push_history` 等私有方法也暴露给 `flush_deferred_messages_if_tool_exchange_closed`（若 Task 6 中未放 `ContextMemory` impl 内，则现在加入）：

  ```rust
  fn flush_deferred_messages_if_tool_exchange_closed(&mut self) {
    if !self.pending_tool_result_ids.is_empty() || self.deferred_messages.is_empty() {
      return;
    }
    let messages = std::mem::take(&mut self.deferred_messages);
    self.push_history(messages);
  }
  ```

- [ ] **Run it and verify it PASSES**

  ```bash
  cd rust-ody && cargo test -p agent-rs --test context_memory_undo_compaction
  ```

  Expected: `test result: ok. 5 passed; 0 failed`.

- [ ] **Commit**

  ```bash
  git add rust-ody/crates/agent-rs/src/context/memory.rs \
         rust-ody/crates/agent-rs/tests/context_memory_undo_compaction.rs
  git commit -m "feat(agent-rs): ContextMemory undo and compaction"

---

### Task 8: Loop 事件、工具结果格式化与 deferred 消息

**Depends on:** Task 7

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/context/memory.rs`（追加 `append_loop_event`、工具格式化辅助函数）
- Create: `rust-ody/crates/agent-rs/tests/context_memory_loop_event.rs`

- [ ] **Write the failing test**

  ```rust
  // rust-ody/crates/agent-rs/tests/context_memory_loop_event.rs
  mod common;

  use agent_rs::context::ContextMemory;
  use agent_rs::records::nested::{
    ContextMessage, ExecutableToolErrorResult, ExecutableToolOutput, ExecutableToolResult,
    ExecutableToolSuccessResult, LoopRecordedEvent, PromptOrigin,
  };
  use common::TestAgent;
  use kosong_rs::message::{ContentPart, Message, Role, ToolCall};

  fn user_msg(text: &str) -> ContextMessage {
    ContextMessage {
      message: Message::user_text(text),
      origin: Some(PromptOrigin::User),
      is_error: None,
    }
  }

  fn assistant_with_call(id: &str) -> ContextMessage {
    ContextMessage {
      message: Message::assistant(
        vec![],
        vec![ToolCall {
          call_type: "function".into(),
          id: id.into(),
          name: "read".into(),
          arguments: Some("{}".into()),
          extras: None,
          stream_index: None,
        }],
      ),
      origin: None,
      is_error: None,
    }
  }

  #[test]
  fn append_loop_event_step_begin_end_tracks_open_steps_and_usage() {
    let agent = TestAgent::new();
    let mut memory = ContextMemory::new(&agent);

    memory.append_loop_event(LoopRecordedEvent::StepBegin {
      uuid: "s1".into(),
      turn_id: "t1".into(),
      step: 1,
    });
    assert!(memory.has_open_steps());
    assert_eq!(memory.history().len(), 1);

    memory.append_loop_event(LoopRecordedEvent::ContentPartEvent {
      uuid: "p1".into(),
      turn_id: "t1".into(),
      step: 1,
      step_uuid: "s1".into(),
      part: ContentPart::Text { text: "hi".into() },
    });

    memory.append_loop_event(LoopRecordedEvent::StepEnd {
      uuid: "s1".into(),
      turn_id: "t1".into(),
      step: 1,
      usage: Some(kosong_rs::usage::TokenUsage {
        input_cache_read: 1,
        input_cache_creation: 2,
        input_other: 3,
        output: 4,
      }),
      finish_reason: None,
      llm_first_token_latency_ms: None,
      llm_stream_duration_ms: None,
      provider_finish_reason: None,
      raw_finish_reason: None,
    });

    assert!(!memory.has_open_steps());
    assert_eq!(memory.token_count(), 10);
    assert_eq!(memory.token_count_covered_message_count(), 1);
  }

  #[test]
  fn append_loop_event_tool_call_and_result_closes_exchange() {
    let agent = TestAgent::new();
    let mut memory = ContextMemory::new(&agent);

    memory.append_loop_event(LoopRecordedEvent::StepBegin {
      uuid: "s1".into(),
      turn_id: "t1".into(),
      step: 1,
    });
    memory.append_loop_event(LoopRecordedEvent::ToolCallEvent {
      uuid: "tc1".into(),
      turn_id: "t1".into(),
      step: 1,
      step_uuid: "s1".into(),
      tool_call_id: "call_1".into(),
      name: "read".into(),
      args: serde_json::json!({"path":"README.md"}),
      description: None,
      display: None,
    });

    memory.append_message(user_msg("deferred until tool result"));

    memory.append_loop_event(LoopRecordedEvent::ToolResultEvent {
      parent_uuid: "s1".into(),
      tool_call_id: "call_1".into(),
      result: ExecutableToolResult::Success(ExecutableToolSuccessResult {
        output: ExecutableToolOutput::Text("ok".into()),
        is_error: None,
        stop_turn: None,
        message: None,
      }),
    });

    let history = memory.history();
    assert_eq!(history.len(), 3); // assistant step, tool result, deferred user
    assert_eq!(history[2].message.role, Role::User);
  }

  #[test]
  fn tool_result_error_formatting_prefixes_system_error() {
    let agent = TestAgent::new();
    let mut memory = ContextMemory::new(&agent);

    memory.append_loop_event(LoopRecordedEvent::StepBegin {
      uuid: "s1".into(),
      turn_id: "t1".into(),
      step: 1,
    });
    memory.append_loop_event(LoopRecordedEvent::ToolCallEvent {
      uuid: "tc1".into(),
      turn_id: "t1".into(),
      step: 1,
      step_uuid: "s1".into(),
      tool_call_id: "call_1".into(),
      name: "read".into(),
      args: serde_json::json!({}),
      description: None,
      display: None,
    });
    memory.append_loop_event(LoopRecordedEvent::ToolResultEvent {
      parent_uuid: "s1".into(),
      tool_call_id: "call_1".into(),
      result: ExecutableToolResult::Error(ExecutableToolErrorResult {
        output: ExecutableToolOutput::Text("file not found".into()),
        is_error: true,
        stop_turn: None,
        message: None,
      }),
    });

    let history = memory.history();
    assert_eq!(history[1].message.role, Role::Tool);
    let text = match &history[1].message.content[0] {
      ContentPart::Text { text } => text.as_str(),
      _ => panic!("expected text"),
    };
    assert!(text.starts_with("<system>ERROR: Tool execution failed.</system>"));
    assert!(text.contains("file not found"));
    assert_eq!(history[1].is_error, Some(true));
  }

  #[test]
  fn tool_result_empty_error_uses_combined_status() {
    let agent = TestAgent::new();
    let mut memory = ContextMemory::new(&agent);

    memory.append_loop_event(LoopRecordedEvent::StepBegin {
      uuid: "s1".into(),
      turn_id: "t1".into(),
      step: 1,
    });
    memory.append_loop_event(LoopRecordedEvent::ToolCallEvent {
      uuid: "tc1".into(),
      turn_id: "t1".into(),
      step: 1,
      step_uuid: "s1".into(),
      tool_call_id: "call_1".into(),
      name: "read".into(),
      args: serde_json::json!({}),
      description: None,
      display: None,
    });
    memory.append_loop_event(LoopRecordedEvent::ToolResultEvent {
      parent_uuid: "s1".into(),
      tool_call_id: "call_1".into(),
      result: ExecutableToolResult::Error(ExecutableToolErrorResult {
        output: ExecutableToolOutput::Text("".into()),
        is_error: true,
        stop_turn: None,
        message: None,
      }),
    });

    let history = memory.history();
    let text = match &history[1].message.content[0] {
      ContentPart::Text { text } => text.as_str(),
      _ => panic!("expected text"),
    };
    assert_eq!(text, "<system>ERROR: Tool execution failed. Tool output is empty.</system>");
  }

  #[test]
  fn reset_runtime_state_clears_open_tracking_without_touching_history() {
    let agent = TestAgent::new();
    let mut memory = ContextMemory::new(&agent);
    memory.append_loop_event(LoopRecordedEvent::StepBegin {
      uuid: "s1".into(),
      turn_id: "t1".into(),
      step: 1,
    });
    memory.append_message(user_msg("deferred"));

    memory.reset_runtime_state();

    assert!(!memory.has_open_steps());
    assert_eq!(memory.history().len(), 1);
  }
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  cd rust-ody && cargo test -p agent-rs --test context_memory_loop_event
  ```

  Expected failure: `error[E0599]: no method named `append_loop_event` found for struct `ContextMemory``.

- [ ] **Write the minimal implementation**

  在 `rust-ody/crates/agent-rs/src/context/memory.rs` 的 `impl ContextMemory` 中追加：

  ```rust
  pub fn append_loop_event(&mut self, event: LoopRecordedEvent) {
    self.agent
      .record_log()
      .log_record(AgentRecord::ContextAppendLoopEvent {
        time: None,
        event: event.clone(),
      });

    match event {
      LoopRecordedEvent::StepBegin { uuid, .. } => {
        let message = ContextMessage {
          message: Message::assistant(vec![], vec![]),
          origin: None,
          is_error: None,
        };
        self.history.push(message.clone());
        self.open_steps.insert(uuid, message);
      }
      LoopRecordedEvent::StepEnd { uuid, usage, .. } => {
        let open_step = self.open_steps.remove(&uuid);
        if let Some(ref usage) = usage {
          let open_step_index = open_step
            .as_ref()
            .and_then(|step| self.history.iter().position(|m| m == step))
            .unwrap_or(self.history.len());
          self.token_count = usage.input_cache_read
            + usage.input_cache_creation
            + usage.input_other
            + usage.output;
          self.token_count_covered_message_count = open_step_index + 1;
        }
        self.flush_deferred_messages_if_tool_exchange_closed();
        self.agent.context_switch_flusher().flush_deferred_context_switch();
      }
      LoopRecordedEvent::ContentPartEvent { step_uuid, part, .. } => {
        let open_step = self
          .open_steps
          .get_mut(&step_uuid)
          .expect(&format!("content.part for unknown step_uuid '{}'", step_uuid));
        open_step.message.content.push(part);
      }
      LoopRecordedEvent::ToolCallEvent {
        step_uuid,
        tool_call_id,
        name,
        args,
        ..
      } => {
        let open_step = self
          .open_steps
          .get_mut(&step_uuid)
          .expect(&format!("tool.call for unknown step_uuid '{}'", step_uuid));
        open_step.message.tool_calls.push(ToolCall {
          call_type: "function".into(),
          id: tool_call_id.clone(),
          name,
          arguments: Some(args.to_string()),
          extras: None,
          stream_index: None,
        });
        self.pending_tool_result_ids.insert(tool_call_id);
      }
      LoopRecordedEvent::ToolResultEvent { tool_call_id, result } => {
        let output = tool_result_output_for_model(&result);
        let message = tool_message(&tool_call_id, output);
        let is_error = match &result {
          ExecutableToolResult::Success(r) => r.is_error.unwrap_or(false),
          ExecutableToolResult::Error(r) => r.is_error,
        };
        self.push_history(vec![ContextMessage {
          message,
          origin: None,
          is_error: Some(is_error),
        }]);
        self.pending_tool_result_ids.remove(&tool_call_id);
        self.flush_deferred_messages_if_tool_exchange_closed();
      }
    }
  }
  ```

  在文件末尾追加工具格式化和 `tool_message` 辅助函数：

  ```rust
  fn tool_message(tool_call_id: &str, output: ExecutableToolOutput) -> Message {
    let content = match output {
      ExecutableToolOutput::Text(text) => vec![ContentPart::Text { text }],
      ExecutableToolOutput::Parts(parts) => parts,
    };
    Message {
      role: Role::Tool,
      name: None,
      content,
      tool_calls: vec![],
      tool_call_id: Some(tool_call_id.into()),
      partial: None,
    }
  }

  fn tool_result_output_for_model(result: &ExecutableToolResult) -> ExecutableToolOutput {
    let (output, is_error) = match result {
      ExecutableToolResult::Success(r) => (&r.output, r.is_error.unwrap_or(false)),
      ExecutableToolResult::Error(r) => (&r.output, r.is_error),
    };

    match output {
      ExecutableToolOutput::Text(text) => {
        if is_error {
          if text.is_empty() {
            return ExecutableToolOutput::Text(TOOL_EMPTY_ERROR_STATUS.into());
          }
          if text.trim_start().starts_with("<system>ERROR:") {
            return ExecutableToolOutput::Text(text.clone());
          }
          return ExecutableToolOutput::Text(format!("{}\n{}", TOOL_ERROR_STATUS, text));
        }
        if is_empty_output_text(text) {
          return ExecutableToolOutput::Text(TOOL_EMPTY_STATUS.into());
        }
        ExecutableToolOutput::Text(text.clone())
      }
      ExecutableToolOutput::Parts(parts) => {
        if parts.is_empty() {
          let status = if is_error {
            TOOL_EMPTY_ERROR_STATUS
          } else {
            TOOL_EMPTY_STATUS
          };
          return ExecutableToolOutput::Text(status.into());
        }
        if is_error {
          let mut content = vec![ContentPart::Text {
            text: TOOL_ERROR_STATUS.into(),
          }];
          content.extend(parts.clone());
          return ExecutableToolOutput::Parts(content);
        }
        ExecutableToolOutput::Parts(parts.clone())
      }
    }
  }

  fn is_empty_output_text(output: &str) -> bool {
    output.is_empty() || output.trim() == TOOL_OUTPUT_EMPTY_TEXT
  }
  ```

- [ ] **Run it and verify it PASSES**

  ```bash
  cd rust-ody && cargo test -p agent-rs --test context_memory_loop_event
  ```

  Expected: `test result: ok. 5 passed; 0 failed`.

- [ ] **Commit**

  ```bash
  git add rust-ody/crates/agent-rs/src/context/memory.rs \
         rust-ody/crates/agent-rs/tests/context_memory_loop_event.rs
  git commit -m "feat(agent-rs): ContextMemory loop events, tool formatting and deferred messages"

---

## Local Self-Review

- [ ] 1. Spec-coverage（本 part）：Task 6 覆盖构造/追加/清空/投影/基本 token 记账；Task 7 覆盖 `undo` 与 `apply_compaction`；Task 8 覆盖 `append_loop_event`、工具结果格式化、deferred 消息与 `reset_runtime_state`。无 GAP。
- [ ] 2. Placeholder scan：所有 trait 方法、状态机分支、格式化常量均已给出真实 Rust 代码；无 TODO/TBD。
- [ ] 3. No phantom tasks：Task 6 产出 `ContextMemory` 骨架、test double 公共模块、6 个 L1 测试；Task 7 产出 `undo`/`apply_compaction` 与 5 个测试；Task 8 产出 loop 事件/工具格式化/deferred 与 5 个测试。
- [ ] 4. Dependency soundness：Task 6 依赖 `types.md`/`projector.md`/`tokens.md`；Task 7 依赖 Task 6；Task 8 依赖 Task 7。无反向依赖。
- [ ] 5. Caller & build soundness：Task 6 修改 `RecordLog` 共享签名，同步更新 `tests/context_host_traits.rs` 中的 test double；Task 6/7/8 结束时分别运行 `cargo check -p agent-rs --workspace --tests` 确保全绿。新增 `ContextMemory` 为新增 API，无现有调用方需要更新。
- [ ] 6. Test-the-risk：
  - `undo` 测试验证只移除真实 user prompt、跳过 injection、在 compaction_summary 边界停止，并断言 `on_context_message_removed` 回调。
  - `apply_compaction` 测试验证 summary 消息 prepend、token_count 与 `token_count_covered_message_count` 重置、compaction 回调参数。
  - `append_loop_event` 测试验证 step begin/end 的 open-step 状态、usage 更新、content/tool-call 追加、deferred 消息在 tool exchange 关闭后 FIFO 刷新。
  - 工具结果格式化测试直接断言 `TOOL_ERROR_STATUS` / `TOOL_EMPTY_ERROR_STATUS` 常量输出，覆盖 error + 空输出组合。
  - must-survive 输入：非 user-slash 的 skill_activation 不被 undo 视为真实 user prompt；无 tool_call_id 的 tool 消息不受孤儿删除影响（由 `projector.md` 保证）。
- [ ] 7. Type一致性：`ContextMemory` 使用 `&dyn ContextAgent` 与 `types.md` 的 `ContextAgent` trait 一致；`AgentContextData`/`USER_PROMPT_ORIGIN`/`PromptOrigin`/`ContextMessage` 复用 `types.md` 定义；工具格式化常量与 TS 逐字对齐；`LoopRecordedEvent`/`CompactionResult` 复用 `records::nested` 定义。
  ```
  ```
  ```
