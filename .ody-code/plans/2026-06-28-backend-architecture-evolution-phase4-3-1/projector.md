# Part 2 — Projection 与孤儿 tool-result 治愈

**Goal:** 在 Rust 中实现与 TS `project()`/`dropOrphanToolResults()` 逐值对齐的纯函数投影层：过滤 partial/空 assistant 占位、合并相邻真实 user 消息、在完整历史边界上丢弃没有前导 tool-call 的孤儿 tool result。

**Architecture:** `projector.rs` 只依赖 `ContextMessage`/`Message`/`ContentPart` 类型，不依赖 host trait，因此可独立对照。`project()` 返回 `Vec<Message>`（已剥离 origin/isError），`drop_orphan_tool_results()` 在 `ContextMemory::messages()` 中于完整历史上调用，是发送给 provider 前的最后一道治愈防线。

**Tech Stack:** Rust 2021, `kosong-rs::message`。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File | Responsibility |
|---|---|
| `rust-ody/crates/agent-rs/src/context/projector.rs` | `project()` / `drop_orphan_tool_results()` |
| `rust-ody/crates/agent-rs/tests/context_projector.rs` | L1 单元测试 |

---

## Dependency Overview

```text
[types.md Task 1-2]
        │
        ▼
[projector.md Task 3]
        │
        ▼
[memory.md Task 6-8]
```

- 本 part 仅依赖 `types.md` 输出的 `ContextMessage`/`PromptOrigin`。
- `memory.md` 的 `messages()` 会调用本 part 的函数，但本 part 不依赖 memory。

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| 合并相邻 user 消息时非 text part 的顺序 | 严格复刻 TS：先合并 text，再依次追加 a、b 中的非 text part |
| `drop_orphan_tool_results` 在 windowed slice 上误删 | 文档与测试强调只应在完整历史或从 0 开始的前缀上调用；`project()` 不调用它 |
| 孤儿治愈时把无 `toolCallId` 的 tool 消息误删 | 显式保留 `tool_call_id` 为 `None` 的消息 |

---

### Task 3: 实现 `project()` 与 `drop_orphan_tool_results()`

**Depends on:** `types.md` Task 2

**Files:**
- Create: `rust-ody/crates/agent-rs/src/context/projector.rs`
- Modify: `rust-ody/crates/agent-rs/src/context/mod.rs`（追加 `pub mod projector;` 与 re-export）
- Create: `rust-ody/crates/agent-rs/tests/context_projector.rs`

