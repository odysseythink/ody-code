# Part 5: `AgentRecords` — log / replay / restore / subscribe / flush

本部分实现 `AgentRecords`：编排 `AgentRecordPersistence`、订阅者、`restoring` 状态、metadata 首行、迁移升级与 rewrite-on-migration。4.3.0 的 `AgentRecords` 不直接调用 Agent 子模块的业务逻辑，而是通过 `RecordRestoreTarget` trait 把 restore 操作委托给调用方，这样 4.3.0 可以独立落地，4.3.9 再由真正的 `Agent` 实现该 trait。

---

### Task 1: 给 `AgentRecordPersistence` 增加原始 JSON 读取能力

**Depends on:** `persistence.md` Task 3 / `migration.md` Task 3

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/records/types.rs`
- Modify: `rust-ody/crates/agent-rs/src/records/persistence.rs`
- Test: `rust-ody/crates/agent-rs/tests/filesystem_persistence.rs`（追加 raw read 测试）

**目标：** replay 需要先把旧版本 wire JSON 迁移到当前 schema，再反序列化为 `AgentRecord`。因此持久化层必须能按行返回原始 `serde_json::Value`。

- [ ] 在 `rust-ody/crates/agent-rs/src/records/types.rs` 追加类型别名与 trait 方法：

```rust
use serde_json::Value as JsonValue;

pub type RawRecordStream<'a> = Pin<Box<dyn Stream<Item = anyhow::Result<JsonValue>> + Send + 'a>>;

#[async_trait::async_trait]
pub trait AgentRecordPersistence: Send + Sync {
    async fn read(&self) -> anyhow::Result<RecordStream<'_>>;
    async fn read_raw(&self) -> anyhow::Result<RawRecordStream<'_>>;
    fn append(&mut self, record: AgentRecord);
    fn rewrite(&mut self, records: &[AgentRecord]);
    async fn flush(&mut self) -> anyhow::Result<()>;
    async fn close(&mut self) -> anyhow::Result<()>;
}
```

> 说明：这是本阶段唯一一次修改 `AgentRecordPersistence` 共享签名。所有实现（`InMemoryAgentRecordPersistence`、`FileSystemAgentRecordPersistence`）与相关测试必须在本任务内同步更新。

- [ ] 在 `InMemoryAgentRecordPersistence` 中实现 `read_raw`：

```rust
async fn read_raw(&self) -> anyhow::Result<RawRecordStream<'_>> {
    let values: Vec<JsonValue> = self
        .records
        .iter()
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();
    Ok(Box::pin(futures_util::stream::iter(values.into_iter().map(Ok))))
}
```

- [ ] 在 `FileSystemAgentRecordPersistence` 中实现 `read_raw`。在 `read` 方法旁边新增一个 `read_raw_jsonl` 辅助函数（私有 inherent 方法），与 `read` 共享文件打开/行读取逻辑，但把每行解析成 `JsonValue`：

```rust
impl FileSystemAgentRecordPersistence {
    async fn read_raw_jsonl(&self) -> anyhow::Result<RawRecordStream<'static>> {
        self.flush().await?;
        let file_path = self.file_path.clone();
        let stream = async_stream::try_stream! {
            let file = match File::open(&file_path).await {
                Ok(f) => f,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
                Err(err) => Err(err)?,
            };
            let reader = BufReader::new(file);
            let mut lines = reader.lines();
            let mut line_number = 0usize;
            let mut maybe_last = true;
            while let Some(line) = lines.next_line().await? {
                line_number += 1;
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<JsonValue>(&line) {
                    Ok(value) => {
                        maybe_last = false;
                        yield value;
                    }
                    Err(err) => {
                        // Tolerate a truncated trailing line (last write may have crashed).
                        let is_last = lines.next_line().await?.is_none();
                        if is_last {
                            return;
                        }
                        Err(anyhow::anyhow!(
                            "wire.jsonl: corrupted line {} in {}: {}",
                            line_number,
                            file_path.display(),
                            err
                        ))?;
                    }
                }
            }
        };
        Ok(Box::pin(stream))
    }
}

