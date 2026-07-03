# Part 1 — schema.md

## 范围

本部分完成 Rust `agent-rs` 4.3.8 所需的共享类型层：

- `Cargo.toml` 依赖：`chrono`、`rand`、`kaos-rs`。
- `AgentEvent` 扩展 `BackgroundTaskStarted`、`BackgroundTaskTerminated`、`CronFired`。
- 后台任务公共类型（`BackgroundTaskInfo`、`BackgroundTaskId`、`BackgroundTaskStatus`）。
- Cron 公共类型（`CronTask`、`CronTaskId`、`CronTaskStatus`、`CronFireContext`）。
- XML 渲染辅助：`render_cron_fire_xml`。
- 基于 id 的 JSON 持久化原子操作：`PerIdJsonStore`。

本部分结束后，`cargo check -p agent-rs` 必须无错，所有新增单元测试通过。

---

## Task 1：依赖 + `AgentEvent` + 后台 / Cron 类型

**Depends on:** 无。

**Files:**
- `rust-ody/crates/agent-rs/Cargo.toml`
- `rust-ody/crates/agent-rs/src/turn/types.rs`
- `rust-ody/crates/agent-rs/src/lib.rs`
- `rust-ody/crates/agent-rs/src/background/types.rs`（新建）
- `rust-ody/crates/agent-rs/src/cron/types.rs`（新建）

### 步骤 1.1：写入失败测试

在 `rust-ody/crates/agent-rs/src/turn/types.rs` 末尾新增测试模块（现有 `#[cfg(test)] mod tests` 已存在，追加即可）：

```rust
#[cfg(test)]
mod event_tests {
    use super::*;
    use crate::background::types::{BackgroundTaskId, BackgroundTaskInfo, BackgroundTaskStatus};
    use crate::cron::types::{CronFireContext, CronTaskId};
    use chrono::Utc;

    #[test]
    fn agent_event_background_and_cron_round_trip() {
        use crate::context::types::PromptOrigin;

        let ts = Utc::now();
        let info = BackgroundTaskInfo {
            id: BackgroundTaskId::new("bt-123"),
            kind: crate::background::types::BackgroundTaskKind::Process,
            description: "echo hello".to_string(),
            status: BackgroundTaskStatus::Running,
            started_at: ts,
            finished_at: None,
            stop_reason: None,
            command: Some("echo hello".to_string()),
            pid: Some(1234),
            exit_code: None,
            output_snapshot: None,
            question_count: None,
            tool_call_id: None,
            agent_id: None,
            subagent_type: None,
            terminal_notification_suppressed: None,
            timeout_ms: None,
        };
        let event = AgentEvent::BackgroundTaskStarted { info: info.clone() };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"background.task.started\""));
        assert!(json.contains("\"taskId\":\"bt-123\""));
        let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentEvent::BackgroundTaskStarted { info: parsed_info } => {
                assert_eq!(parsed_info.id, info.id);
                assert_eq!(parsed_info.command, info.command);
            }
            _ => panic!("expected BackgroundTaskStarted, got {:?}", parsed),
        }

        let event = AgentEvent::CronFired {
            origin: PromptOrigin::default(),
            prompt: "check status".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"cron.fired\""));
        assert!(json.contains("\"prompt\":\"check status\""));
        let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentEvent::CronFired { prompt, .. } => {
                assert_eq!(prompt, "check status");
            }
            _ => panic!("expected CronFired, got {:?}", parsed),
        }
    }
}
```

运行：
```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo test -p agent-rs --lib turn::types::event_tests::agent_event_background_and_cron_round_trip
```

**预期结果：** 编译失败，提示 `BackgroundTaskId`/`BackgroundTaskInfo`/`CronFireContext` 等类型不存在，以及 `AgentEvent` 没有对应变体。

### 步骤 1.2：新增依赖

修改 `rust-ody/crates/agent-rs/Cargo.toml`，在 `[dependencies]` 中追加：

```toml
chrono = { version = "0.4", features = ["serde"] }
rand = "0.8"
kaos-rs = { path = "../kaos-rs" }
```

运行 `cargo check -p agent-rs`，确认依赖可解析。

### 步骤 1.3：新增 `background/types.rs`

创建 `rust-ody/crates/agent-rs/src/background/types.rs`：

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BackgroundTaskId(pub String);

