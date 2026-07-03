# Part 1 — Shared signatures + budget helpers + strategy + render-messages

## Scope

本 part 完成三件事：
1. 扩展 `TurnAgent` 子 trait（`TurnContext` / `TurnTools` / `TurnSessionMode` / `TurnLlmResolver` / `TurnFullCompaction` / `TurnMicroCompaction` / `TurnSplitPlanCheckpoint` / `TurnNormalTaskCheckpoint`），为 `FullCompaction` / `MicroCompaction` / `Checkpoints` 提供访问 Agent 各子系统的最小接口。
2. 实现 completion-budget helper（`resolve_completion_budget` / `compute_completion_budget_cap` / `apply_completion_budget`）与 `estimate_tokens_for_tools`。
3. 实现 `CompactionStrategy` / `DefaultCompactionStrategy` 与 `render_messages_to_text`。

所有共享签名变更集中在本 part 的 Task 1 完成，并同步更新 `fixture_agent.rs` 与 `turn_flow.rs`，以整 workspace typecheck 绿作为该 task 的通过标准。

---

## Task 1: 扩展 TurnAgent 子 trait 以暴露 compaction 所需能力

**Depends on:** none

**Files:**

- Modify: `rust-ody/crates/agent-rs/src/turn/types.rs:24-90`（`TurnContext` / `TurnFullCompaction` / `TurnMicroCompaction` / `TurnSplitPlanCheckpoint` / `TurnNormalTaskCheckpoint` / `TurnSessionMode` / `TurnLlmResolver` 扩展）
- Modify: `rust-ody/crates/agent-rs/src/turn/types.rs:189-214`（`TurnAgent` 保持聚合不变）
- Modify: `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs:208-339`（`TurnContext` / `TurnTools` / `TurnSessionMode` / `TurnFullCompaction` / `TurnMicroCompaction` / `TurnSplitPlanCheckpoint` / `TurnNormalTaskCheckpoint` / `TurnLlmResolver` stub 实现）
- Modify: `rust-ody/crates/agent-rs/src/turn/turn_flow.rs:912-939`（`before_step` / `after_step` 调用新签名）
- Modify: `rust-ody/crates/agent-rs/src/lib.rs:1-10`（新增 `pub mod compaction;`）
- Test: `rust-ody/crates/agent-rs/src/turn/types.rs:372-507`（扩展已有测试模块，新增 `compaction_surface_is_implementable`）

### 步骤

- [ ] 在 `turn/types.rs` 现有 `#[cfg(test)]` 模块末尾新增一个编译期测试，提前引用本任务要添加的所有新能力：