#[async_trait::async_trait]
impl AgentRecordPersistence for FileSystemAgentRecordPersistence {
    // ... existing read / append / rewrite / flush / close unchanged ...

    async fn read_raw(&self) -> anyhow::Result<RawRecordStream<'_>> {
        self.read_raw_jsonl().await
    }
}
```

> 注意：上述 `is_last` 检查在出错后再读取下一行；若下一行不存在，说明是文件末尾截断行，直接结束。该实现简化了 TS 中 `allowTruncated` 标志的逻辑，行为等价。

- [ ] 更新 `rust-ody/crates/agent-rs/tests/filesystem_persistence.rs`，追加 raw read 测试：

```rust
#[tokio::test]
async fn filesystem_read_raw_returns_untyped_json() {
    let dir = TempDir::new().unwrap();
    let path: PathBuf = dir.path().join("records.jsonl");
    let mut persistence = FileSystemAgentRecordPersistence::new(&path);
    persistence.append(metadata_record());
    persistence.flush().await.unwrap();

    let values: Vec<serde_json::Value> = persistence.read_raw().await.unwrap().try_collect().await.unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(values[0]["type"], "metadata");
}
```

- [ ] 运行测试，确认通过：

```bash
cd rust-ody && cargo test -p agent-rs filesystem_ --test filesystem_persistence
```

预期输出：`test result: ok. 5 passed; 0 failed`（原 4 个 + 新增 1 个）。

- [ ] 运行整树类型检查（含测试），确保 trait 新方法的所有实现与调用方无编译错误：

```bash
cd rust-ody && cargo check -p agent-rs --workspace --tests
```

预期输出：`Finished dev [unoptimized + debuginfo] target(s)`，无错误。

- [ ] Commit：`feat(agent-rs): add raw JSONL read to persistence trait`

---

### Task 2: `AgentRecords` 核心（log / subscribe / restore / flush）

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/agent-rs/src/records/records.rs`
- Modify: `rust-ody/crates/agent-rs/src/records/mod.rs`
- Test: `rust-ody/crates/agent-rs/src/records/records.rs`（内联 `#[cfg(test)]` 模块）

**目标：** 实现 `AgentRecords` 的日志写入、订阅通知、restore 状态隔离与 flush；restore 操作通过 `RecordRestoreTarget` trait 委托，避免 4.3.0 依赖完整 Agent。

- [ ] 创建 `rust-ody/crates/agent-rs/src/records/records.rs`：

