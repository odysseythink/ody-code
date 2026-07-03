# Part 4 — Notification XML 渲染

**Goal:** 在 Rust 中实现与 TS `agent/context/notification-xml.ts` 逐字节对齐的 `<notification>` / `<task-notification>` XML 渲染，供 `ContextMemory` 将后台任务与 cron 事件注入为 user message。

**Architecture:** `notification_xml.rs` 为纯函数模块，输入使用 `serde_json::Map<String, Value>` 以镜像 TS `Record<string, unknown>`；输出严格按 TS 顺序拼接行，attribute 转义规则一致（`&` → `&amp;`，`"` → `&quot;`）。

**Tech Stack:** Rust 2021, `serde_json`。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File | Responsibility |
|---|---|
| `rust-ody/crates/agent-rs/src/context/notification_xml.rs` | `render_notification_xml()` 与 attribute 转义 |
| `rust-ody/crates/agent-rs/tests/context_notification_xml.rs` | L1 单元测试 |

---

## Dependency Overview

```text
[types.md Task 1-2]
        │
        ▼
[notification.md Task 5]
        │
        ▼
[memory.md Task 6-8]
```

- 本 part 与 `projector.md`/`tokens.md` 并行，仅依赖 `types.md`。

---

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| attribute 转义与 TS 不完全一致（TS 只转义 `&"`） | 严格复刻 `escapeXmlAttr`：仅替换 `&` 和 `"` |
| `tail_output` 截断在 UTF-8 多字节字符边界出错 | 按 `char` 截断而非 byte，避免 panic |
| 输出行的空行/顺序与 TS 不一致 | 按 TS 代码逐行 push，join 用 `\n` |

---

### Task 5: 实现 `render_notification_xml()`

**Depends on:** `types.md` Task 2

**Files:**
- Create: `rust-ody/crates/agent-rs/src/context/notification_xml.rs`
- Modify: `rust-ody/crates/agent-rs/src/context/mod.rs`（追加 `pub mod notification_xml;` 与 re-export）
- Create: `rust-ody/crates/agent-rs/tests/context_notification_xml.rs`

- [ ] **Write the failing test**

  ```rust
  // rust-ody/crates/agent-rs/tests/context_notification_xml.rs
  use std::collections::HashMap;

  use agent_rs::context::notification_xml::render_notification_xml;
  use serde_json::{json, Map, Value};

  fn data_from_json(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
  }

  #[test]
  fn renders_task_notification_with_escaped_attributes_and_bounded_tail() {
    let data = data_from_json(json!({
      "id": "task-1",
      "category": "task",
      "type": "terminated",
      "source_kind": "background_task",
      "source_id": "bg-1",
      "title": "Task done",
      "severity": "info",
      "body": "Body line",
      "tail_output": "line1\nline2\n...\nline21"
    }));
    let xml = render_notification_xml(&data);
    assert!(xml.starts_with("<notification id=\"task-1\" category=\"task\" type=\"terminated\" source_kind=\"background_task\" source_id=\"bg-1\">"));
    assert!(xml.contains("Title: Task done"));
    assert!(xml.contains("Severity: info"));
    assert!(xml.contains("Body line"));
    assert!(xml.contains("<task-notification>"));
    // tail is last 20 lines and <= 3000 chars
    assert!(!xml.contains("line1\n"));
    assert!(xml.contains("line21"));
    assert!(xml.ends_with("</notification>"));
  }

  #[test]
  fn escapes_attribute_values() {
    let data = data_from_json(json!({
      "id": "a&b\"c",
      "category": "x",
      "type": "y",
      "source_kind": "z",
      "source_id": "w"
    }));
    let xml = render_notification_xml(&data);
    assert!(xml.starts_with("<notification id=\"a&amp;b&quot;c\""));
  }

  #[test]
  fn renders_agent_id_attribute_when_present() {
    let data = data_from_json(json!({
      "id": "n",
      "category": "c",
      "type": "t",
      "source_kind": "background_task",
      "source_id": "s",
      "agent_id": "agent-42"
    }));
    let xml = render_notification_xml(&data);
    assert!(xml.contains(" agent_id=\"agent-42\""));
  }

  #[test]
  fn omits_agent_id_attribute_when_absent() {
    let data = data_from_json(json!({
      "id": "n",
      "category": "c",
      "type": "t",
      "source_kind": "background_task",
      "source_id": "s"
    }));
    let xml = render_notification_xml(&data);
    assert!(!xml.contains("agent_id"));
  }

  #[test]
  fn omits_empty_title_severity_body_lines() {
    let data = data_from_json(json!({
      "id": "n",
      "category": "c",
      "type": "t",
      "source_kind": "cron_job",
      "source_id": "s"
    }));
    let xml = render_notification_xml(&data);
    assert!(!xml.contains("Title:"));
    assert!(!xml.contains("Severity:"));
    assert!(!xml.contains("<task-notification>"));
  }
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  cd rust-ody && cargo test -p agent-rs --test context_notification_xml
  ```

  Expected failure: `error[E0433]: failed to resolve: use of undeclared crate or module `notification_xml`` / `cannot find function `render_notification_xml` in module `agent_rs::context::notification_xml``.