impl BackgroundTaskId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl fmt::Display for BackgroundTaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTaskStatus {
    Running,
    Completed,
    Failed,
    TimedOut,
    Killed,
    Lost,
}

pub const TERMINAL_BACKGROUND_STATUSES: &[BackgroundTaskStatus] = &[
    BackgroundTaskStatus::Completed,
    BackgroundTaskStatus::Failed,
    BackgroundTaskStatus::TimedOut,
    BackgroundTaskStatus::Killed,
    BackgroundTaskStatus::Lost,
];

pub fn is_background_task_terminal(status: BackgroundTaskStatus) -> bool {
    TERMINAL_BACKGROUND_STATUSES.contains(&status)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundTaskInfo {
    pub id: BackgroundTaskId,
    pub kind: BackgroundTaskKind,
    pub description: String,
    pub status: BackgroundTaskStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub stop_reason: Option<String>,
    pub command: Option<String>,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub output_snapshot: Option<String>,
    pub question_count: Option<u32>,
    pub tool_call_id: Option<String>,
    pub agent_id: Option<String>,
    pub subagent_type: Option<String>,
    pub terminal_notification_suppressed: Option<bool>,
    pub timeout_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTaskKind {
    Process,
    Agent,
    Question,
}

pub const BACKGROUND_TASK_STARTED_EVENT: &str = "background_task_started";
pub const BACKGROUND_TASK_TERMINATED_EVENT: &str = "background_task_terminated";
```

### 步骤 1.4：新增 `cron/types.rs`

创建 `rust-ody/crates/agent-rs/src/cron/types.rs`：

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CronTaskId(pub String);

impl CronTaskId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl fmt::Display for CronTaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CronTaskStatus {
    Active,
    Paused,
    Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronTask {
    pub id: CronTaskId,
    pub schedule: String,
    pub prompt: String,
    pub status: CronTaskStatus,
    pub next_fire_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronFireContext {
    pub id: CronTaskId,
    pub schedule: String,
    pub prompt: String,
    pub coalesced_count: u64,
    pub fired_at: DateTime<Utc>,
}

pub const CRON_FIRED_EVENT: &str = "cron_fired";
```

### 步骤 1.5：扩展 `AgentEvent`

修改 `rust-ody/crates/agent-rs/src/turn/types.rs`，在 `AgentEvent` 枚举中新增三个 struct 变体。必须与现有 `#[serde(tag = "type")]` 保持一致，生成与 TS 侧一致的 JSON：

- `background.task.started`：`{ "type": "background.task.started", "info": BackgroundTaskInfo }`
- `background.task.terminated`：`{ "type": "background.task.terminated", "info": BackgroundTaskInfo }`
- `cron.fired`：`{ "type": "cron.fired", "origin": CronJobOrigin, "prompt": String }`

代码：

```rust
use crate::background::types::BackgroundTaskInfo;
use crate::context::types::PromptOrigin;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentEvent {
    // ... existing variants ...
    #[serde(rename = "background.task.started")]
    BackgroundTaskStarted { info: BackgroundTaskInfo },
    #[serde(rename = "background.task.terminated")]
    BackgroundTaskTerminated { info: BackgroundTaskInfo },
    #[serde(rename = "cron.fired")]
    CronFired { origin: PromptOrigin, prompt: String },
}
```

注意：`CronFireContext` 在 schema 层仅用于内部 Cron 调度器；触发 Agent 事件时转换为 TS 兼容的 `{ origin, prompt }` 形式。`BackgroundTaskInfo` 字段名使用 camelCase（由 `#[serde(rename_all = "camelCase")]` 控制），与 TS `BackgroundTaskInfo` 接口一致。

确认现有 `match` 表达式是否需要补齐。当前 `turn/types.rs` 测试仅匹配现有变体，`turn_flow.rs` 不消费 `AgentEvent` 枚举，其余文件产生 `AgentEvent` 但不 match。如 `cargo check` 报 non-exhaustive match，则在对应位置补 `_ => {}` 分支。

### 步骤 1.6：注册模块

修改 `rust-ody/crates/agent-rs/src/lib.rs`，在已有模块列表中追加：

```rust
pub mod background;
pub mod cron;
```

### 步骤 1.7：运行测试与全量类型检查

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo test -p agent-rs --lib turn::types::event_tests::agent_event_background_and_cron_round_trip
cargo check -p agent-rs
```

**预期结果：** 测试通过，`cargo check` 无错。

---

## Task 2：Cron 点火 XML 渲染辅助

**Depends on:** Task 1。

**Files:**
- `rust-ody/crates/agent-rs/src/context/cron_fire_xml.rs`（新建）
- `rust-ody/crates/agent-rs/src/context/mod.rs`

### 步骤 2.1：写入失败测试

创建 `rust-ody/crates/agent-rs/src/context/cron_fire_xml.rs` 并先写入测试：

```rust
use crate::cron::types::{CronFireContext, CronTaskId};
use chrono::Utc;

#[test]
fn renders_cron_fire_xml() {
    let ctx = CronFireContext {
        id: CronTaskId::new("cron-42"),
        schedule: "*/5 * * * *".to_string(),
        prompt: "Check CI status".to_string(),
        coalesced_count: 3,
        fired_at: Utc::now(),
    };
    let xml = render_cron_fire_xml(&ctx);
    assert!(xml.contains("<cron_fire>"));
    assert!(xml.contains("<id>cron-42</id>"));
    assert!(xml.contains("<schedule>*/5 * * * *</schedule>"));
    assert!(xml.contains("<prompt>Check CI status</prompt>"));
    assert!(xml.contains("<coalesced_count>3</coalesced_count>"));
    assert!(xml.contains("<fired_at>"));
}
```

运行：
```bash
cargo test -p agent-rs --lib context::cron_fire_xml::renders_cron_fire_xml
```

**预期结果：** 编译失败，`render_cron_fire_xml` 不存在。

### 步骤 2.2：实现函数

在同一文件追加实现：

```rust
use crate::cron::types::CronFireContext;

pub fn render_cron_fire_xml(ctx: &CronFireContext) -> String {
    format!(
        "<cron_fire>\n  <id>{id}</id>\n  <schedule>{schedule}</schedule>\n  <prompt>{prompt}</prompt>\n  <coalesced_count>{coalesced_count}</coalesced_count>\n  <fired_at>{fired_at}</fired_at>\n</cron_fire>",
        id = ctx.id,
        schedule = xml_escape(&ctx.schedule),
        prompt = xml_escape(&ctx.prompt),
        coalesced_count = ctx.coalesced_count,
        fired_at = ctx.fired_at.to_rfc3339(),
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
```

### 步骤 2.3：注册模块

修改 `rust-ody/crates/agent-rs/src/context/mod.rs`（如不存在则新建），追加：

```rust
pub mod cron_fire_xml;
```

如 `context/mod.rs` 不存在，创建并导出 `notification_xml` 与 `cron_fire_xml`。

### 步骤 2.4：运行测试

```bash
cargo test -p agent-rs --lib context::cron_fire_xml::renders_cron_fire_xml
cargo check -p agent-rs
```

**预期结果：** 测试通过，类型检查无错。

---

## Task 3：基于 id 的 JSON 持久化原子操作

**Depends on:** Task 1。

**Files:**
- `rust-ody/crates/agent-rs/src/persist/per_id_json_store.rs`（新建）
- `rust-ody/crates/agent-rs/src/persist/mod.rs`（新建/更新）
- `rust-ody/crates/agent-rs/src/lib.rs`

### 步骤 3.1：写入失败测试

创建 `rust-ody/crates/agent-rs/src/persist/per_id_json_store.rs`，先写测试：

```rust
use super::*;
use serde::{Deserialize, Serialize};
use tempfile::TempDir;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Task {
    id: String,
    value: i32,
}

#[tokio::test]
async fn per_id_json_store_round_trip() {
    let dir = TempDir::new().unwrap();
    let store = PerIdJsonStore::<Task>::new(dir.path().to_path_buf());

    store
        .write("task-1", &Task { id: "task-1".into(), value: 42 })
        .await
        .unwrap();
    store
        .write("task-2", &Task { id: "task-2".into(), value: 7 })
        .await
        .unwrap();

    let all = store.list().await.unwrap();
    assert_eq!(all.len(), 2);

    let one = store.read("task-1").await.unwrap();
    assert!(one.is_some());
    assert_eq!(one.unwrap().value, 42);

    store.remove("task-1").await.unwrap();
    let one = store.read("task-1").await.unwrap();
    assert!(one.is_none());

    let all = store.list().await.unwrap();
    assert_eq!(all.len(), 1);
}
```

运行：
```bash
cargo test -p agent-rs --lib persist::per_id_json_store::per_id_json_store_round_trip
```

**预期结果：** 编译失败，`PerIdJsonStore` 不存在，且 `tempfile` 未声明为 dev-dependency。

### 步骤 3.2：添加 dev-dependency

在 `rust-ody/crates/agent-rs/Cargo.toml` 的 `[dev-dependencies]` 中追加：

```toml
tempfile = "3"
```

### 步骤 3.3：实现 `PerIdJsonStore`

在 `rust-ody/crates/agent-rs/src/persist/per_id_json_store.rs` 追加实现：

```rust
use serde::{de::DeserializeOwned, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct PerIdJsonStore<T> {
    base_dir: PathBuf,
    _phantom: std::marker::PhantomData<T>,
}

impl<T> PerIdJsonStore<T> {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir, _phantom: std::marker::PhantomData }
    }

    fn path_for(&self, id: &str) -> PathBuf {
        // Sanitize id to a safe filename: replace path separators with underscores.
        let safe = id.replace(['/', '\\'], "_");
        self.base_dir.join(format!("{safe}.json"))
    }
}

impl<T: Serialize + DeserializeOwned + Send + Sync> PerIdJsonStore<T> {
    pub async fn write(&self, id: &str, value: &T) -> std::io::Result<()> {
        tokio::fs::create_dir_all(&self.base_dir).await?;
        let path = self.path_for(id);
        let tmp = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec_pretty(value)?;
        {
            let mut file = std::fs::File::create(&tmp)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        tokio::fs::rename(tmp, path).await?;
        Ok(())
    }

    pub async fn read(&self, id: &str) -> std::io::Result<Option<T>> {
        let path = self.path_for(id);
        match tokio::fs::read(&path).await {
            Ok(bytes) => {
                let value = serde_json::from_slice(&bytes)?;
                Ok(Some(value))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn remove(&self, id: &str) -> std::io::Result<()> {
        let path = self.path_for(id);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub async fn list(&self) -> std::io::Result<Vec<T>> {
        let mut values = Vec::new();
        let mut entries = tokio::fs::read_dir(&self.base_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                let bytes = tokio::fs::read(&path).await?;
                let value = serde_json::from_slice(&bytes)?;
                values.push(value);
            }
        }
        Ok(values)
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }
}
```

### 步骤 3.4：注册模块

创建 `rust-ody/crates/agent-rs/src/persist/mod.rs`：

```rust
pub mod per_id_json_store;
```

修改 `rust-ody/crates/agent-rs/src/lib.rs`，追加：

```rust
pub mod persist;
```

### 步骤 3.5：运行测试与全量类型检查

```bash
cargo test -p agent-rs --lib persist::per_id_json_store::per_id_json_store_round_trip
cargo check -p agent-rs
cargo test -p agent-rs --lib
```

**预期结果：** 所有新增测试通过，全 crate 测试无新增失败。

---

## Task 4：共享签名变更后全树类型检查

**Depends on:** Task 1、Task 2、Task 3。

修改 `AgentEvent` 属于共享签名。Task 1 中若任何文件对 `AgentEvent` 做了非穷举匹配，必须在本任务补齐。确认命令：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo check -p agent-rs
```

若报错缺失 match arm，则在对应位置添加 `_ => {}` 或合适的显式处理。

---

## Self-Review（本 Part）

### 1. 是否完整覆盖 4.3.8 需求中的 schema 层？
是：扩展了 `AgentEvent`、新增后台/Cron 类型、提供 Cron XML 渲染和持久化原子操作。

### 2. 是否每个任务都有明确的依赖关系？
是：Task 1 无依赖；Task 2/3 依赖 Task 1；Task 4 汇总检查。

### 3. 是否有测试覆盖每个新增行为？
是：AgentEvent 序列化、Cron XML 渲染、PerIdJsonStore CRUD 都有单元测试。

### 4. 是否对共享签名做了全树更新？
是：Task 4 专门检查并修复所有 `AgentEvent` match。

### 5. 是否有 TODO / placeholder？
否：所有代码均为可直接执行的具体实现。

### 6. 是否遵循现有代码风格？
是：使用 `serde(transparent)` 包装 id，使用 `tokio::fs` 做异步 IO，保持与 `notification_xml.rs` 一致的 XML 风格。

### 7. 是否引入了不必要的依赖？
否：`chrono` 与 `rand` 是后续 Task 必需；`kaos-rs` 用于后台进程抽象。