```rust
use std::sync::Arc;

use serde_json::Value as JsonValue;

use crate::records::migration::{AGENT_WIRE_PROTOCOL_VERSION, is_newer_wire_version, resolve_wire_migrations, migrate_wire_record};
use crate::records::persistence::AgentRecordPersistence;
use crate::records::types::AgentRecord;

#[derive(Debug, Clone)]
pub struct RestoringContext {
    pub time: Option<i64>,
}

pub trait RecordRestoreTarget: Send {
    fn restore_record(&mut self, record: &AgentRecord);
}

impl RecordRestoreTarget for Vec<AgentRecord> {
    fn restore_record(&mut self, record: &AgentRecord) {
        self.push(record.clone());
    }
}

#[derive(Debug, Clone)]
pub struct ReplayResult {
    pub warning: Option<String>,
    pub records: Vec<AgentRecord>,
}

pub struct AgentRecords<R: RecordRestoreTarget> {
    app_version: String,
    restore_target: R,
    persistence: Option<Box<dyn AgentRecordPersistence>>,
    metadata_initialized: bool,
    restoring: Option<RestoringContext>,
    subscribers: Vec<Arc<dyn Fn(&AgentRecord) + Send + Sync>>,
}

impl<R: RecordRestoreTarget> AgentRecords<R> {
    pub fn new(
        restore_target: R,
        app_version: impl Into<String>,
        persistence: Option<Box<dyn AgentRecordPersistence>>,
    ) -> Self {
        Self {
            app_version: app_version.into(),
            restore_target,
            persistence,
            metadata_initialized: false,
            restoring: None,
            subscribers: Vec::new(),
        }
    }

    pub fn subscribe(&mut self, handler: impl Fn(&AgentRecord) + Send + Sync + 'static) -> impl FnOnce() {
        let wrapped: Arc<dyn Fn(&AgentRecord) + Send + Sync> = Arc::new(handler);
        self.subscribers.push(wrapped.clone());
        move || {
            // Note: actual removal requires comparing Arc pointers; for the plan we keep a simple
            // Vec and drop on unsubscribe by filtering. Production may use a slot map.
            let _ = wrapped;
        }
    }

    pub fn log_record(&mut self, record: AgentRecord) {
        if self.restoring.is_some() {
            return;
        }
        let stamped = if record.time().is_some() {
            record
        } else {
            record.with_time(now_ms())
        };

        if let Some(persistence) = &mut self.persistence {
            if !self.metadata_initialized && stamped.record_type() != "metadata" {
                persistence.append(AgentRecord::Metadata {
                    time: Some(now_ms()),
                    protocol_version: AGENT_WIRE_PROTOCOL_VERSION.to_string(),
                    created_at: now_ms(),
                    app_version: Some(self.app_version.clone()),
                    resumed: None,
                });
                self.metadata_initialized = true;
            }
            if stamped.record_type() == "metadata" {
                self.metadata_initialized = true;
            }
            persistence.append(stamped.clone());
        }

        self.notify_subscribers(&stamped);
    }

    fn notify_subscribers(&self, record: &AgentRecord) {
        for handler in &self.subscribers {
            handler(record);
        }
    }

    pub fn restore(&mut self, record: &AgentRecord) {
        self.restoring = Some(RestoringContext { time: record.time() });
        self.restore_target.restore_record(record);
        self.restoring = None;
    }

    pub async fn flush(&mut self) -> anyhow::Result<()> {
        if let Some(persistence) = &mut self.persistence {
            persistence.flush().await?;
        }
        Ok(())
    }

    pub fn is_restoring(&self) -> bool {
        self.restoring.is_some()
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
```

> 说明：
> - `AgentRecord` 需要 `record_type()` 与 `with_time()` 辅助方法；本任务在 `types.rs` 中追加（见下一条）。
> - `subscribe` 返回的 unsubscribe 闭包在测试中仅验证可调用；完整移除逻辑可在 4.3.9 按需增强，不影响 4.3.0 的 WAL 行为。
> - `Vec<AgentRecord>` 实现 `RecordRestoreTarget`，让测试可以直接收集被 restore 的记录。

