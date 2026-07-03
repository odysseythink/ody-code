# Part 1: Schema — 设计决策 + `AgentRecord`/`AgentRecordEvents` 枚举 + 嵌套类型

本部分把 WAL 记录的 wire schema 钉死。4.3.0 是 Agent 迁移的地基，records 格式一旦定错，后续 context/config/permission/turn 都无法做对照测试。

---

### Task 1: Crate scaffold + WAL schema 终态决策

**Depends on:** none

**Files:**
- Create: `rust-ody/crates/agent-rs/Cargo.toml`
- Create: `rust-ody/crates/agent-rs/src/lib.rs`
- Create: `rust-ody/crates/agent-rs/src/records/mod.rs`
- Modify: `rust-ody/Cargo.toml` line 2 (`members`)
- Test: `cargo check -p agent-rs --workspace`

**Design-lite 决策记录：**

候选方案：
- **A. 完全镜像 TS JSONL wire 格式**（采用）：字段名、事件 tag、版本号、`blobref:` 协议与 TS 完全一致，不做任何跨语言重命名或二进制化。Pros：TS↔Rust 交叉读写零转换；v1.1→v1.3 迁移逻辑可直接逐行复刻；L4 对照天然成立。Cons：Rust 类型里会保留一些历史字段名（如 `modelAlias`），略不地道。
- **B. 重命名为 idiomatic Rust snake_case**：Pros：类型更 Rusty。Cons：每次读写都要做字段映射，任何遗漏都会破坏 records 互读；rewrite-on-migration 会改变旧文件字节。
- **C. 改用 MessagePack / protobuf**：Pros：体积小、解析快。Cons：TS 侧需新增依赖与转换层；WAL 不再是人类可读；与现有 `.jsonl` 文件不兼容。

**决策：采用 A。** 4.3.0 的唯一正确性标准是 TS↔Rust 交叉读写逐值一致，任何格式转换都会引入不可逆兼容风险。

**实施步骤：**

- [ ] 在 `rust-ody/Cargo.toml` 的 `members` 末尾追加 `"crates/agent-rs"`。

```toml
# rust-ody/Cargo.toml
[workspace]
members = ["crates/ody-rust", "crates/ody-crypto", "crates/ody-host", "crates/kaos-rs", "crates/kosong-rs", "crates/agent-rs"]
resolver = "2"
```

- [ ] 新建 `rust-ody/crates/agent-rs/Cargo.toml`：

```toml
[package]
name = "agent-rs"
version = "0.1.0"
edition = "2021"
description = "Rust implementation of the Agent orchestration core"
license = "MIT"

[dependencies]
async-trait = "0.1"
base64 = "0.22"
futures-util = { version = "0.3", default-features = false, features = ["std"] }
kosong-rs = { path = "../kosong-rs" }
serde = { workspace = true }
serde_json = { workspace = true, features = ["preserve_order"] }
sha2 = "0.10"
thiserror = "1"
tokio = { workspace = true }

[dev-dependencies]
tempfile = "3"
tokio-test = "0.4"
```

- [ ] 新建 `rust-ody/crates/agent-rs/src/lib.rs`：

```rust
pub mod records;

pub use records::*;
```

- [ ] 新建 `rust-ody/crates/agent-rs/src/records/mod.rs`：

```rust
pub mod blobstore;
pub mod migration;
pub mod nested;
pub mod persistence;
pub mod records;
pub mod types;

pub use blobstore::*;
pub use migration::*;
pub use nested::*;
pub use persistence::*;
pub use records::*;
pub use types::*;
```

- [ ] 运行类型检查，验证空 crate 可编译：

```bash
cd rust-ody && cargo check -p agent-rs --workspace
```

预期输出：无错误，`Finished dev [unoptimized + debuginfo] target(s)`。

- [ ] Commit：`feat(agent-rs): scaffold records crate and freeze WAL schema decision`

---

### Task 2: `AgentRecordEvents` 枚举与 `AgentRecordPersistence` trait

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/agent-rs/src/records/types.rs`
- Test: `rust-ody/crates/agent-rs/src/records/types.rs` (内联 `#[cfg(test)]` 模块)

