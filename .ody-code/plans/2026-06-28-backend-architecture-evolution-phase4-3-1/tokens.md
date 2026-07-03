# Part 3 — Token 估算

**Goal:** 在 Rust 中实现与 TS `utils/tokens.ts` 逐值一致的字符启发式 token 估算，支撑 `ContextMemory::token_count_with_pending()` 和微压缩的 token 阈值判断。

**Architecture:** `tokens.rs` 为纯函数模块，仅依赖 `kosong-rs::message::{ContentPart, Message, Role}`，与 `ContextMemory` 解耦，便于 L1 golden 对照。估算规则：ASCII 字符按 4 字符 ≈ 1 token 向上取整，非 ASCII 字符按 1 字符 ≈ 1 token。

**Tech Stack:** Rust 2021, `kosong-rs::message`。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File | Responsibility |
|---|---|
| `rust-ody/crates/agent-rs/src/context/tokens.rs` | `estimate_tokens` 族纯函数 |
| `rust-ody/crates/agent-rs/tests/context_tokens.rs` | L1 单元测试 |

---

## Dependency Overview

```text
[types.md Task 1-2]
        │
        ▼
[tokens.md Task 4]
        │
        ▼
[memory.md Task 6-8]
```

- 本 part 不依赖 projector/notification，可与 `projector.md`/`notification.md` 并行。

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| Rust `char` 迭代与 TS `for...of` 在 surrogate pair 上计数差异 | 均按 Unicode scalar value（code point）计数；Rust `.chars()` 与 JS `for...of` 行为一致 |
| `Role` 转字符串与 TS 角色名不一致 | 显式映射为 `"system"/"user"/"assistant"/"tool"` |
| tool-call `arguments` 为 `None` 时取值 | 与 `JSON.stringify(null)` 对齐，按 `"null"` 估算 |

---

### Task 4: 实现 `estimate_tokens` 族

**Depends on:** `types.md` Task 2

**Files:**
- Create: `rust-ody/crates/agent-rs/src/context/tokens.rs`
- Modify: `rust-ody/crates/agent-rs/src/context/mod.rs`（追加 `pub mod tokens;` 与 re-export）
- Create: `rust-ody/crates/agent-rs/tests/context_tokens.rs`

- [ ] **Write the failing test**

  ```rust
  // rust-ody/crates/agent-rs/tests/context_tokens.rs
  use agent_rs::context::tokens::{
    estimate_tokens, estimate_tokens_for_content_part, estimate_tokens_for_content_parts,
    estimate_tokens_for_message, estimate_tokens_for_messages,
  };
  use kosong_rs::message::{ContentPart, Message, Role, ToolCall};

  #[test]
  fn estimate_ascii_tokens_ceils_division_by_four() {
    // "hello" = 5 ASCII chars -> ceil(5/4) = 2
    assert_eq!(estimate_tokens("hello"), 2);
    assert_eq!(estimate_tokens(""), 0);
    assert_eq!(estimate_tokens("abcd"), 1);
    assert_eq!(estimate_tokens("abcde"), 2);
  }

  #[test]
  fn estimate_non_ascii_counts_one_per_char() {
    // "你好" = 2 non-ASCII chars -> 2 tokens
    assert_eq!(estimate_tokens("你好"), 2);
    // "a你b" = 2 ASCII + 1 non-ASCII -> ceil(2/4) + 1 = 1 + 1 = 2
    assert_eq!(estimate_tokens("a你b"), 2);
  }

  #[test]
  fn estimate_message_counts_role_content_and_tool_calls() {
    let message = Message {
      role: Role::User,
      name: None,
      content: vec![ContentPart::Text { text: "hello".into() }],
      tool_calls: vec![ToolCall {
        call_type: "function".into(),
        id: "call_1".into(),
        name: "ToolName".into(),
        arguments: Some("{\"x\":1}".into()),
        extras: None,
        stream_index: None,
      }],
      tool_call_id: None,
      partial: None,
    };
    // role "user" = 1
    // text "hello" = 2
    // tool name "ToolName" = 2 (8 ASCII -> ceil(8/4)=2)
    // arguments "{\"x\":1}" = 7 ASCII -> ceil(7/4)=2
    assert_eq!(estimate_tokens_for_message(&message), 1 + 2 + 2 + 2);
  }

  #[test]
  fn estimate_messages_sums_individual_messages() {
    let m1 = Message::user(vec![ContentPart::Text { text: "hello".into() }]);
    let m2 = Message::assistant(vec![ContentPart::Text { text: "world".into() }], vec![]);
    // user(hello)=1+2=3, assistant(world)=1+2=3
    assert_eq!(estimate_tokens_for_messages(&[m1, m2]), 6);
  }

  #[test]
  fn estimate_think_part_counts_think_text() {
    let part = ContentPart::Think {
      think: "think".into(),
      encrypted: None,
    };
    assert_eq!(estimate_tokens_for_content_part(&part), 2);
  }

  #[test]
  fn estimate_non_text_part_is_zero() {
    let part = ContentPart::ImageUrl {
      image_url: kosong_rs::message::UrlPayload { url: "http://x".into(), id: None },
    };
    assert_eq!(estimate_tokens_for_content_part(&part), 0);
  }

  #[test]
  fn estimate_tool_call_with_none_arguments_uses_null() {
    let message = Message {
      role: Role::Assistant,
      name: None,
      content: vec![],
      tool_calls: vec![ToolCall {
        call_type: "function".into(),
        id: "c".into(),
        name: "N".into(),
        arguments: None,
        extras: None,
        stream_index: None,
      }],
      tool_call_id: None,
      partial: None,
    };
    // role "assistant" = 1
    // name "N" = 1
    // arguments None -> "null" = 1
    assert_eq!(estimate_tokens_for_message(&message), 1 + 1 + 1);
  }
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  cd rust-ody && cargo test -p agent-rs --test context_tokens
  ```

  Expected failure: `error[E0433]: failed to resolve: use of undeclared crate or module `tokens`` / `cannot find function `estimate_tokens` in module `agent_rs::context::tokens``.