- [ ] **Write the failing test**

  ```rust
  // rust-ody/crates/agent-rs/tests/context_projector.rs
  use agent_rs::context::projector::{drop_orphan_tool_results, project};
  use agent_rs::context::types::{ContextMessage, PromptOrigin};
  use kosong_rs::message::{ContentPart, Message, Role, ToolCall};

  fn user(text: &str) -> ContextMessage {
    ContextMessage {
      message: Message::user(vec![ContentPart::Text { text: text.into() }]),
      origin: Some(PromptOrigin::User),
      is_error: None,
    }
  }

  fn system_reminder(text: &str) -> ContextMessage {
    ContextMessage {
      message: Message::user(vec![ContentPart::Text { text: text.into() }]),
      origin: Some(PromptOrigin::Injection { variant: "host".into() }),
      is_error: None,
    }
  }

  fn assistant_text(text: &str) -> ContextMessage {
    ContextMessage {
      message: Message::assistant(vec![ContentPart::Text { text: text.into() }], vec![]),
      origin: None,
      is_error: None,
    }
  }

  fn assistant_with_call(id: &str, text: &str) -> ContextMessage {
    ContextMessage {
      message: Message::assistant(
        if text.is_empty() { vec![] } else { vec![ContentPart::Text { text: text.into() }] },
        vec![ToolCall {
          call_type: "function".into(),
          id: id.into(),
          name: "SomeTool".into(),
          arguments: Some("{}".into()),
          extras: None,
          stream_index: None,
        }],
      ),
      origin: None,
      is_error: None,
    }
  }

  fn tool_result(id: &str, text: &str) -> ContextMessage {
    ContextMessage {
      message: Message {
        role: Role::Tool,
        name: None,
        content: vec![ContentPart::Text { text: text.into() }],
        tool_calls: vec![],
        tool_call_id: Some(id.into()),
        partial: None,
      },
      origin: None,
      is_error: None,
    }
  }

  fn project_and_heal(history: Vec<ContextMessage>) -> Vec<Message> {
    drop_orphan_tool_results(project(&history))
  }

  #[test]
  fn drops_orphan_tool_result() {
    let history = vec![tool_result("orphan", "orphaned"), user("continue")];
    let projected = project_and_heal(history);
    assert!(!projected.iter().any(|m| m.role == Role::Tool));
    assert_eq!(projected.iter().map(|m| m.role).collect::<Vec<_>>(), vec![Role::User]);
  }

  #[test]
  fn keeps_tool_result_with_preceding_call() {
    let history = vec![assistant_with_call("ok", ""), tool_result("ok", "ok"), user("next")];
    let projected = project_and_heal(history);
    assert_eq!(projected.iter().map(|m| m.role).collect::<Vec<_>>(), vec![Role::Assistant, Role::Tool, Role::User]);
    assert_eq!(projected[1].tool_call_id, Some("ok".into()));
  }

  #[test]
  fn drops_only_orphan_and_keeps_valid_exchange() {
    let history = vec![
      tool_result("orphan", "orphaned result at head"),
      assistant_with_call("ok", "calling tool"),
      tool_result("ok", "ok"),
    ];
    let projected = project_and_heal(history);
    assert_eq!(projected.iter().map(|m| m.role).collect::<Vec<_>>(), vec![Role::Assistant, Role::Tool]);
    assert_eq!(projected[1].tool_call_id, Some("ok".into()));
  }

  #[test]
  fn drops_tool_result_appearing_before_its_call() {
    let history = vec![tool_result("late", "too early"), assistant_with_call("late", "")];
    let projected = project_and_heal(history);
    assert_eq!(projected.iter().map(|m| m.role).collect::<Vec<_>>(), vec![Role::Assistant]);
  }

  #[test]
  fn preserves_tool_message_without_tool_call_id() {
    let history = vec![ContextMessage {
      message: Message {
        role: Role::Tool,
        name: None,
        content: vec![ContentPart::Text { text: "tool-like output".into() }],
        tool_calls: vec![],
        tool_call_id: None,
        partial: None,
      },
      origin: None,
      is_error: None,
    }];
    let projected = project_and_heal(history);
    assert_eq!(projected.iter().map(|m| m.role).collect::<Vec<_>>(), vec![Role::Tool]);
  }

  #[test]
  fn project_alone_does_not_heal_windowed_slice() {
    let slice = vec![tool_result("outside", "result only")];
    let projected = project(&slice);
    assert_eq!(projected.iter().map(|m| m.role).collect::<Vec<_>>(), vec![Role::Tool]);
  }

  #[test]
  fn merges_adjacent_real_user_messages() {
    let history = vec![user("hello"), user("world")];
    let projected = project(&history);
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].role, Role::User);
    assert_eq!(projected[0].content, vec![ContentPart::Text { text: "hello\n\nworld".into() }]);
  }

  #[test]
  fn does_not_merge_user_with_non_user_origin() {
    let history = vec![user("hello"), system_reminder("reminder")];
    let projected = project(&history);
    assert_eq!(projected.len(), 2);
  }

  #[test]
  fn merges_text_and_appends_non_text_parts() {
    let history = vec![
      ContextMessage {
        message: Message::user(vec![
          ContentPart::Text { text: "a".into() },
          ContentPart::ImageUrl { image_url: kosong_rs::message::UrlPayload { url: "u1".into(), id: None } },
        ]),
        origin: Some(PromptOrigin::User),
        is_error: None,
      },
      ContextMessage {
        message: Message::user(vec![
          ContentPart::Text { text: "b".into() },
          ContentPart::ImageUrl { image_url: kosong_rs::message::UrlPayload { url: "u2".into(), id: None } },
        ]),
        origin: Some(PromptOrigin::User),
        is_error: None,
      },
    ];
    let projected = project(&history);
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].content.len(), 3);
    assert_eq!(projected[0].content[0], ContentPart::Text { text: "a\n\nb".into() });
  }

  #[test]
  fn strips_origin_and_is_error_from_projected_messages() {
    let history = vec![user("x")];
    let projected = project(&history);
    assert!(projected[0].name.is_none());
    // The Message type does not carry origin/isError, so compilation proves stripping.
  }

  #[test]
  fn filters_partial_and_empty_assistant_placeholders() {
    let history = vec![
      ContextMessage {
        message: Message::assistant(vec![], vec![]),
        origin: None,
        is_error: None,
      },
      ContextMessage {
        message: Message {
          role: Role::Assistant,
          name: None,
          content: vec![],
          tool_calls: vec![],
          tool_call_id: None,
          partial: Some(true),
        },
        origin: None,
        is_error: None,
      },
      user("real"),
    ];
    let projected = project(&history);
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0].role, Role::User);
  }
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  cd rust-ody && cargo test -p agent-rs --test context_projector
  ```

  Expected failure: `error[E0433]: failed to resolve: use of undeclared crate or module `projector`` / `cannot find function `project` in module `agent_rs::context::projector``.