```rust
#[test]
fn compaction_surface_is_implementable() {
    use kosong_rs::message::Message;
    use kosong_rs::provider::{AbortSignal, ChatProvider, Tool};

    struct Dummy;
    #[async_trait::async_trait]
    impl TurnContext for Dummy {
        fn append_user_message(&self, _content: Vec<ContentPart>, _origin: PromptOrigin) {}
        fn append_message(&self, _message: ContextMessage) {}
        fn messages(&self) -> Vec<Message> { vec![] }
        fn history(&self) -> Vec<ContextMessage> { vec![] }
        fn token_count(&self) -> i64 { 0 }
        fn token_count_with_pending(&self) -> i64 { 0 }
        fn apply_compaction(&self, _result: CompactionResult) {}
        fn project(&self, messages: &[ContextMessage]) -> Vec<Message> {
            messages.iter().map(|m| m.message.clone()).collect()
        }
        fn append_loop_event(&self, _event: LoopRecordedEvent) {}
        fn has_open_steps(&self) -> bool { false }
        fn clear(&self) {}
    }
    impl TurnTools for Dummy {
        fn loop_tools(&self) -> Vec<Arc<dyn ExecutableTool>> { vec![] }
        fn store_data(&self) -> std::collections::HashMap<String, serde_json::Value> {
            std::collections::HashMap::new()
        }
    }
    #[async_trait::async_trait]
    impl TurnSessionMode for Dummy {
        fn is_active(&self) -> bool { false }
        fn kind(&self) -> Option<String> { None }
        fn file_path(&self) -> Option<String> { None }
        async fn data(&self) -> Option<String> { None }
    }
    #[async_trait::async_trait]
    impl TurnFullCompaction for Dummy {
        fn reset_for_turn(&self, _agent: Arc<dyn TurnAgent>) {}
        async fn before_step(&self, _agent: Arc<dyn TurnAgent>, _signal: AbortSignal) {}
        async fn after_step(&self, _agent: Arc<dyn TurnAgent>) {}
        async fn handle_overflow_error(&self, _agent: Arc<dyn TurnAgent>, _signal: AbortSignal, _error: anyhow::Error) {}
        fn begin(&self, _agent: Arc<dyn TurnAgent>, _data: CompactionBeginData) {}
        fn cancel(&self, _agent: Arc<dyn TurnAgent>) {}
        fn compacted_history(&self) -> Vec<CompactedHistory> { vec![] }
        fn is_compacting(&self) -> bool { false }
    }
    impl TurnMicroCompaction for Dummy {
        fn detect(&self, _agent: Arc<dyn TurnAgent>) {}
        fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage> { messages.to_vec() }
        fn reset(&self, _max_cutoff: usize) {}
    }
    #[async_trait::async_trait]
    impl TurnSplitPlanCheckpoint for Dummy {
        async fn before_step(&self, _agent: Arc<dyn TurnAgent>, _signal: AbortSignal) {}
        fn reset(&self) {}
    }
    #[async_trait::async_trait]
    impl TurnNormalTaskCheckpoint for Dummy {
        async fn before_step(&self, _agent: Arc<dyn TurnAgent>, _signal: AbortSignal) {}
        fn reset(&self) {}
    }
    #[async_trait::async_trait]
    impl TurnLlmResolver for Dummy {
        fn refresh_llm(&self) {}
        fn llm(&self) -> Arc<dyn crate::agent_loop::llm::Llm> { panic!("noop") }
        async fn generate_one_off(
            &self,
            _provider: Box<dyn ChatProvider>,
            _system_prompt: String,
            _tools: Vec<Tool>,
            _messages: Vec<Message>,
            _signal: AbortSignal,
        ) -> Result<CompactGenerateResult, anyhow::Error> {
            Ok(CompactGenerateResult::default())
        }
    }

    let _: Arc<dyn TurnContext> = Arc::new(Dummy);
    let _: Arc<dyn TurnTools> = Arc::new(Dummy);
    let _: Arc<dyn TurnSessionMode> = Arc::new(Dummy);
}
```

- [ ] 运行测试并验证它**编译失败**（因为新方法和类型尚未定义）：

```bash
cd rust-ody && cargo test -p agent-rs --lib turn::types::tests::compaction_surface_is_implementable
```

预期失败：出现 `cannot find ... in this scope` / `method not found` 等编译错误。

- [ ] 修改 `turn/types.rs`，在文件顶部新增两个数据类型并在对应 trait 中扩展方法：

```rust
// 在 turn/types.rs 靠近顶部、LoopControl 之后新增
#[derive(Debug, Clone, Default)]
pub struct CompactedHistory {
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct CompactGenerateResult {
    pub text: String,
    pub finish_reason: Option<kosong_rs::provider::FinishReason>,
    pub usage: kosong_rs::usage::TokenUsage,
}
```

并修改/扩展 trait（保持原有方法不变，仅展示新增/变更部分）：

