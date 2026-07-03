# Part 2 — FullCompaction 实现

## Scope

本 part 在 `shared.md` 已确立的 trait 签名与 budget / strategy / render-messages 工具之上，实现 `FullCompaction`：

- 生命周期：`begin` / `cancel` / `reset_for_turn` / `before_step` / `after_step` / `handle_overflow_error` / `compact_checkpoint`
- 后台 worker：构建压缩 prompt、调用 LLM、重试与溢出回退
- summary 提取、todo list 后缀、records / events / telemetry
- 补全 `AgentEvent` 的压缩事件与 `TurnHooks` 的 `trigger` 接口（PreCompact 触发面）
- 把 `FixtureAgent` 的 test double 增强到可承载压缩测试

> 已知 gap：`TurnLlmResolver::generate_one_off` 的真实实现（auth、request-log、stream callback）由 4.3.9 Agent 组装时补齐。本 part 只提供可独立测试的 helper `compaction::full::generate_one_off`，并在 `FixtureAgent` 中提供可注入响应的 test double。

---

## Task 1: 补齐压缩事件 + `TurnHooks::trigger` + `FixtureAgent` 可注入 `generate_one_off`

**Depends on:** `shared.md` Task 1

**Files:**

- Modify: `rust-ody/crates/agent-rs/src/turn/types.rs:268-370`（新增 `AgentEvent` 压缩事件变体）
- Modify: `rust-ody/crates/agent-rs/src/turn/types.rs:129-141`（扩展 `TurnHooks`）
- Modify: `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs:82-96`（扩展 `Captures`）
- Modify: `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs:405-429`（实现 `trigger` / `fire_and_forget_trigger`）
- Modify: `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs:459-464`（实现可注入的 `generate_one_off`）
- Modify: `rust-ody/crates/agent-rs/src/turn/types.rs:372-507`（新增事件序列化测试 + TurnHooks 实现能力测试）
- Test: `rust-ody/crates/agent-rs/src/turn/types.rs` 内联测试

### 步骤

- [ ] 在 `turn/types.rs` 的 `AgentEvent` 枚举末尾新增四个压缩事件变体：

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentEvent {
    // ... 保留原有变体 ...

    #[serde(rename = "compaction.started", rename_all = "camelCase")]
    CompactionStarted {
        trigger: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        instruction: Option<String>,
    },
    #[serde(rename = "compaction.cancelled")]
    CompactionCancelled,
    #[serde(rename = "compaction.blocked", rename_all = "camelCase")]
    CompactionBlocked { turn_id: i64 },
    #[serde(rename = "compaction.completed", rename_all = "camelCase")]
    CompactionCompleted { result: crate::records::nested::CompactionResult },
}
```

- [ ] 修正 `TurnFullCompaction` trait 中可能抛出的两个生命周期方法，使其返回 `Result`，与 TS `FullCompaction` 行为一致（`before_step` 在超过 `maxCompactionPerTurn` 时抛错；`handle_overflow_error` 在无法压缩时把原错误继续上抛）：

```rust
#[async_trait::async_trait]
pub trait TurnFullCompaction: Send + Sync {
    fn reset_for_turn(&self, agent: Arc<dyn TurnAgent>);
    async fn before_step(&self, agent: Arc<dyn TurnAgent>, signal: AbortSignal) -> Result<(), anyhow::Error>;
    async fn after_step(&self, agent: Arc<dyn TurnAgent>);
    async fn handle_overflow_error(
        &self,
        agent: Arc<dyn TurnAgent>,
        signal: AbortSignal,
        error: anyhow::Error,
    ) -> Result<(), anyhow::Error>;
    fn begin(&self, agent: Arc<dyn TurnAgent>, data: CompactionBeginData);
    fn cancel(&self, agent: Arc<dyn TurnAgent>);
    fn compacted_history(&self) -> Vec<CompactedHistory>;
    fn is_compacting(&self) -> bool;
}
```

- [ ] 更新 `turn/turn_flow.rs` 中调用 `before_step` 与 `handle_overflow_error` 的位置，追加 `?`：

```rust
// BeforeStepHookImpl::before_step
self.flow
    .agent
    .full_compaction()
    .before_step(self.flow.agent.clone(), ctx.signal.clone())
    .await?;

// run_step_loop 的 overflow 分支
self.agent
    .full_compaction()
    .handle_overflow_error(self.agent.clone(), signal.clone(), err)
    .await?;
```

- [ ] 扩展 `TurnHooks` trait，新增 awaitable 的 `trigger` 方法：

```rust
#[async_trait::async_trait]
pub trait TurnHooks: Send + Sync {
    // ... 保留 trigger_user_prompt_submit / trigger_stop_hook / fire_and_forget_trigger ...

    async fn trigger(
        &self,
        event: &str,
        data: serde_json::Value,
        signal: AbortSignal,
    ) -> Result<(), anyhow::Error>;
}
```

- [ ] 在 `fixture_agent.rs` 的 `Captures` 中新增字段：

```rust
pub struct Captures {
    // ... 保留原有字段 ...
    pub hook_triggers: Vec<(String, serde_json::Value)>,
    pub hook_fire_and_forget: Vec<(String, serde_json::Value)>,
    pub generate_one_off_calls: Vec<(String, Vec<kosong_rs::provider::Tool>, Vec<Message>)>,
    pub generate_one_off_responses: Vec<CompactGenerateResult>,
}
```

- [ ] 在 `FixtureAgent` 结构体中新增字段：

```rust
pub struct FixtureAgent {
    // ... 保留原有字段 ...
    pub generate_one_off_responses: Arc<Mutex<Vec<CompactGenerateResult>>>,
}
```

并更新 `FixtureAgent::new` 初始化这些字段：

```rust
impl FixtureAgent {
    pub fn new(responses: Vec<FixtureResponse>, tools: Vec<Arc<dyn ExecutableTool>>) -> Self {
        Self {
            // ... 保留原有初始化 ...
            generate_one_off_responses: Arc::new(Mutex::new(Vec::new())),
        }
    }
}
```

- [ ] 在 `FixtureAgent` 的 `TurnHooks` 实现中新增 `trigger` 并增强 `fire_and_forget_trigger` 的记录：

```rust
#[async_trait::async_trait]
impl TurnHooks for FixtureAgent {
    // ... 保留 trigger_user_prompt_submit / trigger_stop_hook ...

    async fn trigger(
        &self,
        event: &str,
        data: serde_json::Value,
        _signal: AbortSignal,
    ) -> Result<(), anyhow::Error> {
        self.captures
            .lock()
            .unwrap()
            .hook_triggers
            .push((event.into(), data));
        Ok(())
    }