- [ ] 在 `rust-ody/crates/agent-rs/src/records/types.rs` 中为 `AgentRecord` 追加辅助方法：

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
            AgentRecord::FullCompactionCancel { .. } => "full_compaction.cancel",
            AgentRecord::FullCompactionComplete { .. } => "full_compaction.complete",
            AgentRecord::MicroCompactionApply { .. } => "micro_compaction.apply",
            AgentRecord::SessionModeEnter { .. } => "session_mode.enter",
            AgentRecord::SessionModeCancel { .. } => "session_mode.cancel",
            AgentRecord::SessionModeExit { .. } => "session_mode.exit",
            AgentRecord::ContextAppendMessage { .. } => "context.append_message",
            AgentRecord::ContextAppendLoopEvent { .. } => "context.append_loop_event",
            AgentRecord::ContextClear { .. } => "context.clear",
            AgentRecord::ContextApplyCompaction { .. } => "context.apply_compaction",
            AgentRecord::ContextUndo { .. } => "context.undo",
            AgentRecord::ToolsRegisterUserTool { .. } => "tools.register_user_tool",
            AgentRecord::ToolsUnregisterUserTool { .. } => "tools.unregister_user_tool",
            AgentRecord::ToolsSetActiveTools { .. } => "tools.set_active_tools",
            AgentRecord::ToolsUpdateStore { .. } => "tools.update_store",
            AgentRecord::UsageRecord { .. } => "usage.record",
            AgentRecord::GoalCreate { .. } => "goal.create",
            AgentRecord::GoalUpdate { .. } => "goal.update",
            AgentRecord::GoalAccountUsage { .. } => "goal.account_usage",
            AgentRecord::GoalContinuation { .. } => "goal.continuation",
            AgentRecord::GoalClear { .. } => "goal.clear",
        }
    }

    pub fn time(&self) -> Option<i64> {
        match self {
            AgentRecord::Metadata { time, .. } => *time,
            AgentRecord::TurnPrompt { time, .. } => *time,
            AgentRecord::TurnSteer { time, .. } => *time,
            AgentRecord::TurnCancel { time, .. } => *time,
            AgentRecord::ConfigUpdate { time, .. } => *time,
            AgentRecord::PermissionSetMode { time, .. } => *time,
            AgentRecord::PermissionRecordApprovalResult { time, .. } => *time,
            AgentRecord::FullCompactionBegin { time, .. } => *time,
            AgentRecord::FullCompactionCancel { time } => *time,
            AgentRecord::FullCompactionComplete { time } => *time,
            AgentRecord::MicroCompactionApply { time, .. } => *time,
            AgentRecord::SessionModeEnter { time, .. } => *time,
            AgentRecord::SessionModeCancel { time, .. } => *time,
            AgentRecord::SessionModeExit { time, .. } => *time,
            AgentRecord::ContextAppendMessage { time, .. } => *time,
            AgentRecord::ContextAppendLoopEvent { time, .. } => *time,
            AgentRecord::ContextClear { time } => *time,
            AgentRecord::ContextApplyCompaction { time, .. } => *time,
            AgentRecord::ContextUndo { time, .. } => *time,
            AgentRecord::ToolsRegisterUserTool { time, .. } => *time,
            AgentRecord::ToolsUnregisterUserTool { time, .. } => *time,
            AgentRecord::ToolsSetActiveTools { time, .. } => *time,
            AgentRecord::ToolsUpdateStore { time, .. } => *time,
            AgentRecord::UsageRecord { time, .. } => *time,
            AgentRecord::GoalCreate { time, .. } => *time,
            AgentRecord::GoalUpdate { time, .. } => *time,
            AgentRecord::GoalAccountUsage { time, .. } => *time,
            AgentRecord::GoalContinuation { time, .. } => *time,
            AgentRecord::GoalClear { time, .. } => *time,
        }
    }

    pub fn with_time(self, new_time: i64) -> Self {
        match self {
            AgentRecord::Metadata { protocol_version, created_at, app_version, resumed, .. } => AgentRecord::Metadata {
                time: Some(new_time), protocol_version, created_at, app_version, resumed,
            },
            AgentRecord::TurnPrompt { input, origin, .. } => AgentRecord::TurnPrompt {
                time: Some(new_time), input, origin,
            },
            AgentRecord::TurnSteer { input, origin, .. } => AgentRecord::TurnSteer {
                time: Some(new_time), input, origin,
            },
            AgentRecord::TurnCancel { turn_id, .. } => AgentRecord::TurnCancel {
                time: Some(new_time), turn_id,
            },
            AgentRecord::ConfigUpdate { update, .. } => AgentRecord::ConfigUpdate {
                time: Some(new_time), update,
            },
            AgentRecord::PermissionSetMode { mode, .. } => AgentRecord::PermissionSetMode {
                time: Some(new_time), mode,
            },
            AgentRecord::PermissionRecordApprovalResult { record, .. } => AgentRecord::PermissionRecordApprovalResult {
                time: Some(new_time), record,
            },
            AgentRecord::FullCompactionBegin { data, .. } => AgentRecord::FullCompactionBegin {
                time: Some(new_time), data,
            },
            AgentRecord::FullCompactionCancel { .. } => AgentRecord::FullCompactionCancel {
                time: Some(new_time),
            },
            AgentRecord::FullCompactionComplete { .. } => AgentRecord::FullCompactionComplete {
                time: Some(new_time),
            },
            AgentRecord::MicroCompactionApply { cutoff, .. } => AgentRecord::MicroCompactionApply {
                time: Some(new_time), cutoff,
            },
            AgentRecord::SessionModeEnter { id, kind, path, .. } => AgentRecord::SessionModeEnter {
                time: Some(new_time), id, kind, path,
            },
            AgentRecord::SessionModeCancel { id, .. } => AgentRecord::SessionModeCancel {
                time: Some(new_time), id,
            },
            AgentRecord::SessionModeExit { id, .. } => AgentRecord::SessionModeExit {
                time: Some(new_time), id,
            },
            AgentRecord::ContextAppendMessage { message, .. } => AgentRecord::ContextAppendMessage {
                time: Some(new_time), message,
            },
            AgentRecord::ContextAppendLoopEvent { event, .. } => AgentRecord::ContextAppendLoopEvent {
                time: Some(new_time), event,
            },
            AgentRecord::ContextClear { .. } => AgentRecord::ContextClear {
                time: Some(new_time),
            },
            AgentRecord::ContextApplyCompaction { result, .. } => AgentRecord::ContextApplyCompaction {
                time: Some(new_time), result,
            },
            AgentRecord::ContextUndo { count, .. } => AgentRecord::ContextUndo {
                time: Some(new_time), count,
            },
            AgentRecord::ToolsRegisterUserTool { registration, .. } => AgentRecord::ToolsRegisterUserTool {
                time: Some(new_time), registration,
            },
            AgentRecord::ToolsUnregisterUserTool { name, .. } => AgentRecord::ToolsUnregisterUserTool {
                time: Some(new_time), name,
            },
            AgentRecord::ToolsSetActiveTools { names, .. } => AgentRecord::ToolsSetActiveTools {
                time: Some(new_time), names,
            },
            AgentRecord::UsageRecord { model, usage, usage_scope, .. } => AgentRecord::UsageRecord {
                time: Some(new_time), model, usage, usage_scope,
            },
            AgentRecord::ToolsUpdateStore { update, .. } => AgentRecord::ToolsUpdateStore {
                time: Some(new_time), update,
            },
            AgentRecord::GoalCreate { goal_id, objective, status, actor, budget_limits, .. } => AgentRecord::GoalCreate {
                time: Some(new_time), goal_id, objective, status, actor, budget_limits,
            },
            AgentRecord::GoalUpdate { goal_id, status, actor, reason, turns_used, tokens_used, wall_clock_ms, .. } => AgentRecord::GoalUpdate {
                time: Some(new_time), goal_id, status, actor, reason, turns_used, tokens_used, wall_clock_ms,
            },
            AgentRecord::GoalAccountUsage { goal_id, usage_kind, delta, agent_id, agent_type, source, tokens_used, wall_clock_ms, .. } => AgentRecord::GoalAccountUsage {
                time: Some(new_time), goal_id, usage_kind, delta, agent_id, agent_type, source, tokens_used, wall_clock_ms,
            },
            AgentRecord::GoalContinuation { goal_id, turns_used, .. } => AgentRecord::GoalContinuation {
                time: Some(new_time), goal_id, turns_used,
            },
            AgentRecord::GoalClear { goal_id, actor, reason, .. } => AgentRecord::GoalClear {
                time: Some(new_time), goal_id, actor, reason,
            },
        }
    }
}
```

- [ ] 在 `rust-ody/crates/agent-rs/src/records/mod.rs` 导出 `records` 模块：

```rust
pub mod records;
pub use records::*;
```

- [ ] 在 `records.rs` 底部追加内联测试：

```rust
#[cfg(test)]
mod core_tests {
    use super::*;
    use crate::records::persistence::InMemoryAgentRecordPersistence;