- [ ] **Write the minimal implementation**

  1. Create `rust-ody/crates/agent-rs/src/context/notification_xml.rs`：

     ```rust
     use serde_json::{Map, Value};

     /// 渲染后台/cron 通知 XML，与 TS `renderNotificationXml` 逐字节对齐。
     pub fn render_notification_xml(data: &Map<String, Value>) -> String {
       let id = string_attr(data.get("id"), "unknown");
       let category = string_attr(data.get("category"), "unknown");
       let ty = string_attr(data.get("type"), "unknown");
       let source_kind = string_attr(data.get("source_kind"), "unknown");
       let source_id = string_attr(data.get("source_id"), "unknown");
       let agent_id = optional_string_attr(data.get("agent_id"));
       let title = as_str(data.get("title"));
       let severity = as_str(data.get("severity"));
       let body = as_str(data.get("body"));

       let agent_id_attr = agent_id
         .as_ref()
         .map(|s| format!(" agent_id=\"{}\"", s))
         .unwrap_or_default();

       let mut lines: Vec<String> = vec![format!(
         "<notification id=\"{}\" category=\"{}\" type=\"{}\" source_kind=\"{}\" source_id=\"{}\"{}>",
         id, category, ty, source_kind, source_id, agent_id_attr
       )];

       if !title.is_empty() {
         lines.push(format!("Title: {}", title));
       }
       if !severity.is_empty() {
         lines.push(format!("Severity: {}", severity));
       }
       if !body.is_empty() {
         lines.push(body.to_string());
       }

       if source_kind == "background_task" {
         if let Some(Value::String(tail_raw)) = data.get("tail_output") {
           if !tail_raw.is_empty() {
             let truncated = truncate_tail_output(tail_raw, 20, 3000);
             lines.push("<task-notification>".into());
             lines.push(truncated);
             lines.push("</task-notification>".into());
           }
         }
       }

       lines.push("</notification>".into());
       lines.join("\n")
     }

     fn truncate_tail_output(raw: &str, max_lines: usize, max_chars: usize) -> String {
       let all_lines: Vec<&str> = raw.split('\n').collect();
       let tail_lines = if all_lines.len() > max_lines {
         &all_lines[all_lines.len() - max_lines..]
       } else {
         &all_lines[..]
       };
       let mut result = tail_lines.join("\n");
       if result.chars().count() > max_chars {
         result = result
           .chars()
           .rev()
           .take(max_chars)
           .collect::<Vec<_>>()
           .into_iter()
           .rev()
           .collect();
       }
       result
     }

     fn string_attr(value: Option<&Value>, fallback: &str) -> String {
       match value {
         Some(Value::String(s)) if !s.is_empty() => escape_xml_attr(s),
         _ => fallback.into(),
       }
     }

     fn optional_string_attr(value: Option<&Value>) -> Option<String> {
       match value {
         Some(Value::String(s)) if !s.is_empty() => Some(escape_xml_attr(s)),
         _ => None,
       }
     }

     fn as_str(value: Option<&Value>) -> &str {
       match value {
         Some(Value::String(s)) => s.as_str(),
         _ => "",
       }
     }

     fn escape_xml_attr(input: &str) -> String {
       input.replace('&', "&amp;").replace('"', "&quot;")
     }
     ```

  2. Modify `rust-ody/crates/agent-rs/src/context/mod.rs` 追加：

     ```rust
     pub mod notification_xml;
     pub use notification_xml::render_notification_xml;
     ```

     即完整 `mod.rs`：

     ```rust
     pub mod memory;
     pub mod notification_xml;
     pub mod projector;
     pub mod tokens;
     pub mod types;

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
  cd rust-ody && cargo test -p agent-rs --test context_notification_xml
  ```

  Expected: `test result: ok. 5 passed; 0 failed`.

- [ ] **Commit**

  ```bash
  git add rust-ody/crates/agent-rs/src/context/notification_xml.rs \
         rust-ody/crates/agent-rs/src/context/mod.rs \
         rust-ody/crates/agent-rs/tests/context_notification_xml.rs
  git commit -m "feat(agent-rs): context notification XML renderer"
  ```

---

## Local Self-Review

- [ ] 1. Spec-coverage：本 part 覆盖 roadmap 4.3.1.5（`notification-xml`），无 GAP。
- [ ] 2. Placeholder scan：无 TODO/TBD；所有分支完整实现。
- [ ] 3. No phantom tasks：Task 5 产出 notification XML 实现 + 5 个 L1 测试。
- [ ] 4. Dependency soundness：仅依赖 `types.md`；未引用 memory 等后续模块。
- [ ] 5. Caller & build soundness：新增公开 API，无现有调用方需要更新；Task 5 结束时运行 `cargo check -p agent-rs --workspace --tests` 全绿。
- [ ] 6. Test-the-risk：
  - attribute 转义断言 `"a&b\"c"` 输出 `a&amp;b&quot;c`，验证转义常量正确。
  - tail 截断断言大输入只保留最后 20 行，验证 `max_lines=20` 常量。
  - 空 title/severity/body 行被省略，验证不会输出空行污染 XML。
  - must-survive 输入：非 `background_task` 的 source_kind 不产生 `<task-notification>`；`agent_id` 存在时保留属性。
- [ ] 7. Type一致性：函数签名 `render_notification_xml(data: &Map<String, Value>)` 与 TS `Record<string, unknown>` 语义对应；attribute 名 `source_kind`/`source_id` 与 TS 一致；转义规则与 `escapeXmlAttr` 一致。