```rust
#[async_trait::async_trait]
pub trait TurnContext: Send + Sync {
    // ... 保留原有方法 ...
    fn history(&self) -> Vec<ContextMessage>;
    fn token_count(&self) -> i64;
    fn token_count_with_pending(&self) -> i64;
    fn apply_compaction(&self, result: CompactionResult);
    fn project(&self, messages: &[ContextMessage]) -> Vec<Message>;
}

pub trait TurnTools: Send + Sync {
    fn loop_tools(&self) -> Vec<Arc<dyn ExecutableTool>>;
    fn store_data(&self) -> std::collections::HashMap<String, serde_json::Value>;
}

#[async_trait::async_trait]
pub trait TurnSessionMode: Send + Sync {
    fn is_active(&self) -> bool;
    fn kind(&self) -> Option<String>;
    fn file_path(&self) -> Option<String>;
    async fn data(&self) -> Option<String>;
}

#[async_trait::async_trait]
pub trait TurnFullCompaction: Send + Sync {
    fn reset_for_turn(&self, agent: Arc<dyn TurnAgent>);
    async fn before_step(&self, agent: Arc<dyn TurnAgent>, signal: AbortSignal);
    async fn after_step(&self, agent: Arc<dyn TurnAgent>);
    async fn handle_overflow_error(&self, agent: Arc<dyn TurnAgent>, signal: AbortSignal, error: anyhow::Error);
    fn begin(&self, agent: Arc<dyn TurnAgent>, data: CompactionBeginData);
    fn cancel(&self, agent: Arc<dyn TurnAgent>);
    fn compacted_history(&self) -> Vec<CompactedHistory>;
    fn is_compacting(&self) -> bool;
}

pub trait TurnMicroCompaction: Send + Sync {
    fn detect(&self, agent: Arc<dyn TurnAgent>);
    fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage>;
    fn reset(&self, max_cutoff: usize);
}

#[async_trait::async_trait]
pub trait TurnSplitPlanCheckpoint: Send + Sync {
    async fn before_step(&self, agent: Arc<dyn TurnAgent>, signal: AbortSignal);
    fn reset(&self);
}

#[async_trait::async_trait]
pub trait TurnNormalTaskCheckpoint: Send + Sync {
    async fn before_step(&self, agent: Arc<dyn TurnAgent>, signal: AbortSignal);
    fn reset(&self);
}

#[async_trait::async_trait]
pub trait TurnLlmResolver: Send + Sync {
    fn refresh_llm(&self);
    fn llm(&self) -> Arc<dyn crate::agent_loop::llm::Llm>;
    async fn generate_one_off(
        &self,
        provider: Box<dyn ChatProvider>,
        system_prompt: String,
        tools: Vec<kosong_rs::provider::Tool>,
        messages: Vec<Message>,
        signal: AbortSignal,
    ) -> Result<CompactGenerateResult, anyhow::Error>;
}
```

- [ ] 更新 `fixture_agent.rs`：
  - `TurnContext` 实现新增 `history` / `token_count` / `token_count_with_pending` / `apply_compaction` / `project`；
  - `TurnTools` 实现新增 `store_data`（返回空 `HashMap`）；
  - `TurnSessionMode` 改为 `#[async_trait]` 并新增 `file_path` / `data`；
  - `TurnFullCompaction` / `TurnMicroCompaction` / `TurnSplitPlanCheckpoint` / `TurnNormalTaskCheckpoint` 按新签名实现为 stub，仅保留 `reset_for_turn` 计数与 `full_compaction_reset` 字段原有语义；
  - `TurnLlmResolver` 实现新增 `generate_one_off`，返回 `Err(anyhow!("not configured"))`。

- [ ] 更新 `turn_flow.rs` 中 `BeforeStepHookImpl::before_step` 与 `AfterStepHookImpl::after_step`，把 `self.flow.agent.clone()` 传给 compaction 调用：

```rust
// BeforeStepHookImpl::before_step
self.flow.agent.full_compaction().reset_for_turn(self.flow.agent.clone());
self.flow.agent.micro_compaction().detect(self.flow.agent.clone());
self.flow.agent.full_compaction().before_step(self.flow.agent.clone(), ctx.signal.clone()).await;
self.flow.agent.split_plan_checkpoint().before_step(self.flow.agent.clone(), ctx.signal.clone()).await;
self.flow.agent.normal_mode_task_checkpoint().before_step(self.flow.agent.clone(), ctx.signal.clone()).await;

// AfterStepHookImpl::after_step
self.flow.agent.full_compaction().after_step(self.flow.agent.clone()).await;
```

- [ ] 在 `lib.rs` 新增 `pub mod compaction;`：

```rust
pub mod agent_loop;
pub mod compaction;
pub mod config;
// ...
```

- [ ] 运行测试与全 workspace typecheck：

```bash
cd rust-ody && cargo test -p agent-rs --lib turn::types::tests::compaction_surface_is_implementable
pnpm -r typecheck
cargo test -p agent-rs
```

预期：`compaction_surface_is_implementable` 通过；`pnpm -r typecheck` 与 `cargo test -p agent-rs` 全绿。

- [ ] Commit：`feat(agent-rs): extend TurnAgent traits for compaction`

---

## Task 2: 实现 completion-budget helper 与 `estimate_tokens_for_tools`

**Depends on:** Task 1

**Files:**

- Create: `rust-ody/crates/agent-rs/src/compaction/budget.rs`
- Modify: `rust-ody/crates/agent-rs/src/context/tokens.rs:19-52`（新增 `estimate_tokens_for_tools`）
- Modify: `rust-ody/crates/agent-rs/src/compaction/mod.rs`（在 Task 5 中创建并 re-export）
- Test: `rust-ody/crates/agent-rs/src/compaction/budget.rs`（内联单元测试）
- Test: `rust-ody/crates/agent-rs/src/context/tokens.rs`（内联单元测试）