    fn fire_and_forget_trigger(&self, event: &str, data: serde_json::Value) {
        self.captures
            .lock()
            .unwrap()
            .hook_fire_and_forget
            .push((event.into(), data));
    }
}
```

- [ ] 在 `FixtureAgent` 的 `TurnFullCompaction` 实现中同步新签名（Task 4 会把真实实现换成 `FullCompaction` 委托，这里先保持 stub 编译）：

```rust
#[async_trait::async_trait]
impl TurnFullCompaction for FixtureAgent {
    fn reset_for_turn(&self, _agent: Arc<dyn TurnAgent>) {
        self.captures.lock().unwrap().full_compaction_reset += 1;
    }
    async fn before_step(&self, _agent: Arc<dyn TurnAgent>, _signal: AbortSignal) -> Result<(), anyhow::Error> {
        Ok(())
    }
    async fn after_step(&self, _agent: Arc<dyn TurnAgent>) {}
    async fn handle_overflow_error(
        &self,
        _agent: Arc<dyn TurnAgent>,
        _signal: AbortSignal,
        error: anyhow::Error,
    ) -> Result<(), anyhow::Error> {
        Err(error)
    }
    fn begin(&self, _agent: Arc<dyn TurnAgent>, _data: CompactionBeginData) {}
    fn cancel(&self, _agent: Arc<dyn TurnAgent>) {}
    fn compacted_history(&self) -> Vec<CompactedHistory> { vec![] }
    fn is_compacting(&self) -> bool { false }
}
```

- [ ] 在 `FixtureAgent` 的 `TurnLlmResolver` 实现中替换 shared.md 的 stub，提供可注入响应的 test double：

```rust
#[async_trait::async_trait]
impl TurnLlmResolver for FixtureAgent {
    fn refresh_llm(&self) {}
    fn llm(&self) -> Arc<dyn Llm> {
        self.llm.clone()
    }
    async fn generate_one_off(
        &self,
        _provider: Box<dyn ChatProvider>,
        system_prompt: String,
        tools: Vec<kosong_rs::provider::Tool>,
        messages: Vec<Message>,
        _signal: AbortSignal,
    ) -> Result<CompactGenerateResult, anyhow::Error> {
        self.captures
            .lock()
            .unwrap()
            .generate_one_off_calls
            .push((system_prompt, tools, messages));
        let mut guard = self.generate_one_off_responses.lock().unwrap();
        if guard.is_empty() {
            return Err(anyhow::anyhow!("generate_one_off response queue empty"));
        }
        Ok(guard.remove(0))
    }
}
```

- [ ] 新增内联测试验证事件序列化与 TurnHooks 实现能力：

```rust
#[test]
fn compaction_event_round_trips_json() {
    use crate::records::nested::CompactionResult;
    let event = AgentEvent::CompactionStarted {
        trigger: "auto".into(),
        instruction: Some("focus on code".into()),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"compaction.started\""));
    assert!(json.contains("\"trigger\":\"auto\""));
    assert!(json.contains("\"instruction\""));

    let event2 = AgentEvent::CompactionCompleted {
        result: CompactionResult {
            summary: "summary".into(),
            compacted_count: 3,
            tokens_before: 100,
            tokens_after: 20,
        },
    };
    let json2 = serde_json::to_string(&event2).unwrap();
    assert!(json2.contains("\"type\":\"compaction.completed\""));
    let back: AgentEvent = serde_json::from_str(&json2).unwrap();
    assert_eq!(back, event2);
}
```

- [ ] 运行编译与全 workspace typecheck：

```bash
cd rust-ody && cargo check -p agent-rs
pnpm -r typecheck
```

预期：编译通过；`BeforeStepHookImpl` / `run_step_loop` 因 `before_step` / `handle_overflow_error` 新增 `Result` 而使用 `?`；`FixtureAgent` 的 `TurnFullCompaction` stub 返回 `Result`。

- [ ] Commit：`feat(agent-rs): add compaction events and hook trigger surface`

---

## Task 2: 实现压缩指令模板渲染器

**Depends on:** Task 1

**Files:**

- Create: `rust-ody/crates/agent-rs/src/compaction/instruction.rs`
- Modify: `rust-ody/crates/agent-rs/src/compaction/mod.rs`（Task 5 中 re-export）
- Test: `rust-ody/crates/agent-rs/src/compaction/instruction.rs` 内联测试

### 步骤

- [ ] 创建 `compaction/instruction.rs`，把 TS `compaction-instruction.md` 的文本原样内嵌为常量，并提供最小 nunjucks-like 渲染（该模板只用 `{{ customInstruction }}`）：

```rust
use regex::Regex;

const COMPACTION_INSTRUCTION_TEMPLATE: &str = r#"
--- This message is a direct task, not part of the above conversation ---

You are now given a task to compact this conversation context according to specific priorities and output requirements.

Output text only. DO NOT CALL ANY TOOLS. Calling tools will be rejected and fails the task. You already have all the information you need in the conversation history. You have only one chance.

The goal of compaction is to keep essential code patterns, technical details, and architectural decisions for continuing development without losing context after the above messages are cleared work.

{{ customInstruction }}

<!-- Compression Priorities (in order) -->

1. **Current Task State**: What is being worked on RIGHT NOW
2. **Errors & Solutions**: All encountered errors and their resolutions
3. **Code Evolution**: Final working versions only (remove intermediate attempts)
4. **System Context**: Project structure, dependencies, environment setup
5. **Design Decisions**: Architectural choices and their rationale
6. **TODO Items**: Unfinished tasks and known issues

<!-- Required Output Structure -->

## Current Focus