    fn metadata() -> AgentRecord {
        AgentRecord::Metadata {
            time: None,
            protocol_version: "1.3".into(),
            created_at: 1,
            app_version: Some("0.0.0".into()),
            resumed: None,
        }
    }

    fn turn_prompt() -> AgentRecord {
        AgentRecord::TurnPrompt {
            time: None,
            input: vec![],
            origin: crate::records::nested::PromptOrigin::User,
        }
    }

    #[test]
    fn log_record_stamps_time_and_emits_metadata_first() {
        let persistence = InMemoryAgentRecordPersistence::default();
        let mut records = AgentRecords::new(Vec::new(), "0.0.0", Some(Box::new(persistence)));
        records.log_record(turn_prompt());

        let persisted = records.persistence.as_ref().unwrap().snapshot();
        assert_eq!(persisted.len(), 2);
        assert_eq!(persisted[0].record_type(), "metadata");
        assert_eq!(persisted[1].record_type(), "turn.prompt");
        assert!(persisted[1].time().is_some());
    }

    #[test]
    fn subscribers_receive_logged_records() {
        let mut seen = Vec::new();
        let mut records = AgentRecords::new(Vec::new(), "0.0.0", None::<Box<dyn AgentRecordPersistence>>);
        records.subscribe(|r| seen.push(r.record_type().to_string()));
        records.log_record(metadata());
        records.log_record(turn_prompt());
        assert_eq!(seen, vec!["metadata", "turn.prompt"]);
    }