- [ ] **Write the minimal implementation**

  1. Create `rust-ody/crates/agent-rs/src/context/projector.rs`:

     ```rust
     use kosong_rs::message::{ContentPart, Message, Role, TextPart};

     use crate::context::types::{ContextMessage, PromptOrigin};

     /// 将内部 history 转换为 provider 可见的消息序列。
     /// 过滤 partial/空 assistant 占位，并合并相邻的真实 user 消息。
     pub fn project(history: &[ContextMessage]) -> Vec<Message> {
       let usable: Vec<&ContextMessage> = history
         .iter()
         .filter(|message| {
           let m = &message.message;
           !m.partial.unwrap_or(false)
             && !(m.role == Role::Assistant && m.content.is_empty() && m.tool_calls.is_empty())
         })
         .collect();
       merge_adjacent_user_messages(&usable)
         .into_iter()
         .map(strip_context_metadata)
         .collect()
     }

     /// 丢弃没有前导 assistant tool-call 的孤儿 tool result。
     /// 只应在完整历史或从 0 开始的前缀上调用。
     pub fn drop_orphan_tool_results(messages: Vec<Message>) -> Vec<Message> {
       let mut seen_call_ids = std::collections::HashSet::new();
       let mut out = Vec::with_capacity(messages.len());
       for message in messages {
         if message.role == Role::Assistant {
           for call in &message.tool_calls {
             seen_call_ids.insert(call.id.clone());
           }
           out.push(message);
           continue;
         }
         if message.role == Role::Tool {
           if let Some(ref tool_call_id) = message.tool_call_id {
             if !seen_call_ids.contains(tool_call_id) {
               continue;
             }
           }
         }
         out.push(message);
       }
       out
     }

     fn merge_adjacent_user_messages(history: &[&ContextMessage]) -> Vec<ContextMessage> {
       let mut out: Vec<ContextMessage> = Vec::with_capacity(history.len());
       for message in history {
         if let Some(previous) = out.last_mut() {
           if can_merge_user_message(message) && can_merge_user_message(previous) {
             *previous = merge_two_user_messages(previous, message);
             continue;
           }
         }
         out.push((*message).clone());
       }
       out
     }

     fn can_merge_user_message(message: &ContextMessage) -> bool {
       message.message.role == Role::User && message.origin == Some(PromptOrigin::User)
     }

     fn merge_two_user_messages(a: &ContextMessage, b: &ContextMessage) -> ContextMessage {
       let a_text = extract_text_only(&a.message);
       let b_text = extract_text_only(&b.message);
       let non_text_parts: Vec<ContentPart> = a
         .message
         .content
         .iter()
         .chain(b.message.content.iter())
         .filter(|p| !matches!(p, ContentPart::Text { .. }))
         .cloned()
         .collect();
       let merged_text = ContentPart::Text {
         text: format!("{}\n\n{}", a_text, b_text),
       };
       let mut content = vec![merged_text];
       content.extend(non_text_parts);
       ContextMessage {
         message: Message {
           role: Role::User,
           name: None,
           content,
           tool_calls: vec![],
           tool_call_id: None,
           partial: None,
         },
         origin: a.origin.clone(),
         is_error: None,
       }
     }

     fn extract_text_only(message: &Message) -> String {
       message
         .content
         .iter()
         .filter_map(|p| match p {
           ContentPart::Text { text } => Some(text.as_str()),
           _ => None,
         })
         .collect()
     }

     fn strip_context_metadata(message: ContextMessage) -> Message {
       message.message
     }
     ```

  2. Modify `rust-ody/crates/agent-rs/src/context/mod.rs` 追加：

     ```rust
     pub mod projector;
     // ... existing modules ...
     pub use projector::{drop_orphan_tool_results, project};
     ```

     即：

     ```rust
     pub mod memory;
     pub mod notification_xml;
     pub mod projector;
     pub mod tokens;
     pub mod types;

     pub use projector::{drop_orphan_tool_results, project};
     pub use types::*;
     ```