### 步骤

- [ ] 在 `context/tokens.rs` 新增 `estimate_tokens_for_tools`：

```rust
pub fn estimate_tokens_for_tools(tools: &[kosong_rs::provider::Tool]) -> i64 {
    tools
        .iter()
        .map(|t| {
            estimate_tokens(&t.name)
                + estimate_tokens(&t.description)
                + estimate_tokens(&serde_json::to_string(&t.parameters).unwrap_or_default())
        })
        .sum()
}
```

并新增内联测试：

```rust
#[test]
fn estimate_tokens_for_tools_counts_name_description_params() {
    let tools = vec![kosong_rs::provider::Tool {
        name: "read".into(),
        description: "read a file".into(),
        parameters: serde_json::json!({"path": {"type": "string"}}),
    }];
    let tokens = estimate_tokens_for_tools(&tools);
    assert!(tokens > 0);
}
```

- [ ] 创建 `compaction/budget.rs`，完整移植 TS `packages/agent-core/src/utils/completion-budget.ts`：

```rust
use kosong_rs::provider::{ChatProvider, ModelCapability};

pub const MIN_FLOOR: i64 = 1;
pub const DEFAULT_UNKNOWN_OUTPUT_FALLBACK: i64 = 32000;

const CONTEXT_WINDOW_OVERHEAD_TOKENS: i64 = 8192;
const MAX_CONTEXT_COMPLETION_RATIO: f64 = 0.25;

#[derive(Debug, Clone, Default)]
pub struct CompletionBudgetConfig {
    pub hard_cap: Option<i64>,
    pub fallback: Option<i64>,
}

pub fn resolve_completion_budget(reserved_context_size: Option<i64>) -> Option<CompletionBudgetConfig> {
    if let Some(size) = reserved_context_size {
        if size > 0 {
            return Some(CompletionBudgetConfig {
                hard_cap: None,
                fallback: Some(size),
            });
        }
        return None;
    }
    Some(CompletionBudgetConfig {
        hard_cap: None,
        fallback: Some(DEFAULT_UNKNOWN_OUTPUT_FALLBACK),
    })
}

pub fn compute_completion_budget_cap(
    budget: &CompletionBudgetConfig,
    capability: &ModelCapability,
    input_tokens: Option<i64>,
) -> i64 {
    let max_output = capability.max_output_tokens.unwrap_or(0);
    let max_context = capability.max_context_tokens.unwrap_or(0);

    let mut cap = budget.hard_cap.unwrap_or_else(|| {
        if max_output > 0 {
            max_output
        } else {
            budget.fallback.unwrap_or(DEFAULT_UNKNOWN_OUTPUT_FALLBACK)
        }
    });

    if max_context > 0 {
        if let Some(input) = input_tokens {
            if input > 0 {
                let remaining = max_context - input - CONTEXT_WINDOW_OVERHEAD_TOKENS;
                cap = cap.min(remaining.max(MIN_FLOOR));
            }
        }
        cap = cap.min((max_context as f64 * MAX_CONTEXT_COMPLETION_RATIO).floor() as i64);
    }

    cap.max(MIN_FLOOR)
}

pub fn apply_completion_budget(
    provider: Box<dyn ChatProvider>,
    budget: Option<&CompletionBudgetConfig>,
    capability: &ModelCapability,
    input_tokens: Option<i64>,
) -> Box<dyn ChatProvider> {
    let Some(budget) = budget else { return provider };
    let Some(provider) = provider.with_max_completion_tokens(compute_completion_budget_cap(
        budget,
        capability,
        input_tokens,
    )) else {
        return provider;
    };
    provider
}
```