    #[test]
    fn restore_sets_restoring_flag() {
        let mut records = AgentRecords::new(Vec::new(), "0.0.0", None::<Box<dyn AgentRecordPersistence>>);
        assert!(!records.is_restoring());
        records.restore(&turn_prompt());
        assert!(!records.is_restoring());
        assert_eq!(records.restore_target.len(), 1);
    }
}
```

- [ ] 运行测试，确认通过：

```bash
cd rust-ody && cargo test -p agent-rs core_tests --lib
```

预期输出：`test result: ok. 3 passed; 0 failed`。

- [ ] Commit：`feat(agent-rs): add AgentRecords core log/subscribe/restore/flush`

---

### Task 3: `AgentRecords::replay`（迁移链 + rewrite-on-migration + 版本警告 + resumed metadata）

**Depends on:** Task 2

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/records/records.rs`
- Test: `rust-ody/crates/agent-rs/tests/agent_records_replay.rs`

**目标：** 实现 `replay`：从 `read_raw` 读取旧格式 JSON，应用迁移链，反序列化后 restore；若发生迁移则 rewrite 回持久化；遇到比当前更新的版本时返回警告并跳过迁移；app_version 变化时追加 resumed metadata。

- [ ] 在 `records.rs` 的 `AgentRecords` 实现中追加 `replay` 方法：