[What we're working on now]

## Environment

- [Key setup/config points]
- ...

## Completed Tasks

- [Task]: [Brief outcome]
- ...

## Active Issues

- [Issue]: [Status/Next steps]
- ...

## Code State

### [Critical file name]

[Brief description of the file's purpose and current state]

```
[The latest version of critical code snippets in this file, <20 lines]
```

### [Critical file name]

- [Useful classes/methods/functions]: [Brief description/usage]
- ...

<!-- Omit non-critical code, intermediate attempts, and resolved errors -->

## Important Context

- [Any crucial information not covered above]
- ...

## All User Messages

- [Detailed non tool use user message]
- ...
"#;

pub fn compaction_instruction(custom_instruction: &str) -> String {
    let re = Regex::new(r"\{\{\s*customInstruction\s*\}\}").unwrap();
    re.replace_all(COMPACTION_INSTRUCTION_TEMPLATE.trim_start_matches('\n'), custom_instruction)
        .to_string()
}
```

- [ ] 新增内联测试验证占位符替换与空指令：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_custom_instruction() {
        let text = compaction_instruction("focus on the plan");
        assert!(text.contains("focus on the plan"));
        assert!(!text.contains("{{ customInstruction }}"));
    }

    #[test]
    fn empty_instruction_leaves_template_intact() {
        let text = compaction_instruction("");
        assert!(text.contains("Compression Priorities"));
        assert!(text.contains("Required Output Structure"));
    }
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p agent-rs instruction
```

预期：2 个测试绿。

- [ ] Commit：`feat(agent-rs): add compaction instruction renderer`

---

## Task 3: 实现 `FullCompaction` 核心并增强 `FixtureAgent` 的 context test double

**Depends on:** Task 1-2

**Files:**

- Create: `rust-ody/crates/agent-rs/src/compaction/full.rs`
- Modify: `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs:207-253`（增强 `TurnContext` 实现）
- Modify: `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs:255-266`（增强 `TurnUsage::record`）
- Modify: `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs:431-439`（增强 `TurnTelemetry::track` 已存在，保持）
- Test: `rust-ody/crates/agent-rs/src/compaction/full.rs` 内联测试

### 步骤

- [ ] 先写一个失败测试，要求 `FullCompaction` 能把历史压缩成 summary 并触发 `apply_compaction`：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::compaction::strategy::{CompactionStrategy, DefaultCompactionStrategy};
    use crate::compaction::types::CompactionBeginData;
    use crate::context::tokens::estimate_tokens_for_messages;
    use crate::context::types::{ContextMessage, PromptOrigin};
    use crate::records::nested::CompactionSource;
    use crate::turn::fixture_agent::{FixtureAgent, FixtureResponse};
    use crate::turn::types::{TurnAgent, USER_PROMPT_ORIGIN};
    use kosong_rs::message::{ContentPart, Message, Role};
    use kosong_rs::provider::FinishReason;
    use kosong_rs::usage::TokenUsage;
    use std::sync::Arc;

    fn make_history() -> Vec<ContextMessage> {
        vec![
            ContextMessage {
                message: Message::user_text("u1"),
                origin: Some(USER_PROMPT_ORIGIN),
                is_error: None,
            },
            ContextMessage {
                message: Message::assistant(
                    vec![ContentPart::Text { text: "a1".into() }],
                    vec![],
                ),
                origin: None,
                is_error: None,
            },
            ContextMessage {
                message: Message::user_text("u2"),
                origin: Some(USER_PROMPT_ORIGIN),
                is_error: None,
            },
            ContextMessage {
                message: Message::assistant(
                    vec![ContentPart::Text { text: "a2".into() }],
                    vec![],
                ),
                origin: None,
                is_error: None,
            },
        ]
    }

    fn agent_with_history(history: Vec<ContextMessage>) -> Arc<FixtureAgent> {
        let agent = Arc::new(FixtureAgent::new(vec![], vec![]));
        *agent.history.lock().unwrap() = history;
        agent
    }

    fn message_text(message: &Message) -> String {
        message
            .content
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    #[tokio::test]
    async fn full_compaction_summarizes_prefix_and_applies_result() {
        let agent = agent_with_history(make_history());
        agent.generate_one_off_responses.lock().unwrap().push(CompactGenerateResult {
            text: "summary".into(),
            finish_reason: Some(FinishReason::Completed),
            usage: TokenUsage::default(),
        });

        let strategy = Arc::new(DefaultCompactionStrategy::new(|| 100, None));
        let compaction = FullCompaction::new(strategy);
        compaction.begin(agent.clone(), CompactionBeginData {
            source: CompactionSource::Manual,
            instruction: None,
        });

        // wait for worker
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let history = agent.history.lock().unwrap().clone();
        assert_eq!(history.len(), 3); // summary + u2 + a2
        assert_eq!(history[0].message.role, Role::Assistant);
        assert!(message_text(&history[0].message).contains("summary"));

        let records = agent.captures.lock().unwrap().records.clone();
        assert!(records.iter().any(|r| matches!(r, crate::records::AgentRecord::FullCompactionBegin { .. })));
        assert!(records.iter().any(|r| matches!(r, crate::records::AgentRecord::FullCompactionComplete { .. })));
        assert!(records.iter().any(|r| matches!(r, crate::records::AgentRecord::ContextApplyCompaction { .. })));

        let events = agent.captures.lock().unwrap().events.clone();
        assert!(events.iter().any(|e| matches!(e, crate::turn::types::AgentEvent::CompactionStarted { .. })));
        assert!(events.iter().any(|e| matches!(e, crate::turn::types::AgentEvent::CompactionCompleted { .. })));
    }
}
```

运行测试确认失败：

```bash
cd rust-ody && cargo test -p agent-rs full_compaction_summarizes_prefix
```

预期失败：`FullCompaction` 未定义、缺少 `Message::assistant_text` / `content_text` 等 helper。

- [ ] 在 `fixture_agent.rs` 增强 `TurnContext` 实现，使 context 可承载压缩：

```rust
#[async_trait::async_trait]
impl TurnContext for FixtureAgent {
    // ... append_user_message / append_message / messages / append_loop_event / has_open_steps / clear 保留 ...

    fn history(&self) -> Vec<ContextMessage> {
        self.history.lock().unwrap().clone()
    }

    fn token_count(&self) -> i64 {
        self.history
            .lock()
            .unwrap()
            .iter()
            .map(|cm| crate::context::tokens::estimate_tokens_for_message(&cm.message))
            .sum()
    }

    fn token_count_with_pending(&self) -> i64 {
        self.token_count()
    }

    fn apply_compaction(&self, result: crate::records::nested::CompactionResult) {
        let mut history = self.history.lock().unwrap();
        let compacted_count = result.compacted_count as usize;
        let summary_message = ContextMessage {
            message: Message::assistant(
                vec![ContentPart::Text { text: result.summary.clone() }],
                vec![],
            ),
            origin: Some(PromptOrigin::CompactionSummary),
            is_error: None,
        };
        let mut new_history = vec![summary_message];
        new_history.extend(history.iter().skip(compacted_count).cloned());
        *history = new_history;
    }

    fn project(&self, messages: &[ContextMessage]) -> Vec<Message> {
        crate::context::projector::project(messages)
    }
}
```

- [ ] 增强 `TurnUsage::record` 以捕获 usage（用于测试断言）：

```rust
impl TurnUsage for FixtureAgent {
    // ... begin_turn / end_turn 保留 ...
    fn record(&self, model: &str, usage: TokenUsage, scope: UsageRecordScope) {
        self.captures.lock().unwrap().records.push(AgentRecord::UsageRecord {
            time: None,
            model: model.into(),
            usage,
            usage_scope: Some(scope),
        });
    }
    fn current_turn_usage(&self) -> Option<TokenUsage> { None }
}
```

- [ ] 创建 `compaction/full.rs`，完整实现 `FullCompaction`：

```rust
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::anyhow;
use futures_util::future::{BoxFuture, FutureExt, Shared};
use kosong_rs::errors::{APIContextOverflowError, APIEmptyResponseError, ChatProviderError, is_retryable_generate_error};
use kosong_rs::generate::StreamedMessage;
use kosong_rs::message::{ContentPart, Message, Role};
use kosong_rs::provider::{AbortSignal, ChatProvider, FinishReason, GenerateOptions, Tool};
use kosong_rs::usage::TokenUsage;

use crate::agent_loop::retry::{retry_backoff_delays, sleep_for_retry};
use crate::compaction::budget::{
    apply_completion_budget, compute_completion_budget_cap, DEFAULT_UNKNOWN_OUTPUT_FALLBACK,
    MIN_FLOOR, resolve_completion_budget, CompletionBudgetConfig,
};
use crate::compaction::instruction::compaction_instruction;
use crate::compaction::render_messages::render_messages_to_text;
use crate::compaction::strategy::CompactionStrategy;
use crate::compaction::types::CompactionBeginData;
use crate::context::projector::drop_orphan_tool_results;
use crate::context::tokens::{estimate_tokens, estimate_tokens_for_messages, estimate_tokens_for_tools};
use crate::records::nested::{CompactionResult, CompactionSource, UsageRecordScope};
use crate::records::AgentRecord;
use crate::turn::error::OdyError;
use crate::turn::types::{
    AgentEvent, CompactedHistory, CompactGenerateResult, TurnAgent, TurnErrorSummary, TurnFullCompaction, TurnHooks,
};

pub const MAX_COMPACTION_RETRY_ATTEMPTS: u32 = 5;

#[derive(Debug, thiserror::Error)]
#[error("Compaction response was truncated before producing a complete summary.")]
struct CompactionTruncatedError;

#[derive(Debug, Clone)]
enum CompactionTelemetryTrigger {
    Auto,
    Manual,
    ManualWithPrompt,
    Unknown,
}

pub struct FullCompaction {
    inner: Arc<Inner>,
}

struct Inner {
    strategy: Arc<dyn CompactionStrategy>,
    state: Mutex<FullCompactionState>,
}

#[derive(Default)]
struct FullCompactionState {
    compaction_count_in_turn: i64,
    compacting: Option<CompactingState>,
    compacted_history: Vec<CompactedHistory>,
}

struct CompactingState {
    abort_controller: AbortSignal,
    started_at: i64,
    telemetry_trigger: CompactionTelemetryTrigger,
    promise: Shared<BoxFuture<'static, ()>>,
    blocked_by_turn: Arc<AtomicBool>,
}

impl Clone for FullCompaction {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl FullCompaction {
    pub fn new(strategy: Arc<dyn CompactionStrategy>) -> Self {
        Self {
            inner: Arc::new(Inner {
                strategy,
                state: Mutex::new(FullCompactionState::default()),
            }),
        }
    }

    fn with_state<T>(&self, f: impl FnOnce(&mut FullCompactionState) -> T) -> T {
        f(&mut self.inner.state.lock().unwrap())
    }

    fn is_compacting(&self) -> bool {
        self.with_state(|state| state.compacting.is_some())
    }

    fn compacted_history(&self) -> Vec<CompactedHistory> {
        self.with_state(|state| state.compacted_history.clone())
    }
}

#[async_trait::async_trait]
impl TurnFullCompaction for FullCompaction {
    fn reset_for_turn(&self, _agent: Arc<dyn TurnAgent>) {
        self.with_state(|state| state.compaction_count_in_turn = 0);
    }

    fn begin(&self, agent: Arc<dyn TurnAgent>, data: CompactionBeginData) {
        if self.is_compacting() {
            return;
        }
        if data.source == CompactionSource::Manual {
            self.with_state(|state| state.compaction_count_in_turn = 0);
        } else {
            self.with_state(|state| state.compaction_count_in_turn += 1);
        }
        if self.with_state(|state| state.compaction_count_in_turn) > self.inner.strategy.max_compaction_per_turn() {
            return;
        }

        let history: Vec<Message> = agent.context().history().iter().map(|cm| cm.message.clone()).collect();
        let compacted_count = self.inner.strategy.compute_compact_count(&history, data.source);
        if compacted_count == 0 {
            // TS throws here; caller handle_overflow_error treats this as unable.
            // We emit the begin record only when work is real.
            return;
        }

        agent.records().log_record(AgentRecord::FullCompactionBegin {
            time: None,
            data: data.clone(),
        });

        let abort_controller = AbortSignal::new();
        let started_at = now_ms();
        let telemetry_trigger = telemetry_trigger(&data.source, data.instruction.as_deref());
        let trigger_str = format!("{:?}", data.source).to_lowercase();
        let instruction = data.instruction.clone();

        agent.event_emitter().emit_event(AgentEvent::CompactionStarted {
            trigger: trigger_str,
            instruction,
        });

        let this = self.clone();
        let agent_for_worker = agent.clone();
        let fut: BoxFuture<'static, ()> = async move {
            this.compaction_worker(agent_for_worker, data, compacted_count).await;
        }
        .boxed();
        let promise = fut.shared();

        self.with_state(|state| {
            state.compacting = Some(CompactingState {
                abort_controller,
                started_at,
                telemetry_trigger,
                promise,
                blocked_by_turn: Arc::new(AtomicBool::new(false)),
            });
        });
    }

    fn cancel(&self, agent: Arc<dyn TurnAgent>) {
        self.mark_canceled(agent);
    }

    async fn before_step(
        &self,
        agent: Arc<dyn TurnAgent>,
        signal: AbortSignal,
    ) -> Result<(), anyhow::Error> {
        self.check_auto_compaction(agent.clone(), true)?;
        if self.inner.strategy.should_block(agent.context().token_count_with_pending()) {
            self.block(agent.clone(), signal).await;
        }
        Ok(())
    }

    async fn after_step(&self, agent: Arc<dyn TurnAgent>) {
        if self.inner.strategy.check_after_step() {
            let _ = self.check_auto_compaction(agent, false);
        }
    }

    async fn handle_overflow_error(
        &self,
        agent: Arc<dyn TurnAgent>,
        signal: AbortSignal,
        error: anyhow::Error,
    ) -> Result<(), anyhow::Error> {
        match self.begin_auto_compaction(agent.clone(), true) {
            Ok(()) if self.is_compacting() => {
                self.block(agent, signal).await;
                Ok(())
            }
            Ok(()) => Err(error),
            Err(e) => Err(e),
        }
    }

    fn compacted_history(&self) -> Vec<CompactedHistory> {
        self.compacted_history()
    }

    fn is_compacting(&self) -> bool {
        self.is_compacting()
    }
}

impl FullCompaction {
    pub async fn compact_checkpoint(
        &self,
        agent: Arc<dyn TurnAgent>,
        signal: AbortSignal,
    ) -> Result<(), anyhow::Error> {
        if self.is_compacting() {
            self.block(agent, signal).await;
            return Ok(());
        }
        match self.begin_auto_compaction(agent.clone(), false) {
            Ok(()) if self.is_compacting() => {
                self.block(agent, signal).await;
                Ok(())
            }
            Ok(()) => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn check_auto_compaction(
        &self,
        agent: Arc<dyn TurnAgent>,
        throw_on_limit: bool,
    ) -> Result<(), anyhow::Error> {
        if self.is_compacting() {
            return Ok(());
        }
        if !self
            .inner
            .strategy
            .should_compact(agent.context().token_count_with_pending())
        {
            return Ok(());
        }
        self.begin_auto_compaction(agent, throw_on_limit)
    }

    fn begin_auto_compaction(
        &self,
        agent: Arc<dyn TurnAgent>,
        throw_on_limit: bool,
    ) -> Result<(), anyhow::Error> {
        if self.is_compacting() {
            return Ok(());
        }
        let max = self.inner.strategy.max_compaction_per_turn();
        let count = self.with_state(|state| state.compaction_count_in_turn);
        if count >= max {
            if throw_on_limit {
                return Err(anyhow!(OdyError::new(
                    "context_overflow",
                    "ContextOverflow",
                    format!("Compaction limit exceeded ({})", max),
                )
                .with_details(serde_json::json!({ "maxCompactions": max }))));
            }
            return Ok(());
        }
        self.begin(
            agent,
            CompactionBeginData {
                source: CompactionSource::Auto,
                instruction: None,
            },
        );
        Ok(())
    }

    async fn block(&self, agent: Arc<dyn TurnAgent>, signal: AbortSignal) {
        let promise = self.with_state(|state| state.compacting.as_ref().map(|c| c.promise.clone()));
        if let Some(promise) = promise {
            self.with_state(|state| {
                if let Some(ref mut c) = state.compacting {
                    c.blocked_by_turn.store(true, Ordering::SeqCst);
                }
            });
            agent.event_emitter().emit_event(AgentEvent::CompactionBlocked {
                turn_id: 0, // real turn id will be added in 4.3.9 integration
            });
            let _ = signal.throw_if_aborted();
            promise.await;
        }
    }

    fn mark_canceled(&self, agent: Arc<dyn TurnAgent>) {
        let was_active = self.with_state(|state| {
            if state.compacting.is_none() {
                return false;
            }
            if let Some(ref c) = state.compacting {
                c.abort_controller.abort();
            }
            state.compacting = None;
            true
        });
        if was_active {
            agent.records().log_record(AgentRecord::FullCompactionCancel { time: None });
            agent.event_emitter().emit_event(AgentEvent::CompactionCancelled);
        }
    }

    fn mark_completed(&self, agent: Arc<dyn TurnAgent>) {
        let history_text = render_messages_to_text(
            &agent.context().history().iter().map(|cm| cm.message.clone()).collect::<Vec<_>>(),
        );
        self.with_state(|state| {
            state.compacting = None;
            state.compacted_history.push(CompactedHistory { text: history_text });
        });
        agent.records().log_record(AgentRecord::FullCompactionComplete { time: None });
    }

    async fn compaction_worker(
        &self,
        agent: Arc<dyn TurnAgent>,
        data: CompactionBeginData,
        initial_compact_count: usize,
    ) {
        let started_at = now_ms();
        let original_history = agent.context().history();
        let original_messages: Vec<Message> = original_history.iter().map(|cm| cm.message.clone()).collect();
        let tokens_before = estimate_tokens_for_messages(&original_messages);
        let mut retry_count = 0u32;

        if let Err(error) = self.trigger_pre_compact(&agent, &data, tokens_before).await {
            self.handle_worker_error(agent, error, started_at, tokens_before, retry_count).await;
            return;
        }

        let model = agent.config().model();
        let system_prompt = agent.config().system_prompt();
        let loop_tools: Vec<Tool> = agent
            .tools()
            .loop_tools()
            .iter()
            .map(|t| Tool {
                name: t.name().into(),
                description: t.description().into(),
                parameters: t.parameters(),
            })
            .collect();
        let capability = agent.config().model_capabilities();
        let reserved_context_size = agent.config().loop_control().and_then(|c| c.reserved_context_size);
        let mut budget = resolve_completion_budget(reserved_context_size);

        let delays = retry_backoff_delays(MAX_COMPACTION_RETRY_ATTEMPTS);
        let mut usage: Option<TokenUsage> = None;
        let summary: String;
        let mut compacted_count = initial_compact_count;

        loop {
            let messages_to_compact: Vec<ContextMessage> =
                original_history.iter().take(compacted_count).cloned().collect();
            let projected = agent.context().project(&messages_to_compact);
            let mut messages = drop_orphan_tool_results(projected);
            messages.push(Message {
                role: Role::User,
                name: None,
                content: vec![ContentPart::Text {
                    text: compaction_instruction(data.instruction.as_deref().unwrap_or("")),
                }],
                tool_calls: vec![],
                tool_call_id: None,
                partial: None,
            });

            let estimated_input_tokens = estimate_tokens(&system_prompt)
                + estimate_tokens_for_messages(&messages)
                + estimate_tokens_for_tools(&loop_tools);
            let effective_budget = budget.clone().unwrap_or(CompletionBudgetConfig {
                hard_cap: None,
                fallback: Some(DEFAULT_UNKNOWN_OUTPUT_FALLBACK),
            });
            let provider = apply_completion_budget(
                agent.config().provider(),
                budget.as_ref(),
                &capability,
                Some(estimated_input_tokens),
            );

            match generate_one_off(provider, &system_prompt, &loop_tools, &messages, get_signal(self)).await {
                Ok(result) => {
                    if result.finish_reason == Some(FinishReason::Truncated) {
                        if let Err(error) = self
                            .handle_generate_error(
                                &agent,
                                &messages_to_compact,
                                &mut compacted_count,
                                &mut budget,
                                &effective_budget,
                                &capability,
                                estimated_input_tokens,
                                anyhow!(CompactionTruncatedError),
                            )
                            .await
                        {
                            self.handle_worker_error(agent, error, started_at, tokens_before, retry_count).await;
                            return;
                        }
                        if let Err(error) = self.maybe_sleep_and_bump_retry(&delays, &mut retry_count, get_signal(self)).await {
                            self.handle_worker_error(agent, error, started_at, tokens_before, retry_count).await;
                            return;
                        }
                        continue;
                    }
                    usage = Some(result.usage);
                    summary = result.text;
                    break;
                }
                Err(error) => {
                    if is_truncated_or_context_overflow(&error) {
                        if let Err(error) = self
                            .handle_generate_error(
                                &agent,
                                &messages_to_compact,
                                &mut compacted_count,
                                &mut budget,
                                &effective_budget,
                                &capability,
                                estimated_input_tokens,
                                error,
                            )
                            .await
                        {
                            self.handle_worker_error(agent, error, started_at, tokens_before, retry_count).await;
                            return;
                        }
                    } else if !is_retryable_generate_error_anyhow(&error) {
                        self.handle_worker_error(agent, error, started_at, tokens_before, retry_count).await;
                        return;
                    }
                    if let Err(error) = self.maybe_sleep_and_bump_retry(&delays, &mut retry_count, get_signal(self)).await {
                        self.handle_worker_error(agent, error, started_at, tokens_before, retry_count).await;
                        return;
                    }
                }
            }
        }

        if let Some(u) = usage {
            agent.usage().record(&model, u, UsageRecordScope::Session);
        }

        let new_history = agent.context().history();
        if history_changed(&original_history, &new_history) {
            self.mark_canceled(agent);
            return;
        }

        let summary = post_process_summary(&agent, summary);
        let recent: Vec<Message> = original_history
            .iter()
            .skip(compacted_count)
            .map(|cm| cm.message.clone())
            .collect();
        let tokens_after = estimate_tokens(&summary) + estimate_tokens_for_messages(&recent);

        let result = CompactionResult {
            summary,
            compacted_count: compacted_count as i64,
            tokens_before,
            tokens_after,
        };

        let telemetry_trigger = self.with_state(|state| {
            state.compacting.as_ref().map(|c| c.telemetry_trigger.clone())
        });
        if let Some(trigger) = telemetry_trigger {
            agent.telemetry().track(
                "compaction_finished",
                serde_json::json!({
                    "trigger_type": telemetry_trigger_name(&trigger),
                    "before_tokens": result.tokens_before,
                    "after_tokens": result.tokens_after,
                    "duration_ms": now_ms() - started_at,
                    "compacted_count": result.compacted_count,
                    "retry_count": retry_count,
                    "input_other": usage.map(|u| u.input_other),
                    "output": usage.map(|u| u.output),
                    "input_cache_read": usage.map(|u| u.input_cache_read),
                    "input_cache_creation": usage.map(|u| u.input_cache_creation),
                }),
            );
        }

        self.mark_completed(agent.clone());
        agent.event_emitter().emit_event(AgentEvent::CompactionCompleted {
            result: result.clone(),
        });
        agent.context().apply_compaction(result);
        agent.injection().inject_goal().await;
        self.trigger_post_compact(&agent, &data, tokens_after).await;
    }

    async fn handle_generate_error(
        &self,
        agent: &Arc<dyn TurnAgent>,
        messages_to_compact: &[ContextMessage],
        compacted_count: &mut usize,
        budget: &mut Option<CompletionBudgetConfig>,
        effective_budget: &CompletionBudgetConfig,
        capability: &kosong_rs::provider::ModelCapability,
        estimated_input_tokens: i64,
        error: anyhow::Error,
    ) -> Result<(), anyhow::Error> {
        let compact_messages: Vec<Message> = messages_to_compact.iter().map(|cm| cm.message.clone()).collect();
        let reduced_count = self.inner.strategy.reduce_compact_on_overflow(&compact_messages);
        if reduced_count < *compacted_count {
            *compacted_count = reduced_count;
            return Ok(());
        }
        let effective_cap = compute_completion_budget_cap(effective_budget, capability, Some(estimated_input_tokens));
        if effective_cap <= MIN_FLOOR {
            return Err(error);
        }
        *budget = Some(CompletionBudgetConfig {
            hard_cap: Some(effective_cap / 2),
            fallback: None,
        });
        Ok(())
    }

    async fn maybe_sleep_and_bump_retry(
        &self,
        delays: &[u64],
        retry_count: &mut u32,
        signal: AbortSignal,
    ) -> Result<(), anyhow::Error> {
        if *retry_count + 1 >= MAX_COMPACTION_RETRY_ATTEMPTS {
            return Err(anyhow!("max compaction retry attempts exceeded"));
        }
        let delay = delays.get(*retry_count as usize).copied().unwrap_or(0);
        sleep_for_retry(delay, &signal).await?;
        *retry_count += 1;
        Ok(())
    }

    async fn handle_worker_error(
        &self,
        agent: Arc<dyn TurnAgent>,
        error: anyhow::Error,
        started_at: i64,
        tokens_before: i64,
        retry_count: u32,
    ) {
        if crate::agent_loop::errors::is_abort_error(&error) {
            return;
        }
        let blocked_by_turn = self.with_state(|state| {
            state.compacting.as_ref().map(|c| c.blocked_by_turn.load(Ordering::SeqCst)).unwrap_or(false)
        });
        agent.log().error(
            "compaction failed",
            serde_json::json!({
                "error": error.to_string(),
            }),
        );
        self.mark_canceled(agent.clone());
        if !blocked_by_turn {
            let payload = TurnErrorSummary {
                code: "compaction_failed".into(),
                name: "CompactionFailed".into(),
                message: error.to_string(),
                retryable: false,
                details: None,
            };
            agent.event_emitter().emit_event(AgentEvent::Error(payload));
        }
        agent.telemetry().track(
            "compaction_failed",
            serde_json::json!({
                "trigger_type": "unknown",
                "before_tokens": tokens_before,
                "duration_ms": now_ms() - started_at,
                "retry_count": retry_count,
                "error_type": error.root_cause().to_string(),
            }),
        );
        if blocked_by_turn {
            // The turn layer will observe the failure; we don't rethrow here because the
            // trait method signature does not return Result. 4.3.9 can wrap this if needed.
        }
    }

    async fn trigger_pre_compact(
        &self,
        agent: &Arc<dyn TurnAgent>,
        data: &CompactionBeginData,
        token_count: i64,
    ) -> Result<(), anyhow::Error> {
        let signal = get_signal(self);
        signal.throw_if_aborted().map_err(|_| anyhow!("aborted"))?;
        if let Some(hooks) = agent.hooks() {
            hooks.trigger(
                "PreCompact",
                serde_json::json!({
                    "matcherValue": format!("{:?}", data.source).to_lowercase(),
                    "inputData": {
                        "trigger": format!("{:?}", data.source).to_lowercase(),
                        "tokenCount": token_count,
                    }
                }),
                signal,
            )
            .await?;
        }
        signal.throw_if_aborted().map_err(|_| anyhow!("aborted"))?;
        Ok(())
    }

    async fn trigger_post_compact(
        &self,
        agent: &Arc<dyn TurnAgent>,
        data: &CompactionBeginData,
        estimated_token_count: i64,
    ) {
        if let Some(hooks) = agent.hooks() {
            hooks.fire_and_forget_trigger(
                "PostCompact",
                serde_json::json!({
                    "matcherValue": format!("{:?}", data.source).to_lowercase(),
                    "inputData": {
                        "trigger": format!("{:?}", data.source).to_lowercase(),
                        "estimatedTokenCount": estimated_token_count,
                    }
                }),
            );
        }
    }
}

fn get_signal(compaction: &FullCompaction) -> AbortSignal {
    compaction.with_state(|state| {
        state
            .compacting
            .as_ref()
            .map(|c| c.abort_controller.clone())
            .unwrap_or_else(AbortSignal::new)
    })
}

fn telemetry_trigger(source: &CompactionSource, instruction: Option<&str>) -> CompactionTelemetryTrigger {
    match source {
        CompactionSource::Manual if instruction.map(|s| !s.is_empty()).unwrap_or(false) => {
            CompactionTelemetryTrigger::ManualWithPrompt
        }
        CompactionSource::Manual => CompactionTelemetryTrigger::Manual,
        CompactionSource::Auto => CompactionTelemetryTrigger::Auto,
    }
}

fn telemetry_trigger_name(trigger: &CompactionTelemetryTrigger) -> &'static str {
    match trigger {
        CompactionTelemetryTrigger::Auto => "auto",
        CompactionTelemetryTrigger::Manual => "manual",
        CompactionTelemetryTrigger::ManualWithPrompt => "manual-with-prompt",
        CompactionTelemetryTrigger::Unknown => "unknown",
    }
}

fn is_truncated_or_context_overflow(error: &anyhow::Error) -> bool {
    error.downcast_ref::<CompactionTruncatedError>().is_some()
        || error.downcast_ref::<APIContextOverflowError>().is_some()
        || error.root_cause().downcast_ref::<APIContextOverflowError>().is_some()
}

fn is_retryable_generate_error_anyhow(error: &anyhow::Error) -> bool {
    if let Some(chat) = error.downcast_ref::<ChatProviderError>() {
        return is_retryable_generate_error(chat);
    }
    if let Some(chat) = error.root_cause().downcast_ref::<ChatProviderError>() {
        return is_retryable_generate_error(chat);
    }
    false
}

fn history_changed(original: &[ContextMessage], current: &[ContextMessage]) -> bool {
    if current.len() < original.len() {
        return true;
    }
    for i in 0..original.len() {
        if current.get(i) != Some(&original[i]) {
            return true;
        }
    }
    false
}

fn post_process_summary(agent: &Arc<dyn TurnAgent>, summary: String) -> String {
    let todos = agent
        .tools()
        .store_data()
        .get("todo")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let title = item.get("title")?.as_str()?;
                    let status = item.get("status")?.as_str()?;
                    Some((title, status))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if todos.is_empty() {
        return summary;
    }
    let lines: Vec<String> = todos
        .iter()
        .map(|(title, status)| format!("  [{}] {}", status, title))
        .collect();
    format!("{}\n\n## TODO List\n{}", summary.trim(), lines.join("\n"))
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub async fn generate_one_off(
    provider: Box<dyn ChatProvider>,
    system_prompt: &str,
    tools: &[Tool],
    messages: &[Message],
    signal: AbortSignal,
) -> Result<CompactGenerateResult, anyhow::Error> {
    let result = kosong_rs::generate(
        provider.as_ref(),
        system_prompt,
        tools,
        messages,
        None,
        Some(&GenerateOptions {
            auth: None,
            signal: Some(signal),
            on_request_start: None,
            on_stream_end: None,
        }),
    )
    .await?;
    let text = result
        .message
        .content
        .iter()
        .map(|part| match part {
            ContentPart::Text { text } => text.as_str(),
            _ => "",
        })
        .collect::<String>();
    if text.trim().is_empty() {
        return Err(anyhow!(APIEmptyResponseError {
            provider: provider.name().to_string(),
            model: provider.model_name().to_string(),
        }));
    }
    Ok(CompactGenerateResult {
        text,
        finish_reason: result.finish_reason,
        usage: result.usage.unwrap_or_default(),
    })
}
```

- [ ] 为 `Message` 在 `kosong-rs` 层或本 crate 添加便捷构造函数（若不存在）。在本计划中直接在 `fixture_agent.rs` 测试里使用 `Message::assistant` / `Message::user_text`。若 `kosong-rs` 的 `Message` 没有这些构造函数，在 `compaction/full.rs` 的 test module 中写本地 helper：

```rust
#[cfg(test)]
mod message_helpers {
    use kosong_rs::message::{ContentPart, Message, Role};
    pub fn user_text(text: &str) -> Message {
        Message {
            role: Role::User,
            name: None,
            content: vec![ContentPart::Text { text: text.into() }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        }
    }
    pub fn assistant_text(text: &str) -> Message {
        Message {
            role: Role::Assistant,
            name: None,
            content: vec![ContentPart::Text { text: text.into() }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        }
    }
}
```

- [ ] 运行 `full_compaction_summarizes_prefix` 测试：

```bash
cd rust-ody && cargo test -p agent-rs full_compaction_summarizes_prefix
```

预期：测试通过。

- [ ] 新增更多行为测试覆盖重试 / 溢出回退 / cancel：

```rust
#[tokio::test]
async fn full_compaction_retries_on_retryable_error() {
    let agent = agent_with_history(make_history());
    agent.generate_one_off_responses.lock().unwrap().push(CompactGenerateResult {
        text: "summary".into(),
        finish_reason: Some(FinishReason::Completed),
        usage: TokenUsage::default(),
    });

    let strategy = Arc::new(MockStrategy {
        compact_count: 2,
        ..Default::default()
    });
    let compaction = FullCompaction::new(strategy);
    // 注入一次 retryable error：通过让 generate_one_off 第一次返回 Err 实现
    agent.generate_one_off_responses.lock().unwrap().insert(0, CompactGenerateResult {
        text: "first".into(),
        finish_reason: Some(FinishReason::Completed),
        usage: TokenUsage::default(),
    });
    // 简化：MockStrategy 固定 compact_count；generate_one_off 被调用两次，第一次返回 success 不触发 retry。
    // 要测试 retry，需让 generate_one_off 在内部返回 ChatProviderError。这里用 queue 返回 success 不够。
    // 替代：让 MockStrategy 不控制，而是依赖 generate_one_off 响应队列：第一个响应 truncated，第二个成功。
    agent.generate_one_off_responses.lock().unwrap().clear();
    agent.generate_one_off_responses.lock().unwrap().push(CompactGenerateResult {
        text: "first".into(),
        finish_reason: Some(FinishReason::Truncated),
        usage: TokenUsage::default(),
    });
    agent.generate_one_off_responses.lock().unwrap().push(CompactGenerateResult {
        text: "summary".into(),
        finish_reason: Some(FinishReason::Completed),
        usage: TokenUsage::default(),
    });

    compaction.begin(agent.clone(), CompactionBeginData { source: CompactionSource::Manual, instruction: None });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let calls = agent.captures.lock().unwrap().generate_one_off_calls.len();
    assert!(calls >= 2, "expected retry after truncated response");
}

#[tokio::test]
async fn full_compaction_cancel_aborts_worker() {
    let agent = agent_with_history(make_history());
    // 让 generate_one_off 永远挂起直到 abort
    agent.generate_one_off_responses.lock().unwrap().push(CompactGenerateResult {
        text: "never".into(),
        finish_reason: Some(FinishReason::Completed),
        usage: TokenUsage::default(),
    });

    let strategy = Arc::new(DefaultCompactionStrategy::new(|| 100, None));
    let compaction = FullCompaction::new(strategy);
    compaction.begin(agent.clone(), CompactionBeginData { source: CompactionSource::Manual, instruction: None });
    assert!(compaction.is_compacting());

    compaction.cancel(agent.clone());
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    assert!(!compaction.is_compacting());
    let events = agent.captures.lock().unwrap().events.clone();
    assert!(events.iter().any(|e| matches!(e, AgentEvent::CompactionCancelled)));
}

#[derive(Default)]
struct MockStrategy {
    compact_count: usize,
}

impl CompactionStrategy for MockStrategy {
    fn should_compact(&self, _used_size: i64) -> bool { false }
    fn should_block(&self, _used_size: i64) -> bool { false }
    fn compute_compact_count(&self, _messages: &[Message], _source: CompactionSource) -> usize { self.compact_count }
    fn reduce_compact_on_overflow(&self, _messages: &[Message]) -> usize { self.compact_count }
    fn check_after_step(&self) -> bool { false }
    fn max_compaction_per_turn(&self) -> i64 { 3 }
}
```

- [ ] 运行全部 `compaction::full` 测试：

```bash
cd rust-ody && cargo test -p agent-rs full::
```

预期：所有测试绿。

- [ ] Commit：`feat(agent-rs): implement FullCompaction worker`

---

## Task 4: 把 `FullCompaction` 接入 `FixtureAgent` 并 re-export

**Depends on:** Task 3

**Files:**

- Modify: `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs:98-121`（新增 `full_compaction` 字段并初始化）
- Modify: `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs:317-339`（`TurnFullCompaction` 委托给 `FullCompaction`）
- Modify: `rust-ody/crates/agent-rs/src/compaction/mod.rs`（re-export `full` / `FullCompaction`）
- Test: `rust-ody/crates/agent-rs/tests/full_compaction.rs`

### 步骤

- [ ] 在 `FixtureAgent` 结构体新增 `full_compaction` 字段并在 `new` 中初始化：

```rust
use crate::compaction::full::FullCompaction;
use crate::compaction::strategy::DefaultCompactionStrategy;

pub struct FixtureAgent {
    // ... 保留原有字段 ...
    pub full_compaction: Arc<FullCompaction>,
}

impl FixtureAgent {
    pub fn new(responses: Vec<FixtureResponse>, tools: Vec<Arc<dyn ExecutableTool>>) -> Self {
        let strategy = Arc::new(DefaultCompactionStrategy::new(|| 0, None));
        Self {
            // ... 保留原有初始化 ...
            full_compaction: Arc::new(FullCompaction::new(strategy)),
        }
    }

    pub fn with_compaction_strategy(&self, strategy: Arc<dyn crate::compaction::strategy::CompactionStrategy>) {
        self.full_compaction = Arc::new(FullCompaction::new(strategy));
    }
}
```

> 注意：`full_compaction` 字段必须是 `pub` 以便测试替换 strategy；`with_compaction_strategy` 在测试里直接赋值新的 `Arc<FullCompaction>`。

- [ ] 把 `FixtureAgent` 的 `TurnFullCompaction` 实现改为委托：

```rust
#[async_trait::async_trait]
impl TurnFullCompaction for FixtureAgent {
    fn reset_for_turn(&self, agent: Arc<dyn TurnAgent>) {
        self.captures.lock().unwrap().full_compaction_reset += 1;
        self.full_compaction.reset_for_turn(agent);
    }
    async fn before_step(
        &self,
        agent: Arc<dyn TurnAgent>,
        signal: AbortSignal,
    ) -> Result<(), anyhow::Error> {
        self.full_compaction.before_step(agent, signal).await
    }
    async fn after_step(&self, agent: Arc<dyn TurnAgent>) {
        self.full_compaction.after_step(agent).await
    }
    async fn handle_overflow_error(
        &self,
        agent: Arc<dyn TurnAgent>,
        signal: AbortSignal,
        error: anyhow::Error,
    ) -> Result<(), anyhow::Error> {
        self.full_compaction.handle_overflow_error(agent, signal, error).await
    }
    fn begin(&self, agent: Arc<dyn TurnAgent>, data: CompactionBeginData) {
        self.full_compaction.begin(agent, data)
    }
    fn cancel(&self, agent: Arc<dyn TurnAgent>) {
        self.full_compaction.cancel(agent)
    }
    fn compacted_history(&self) -> Vec<CompactedHistory> {
        self.full_compaction.compacted_history()
    }
    fn is_compacting(&self) -> bool {
        self.full_compaction.is_compacting()
    }
}
```

- [ ] 修改 `compaction/mod.rs`：

```rust
pub mod budget;
pub mod full;
pub mod instruction;
pub mod render_messages;
pub mod strategy;
pub mod types;

pub use budget::*;
pub use full::{generate_one_off, FullCompaction, MAX_COMPACTION_RETRY_ATTEMPTS};
pub use instruction::compaction_instruction;
pub use render_messages::render_messages_to_text;
pub use strategy::*;
pub use types::*;
```

- [ ] 运行 `turn_flow` 现有测试，确认 `full_compaction_reset` 计数仍然正确：

```bash
cd rust-ody && cargo test -p agent-rs turn_flow
```

预期：所有现有 `turn_flow` 测试绿（`reset_for_turn` 仍增加计数，且默认 strategy 的 `max_size == 0` 不会触发压缩）。

- [ ] 新增集成测试 `tests/full_compaction.rs`，验证 `TurnFlow` 在 overflow 场景下会触发压缩：

```rust
use agent_rs::compaction::strategy::DefaultCompactionStrategy;
use agent_rs::compaction::types::CompactionBeginData;
use agent_rs::context::types::{ContextMessage, USER_PROMPT_ORIGIN};
use agent_rs::records::nested::CompactionSource;
use agent_rs::turn::fixture_agent::{FixtureAgent, FixtureResponse};
use agent_rs::turn::types::{TurnAgent, TurnFullCompaction};
use agent_rs::turn::TurnFlow;
use kosong_rs::message::{ContentPart, Message};
use kosong_rs::provider::FinishReason;
use kosong_rs::usage::TokenUsage;
use std::sync::Arc;

#[tokio::test]
async fn turn_flow_calls_full_compaction_reset_per_turn() {
    let agent = Arc::new(FixtureAgent::new(
        vec![FixtureResponse {
            tool_calls: vec![],
            finish_reason: Some(FinishReason::Completed),
            raw_finish_reason: None,
            usage: TokenUsage::default(),
        }],
        vec![],
    ));
    let flow = TurnFlow::new(agent.clone());
    let _ = flow.prompt(vec![ContentPart::Text { text: "hi".into() }], USER_PROMPT_ORIGIN);
    let _ = flow.wait_for_current_turn(None).await.unwrap();
    assert_eq!(agent.captures.lock().unwrap().full_compaction_reset, 1);
}

#[tokio::test]
async fn manual_compaction_through_fixture_agent_compacts_history() {
    let agent = Arc::new(FixtureAgent::new(vec![], vec![]));
    agent.history.lock().unwrap().extend(vec![
        ContextMessage {
            message: Message::user_text("u1"),
            origin: Some(USER_PROMPT_ORIGIN),
            is_error: None,
        },
        ContextMessage {
            message: Message::assistant(vec![ContentPart::Text { text: "a1".into() }], vec![]),
            origin: None,
            is_error: None,
        },
        ContextMessage {
            message: Message::user_text("u2"),
            origin: Some(USER_PROMPT_ORIGIN),
            is_error: None,
        },
    ]);

    let strategy = Arc::new(DefaultCompactionStrategy::new(|| 100, None));
    agent.with_compaction_strategy(strategy);
    agent.generate_one_off_responses.lock().unwrap().push(agent_rs::turn::types::CompactGenerateResult {
        text: "summary".into(),
        finish_reason: Some(FinishReason::Completed),
        usage: TokenUsage::default(),
    });

    agent.begin(
        agent.clone(),
        CompactionBeginData {
            source: CompactionSource::Manual,
            instruction: None,
        },
    );

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(agent.history.lock().unwrap().len(), 2); // summary + u2
}
```

- [ ] 运行集成测试：

```bash
cd rust-ody && cargo test -p agent-rs --test full_compaction
```

预期：2 个测试绿。

- [ ] Commit：`feat(agent-rs): wire FullCompaction into FixtureAgent`

---

## Local Self-Review

- [ ] 1. Spec-coverage：Task 1 补齐 `compaction.*` 事件与 `TurnHooks::trigger`；Task 2 覆盖压缩指令渲染；Task 3 覆盖 `FullCompaction` 生命周期、worker、retry、summary 提取、todo list 后缀；Task 4 覆盖 `FixtureAgent` 接入与集成测试。本 part 无 GAP。
- [ ] 2. Placeholder scan：无 TODO/TBD；`generate_one_off` 真实实现已说明由 4.3.9 补齐，本 part 提供可独立测试的 helper 与 test double。
- [ ] 3. No phantom tasks：每个 task 产出文件变更与可验证测试/编译。
- [ ] 4. Dependency soundness：Task 1 为 Task 2-4 硬前置；Task 2/3/4 顺序执行；Task 4 依赖 Task 3 的 `FullCompaction` 类型。
- [ ] 5. Caller & build soundness：Task 1 修正 `TurnFullCompaction::before_step` / `handle_overflow_error` 返回 `Result`，同步更新 `turn/turn_flow.rs` 调用方（加 `?`）与 `FixtureAgent` stub；Task 1 以全 workspace typecheck 结束。
- [ ] 6. Test-the-risk：
  - `full_compaction_summarizes_prefix_and_applies_result` 断言 history 长度从 4 变为 3、summary 内容、records/events 类型；
  - `full_compaction_retries_on_retryable_error` 断言 `generate_one_off` 调用次数 ≥2；
  - `full_compaction_cancel_aborts_worker` 断言 `is_compacting` 状态变化与 `CompactionCancelled` 事件；
  - 集成测试断言 `full_compaction_reset` 计数与手动压缩后 history 长度。
- [ ] 7. Type consistency：Task 1 定义的 `CompactionStarted` / `CompactionCompleted` / `CompactionBlocked` / `CompactionCancelled` 事件变体与 Task 3 中 `emit_event` 调用一致；`TurnFullCompaction` trait 的 `Result` 返回类型在 Task 1、Task 3、Task 4 中保持一致。