- [ ] 新增内联测试覆盖阈值边界：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use kosong_rs::provider::ModelCapability;

    #[test]
    fn resolve_uses_reserved_context_size() {
        let cfg = resolve_completion_budget(Some(50_000));
        assert_eq!(cfg.unwrap().fallback, Some(50_000));
    }

    #[test]
    fn cap_is_limited_by_context_window_ratio() {
        let mut cap = ModelCapability::unknown();
        cap.max_context_tokens = Some(128_000);
        cap.max_output_tokens = Some(16_000);
        let budget = CompletionBudgetConfig {
            hard_cap: None,
            fallback: Some(32_000),
        };
        let result = compute_completion_budget_cap(&budget, &cap, Some(10_000));
        assert_eq!(result, (128_000.0 * 0.25).floor() as i64);
    }

    #[test]
    fn cap_respects_min_floor() {
        let mut cap = ModelCapability::unknown();
        cap.max_context_tokens = Some(100);
        let budget = CompletionBudgetConfig {
            hard_cap: None,
            fallback: Some(32_000),
        };
        assert_eq!(compute_completion_budget_cap(&budget, &cap, Some(10_000)), MIN_FLOOR);
    }
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p agent-rs budget
```

预期：3 个 budget 测试绿。

- [ ] Commit：`feat(agent-rs): add completion-budget helpers`

---

## Task 3: 实现 `CompactionStrategy` / `DefaultCompactionStrategy`

**Depends on:** Task 1

**Files:**

- Create: `rust-ody/crates/agent-rs/src/compaction/types.rs`
- Create: `rust-ody/crates/agent-rs/src/compaction/strategy.rs`
- Modify: `rust-ody/crates/agent-rs/src/compaction/mod.rs`（Task 5 re-export）
- Test: `rust-ody/crates/agent-rs/src/compaction/strategy.rs`（内联单元测试）

### 步骤

- [ ] 创建 `compaction/types.rs`，re-export 已有类型并定义默认配置：

```rust
pub use crate::records::nested::{CompactionBeginData, CompactionResult, CompactionSource};

#[derive(Debug, Clone)]
pub struct CompactionConfig {
    pub trigger_ratio: f64,
    pub block_ratio: f64,
    pub reserved_context_size: i64,
    pub max_compaction_per_turn: i64,
    pub max_recent_messages: usize,
    pub max_recent_user_messages: usize,
    pub max_recent_size_ratio: f64,
    pub min_overflow_reduction_ratio: f64,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            trigger_ratio: 0.85,
            block_ratio: 0.85,
            reserved_context_size: 50_000,
            max_compaction_per_turn: 3,
            max_recent_messages: 4,
            max_recent_user_messages: usize::MAX,
            max_recent_size_ratio: 0.2,
            min_overflow_reduction_ratio: 0.05,
        }
    }
}
```

- [ ] 创建 `compaction/strategy.rs`，完整移植 `packages/agent-core/src/agent/compaction/strategy.ts`：

```rust
use kosong_rs::message::{Message, Role};

use crate::context::tokens::estimate_tokens_for_message;
use crate::records::nested::{CompactionSource};

use super::types::CompactionConfig;

pub trait CompactionStrategy: Send + Sync {
    fn should_compact(&self, used_size: i64) -> bool;
    fn should_block(&self, used_size: i64) -> bool;
    fn compute_compact_count(&self, messages: &[Message], source: CompactionSource) -> usize;
    fn reduce_compact_on_overflow(&self, messages: &[Message]) -> usize;
    fn check_after_step(&self) -> bool;
    fn max_compaction_per_turn(&self) -> i64;
}

pub struct DefaultCompactionStrategy {
    max_size_provider: Box<dyn Fn() -> i64 + Send + Sync>,
    config: CompactionConfig,
}

impl DefaultCompactionStrategy {
    pub fn new<F>(max_size_provider: F, config: Option<CompactionConfig>) -> Self
    where
        F: Fn() -> i64 + Send + Sync + 'static,
    {
        Self {
            max_size_provider: Box::new(max_size_provider),
            config: config.unwrap_or_default(),
        }
    }

    fn max_size(&self) -> i64 {
        (self.max_size_provider)()
    }

    fn should_use_reserved_context(&self, used_size: i64) -> bool {
        let reserved = self.config.reserved_context_size;
        reserved > 0 && reserved < self.max_size() && used_size + reserved >= self.max_size()
    }
}

impl CompactionStrategy for DefaultCompactionStrategy {
    fn should_compact(&self, used_size: i64) -> bool {
        if self.max_size() <= 0 {
            return false;
        }
        used_size as f64 >= self.max_size() as f64 * self.config.trigger_ratio
            || self.should_use_reserved_context(used_size)
    }

    fn should_block(&self, used_size: i64) -> bool {
        if self.max_size() <= 0 {
            return false;
        }
        used_size as f64 >= self.max_size() as f64 * self.config.block_ratio
            || self.should_use_reserved_context(used_size)
    }