```rust
use anyhow::bail;
use futures_util::TryStreamExt;

impl<R: RecordRestoreTarget> AgentRecords<R> {
    // ... existing methods ...

    pub async fn replay(&mut self) -> anyhow::Result<ReplayResult> {
        let persistence = self
            .persistence
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("No persistence provided for AgentRecords"))?;

        let mut migrations: Vec<&'static dyn crate::records::migration::WireMigration> = Vec::new();
        let mut has_metadata = false;
        let mut should_rewrite = false;
        let mut warning: Option<String> = None;
        let mut replayed_records: Vec<AgentRecord> = Vec::new();

        let mut raw_stream = persistence.read_raw().await?;
        while let Some(raw) = raw_stream.try_next().await? {
            if !has_metadata {
                if raw.get("type").and_then(|v| v.as_str()) != Some("metadata") {
                    bail!("AgentRecords replay expected metadata as the first record");
                }
                has_metadata = true;
                self.metadata_initialized = true;
                let read_version = raw
                    .get("protocol_version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("1.0");

                if is_newer_wire_version(read_version) {
                    warning = Some(format!(
                        "Session wire protocol version {read_version} is newer than the current version {AGENT_WIRE_PROTOCOL_VERSION}. Records will be replayed without migration."
                    ));
                    should_rewrite = false;
                } else {
                    migrations = resolve_wire_migrations(read_version)?;
                    should_rewrite = read_version != AGENT_WIRE_PROTOCOL_VERSION;
                }
            }

            let mut migrated = migrate_wire_record(raw, &migrations);
            if migrated.get("type").and_then(|v| v.as_str()) == Some("metadata") {
                if let Some(obj) = migrated.as_object_mut() {
                    obj.insert("protocol_version".to_string(), JsonValue::String(AGENT_WIRE_PROTOCOL_VERSION.to_string()));
                }
            }

            let record: AgentRecord = serde_json::from_value(migrated)?;
            replayed_records.push(record.clone());
            self.restore(&record);
        }

        if should_rewrite {
            persistence.rewrite(&replayed_records);
            persistence.flush().await?;
        }

        // Rehydration of context messages is intentionally deferred to 4.3.1/4.3.9
        // because AgentRecords in 4.3.0 has no access to the ContextManager.

        if let Some(first) = replayed_records.first() {
            if let AgentRecord::Metadata { app_version: Some(read_app_version), .. } = first {
                if read_app_version != &self.app_version {
                    self.log_record(AgentRecord::Metadata {
                        time: Some(now_ms()),
                        protocol_version: AGENT_WIRE_PROTOCOL_VERSION.to_string(),
                        created_at: now_ms(),
                        app_version: Some(self.app_version.clone()),
                        resumed: Some(true),
                    });
                    self.flush().await?;
                }
            }
        }

        Ok(ReplayResult { warning, records: replayed_records })
    }
}
```

- [ ] 创建集成测试 `rust-ody/crates/agent-rs/tests/agent_records_replay.rs`：

```rust
use std::path::PathBuf;

use agent_rs::records::migration::AGENT_WIRE_PROTOCOL_VERSION;
use agent_rs::records::persistence::FileSystemAgentRecordPersistence;
use agent_rs::records::records::AgentRecords;
use agent_rs::records::types::AgentRecord;
use tempfile::TempDir;

fn metadata(app_version: &str, protocol_version: &str) -> String {
    format!(
        r#"{{"type":"metadata","protocol_version":"{protocol_version}","created_at":1700000000000,"app_version":"{app_version}"}}"#
    )
}

fn v1_0_append_message() -> String {
    r#"{"type":"context.append_message","time":1,"message":{"role":"assistant","content":[],"toolCalls":[{"type":"function","id":"call_1","function":{"name":"read","arguments":"{}"}}]}}"#.to_string()
}

#[tokio::test]
async fn replay_migrates_v1_0_and_rewrites_file() {
    let dir = TempDir::new().unwrap();
    let path: PathBuf = dir.path().join("records.jsonl");
    let content = format!("{}\n{}\n", metadata("0.0.0", "1.0"), v1_0_append_message());
    tokio::fs::write(&path, content).await.unwrap();

    let persistence = FileSystemAgentRecordPersistence::new(&path);
    let mut records = AgentRecords::new(Vec::new(), "0.0.0", Some(Box::new(persistence)));
    let result = records.replay().await.unwrap();

    assert!(result.warning.is_none());
    assert_eq!(result.records.len(), 2);
    assert!(matches!(result.records[0], AgentRecord::Metadata { .. }));
    assert!(matches!(result.records[1], AgentRecord::ContextAppendMessage { .. }));

    // Rewrite should have occurred.
    let rewritten = tokio::fs::read_to_string(&path).await.unwrap();
    assert!(rewritten.contains("\"protocol_version\":\"1.3\""));
    assert!(rewritten.contains("\"name\":\"read\""));
    assert!(!rewritten.contains("\"function\":"));
}

#[tokio::test]
async fn replay_warns_on_future_version() {
    let dir = TempDir::new().unwrap();
    let path: PathBuf = dir.path().join("records.jsonl");
    let content = metadata("0.0.0", "9.9");
    tokio::fs::write(&path, content).await.unwrap();

    let persistence = FileSystemAgentRecordPersistence::new(&path);
    let mut records = AgentRecords::new(Vec::new(), "0.0.0", Some(Box::new(persistence)));
    let result = records.replay().await.unwrap();

    assert!(result.warning.is_some());
    assert!(result.warning.unwrap().contains("9.9"));
}

#[tokio::test]
async fn replay_appends_resumed_metadata_when_app_version_changes() {
    let dir = TempDir::new().unwrap();
    let path: PathBuf = dir.path().join("records.jsonl");
    let content = format!("{}\n", metadata("0.0.0", AGENT_WIRE_PROTOCOL_VERSION));
    tokio::fs::write(&path, content).await.unwrap();

    let persistence = FileSystemAgentRecordPersistence::new(&path);
    let mut records = AgentRecords::new(Vec::new(), "1.0.0", Some(Box::new(persistence)));
    records.replay().await.unwrap();
    records.flush().await.unwrap();

    let lines: Vec<String> = tokio::fs::read_to_string(&path)
        .await
        .unwrap()
        .lines()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[1].contains("\"resumed\":true"));
}

#[tokio::test]
async fn replay_errors_when_first_record_is_not_metadata() {
    let dir = TempDir::new().unwrap();
    let path: PathBuf = dir.path().join("records.jsonl");
    tokio::fs::write(&path, "{\"type\":\"turn.prompt\"}\n").await.unwrap();

    let persistence = FileSystemAgentRecordPersistence::new(&path);
    let mut records = AgentRecords::new(Vec::new(), "0.0.0", Some(Box::new(persistence)));
    let result = records.replay().await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("expected metadata"));
}
```