- [ ] **Run it and verify it PASSES**

  ```bash
  cd rust-ody && cargo test -p agent-rs --test context_projector
  ```

  Expected: `test result: ok. 11 passed; 0 failed`.

- [ ] **Commit**

  ```bash
  git add rust-ody/crates/agent-rs/src/context/projector.rs \
         rust-ody/crates/agent-rs/src/context/mod.rs \
         rust-ody/crates/agent-rs/tests/context_projector.rs
  git commit -m "feat(agent-rs): context projector and orphan tool-result guard"
  ```

---

## Local Self-Review

- [ ] 1. Spec-coverage：本 part 覆盖 roadmap 4.3.1.3（`project()` 与 `dropOrphanToolResults()`），无 GAP。
- [ ] 2. Placeholder scan：所有函数实现完整，无 TODO/TBD；`drop_orphan_tool_results` 显式处理 `tool_call_id` 为 `None` 的情况。
- [ ] 3. No phantom tasks：Task 3 产出 projector 实现 + 11 个 L1 测试。
- [ ] 4. Dependency soundness：仅依赖 `types.md` 已定义的 `ContextMessage`/`PromptOrigin`。
- [ ] 5. Caller & build soundness：新增 `project()`/`drop_orphan_tool_results()` 为新增公开 API，无现有调用方需要更新；Task 3 结束时运行 `cargo check -p agent-rs --workspace --tests` 全绿。
- [ ] 6. Test-the-risk：
  - 孤儿删除：构造 4 种孤儿输入（无 call、出现在 call 前、有效交换中的孤儿、无 toolCallId），断言只有孤儿被删。
  - 相邻合并：断言 "hello\n\nworld" 文本与非 text part 顺序，防止合并规则漂移。
  - must-survive 输入：有效 multi-call exchange、带 call id 的 tool result、无 toolCallId 的 tool 消息均保留。
- [ ] 7. Type consistency：`project()` 返回 `Vec<Message>`，`drop_orphan_tool_results()` 输入/输出均为 `Vec<Message>`，与 TS 函数签名对应；`ContextMessage` 沿用 `types.md` 定义。