    fn compute_compact_count(&self, messages: &[Message], source: CompactionSource) -> usize {
        if messages.is_empty() {
            return 0;
        }

        if source == CompactionSource::Manual {
            for i in (1..messages.len()).rev() {
                if can_split_after(messages, i - 1) {
                    return i;
                }
            }
            return 0;
        }

        let mut recent_messages = 1usize;
        let mut recent_user_messages = 0usize;
        let mut recent_size = 0i64;
        let mut best_n: Option<usize> = None;

        while recent_messages < messages.len() {
            let split_index = messages.len() - recent_messages - 1;
            let m2 = &messages[messages.len() - recent_messages];

            if m2.role == Role::User {
                recent_user_messages += 1;
            }
            recent_size += estimate_tokens_for_message(m2);

            if can_split_after(messages, split_index) {
                best_n = Some(split_index + 1);
            }

            let reaches_max = recent_messages >= self.config.max_recent_messages
                || recent_user_messages >= self.config.max_recent_user_messages
                || recent_size as f64 >= self.max_size() as f64 * self.config.max_recent_size_ratio;

            if reaches_max && best_n.is_some() {
                break;
            }
            recent_messages += 1;
        }

        best_n.unwrap_or(0)
    }

    fn reduce_compact_on_overflow(&self, messages: &[Message]) -> usize {
        let min_reduced_size = ((self.max_size() as f64 * self.config.min_overflow_reduction_ratio)
            .ceil() as i64)
            .max(1);
        let mut reduced_size = 0i64;
        let mut best_n: Option<usize> = None;

        for i in (1..messages.len().saturating_sub(1)).rev() {
            reduced_size += estimate_tokens_for_message(&messages[i + 1]);
            if can_split_after(messages, i) {
                best_n = Some(i + 1);
                if reduced_size >= min_reduced_size {
                    return i + 1;
                }
            }
        }

        best_n.unwrap_or(messages.len())
    }

    fn check_after_step(&self) -> bool {
        self.config.trigger_ratio != self.config.block_ratio
    }

    fn max_compaction_per_turn(&self) -> i64 {
        self.config.max_compaction_per_turn
    }
}

fn can_split_after(messages: &[Message], index: usize) -> bool {
    let Some(m) = messages.get(index) else {
        return false;
    };
    if m.role == Role::User {
        return false;
    }
    if m.role == Role::Assistant && !m.tool_calls.is_empty() {
        return false;
    }
    if messages.get(index + 1).map(|m| m.role) == Some(Role::Tool) {
        return false;
    }
    true
}
```

- [ ] 新增内联测试，覆盖 manual/auto、safe split、overflow 回退：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use kosong_rs::message::{ContentPart, Message, Role, ToolCall};

    fn make_message(role: Role, text: &str) -> Message {
        Message {
            role,
            name: None,
            content: vec![ContentPart::Text { text: text.into() }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        }
    }

    fn make_asst_with_tool(text: &str, tool_call_id: &str) -> Message {
        Message {
            role: Role::Assistant,
            name: None,
            content: vec![ContentPart::Text { text: text.into() }],
            tool_calls: vec![ToolCall {
                call_type: "function".into(),
                id: tool_call_id.into(),
                name: "read".into(),
                arguments: Some("{}".into()),
                extras: None,
                stream_index: None,
            }],
            tool_call_id: None,
            partial: None,
        }
    }

    fn strategy_with_max(max: i64) -> DefaultCompactionStrategy {
        DefaultCompactionStrategy::new(move || max, None)
    }

    #[test]
    fn should_compact_when_crossing_trigger_ratio() {
        let s = strategy_with_max(100);
        assert!(!s.should_compact(80));
        assert!(s.should_compact(86));
    }

    #[test]
    fn manual_compaction_prefers_latest_safe_split() {
        let messages = vec![
            make_message(Role::User, "u1"),
            make_message(Role::Assistant, "a1"),
            make_message(Role::User, "u2"),
            make_message(Role::Assistant, "a2"),
        ];
        // split after a1 -> compact first 2 messages
        assert_eq!(s.compute_compact_count(&messages, CompactionSource::Manual), 2);
    }

    #[test]
    fn auto_preserves_at_least_one_recent_message() {
        let messages = vec![
            make_message(Role::User, "u1"),
            make_message(Role::Assistant, "a1"),
            make_message(Role::User, "u2"),
            make_message(Role::Assistant, "a2"),
            make_message(Role::User, "u3"),
        ];
        let s = strategy_with_max(10_000);
        // can split after a1 (index 1), preserving u2/a2/u3
        assert_eq!(s.compute_compact_count(&messages, CompactionSource::Auto), 2);
    }

    #[test]
    fn cannot_split_after_asst_with_tool_calls() {
        let messages = vec![
            make_message(Role::User, "u1"),
            make_asst_with_tool("a1", "tc1"),
            make_message(Role::Tool, "r1"),
        ];
        let s = strategy_with_max(10_000);
        assert_eq!(s.compute_compact_count(&messages, CompactionSource::Auto), 0);
    }

    #[test]
    fn reduce_compact_on_overflow_finds_shortest_safe_prefix() {
        let messages = vec![
            make_message(Role::User, "u1"),
            make_message(Role::Assistant, "a1"),
            make_message(Role::User, "u2"),
            make_message(Role::Assistant, "a2"),
        ];
        let s = strategy_with_max(10_000);
        // 从后往前，先找到 split after a1 -> return 2
        assert_eq!(s.reduce_compact_on_overflow(&messages), 2);
    }
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p agent-rs strategy
```