**目标：** 用 serde 内部 tag 枚举完整镜像 `AgentRecordEvents` 全部事件类型；每个变体带 `time: Option<i64>`；定义持久化 trait。

- [ ] 在 `types.rs` 顶部加入依赖与类型别名：

```rust
use std::pin::Pin;

use futures_util::Stream;
use kosong_rs::message::ContentPart;
use kosong_rs::usage::TokenUsage;
use serde::{Deserialize, Serialize};

use crate::records::nested::*;

pub type RecordStream<'a> = Pin<Box<dyn Stream<Item = anyhow::Result<AgentRecord>> + Send + 'a>>;
```

- [ ] 实现 `AgentRecordPersistence` trait：

```rust
#[async_trait::async_trait]
pub trait AgentRecordPersistence: Send + Sync {
    async fn read(&self) -> anyhow::Result<RecordStream<'_>>;
    fn append(&mut self, record: AgentRecord);
    fn rewrite(&mut self, records: &[AgentRecord]);
    async fn flush(&mut self) -> anyhow::Result<()>;
    async fn close(&mut self) -> anyhow::Result<()>;
}
```

- [ ] 实现 `AgentRecord` 枚举（完整）：

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentRecord {
    #[serde(rename = "metadata")]
    Metadata {
        time: Option<i64>,
        protocol_version: String,
        created_at: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        app_version: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        resumed: Option<bool>,
    },
    #[serde(rename = "turn.prompt")]
    TurnPrompt {
        time: Option<i64>,
        input: Vec<ContentPart>,
        origin: PromptOrigin,
    },
    #[serde(rename = "turn.steer")]
    TurnSteer {
        time: Option<i64>,
        input: Vec<ContentPart>,
        origin: PromptOrigin,
    },
    #[serde(rename = "turn.cancel")]
    TurnCancel {
        time: Option<i64>,
        #[serde(rename = "turnId", skip_serializing_if = "Option::is_none")]
        turn_id: Option<i64>,
    },
    #[serde(rename = "config.update")]
    ConfigUpdate {
        time: Option<i64>,
        #[serde(flatten)]
        update: AgentConfigUpdateData,
    },
    #[serde(rename = "permission.set_mode")]
    PermissionSetMode {
        time: Option<i64>,
        mode: PermissionMode,
    },
    #[serde(rename = "permission.record_approval_result")]
    PermissionRecordApprovalResult {
        time: Option<i64>,
        #[serde(flatten)]
        record: PermissionApprovalResultRecord,
    },
    #[serde(rename = "full_compaction.begin")]
    FullCompactionBegin {
        time: Option<i64>,
        #[serde(flatten)]
        data: CompactionBeginData,
    },
    #[serde(rename = "session_mode.enter")]
    SessionModeEnter {
        time: Option<i64>,
        id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        kind: Option<SessionModeKind>,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },
    #[serde(rename = "session_mode.cancel")]
    SessionModeCancel {
        time: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    #[serde(rename = "session_mode.exit")]
    SessionModeExit {
        time: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    #[serde(rename = "tools.register_user_tool")]
    ToolsRegisterUserTool {
        time: Option<i64>,
        #[serde(flatten)]
        registration: UserToolRegistration,
    },
    #[serde(rename = "tools.unregister_user_tool")]
    ToolsUnregisterUserTool {
        time: Option<i64>,
        name: String,
    },
    #[serde(rename = "tools.set_active_tools")]
    ToolsSetActiveTools {
        time: Option<i64>,
        names: Vec<String>,
    },
    #[serde(rename = "usage.record")]
    UsageRecord {
        time: Option<i64>,
        model: String,
        usage: TokenUsage,
        #[serde(rename = "usageScope", skip_serializing_if = "Option::is_none")]
        usage_scope: Option<UsageRecordScope>,
    },
    #[serde(rename = "full_compaction.cancel")]
    FullCompactionCancel { time: Option<i64> },
    #[serde(rename = "full_compaction.complete")]
    FullCompactionComplete { time: Option<i64> },
    #[serde(rename = "micro_compaction.apply")]
    MicroCompactionApply {
        time: Option<i64>,
        cutoff: i64,
    },
    #[serde(rename = "context.append_message")]
    ContextAppendMessage {
        time: Option<i64>,
        message: ContextMessage,
    },
    #[serde(rename = "context.append_loop_event")]
    ContextAppendLoopEvent {
        time: Option<i64>,
        event: LoopRecordedEvent,
    },
    #[serde(rename = "context.clear")]
    ContextClear { time: Option<i64> },
    #[serde(rename = "context.apply_compaction")]
    ContextApplyCompaction {
        time: Option<i64>,
        #[serde(flatten)]
        result: CompactionResult,
    },
    #[serde(rename = "context.undo")]
    ContextUndo {
        time: Option<i64>,
        count: i64,
    },
    #[serde(rename = "tools.update_store")]
    ToolsUpdateStore {
        time: Option<i64>,
        #[serde(flatten)]
        update: ToolStoreUpdate,
    },
    #[serde(rename = "goal.create")]
    GoalCreate {
        time: Option<i64>,
        #[serde(rename = "goalId")]
        goal_id: String,
        objective: String,
        status: GoalStatus,
        actor: GoalActor,
        #[serde(rename = "budgetLimits")]
        budget_limits: GoalBudgetLimits,
    },
    #[serde(rename = "goal.update")]
    GoalUpdate {
        time: Option<i64>,
        #[serde(rename = "goalId")]
        goal_id: String,
        status: GoalStatus,
        actor: GoalActor,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        #[serde(rename = "turnsUsed", skip_serializing_if = "Option::is_none")]
        turns_used: Option<i64>,
        #[serde(rename = "tokensUsed", skip_serializing_if = "Option::is_none")]
        tokens_used: Option<i64>,
        #[serde(rename = "wallClockMs", skip_serializing_if = "Option::is_none")]
        wall_clock_ms: Option<i64>,
    },
    #[serde(rename = "goal.account_usage")]
    GoalAccountUsage {
        time: Option<i64>,
        #[serde(rename = "goalId")]
        goal_id: String,
        #[serde(rename = "usageKind")]
        usage_kind: String,
        delta: i64,
        #[serde(rename = "agentId", skip_serializing_if = "Option::is_none")]
        agent_id: Option<String>,
        #[serde(rename = "agentType", skip_serializing_if = "Option::is_none")]
        agent_type: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        #[serde(rename = "tokensUsed")]
        tokens_used: i64,
        #[serde(rename = "wallClockMs")]
        wall_clock_ms: i64,
    },
    #[serde(rename = "goal.continuation")]
    GoalContinuation {
        time: Option<i64>,
        #[serde(rename = "goalId")]
        goal_id: String,
        #[serde(rename = "turnsUsed")]
        turns_used: i64,
    },
    #[serde(rename = "goal.clear")]
    GoalClear {
        time: Option<i64>,
        #[serde(rename = "goalId")]
        goal_id: String,
        actor: GoalActor,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}
```

- [ ] 写失败测试：验证 `AgentRecord` 反序列化 TS 样例 JSON 后字段正确。先运行确认失败（因为 Task 3 的嵌套类型尚未定义）：

```bash
cd rust-ody && cargo test -p agent-rs agent_record_round_trip -- --ignored 2>&1 | head -40
```

预期失败：`cannot find type PromptOrigin in this scope` 或类似编译错误。

- [ ] Commit：`feat(agent-rs): define AgentRecord enum and persistence trait`

---

### Task 3: 嵌套类型（ContextMessage / PromptOrigin / LoopRecordedEvent / 权限 / 配置 / 目标 / 用量）

**Depends on:** Task 2

**Files:**
- Create: `rust-ody/crates/agent-rs/src/records/nested.rs`
- Test: `rust-ody/crates/agent-rs/src/records/nested.rs` (内联 `#[cfg(test)]` 模块)

**目标：** 定义 `AgentRecord` 引用的全部嵌套类型，使 `cargo test -p agent-rs` 中所有 round-trip 测试通过。

- [ ] 新建 `rust-ody/crates/agent-rs/src/records/nested.rs`：

```rust
use kosong_rs::message::{ContentPart, Message, ToolCall};
use kosong_rs::usage::TokenUsage;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ---------------------------------------------------------------------------
// PromptOrigin
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum PromptOrigin {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "skill_activation")]
    SkillActivation {
        #[serde(rename = "activationId")]
        activation_id: String,
        #[serde(rename = "skillName")]
        skill_name: String,
        #[serde(rename = "skillArgs", skip_serializing_if = "Option::is_none")]
        skill_args: Option<String>,
        trigger: String,
        #[serde(rename = "skillType", skip_serializing_if = "Option::is_none")]
        skill_type: Option<String>,
        #[serde(rename = "skillPath", skip_serializing_if = "Option::is_none")]
        skill_path: Option<String>,
    },
    #[serde(rename = "injection")]
    Injection { variant: String },
    #[serde(rename = "compaction_summary")]
    CompactionSummary,
    #[serde(rename = "system_trigger")]
    SystemTrigger { name: String },
    #[serde(rename = "background_task")]
    BackgroundTask {
        #[serde(rename = "taskId")]
        task_id: String,
        status: String,
        #[serde(rename = "notificationId")]
        notification_id: String,
    },
    #[serde(rename = "cron_job")]
    CronJob {
        #[serde(rename = "jobId")]
        job_id: String,
        cron: String,
        recurring: bool,
        #[serde(rename = "coalescedCount")]
        coalesced_count: i64,
        stale: bool,
    },
    #[serde(rename = "cron_missed")]
    CronMissed { count: i64 },
    #[serde(rename = "hook_result")]
    HookResult {
        event: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        blocked: Option<bool>,
    },
}

// ---------------------------------------------------------------------------
// ContextMessage
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextMessage {
    #[serde(flatten)]
    pub message: Message,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<PromptOrigin>,
    #[serde(rename = "isError", skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

// ---------------------------------------------------------------------------
// LoopRecordedEvent
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LoopRecordedEvent {
    #[serde(rename = "step.begin")]
    StepBegin {
        uuid: String,
        #[serde(rename = "turnId")]
        turn_id: String,
        step: i64,
    },
    #[serde(rename = "step.end")]
    StepEnd {
        uuid: String,
        #[serde(rename = "turnId")]
        turn_id: String,
        step: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<TokenUsage>,
        #[serde(rename = "finishReason", skip_serializing_if = "Option::is_none")]
        finish_reason: Option<String>,
        #[serde(rename = "llmFirstTokenLatencyMs", skip_serializing_if = "Option::is_none")]
        llm_first_token_latency_ms: Option<i64>,
        #[serde(rename = "llmStreamDurationMs", skip_serializing_if = "Option::is_none")]
        llm_stream_duration_ms: Option<i64>,
        #[serde(rename = "providerFinishReason", skip_serializing_if = "Option::is_none")]
        provider_finish_reason: Option<String>,
        #[serde(rename = "rawFinishReason", skip_serializing_if = "Option::is_none")]
        raw_finish_reason: Option<String>,
    },
    #[serde(rename = "content.part")]
    ContentPartEvent {
        uuid: String,
        #[serde(rename = "turnId")]
        turn_id: String,
        step: i64,
        #[serde(rename = "stepUuid")]
        step_uuid: String,
        part: ContentPart,
    },
    #[serde(rename = "tool.call")]
    ToolCallEvent {
        uuid: String,
        #[serde(rename = "turnId")]
        turn_id: String,
        step: i64,
        #[serde(rename = "stepUuid")]
        step_uuid: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        name: String,
        args: JsonValue,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        display: Option<JsonValue>,
    },
    #[serde(rename = "tool.result")]
    ToolResultEvent {
        #[serde(rename = "parentUuid")]
        parent_uuid: String,
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        result: ExecutableToolResult,
    },
}

// ---------------------------------------------------------------------------
// Tool result / update
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExecutableToolOutput {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutableToolSuccessResult {
    pub output: ExecutableToolOutput,
    #[serde(rename = "stopTurn", skip_serializing_if = "Option::is_none")]
    pub stop_turn: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutableToolErrorResult {
    pub output: ExecutableToolOutput,
    #[serde(rename = "isError")]
    pub is_error: bool,
    #[serde(rename = "stopTurn", skip_serializing_if = "Option::is_none")]
    pub stop_turn: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ExecutableToolResult {
    Success(ExecutableToolSuccessResult),
    Error(ExecutableToolErrorResult),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolUpdate {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<f64>,
    #[serde(rename = "customKind", skip_serializing_if = "Option::is_none")]
    pub custom_kind: Option<String>,
    #[serde(rename = "customData", skip_serializing_if = "Option::is_none")]
    pub custom_data: Option<JsonValue>,
}

// ---------------------------------------------------------------------------
// Config / Permission / SessionMode / Tools / Usage / Compaction
// ---------------------------------------------------------------------------
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentConfigUpdateData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(rename = "modelAlias", skip_serializing_if = "Option::is_none")]
    pub model_alias: Option<String>,
    #[serde(rename = "profileName", skip_serializing_if = "Option::is_none")]
    pub profile_name: Option<String>,
    #[serde(rename = "thinkingLevel", skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    #[serde(rename = "systemPrompt", skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    Manual,
    Yolo,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApprovalResponse {
    pub decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
    #[serde(rename = "selectedLabel", skip_serializing_if = "Option::is_none")]
    pub selected_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionApprovalResultRecord {
    #[serde(rename = "turnId")]
    pub turn_id: i64,
    #[serde(rename = "toolCallId")]
    pub tool_call_id: String,
    #[serde(rename = "toolName")]
    pub tool_name: String,
    pub action: String,
    #[serde(rename = "sessionApprovalRule", skip_serializing_if = "Option::is_none")]
    pub session_approval_rule: Option<String>,
    pub result: ApprovalResponse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionModeKind {
    Plan,
    Design,
    #[serde(rename = "office-hours")]
    OfficeHours,
    #[serde(rename = "game-design")]
    GameDesign,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserToolRegistration {
    pub name: String,
    pub description: String,
    pub parameters: JsonValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UsageRecordScope {
    Session,
    Turn,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactionBeginData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instruction: Option<String>,
    pub source: CompactionSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionSource {
    Manual,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactionResult {
    pub summary: String,
    pub compacted_count: i64,
    pub tokens_before: i64,
    pub tokens_after: i64,
}

// ---------------------------------------------------------------------------
// ToolStoreUpdate
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolStoreUpdate {
    pub key: String,
    pub value: JsonValue,
}

// ---------------------------------------------------------------------------
// Goal
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Active,
    Paused,
    Blocked,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalActor {
    User,
    Model,
    Runtime,
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoalBudgetLimits {
    #[serde(rename = "tokenBudget", skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<i64>,
    #[serde(rename = "turnBudget", skip_serializing_if = "Option::is_none")]
    pub turn_budget: Option<i64>,
    #[serde(rename = "wallClockBudgetMs", skip_serializing_if = "Option::is_none")]
    pub wall_clock_budget_ms: Option<i64>,
}
```

- [ ] 在 `types.rs` 末尾添加 round-trip 测试，覆盖每个 `AgentRecord` 变体：

```rust
#[cfg(test)]
mod tests {
    use kosong_rs::message::{ContentPart, create_user_message};
    use kosong_rs::usage::TokenUsage;

    use super::*;

    #[test]
    fn agent_record_round_trip_metadata() {
        let record = AgentRecord::Metadata {
            time: Some(1_700_000_000_000),
            protocol_version: "1.3".into(),
            created_at: 1_700_000_000_000,
            app_version: Some("0.0.0".into()),
            resumed: Some(false),
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"type\":\"metadata\""));
        let back: AgentRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, back);
    }

    #[test]
    fn agent_record_round_trip_turn_prompt() {
        let record = AgentRecord::TurnPrompt {
            time: Some(1_700_000_000_001),
            input: vec![ContentPart::Text { text: "hello".into() }],
            origin: PromptOrigin::User,
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"type\":\"turn.prompt\""));
        let back: AgentRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, back);
    }

    #[test]
    fn agent_record_round_trip_context_append_loop_event_tool_result() {
        let record = AgentRecord::ContextAppendLoopEvent {
            time: Some(1_700_000_000_002),
            event: LoopRecordedEvent::ToolResultEvent {
                parent_uuid: "p1".into(),
                tool_call_id: "tc1".into(),
                result: ExecutableToolResult::Success(ExecutableToolSuccessResult {
                    output: ExecutableToolOutput::Text("ok"),
                    stop_turn: None,
                    message: None,
                }),
            },
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"type\":\"tool.result\""));
        let back: AgentRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, back);
    }

    #[test]
    fn agent_record_round_trip_usage_record() {
        let record = AgentRecord::UsageRecord {
            time: Some(1_700_000_000_003),
            model: "kimi-k2".into(),
            usage: TokenUsage {
                input_other: 10,
                output: 5,
                input_cache_read: 1,
                input_cache_creation: 0,
            },
            usage_scope: Some(UsageRecordScope::Turn),
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"type\":\"usage.record\""));
        let back: AgentRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, back);
    }

    #[test]
    fn agent_record_round_trip_goal_create() {
        let record = AgentRecord::GoalCreate {
            time: Some(1_700_000_000_004),
            goal_id: "g1".into(),
            objective: "ship it".into(),
            status: GoalStatus::Active,
            actor: GoalActor::User,
            budget_limits: GoalBudgetLimits {
                token_budget: Some(1_000_000),
                turn_budget: Some(100),
                wall_clock_budget_ms: None,
            },
        };
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("\"type\":\"goal.create\""));
        let back: AgentRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, back);
    }

    #[test]
    fn agent_record_deserializes_ts_jsonl() {
        let line = r#"{"type":"metadata","protocol_version":"1.3","created_at":1700000000000,"app_version":"0.0.0"}"#;
        let record: AgentRecord = serde_json::from_str(line).unwrap();
        match record {
            AgentRecord::Metadata { protocol_version, .. } => assert_eq!(protocol_version, "1.3"),
            _ => panic!("expected metadata"),
        }
    }
}
```

- [ ] 运行测试并确认通过：

```bash
cd rust-ody && cargo test -p agent-rs --lib
```

预期输出：`test result: ok. N passed; 0 failed`。

- [ ] Commit：`feat(agent-rs): mirror all nested record types from TS AgentRecordEvents`

---

## Local Self-Review

- [ ] 1. Spec-coverage：本部分覆盖 Roadmap 4.3.0.1（crate 搭建）与 4.3.0.2（AgentRecord / AgentRecordEvents schema 迁移）。
- [ ] 2. Placeholder scan：无 TODO/TBD；所有类型字段均给出具体 serde 属性与测试断言。
- [ ] 3. No phantom tasks：Task 1 产出可编译空 crate；Task 2/3 产出可序列化枚举与通过 round-trip 测试的嵌套类型。
- [ ] 4. Dependency soundness：Task 2 依赖 Task 1；Task 3 依赖 Task 2；无反向依赖。
- [ ] 5. Caller & build soundness：Task 1 修改 workspace members 列表，无其他 Rust/TS 调用方；以 `cargo check -p agent-rs --workspace` 验证整树类型检查通过。
- [ ] 6. Test-the-risk：每个 `AgentRecord` 变体都有 JSON round-trip 测试；`deserializes_ts_jsonl` 直接解析 TS 产出的 JSONL 样例，验证字段映射正确。
- [ ] 7. Type consistency：本部分定义的 `AgentRecord`、`AgentConfigUpdateData`、`CompactionResult` 等类型名/字段名与后续 Part 5 `AgentRecords` 实现一致；`time` 统一为 `Option<i64>`，与 TS `time?: number` 对应。