- [ ] **Write the minimal implementation**

  1. Create `rust-ody/crates/agent-rs/src/context/tokens.rs`：

     ```rust
     use kosong_rs::message::{ContentPart, Message, Role};

     /// 字符启发式 token 估算。
     /// - ASCII 字符：ceil(count / 4)
     /// - 非 ASCII 字符：count
     pub fn estimate_tokens(text: &str) -> i64 {
       let mut ascii_count = 0i64;
       let mut non_ascii_count = 0i64;
       for ch in text.chars() {
         if (ch as u32) <= 127 {
           ascii_count += 1;
         } else {
           non_ascii_count += 1;
         }
       }
       ((ascii_count + 3) / 4) + non_ascii_count
     }

     pub fn estimate_tokens_for_messages(messages: &[Message]) -> i64 {
       messages.iter().map(estimate_tokens_for_message).sum()
     }

     pub fn estimate_tokens_for_message(message: &Message) -> i64 {
       let mut total = estimate_tokens(role_token_text(message.role));
       total += estimate_tokens_for_content_parts(&message.content);
       for call in &message.tool_calls {
         total += estimate_tokens(&call.name);
         total += estimate_tokens(call.arguments.as_deref().unwrap_or("null"));
       }
       total
     }

     pub fn estimate_tokens_for_content_parts(parts: &[ContentPart]) -> i64 {
       parts.iter().map(estimate_tokens_for_content_part).sum()
     }

     pub fn estimate_tokens_for_content_part(part: &ContentPart) -> i64 {
       match part {
         ContentPart::Text { text } => estimate_tokens(text),
         ContentPart::Think { think, .. } => estimate_tokens(think),
         _ => 0,
       }
     }

     fn role_token_text(role: Role) -> &'static str {
       match role {
         Role::System => "system",
         Role::User => "user",
         Role::Assistant => "assistant",
         Role::Tool => "tool",
       }
     }
     ```

  2. Modify `rust-ody/crates/agent-rs/src/context/mod.rs` 追加：

     ```rust
     pub mod tokens;
     pub use tokens::{
       estimate_tokens, estimate_tokens_for_content_part, estimate_tokens_for_content_parts,
       estimate_tokens_for_message, estimate_tokens_for_messages,
     };
     ```

     即完整 `mod.rs`：

     ```rust
     pub mod memory;
     pub mod notification_xml;
     pub mod projector;
     pub mod tokens;
     pub mod types;

     pub use projector::{drop_orphan_tool_results, project};
     pub use tokens::{
       estimate_tokens, estimate_tokens_for_content_part, estimate_tokens_for_content_parts,
       estimate_tokens_for_message, estimate_tokens_for_messages,
     };
     pub use types::*;
     ```

- [ ] **Run it and verify it PASSES**

  ```bash
  cd rust-ody && cargo test -p agent-rs --test context_tokens
  ```

  Expected: `test result: ok. 7 passed; 0 failed`.

- [ ] **Commit**

  ```bash
  git add rust-ody/crates/agent-rs/src/context/tokens.rs \
         rust-ody/crates/agent-rs/src/context/mod.rs \
         rust-ody/crates/agent-rs/tests/context_tokens.rs
  git commit -m "feat(agent-rs): context token estimator"
  ```

---

## Local Self-Review

- [ ] 1. Spec-coverage：本 part 覆盖 roadmap 4.3.1.4（token 计数），无 GAP。
- [ ] 2. Placeholder scan：无 TODO/TBD；所有函数完整实现。
- [ ] 3. No phantom tasks：Task 4 产出 token 估算实现 + 7 个 L1 测试。
- [ ] 4. Dependency soundness：仅依赖 `types.md` 间接引入的 `kosong-rs::message` 类型，无反向依赖。
- [ ] 5. Caller & build soundness：新增公开 API，无现有调用方需要更新；Task 4 结束时运行 `cargo check -p agent-rs --workspace --tests` 全绿。
- [ ] 6. Test-the-risk：
  - ASCII 向上取整边界（0/4/5/8）已覆盖。
  - 非 ASCII 与混合字符已覆盖，验证 `(ascii+3)/4 + non_ascii` 公式。
  - message 估算追踪 role 常量（"user"=1 token）、text、tool name、arguments；`None` arguments 按 "null" 处理，与 `JSON.stringify(null)` 对齐。
  - must-survive 输入：think part、image_url part 均按预期计数（think 计费，image 不计）。
- [ ] 7. Type一致性：`estimate_tokens` 返回 `i64`，与 TS `number` 对应；`Role` 转字符串显式映射，避免依赖 `Display` 实现导致的命名漂移。