预期：所有 strategy 测试绿。

- [ ] Commit：`feat(agent-rs): add CompactionStrategy`

---

## Task 4: 实现 `render_messages_to_text`

**Depends on:** Task 1

**Files:**

- Create: `rust-ody/crates/agent-rs/src/compaction/render_messages.rs`
- Modify: `rust-ody/crates/agent-rs/src/compaction/mod.rs`（Task 5 re-export）
- Test: `rust-ody/crates/agent-rs/src/compaction/render_messages.rs`（内联单元测试）

### 步骤

- [ ] 创建 `compaction/render_messages.rs`，完整移植 `packages/agent-core/src/agent/compaction/render-messages.ts`：

```rust
use kosong_rs::message::{ContentPart, Message, ToolCall};
use serde_json::Value;

pub fn render_messages_to_text(messages: &[Message]) -> String {
    messages
        .iter()
        .enumerate()
        .map(|(index, message)| render_message_to_text(message, index))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_message_to_text(message: &Message, index: usize) -> String {
    let mut header = vec![
        format!("message {}", index + 1),
        format!("role={:?}", message.role).to_lowercase(),
    ];
    if let Some(name) = &message.name {
        header.push(format!("name={}", serde_json::to_string(name).unwrap_or_default()));
    }
    if let Some(tool_call_id) = &message.tool_call_id {
        header.push(format!("toolCallId={}", serde_json::to_string(tool_call_id).unwrap_or_default()));
    }
    if message.partial == Some(true) {
        header.push("partial=true".into());
    }

    let mut lines = vec![format!("--- {} ---", header.join(" "))];
    if message.content.is_empty() {
        lines.push("[empty content]".into());
    } else {
        lines.extend(message.content.iter().map(render_content_part_to_text));
    }

    if !message.tool_calls.is_empty() {
        lines.push("tool calls:".into());
        for tool_call in &message.tool_calls {
            lines.push(render_tool_call_to_text(tool_call));
        }
    }

    lines.join("\n")
}

fn render_content_part_to_text(part: &ContentPart) -> String {
    match part {
        ContentPart::Text { text } => render_block("text", text),
        ContentPart::Think { think, .. } => render_block("think", think),
        ContentPart::ImageUrl { image_url } => render_media_part("image_url", &image_url.url, image_url.id.as_deref()),
        ContentPart::AudioUrl { audio_url } => render_media_part("audio_url", &audio_url.url, audio_url.id.as_deref()),
        ContentPart::VideoUrl { video_url } => render_media_part("video_url", &video_url.url, video_url.id.as_deref()),
        _ => render_block("content", &stringify_jsonish(&serde_json::to_value(part).unwrap_or_default())),
    }
}

fn render_tool_call_to_text(tool_call: &ToolCall) -> String {
    let mut lines = vec![
        format!("- {}: {}", tool_call.id, tool_call.name),
        render_block("arguments", &render_tool_call_arguments(tool_call.arguments.as_deref())),
    ];
    if let Some(extras) = &tool_call.extras {
        lines.push(render_block("extras", &stringify_jsonish(extras)));
    }
    lines.join("\n")
}

fn render_tool_call_arguments(args: Option<&str>) -> String {
    match args {
        None => "null".into(),
        Some(args) => match serde_json::from_str::<Value>(args) {
            Ok(value) => stringify_jsonish(&value),
            Err(_) => args.into(),
        },
    }
}

fn render_media_part(kind: &str, url: &str, id: Option<&str>) -> String {
    match id {
        None => format!("{}: {}", kind, url),
        Some(id) => format!("{}: {} (id={})", kind, url, id),
    }
}

fn render_block(label: &str, value: &str) -> String {
    format!("{}:\n{}", label, indent_block(value))
}

fn indent_block(value: &str) -> String {
    if value.is_empty() {
        return "  ".into();
    }
    value
        .split('\n')
        .map(|line| format!("  {}", line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn stringify_jsonish(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}
```