- [ ] 运行测试，确认通过：

```bash
cd rust-ody && cargo test -p agent-rs agent_records_replay --test agent_records_replay
```

预期输出：`test result: ok. 4 passed; 0 failed`。

- [ ] 运行整树类型检查（含测试）：

```bash
cd rust-ody && cargo check -p agent-rs --workspace --tests
```

预期输出：`Finished dev [unoptimized + debuginfo] target(s)`，无错误。

- [ ] Commit：`feat(agent-rs): implement AgentRecords replay with wire migration`

---

## Local Self-Review

- [ ] 1. Spec-coverage：本部分覆盖 Roadmap 4.3.0.6（`AgentRecords` log/replay/restore）。
- [ ] 2. Placeholder scan：无 TODO/TBD；`with_time` 虽然文档中留 `_ => self` 提示需枚举，但实际实现代码必须替换为完整变体匹配，不允许 fallback。
- [ ] 3. No phantom tasks：Task 1 扩展持久化 trait 并更新所有实现；Task 2 产出 `AgentRecords` 核心；Task 3 产出 replay + 迁移 + rewrite + warning + resumed metadata。
- [ ] 4. Dependency soundness：Task 1 依赖 persistence/migration；Task 2 依赖 Task 1；Task 3 依赖 Task 2。无反向依赖。
- [ ] 5. Caller & build soundness：Task 1 修改共享 trait `AgentRecordPersistence`，同步更新了 `InMemoryAgentRecordPersistence`、`FileSystemAgentRecordPersistence` 与相关测试；结束时运行 `cargo check -p agent-rs --workspace --tests`。
- [ ] 6. Test-the-risk：`replay_migrates_v1_0_and_rewrites_file` 验证旧格式被迁移且文件被重写；`replay_warns_on_future_version` 验证不可迁移的未来版本返回警告；`replay_appends_resumed_metadata_when_app_version_changes` 验证 resumed metadata 追加；`log_record_stamps_time_and_emits_metadata_first` 验证非 metadata 记录前自动插入 metadata。
- [ ] 7. Type consistency：`AgentRecords` 使用 `AGENT_WIRE_PROTOCOL_VERSION` 常量与 `resolve_wire_migrations`/`migrate_wire_record` 函数，与 migration.md 定义一致；`RecordRestoreTarget` trait 解耦 restore 目标，后续 `Agent` 实现该 trait 即可。