- [ ] 新增内联测试，覆盖 text/think/tool-call/media/header：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use kosong_rs::message::{ContentPart, Message, Role, ToolCall};

    #[test]
    fn renders_text_message() {
        let messages = vec![Message {
            role: Role::User,
            name: None,
            content: vec![ContentPart::Text { text: "hello".into() }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        }];
        let text = render_messages_to_text(&messages);
        assert!(text.contains("message 1"));
        assert!(text.contains("role=user"));
        assert!(text.contains("text:"));
        assert!(text.contains("  hello"));
    }

    #[test]
    fn renders_tool_call() {
        let messages = vec![Message {
            role: Role::Assistant,
            name: None,
            content: vec![],
            tool_calls: vec![ToolCall {
                call_type: "function".into(),
                id: "call-1".into(),
                name: "read".into(),
                arguments: Some(r#"{"path":"/a"}"#.into()),
                extras: None,
                stream_index: None,
            }],
            tool_call_id: None,
            partial: None,
        }];
        let text = render_messages_to_text(&messages);
        assert!(text.contains("tool calls:"));
        assert!(text.contains("- call-1: read"));
        assert!(text.contains("arguments:"));
        assert!(text.contains('"' + "path" + '"' + ":"));
    }
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p agent-rs render_messages
```

预期：render_messages 测试绿。

- [ ] Commit：`feat(agent-rs): add render_messages_to_text`

---

## Task 5: 创建 `compaction` 模块入口

**Depends on:** Task 2-4

**Files:**

- Create: `rust-ody/crates/agent-rs/src/compaction/mod.rs`
- Modify: `rust-ody/crates/agent-rs/src/lib.rs:1-10`（已在 Task 1 添加 `pub mod compaction;`）

### 步骤

- [ ] 创建 `compaction/mod.rs`：

```rust
pub mod budget;
pub mod render_messages;
pub mod strategy;
pub mod types;

pub use budget::*;
pub use render_messages::render_messages_to_text;
pub use strategy::*;
pub use types::*;
```

- [ ] 运行整 crate 编译：

```bash
cd rust-ody && cargo check -p agent-rs
```

预期：无编译错误。

- [ ] Commit：`feat(agent-rs): wire compaction module`

---

## Local Self-Review

- [ ] 1. Spec-coverage：Task 1 覆盖所有 trait 扩展；Task 2 覆盖 budget helper；Task 3 覆盖 4.3.6.1；Task 4 覆盖 4.3.6.5；Task 5 为 wiring。本 part 无 GAP。
- [ ] 2. Placeholder scan：所有代码片段完整，无 TODO/TBD。
- [ ] 3. No phantom tasks：每个 task 产出文件变更与可验证测试/编译。
- [ ] 4. Dependency soundness：Task 1 为后续 task 硬前置；Task 2/3/4 彼此独立，均依赖 Task 1；Task 5 依赖 2/3/4。
- [ ] 5. Caller & build soundness：Task 1 共享签名变更同步更新 `fixture_agent.rs` 与 `turn_flow.rs`，并以 `pnpm -r typecheck` + `cargo test -p agent-rs` 结束。
- [ ] 6. Test-the-risk：strategy 测试覆盖 split 安全规则、recent 保留规则、overflow 回退；budget 测试覆盖阈值边界；render 测试覆盖 header/content/tool-call。
- [ ] 7. Type consistency：`CompactionBeginData` / `CompactionResult` / `CompactionSource` 复用 `records::nested` 已定义类型；`CompactGenerateResult` / `CompactedHistory` 在 `turn/types.rs` 定义，避免 turn↔compaction 循环依赖。
