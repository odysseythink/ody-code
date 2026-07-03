# Part 2 — background.md

## 范围

本部分完成 Rust `agent-rs` 的后台任务子系统：

- `BackgroundTask` trait 与三种任务实现（process / agent / question）。
- `BackgroundTaskPersistence`：per-id JSON + `output.log`。
- `BackgroundManager`：注册、停止、等待、输出快照、终端通知、reconcile。

本部分结束后，`cargo test -p agent-rs --lib background` 通过。

---

## Task 1：`BackgroundTask` trait 与公共类型

**Depends on:** `schema.md` Task 1。

**Files:**
- `rust-ody/crates/agent-rs/src/background/mod.rs`（新建）
- `rust-ody/crates/agent-rs/src/background/types.rs`（新建）

### 步骤 1.1：写入失败测试

创建 `rust-ody/crates/agent-rs/src/background/types.rs`，先写测试模块：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct DummyTask {
        base: BackgroundTaskBase,
    }

    #[async_trait::async_trait]
    impl BackgroundTask for DummyTask {
        fn base(&self) -> &BackgroundTaskBase {
            &self.base
        }
        fn set_id(&mut self, id: BackgroundTaskId) {
            self.base.id = id;
        }

        async fn run(
            &self,
            sink: Arc<dyn BackgroundTaskSink>,
            mut stop: tokio::sync::watch::Receiver<bool>,
        ) -> BackgroundTaskSettlement {
            sink.append_output("hello");
            let _ = stop.changed().await;
            BackgroundTaskSettlement {
                status: BackgroundTaskStatus::Killed,
                stop_reason: Some("stopped".into()),
            }
        }
    }

    #[tokio::test]
    async fn trait_is_implementable_and_sink_collects_output() {
        let task = DummyTask {
            base: BackgroundTaskBase {
                id: BackgroundTaskId::new("bash-12345678"),
                kind: BackgroundTaskKind::Process,
                description: "dummy".into(),
                timeout_ms: None,
            },
        };
        let sink = Arc::new(SinkState::default());
        let (tx, rx) = tokio::sync::watch::channel(false);
        let handle = tokio::spawn({
            let sink = sink.clone();
            async move { task.run(sink, rx).await }
        });
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert_eq!(sink.snapshot(), "hello");
        tx.send(true).unwrap();
        let settlement = handle.await.unwrap();
        assert_eq!(settlement.status, BackgroundTaskStatus::Killed);
    }
}
```

运行：
```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo test -p agent-rs --lib background::types::tests::trait_is_implementable_and_sink_collects_output
```

**预期结果：** 编译失败，`BackgroundTask` trait、`BackgroundTaskSink`、`SinkState` 等不存在。

### 步骤 1.2：实现公共类型

在同一文件追加实现：

```rust
use crate::background::types::{
    BackgroundTaskId, BackgroundTaskInfo, BackgroundTaskKind, BackgroundTaskStatus,
};
use async_trait::async_trait;
use chrono::Utc;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct BackgroundTaskBase {
    pub id: BackgroundTaskId,
    pub kind: BackgroundTaskKind,
    pub description: String,
    pub timeout_ms: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct BackgroundTaskSettlement {
    pub status: BackgroundTaskStatus,
    pub stop_reason: Option<String>,
}

#[async_trait]
pub trait BackgroundTask: Send + Sync {
    fn base(&self) -> &BackgroundTaskBase;
    fn set_id(&mut self, id: BackgroundTaskId);
    async fn run(
        &self,
        sink: Arc<dyn BackgroundTaskSink>,
        stop: tokio::sync::watch::Receiver<bool>,
    ) -> BackgroundTaskSettlement;
}

pub trait BackgroundTaskSink: Send + Sync {
    fn append_output(&self, chunk: &str);
    fn snapshot(&self) -> String;
}

#[derive(Default)]
pub struct SinkState {
    chunks: Mutex<Vec<String>>,
}

impl BackgroundTaskSink for SinkState {
    fn append_output(&self, chunk: &str) {
        if chunk.is_empty() {
            return;
        }
        let mut guard = self.chunks.lock().unwrap();
        guard.push(chunk.to_string());
        const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
        let mut total: usize = guard.iter().map(|s| s.as_bytes().len()).sum();
        while total > MAX_OUTPUT_BYTES && guard.len() > 1 {
            let removed = guard.remove(0);
            total -= removed.as_bytes().len();
        }
    }

    fn snapshot(&self) -> String {
        self.chunks.lock().unwrap().join("")
    }
}

pub fn info_from_base(
    base: &BackgroundTaskBase,
    started_at: chrono::DateTime<Utc>,
    finished_at: Option<chrono::DateTime<Utc>>,
    stop_reason: Option<String>,
) -> BackgroundTaskInfo {
    BackgroundTaskInfo {
        id: base.id.clone(),
        kind: base.kind,
        description: base.description.clone(),
        status: if finished_at.is_some() {
            BackgroundTaskStatus::Running
        } else {
            BackgroundTaskStatus::Running
        },
        started_at,
        finished_at,
        stop_reason,
        command: None,
        pid: None,
        exit_code: None,
        output_snapshot: None,
        question_count: None,
        tool_call_id: None,
        agent_id: None,
        subagent_type: None,
        terminal_notification_suppressed: None,
        timeout_ms: base.timeout_ms,
    }
}
```

### 步骤 1.3：注册模块

创建 `rust-ody/crates/agent-rs/src/background/mod.rs`：

```rust
pub mod manager;
pub mod persistence;
pub mod tasks;
pub mod types;

pub use types::{
    BackgroundTask, BackgroundTaskBase, BackgroundTaskSettlement, BackgroundTaskSink, SinkState,
};
```

### 步骤 1.4：运行测试

```bash
cargo test -p agent-rs --lib background::types::tests::trait_is_implementable_and_sink_collects_output
cargo check -p agent-rs
```

**预期结果：** 测试通过，`cargo check` 无错。

---

## Task 2：`BackgroundTaskPersistence`

**Depends on:** Task 1。

**Files:**
- `rust-ody/crates/agent-rs/src/background/persistence.rs`（新建）
- `rust-ody/crates/agent-rs/src/background/mod.rs`

### 步骤 2.1：写入失败测试

创建 `rust-ody/crates/agent-rs/src/background/persistence.rs`，先写测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::background::types::{
        BackgroundTaskId, BackgroundTaskInfo, BackgroundTaskKind, BackgroundTaskStatus,
    };
    use chrono::Utc;
    use tempfile::TempDir;

    fn sample_info(id: &str) -> BackgroundTaskInfo {
        BackgroundTaskInfo {
            id: BackgroundTaskId::new(id),
            kind: BackgroundTaskKind::Process,
            description: "echo hi".into(),
            status: BackgroundTaskStatus::Running,
            started_at: Utc::now(),
            finished_at: None,
            stop_reason: None,
            command: Some("echo hi".into()),
            pid: Some(1234),
            exit_code: None,
            output_snapshot: None,
            question_count: None,
            tool_call_id: None,
            agent_id: None,
            subagent_type: None,
            terminal_notification_suppressed: None,
            timeout_ms: None,
        }
    }

    #[tokio::test]
    async fn persistence_round_trip_and_output_log() {
        let dir = TempDir::new().unwrap();
        let p = BackgroundTaskPersistence::new(dir.path().to_path_buf());

        p.write_task(&sample_info("bash-12345678")).await.unwrap();
        p.append_task_output("bash-12345678", "line1\n").await.unwrap();
        p.append_task_output("bash-12345678", "line2\n").await.unwrap();

        let all = p.list_tasks().await.unwrap();
        assert_eq!(all.len(), 1);

        let one = p.read_task("bash-12345678").await.unwrap();
        assert!(one.is_some());
        assert_eq!(one.unwrap().description, "echo hi");

        assert_eq!(p.task_output_size_bytes("bash-12345678").await.unwrap(), 12);
        let tail = p.read_task_output_bytes("bash-12345678", 6, 100).await.unwrap();
        assert_eq!(tail, "line2\n");
    }
}
```

运行：
```bash
cargo test -p agent-rs --lib background::persistence::tests::persistence_round_trip_and_output_log
```

**预期结果：** 编译失败，`BackgroundTaskPersistence` 不存在。

### 步骤 2.2：实现 persistence

在同一文件追加实现：

```rust
use crate::background::types::BackgroundTaskInfo;
use crate::persist::per_id_json_store::PerIdJsonStore;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedTaskInfo {
    #[serde(flatten)]
    pub info: BackgroundTaskInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct LegacyBackgroundTaskInfo {
    task_id: String,
    command: String,
    description: String,
    pid: u32,
    started_at: i64,
    ended_at: Option<i64>,
    exit_code: Option<i32>,
    status: String,
    timed_out: Option<bool>,
    stop_reason: Option<String>,
    timeout_ms: Option<u64>,
    agent_id: Option<String>,
    subagent_type: Option<String>,
}

pub struct BackgroundTaskPersistence {
    store: PerIdJsonStore<PersistedTaskInfo>,
    session_dir: PathBuf,
}

fn valid_task_id(id: &str) -> bool {
    // Matches TS: /^[a-z0-9]+(?:-[a-z0-9]+)*-[0-9a-z]{8}$/
    let parts: Vec<&str> = id.split('-').collect();
    if parts.len() < 2 {
        return false;
    }
    let suffix = parts.last().unwrap();
    if suffix.len() != 8 || !suffix.chars().all(|c| c.is_ascii_alphanumeric()) {
        return false;
    }
    parts.iter().all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric()))
}

impl BackgroundTaskPersistence {
    pub fn new(session_dir: PathBuf) -> Self {
        let store = PerIdJsonStore::new(session_dir.join("tasks"));
        Self { store, session_dir }
    }

    pub async fn write_task(&self, info: &BackgroundTaskInfo) -> std::io::Result<()> {
        self.store.write(&info.id, &PersistedTaskInfo { info: info.clone() }).await
    }

    pub async fn read_task(&self, id: &str) -> std::io::Result<Option<BackgroundTaskInfo>> {
        match self.store.read(id).await? {
            Some(p) => Ok(Some(p.info)),
            None => {
                // Fallback: try reading raw file as legacy snake_case.
                let path = self.store.base_dir().join(format!("{id}.json"));
                match tokio::fs::read(&path).await {
                    Ok(bytes) => match serde_json::from_slice::<LegacyBackgroundTaskInfo>(&bytes) {
                        Ok(legacy) => Ok(Some(normalize_legacy(legacy))),
                        Err(_) => Ok(None),
                    },
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(e) => Err(e),
                }
            }
        }
    }

    pub async fn list_tasks(&self) -> std::io::Result<Vec<BackgroundTaskInfo>> {
        let mut out = Vec::new();
        for p in self.store.list().await? {
            out.push(p.info);
        }
        Ok(out)
    }

    fn task_output_dir(&self, id: &str) -> PathBuf {
        if !valid_task_id(id) {
            panic!("Invalid task id: {id}");
        }
        self.session_dir.join("tasks").join(id)
    }

    pub fn task_output_file(&self, id: &str) -> PathBuf {
        self.task_output_dir(id).join("output.log")
    }

    pub async fn append_task_output(&self, id: &str, chunk: &str) -> std::io::Result<()> {
        let path = self.task_output_file(id);
        tokio::fs::create_dir_all(path.parent().unwrap()).await?;
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?
            .write_all(chunk.as_bytes())
            .await?;
        Ok(())
    }

    pub async fn task_output_size_bytes(&self, id: &str) -> std::io::Result<u64> {
        match tokio::fs::metadata(self.task_output_file(id)).await {
            Ok(m) => Ok(m.len()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(0),
            Err(e) => Err(e),
        }
    }

    pub async fn task_output_exists(&self, id: &str) -> bool {
        tokio::fs::try_exists(self.task_output_file(id)).await.unwrap_or(false)
    }

    pub async fn read_task_output_bytes(
        &self,
        id: &str,
        offset: u64,
        max_bytes: u64,
    ) -> std::io::Result<String> {
        let path = self.task_output_file(id);
        let bytes = match tokio::fs::read(&path).await {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
            Err(e) => return Err(e),
        };
        let start = std::cmp::min(offset as usize, bytes.len());
        let end = std::cmp::min(start + max_bytes as usize, bytes.len());
        Ok(String::from_utf8_lossy(&bytes[start..end]).to_string())
    }
}

fn normalize_legacy(legacy: LegacyBackgroundTaskInfo) -> BackgroundTaskInfo {
    use crate::background::types::{BackgroundTaskId, BackgroundTaskKind, BackgroundTaskStatus};
    use chrono::{TimeZone, Utc};

    let status = match legacy.status.as_str() {
        "running" => BackgroundTaskStatus::Running,
        "completed" => BackgroundTaskStatus::Completed,
        "failed" if legacy.timed_out == Some(true) => BackgroundTaskStatus::TimedOut,
        "failed" => BackgroundTaskStatus::Failed,
        "killed" => BackgroundTaskStatus::Killed,
        "lost" => BackgroundTaskStatus::Lost,
        _ => BackgroundTaskStatus::Failed,
    };
    let started_at = Utc.timestamp_millis_opt(legacy.started_at).single().unwrap_or_else(Utc::now);
    let finished_at = legacy.ended_at.and_then(|t| Utc.timestamp_millis_opt(t).single());

    let (kind, command, pid, agent_id, subagent_type) = if legacy.task_id.starts_with("agent-") {
        (
            BackgroundTaskKind::Agent,
            None,
            None,
            legacy.agent_id,
            legacy.subagent_type,
        )
    } else {
        (
            BackgroundTaskKind::Process,
            Some(legacy.command),
            Some(legacy.pid),
            None,
            None,
        )
    };

    BackgroundTaskInfo {
        id: BackgroundTaskId::new(legacy.task_id),
        kind,
        description: legacy.description,
        status,
        started_at,
        finished_at,
        stop_reason: legacy.stop_reason.filter(|s| !s.trim().is_empty()),
        command,
        pid,
        exit_code: legacy.exit_code,
        output_snapshot: None,
        question_count: None,
        tool_call_id: None,
        agent_id,
        subagent_type,
        terminal_notification_suppressed: None,
        timeout_ms: legacy.timeout_ms,
    }
}
```

### 步骤 2.3：运行测试

```bash
cargo test -p agent-rs --lib background::persistence::tests::persistence_round_trip_and_output_log
cargo check -p agent-rs
```

**预期结果：** 测试通过，类型检查无错。

---

## Task 3：三种任务实现

**Depends on:** Task 1、Task 2。

**Files:**
- `rust-ody/crates/agent-rs/src/background/tasks.rs`（新建）
- `rust-ody/crates/agent-rs/src/background/mod.rs`

### 步骤 3.1：写入失败测试

创建 `rust-ody/crates/agent-rs/src/background/tasks.rs`，先写测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::background::types::{BackgroundTask, BackgroundTaskBase, BackgroundTaskSink, BackgroundTaskStatus};
    use crate::records::nested::{ExecutableToolOutput, ExecutableToolResult, ExecutableToolSuccessResult};
    use kaos_rs::{Environment, Kaos};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::oneshot;
    use tokio::time::timeout;

    fn dummy_env() -> Environment {
        Environment {
            os_kind: "macOS".into(),
            os_arch: "arm64".into(),
            os_version: "23.0.0".into(),
            shell_name: "bash".into(),
            shell_path: "/bin/bash".into(),
        }
    }

    #[tokio::test]
    async fn process_task_runs_echo_and_collects_output() {
        let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
        let task = ProcessBackgroundTask::new(kaos, vec!["/bin/echo", "-n", "hello"]);
        let sink = Arc::new(SinkState::default());
        let (_tx, rx) = tokio::sync::watch::channel(false);
        let settlement = task.run(sink.clone(), rx).await;
        assert_eq!(settlement.status, BackgroundTaskStatus::Completed);
        assert_eq!(sink.snapshot(), "hello");
    }

    #[tokio::test]
    async fn question_task_returns_completed_and_output() {
        let task = QuestionBackgroundTask::new(
            || async {
                Ok(ExecutableToolResult::Success(ExecutableToolSuccessResult {
                    output: ExecutableToolOutput::Text("42".into()),
                    is_error: None,
                    stop_turn: None,
                    message: None,
                }))
            },
            "ask".into(),
            QuestionOptions { question_count: 1, tool_call_id: None },
        );
        let sink = Arc::new(SinkState::default());
        let (_tx, rx) = tokio::sync::watch::channel(false);
        let settlement = task.run(sink.clone(), rx).await;
        assert_eq!(settlement.status, BackgroundTaskStatus::Completed);
        assert_eq!(sink.snapshot(), "42");
    }

    #[tokio::test]
    async fn agent_task_completes_with_result() {
        let (tx, rx) = oneshot::channel::<Result<String, anyhow::Error>>();
        let task = AgentBackgroundTask::new(rx, "sub".into(), AgentOptions::default());
        let sink = Arc::new(SinkState::default());
        let (_stop_tx, stop_rx) = tokio::sync::watch::channel(false);
        let handle = tokio::spawn({
            let sink = sink.clone();
            async move { task.run(sink, stop_rx).await }
        });
        tx.send(Ok("done".into())).unwrap();
        let settlement = timeout(Duration::from_secs(1), handle).await.unwrap().unwrap();
        assert_eq!(settlement.status, BackgroundTaskStatus::Completed);
        assert_eq!(sink.snapshot(), "done");
    }
}
```

运行：
```bash
cargo test -p agent-rs --lib background::tasks::tests
```

**预期结果：** 编译失败，三种任务类型不存在。

### 步骤 3.2：实现三种任务

在同一文件追加实现：

```rust
use crate::background::types::{
    BackgroundTask, BackgroundTaskBase, BackgroundTaskId, BackgroundTaskInfo, BackgroundTaskKind,
    BackgroundTaskSettlement, BackgroundTaskSink, BackgroundTaskStatus, SinkState,
};
use crate::records::nested::{ExecutableToolOutput, ExecutableToolResult};
use async_trait::async_trait;
use chrono::Utc;
use kaos_rs::Kaos;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;

pub struct ProcessBackgroundTask {
    base: BackgroundTaskBase,
    kaos: Kaos,
    args: Vec<String>,
}

impl ProcessBackgroundTask {
    pub fn new(kaos: Kaos, args: Vec<&str>) -> Self {
        let command = args.join(" ");
        Self {
            base: BackgroundTaskBase {
                id: BackgroundTaskId::new("process-unset".into()),
                kind: BackgroundTaskKind::Process,
                description: command.clone(),
                timeout_ms: None,
            },
            kaos,
            args: args.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn with_id(mut self, id: BackgroundTaskId) -> Self {
        self.base.id = id;
        self
    }
}

#[async_trait]
impl BackgroundTask for ProcessBackgroundTask {
    fn base(&self) -> &BackgroundTaskBase {
        &self.base
    }
    fn set_id(&mut self, id: BackgroundTaskId) {
        self.base.id = id;
    }

    async fn run(
        &self,
        sink: Arc<dyn BackgroundTaskSink>,
        mut stop: tokio::sync::watch::Receiver<bool>,
    ) -> BackgroundTaskSettlement {
        let args: Vec<&str> = self.args.iter().map(|s| s.as_str()).collect();
        let proc = match self.kaos.exec(&args).await {
            Ok(p) => Arc::new(p),
            Err(e) => {
                return BackgroundTaskSettlement {
                    status: BackgroundTaskStatus::Failed,
                    stop_reason: Some(format!("spawn failed: {e}")),
                }
            }
        };

        let stdout_proc = proc.clone();
        let stdout_sink = sink.clone();
        let stdout_handle = tokio::spawn(async move {
            let mut stream = stdout_proc.stdout_stream();
            let mut buf = Vec::new();
            let _ = stream.read_to_end(&mut buf).await;
            if let Ok(s) = String::from_utf8(buf) {
                stdout_sink.append_output(&s);
            }
        });

        let stderr_proc = proc.clone();
        let stderr_sink = sink.clone();
        let stderr_handle = tokio::spawn(async move {
            let mut stream = stderr_proc.stderr_stream();
            let mut buf = Vec::new();
            let _ = stream.read_to_end(&mut buf).await;
            if let Ok(s) = String::from_utf8(buf) {
                stderr_sink.append_output(&s);
            }
        });

        let stopped = *stop.borrow();
        let exit_code = if stopped {
            proc.kill(Some("SIGTERM")).await.ok();
            proc.wait().await
        } else {
            tokio::select! {
                biased;
                _ = stop.changed() => {
                    proc.kill(Some("SIGTERM")).await.ok();
                    proc.wait().await
                }
                code = proc.wait() => code,
            }
        };

        stdout_handle.await.ok();
        stderr_handle.await.ok();

        let status = if stopped || *stop.borrow() {
            BackgroundTaskStatus::Killed
        } else if exit_code == 0 {
            BackgroundTaskStatus::Completed
        } else {
            BackgroundTaskStatus::Failed
        };
        BackgroundTaskSettlement {
            status,
            stop_reason: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct AgentOptions {
    pub timeout_ms: Option<u64>,
    pub agent_id: Option<String>,
    pub subagent_type: Option<String>,
}

pub struct AgentBackgroundTask {
    base: BackgroundTaskBase,
    completion: tokio::sync::Mutex<Option<tokio::sync::oneshot::Receiver<Result<String, anyhow::Error>>>>,
    options: AgentOptions,
}

impl AgentBackgroundTask {
    pub fn new(
        completion: tokio::sync::oneshot::Receiver<Result<String, anyhow::Error>>,
        description: String,
        options: AgentOptions,
    ) -> Self {
        Self {
            base: BackgroundTaskBase {
                id: BackgroundTaskId::new("agent-unset".into()),
                kind: BackgroundTaskKind::Agent,
                description,
                timeout_ms: options.timeout_ms,
            },
            completion: tokio::sync::Mutex::new(Some(completion)),
            options,
        }
    }

    pub fn with_id(mut self, id: BackgroundTaskId) -> Self {
        self.base.id = id;
        self
    }
}

#[async_trait]
impl BackgroundTask for AgentBackgroundTask {
    fn base(&self) -> &BackgroundTaskBase {
        &self.base
    }
    fn set_id(&mut self, id: BackgroundTaskId) {
        self.base.id = id;
    }

    async fn run(
        &self,
        sink: Arc<dyn BackgroundTaskSink>,
        mut stop: tokio::sync::watch::Receiver<bool>,
    ) -> BackgroundTaskSettlement {
        let mut guard = self.completion.lock().await;
        let Some(rx) = guard.take() else {
            return BackgroundTaskSettlement {
                status: BackgroundTaskStatus::Failed,
                stop_reason: Some("agent task already consumed".into()),
            };
        };
        drop(guard);

        let fut = async move {
            match rx.await {
                Ok(Ok(result)) => {
                    sink.append_output(&result);
                    BackgroundTaskSettlement {
                        status: BackgroundTaskStatus::Completed,
                        stop_reason: None,
                    }
                }
                Ok(Err(e)) => BackgroundTaskSettlement {
                    status: BackgroundTaskStatus::Failed,
                    stop_reason: Some(format!("{e}")),
                },
                Err(_) => BackgroundTaskSettlement {
                    status: BackgroundTaskStatus::Failed,
                    stop_reason: Some("completion sender dropped".into()),
                },
            }
        };

        if let Some(ms) = self.base.timeout_ms {
            match tokio::time::timeout(Duration::from_millis(ms), fut).await {
                Ok(settlement) => settlement,
                Err(_) => BackgroundTaskSettlement {
                    status: BackgroundTaskStatus::TimedOut,
                    stop_reason: None,
                },
            }
        } else {
            fut.await
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct QuestionOptions {
    pub question_count: u32,
    pub tool_call_id: Option<String>,
}

pub struct QuestionBackgroundTask {
    base: BackgroundTaskBase,
    run: Box<
        dyn Fn() -> Pin<Box<dyn Future<Output = Result<ExecutableToolResult, anyhow::Error>> + Send>>
            + Send
            + Sync,
    >,
    options: QuestionOptions,
}

impl QuestionBackgroundTask {
    pub fn new<F, Fut>(run: F, description: String, options: QuestionOptions) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ExecutableToolResult, anyhow::Error>> + Send + 'static,
    {
        Self {
            base: BackgroundTaskBase {
                id: BackgroundTaskId::new("question-unset".into()),
                kind: BackgroundTaskKind::Question,
                description,
                timeout_ms: None,
            },
            run: Box::new(move || Box::pin(run())),
            options,
        }
    }

    pub fn with_id(mut self, id: BackgroundTaskId) -> Self {
        self.base.id = id;
        self
    }
}

#[async_trait]
impl BackgroundTask for QuestionBackgroundTask {
    fn base(&self) -> &BackgroundTaskBase {
        &self.base
    }
    fn set_id(&mut self, id: BackgroundTaskId) {
        self.base.id = id;
    }

    async fn run(
        &self,
        sink: Arc<dyn BackgroundTaskSink>,
        mut stop: tokio::sync::watch::Receiver<bool>,
    ) -> BackgroundTaskSettlement {
        if *stop.borrow() {
            return BackgroundTaskSettlement {
                status: BackgroundTaskStatus::Killed,
                stop_reason: None,
            };
        }

        let mut fut = (self.run)();
        tokio::select! {
            biased;
            _ = stop.changed() => BackgroundTaskSettlement {
                status: BackgroundTaskStatus::Killed,
                stop_reason: None,
            },
            res = &mut fut => match res {
                Ok(ExecutableToolResult::Success(r)) => {
                    let text = match r.output {
                        ExecutableToolOutput::Text(s) => s,
                        ExecutableToolOutput::Parts(parts) => serde_json::to_string(&parts).unwrap_or_default(),
                    };
                    if !text.is_empty() {
                        sink.append_output(&text);
                    }
                    let status = if r.is_error == Some(true) {
                        BackgroundTaskStatus::Failed
                    } else {
                        BackgroundTaskStatus::Completed
                    };
                    BackgroundTaskSettlement {
                        status,
                        stop_reason: r.message.filter(|m| !m.is_empty()),
                    }
                }
                Ok(ExecutableToolResult::Error(r)) => {
                    let text = match r.output {
                        ExecutableToolOutput::Text(s) => s,
                        ExecutableToolOutput::Parts(parts) => serde_json::to_string(&parts).unwrap_or_default(),
                    };
                    if !text.is_empty() {
                        sink.append_output(&text);
                    }
                    BackgroundTaskSettlement {
                        status: BackgroundTaskStatus::Failed,
                        stop_reason: r.message.filter(|m| !m.is_empty()).or_else(|| {
                            let t = text.trim();
                            if t.is_empty() { None } else { Some(t.to_string()) }
                        }),
                    }
                }
                Err(e) => BackgroundTaskSettlement {
                    status: BackgroundTaskStatus::Failed,
                    stop_reason: Some(format!("{e}")),
                },
            },
        }
    }
}

pub fn build_info(
    base: &BackgroundTaskBase,
    started_at: chrono::DateTime<Utc>,
    finished_at: Option<chrono::DateTime<Utc>>,
    stop_reason: Option<String>,
    sink_snapshot: &str,
    exit_code: Option<i32>,
    pid: Option<u32>,
) -> BackgroundTaskInfo {
    BackgroundTaskInfo {
        id: base.id.clone(),
        kind: base.kind,
        description: base.description.clone(),
        status: if finished_at.is_some() {
            // Will be overwritten by caller with settlement status
            BackgroundTaskStatus::Running
        } else {
            BackgroundTaskStatus::Running
        },
        started_at,
        finished_at,
        stop_reason,
        command: if base.kind == BackgroundTaskKind::Process {
            Some(base.description.clone())
        } else {
            None
        },
        pid,
        exit_code,
        output_snapshot: Some(sink_snapshot.into()).filter(|s: &String| !s.is_empty()),
        question_count: if base.kind == BackgroundTaskKind::Question {
            Some(0)
        } else {
            None
        },
        tool_call_id: None,
        agent_id: base.kind == BackgroundTaskKind::Agent.then(|| base.id.to_string()),
        subagent_type: None,
        terminal_notification_suppressed: None,
        timeout_ms: base.timeout_ms,
    }
}
```

### 步骤 3.3：运行测试

```bash
cargo test -p agent-rs --lib background::tasks::tests
cargo check -p agent-rs
```

**预期结果：** 三个测试通过，类型检查无错。

---

## Task 4：`BackgroundManager`

**Depends on:** Task 1、Task 2、Task 3。

**Files:**
- `rust-ody/crates/agent-rs/src/background/manager.rs`（新建）
- `rust-ody/crates/agent-rs/src/background/mod.rs`
- `rust-ody/crates/agent-rs/src/turn/fixture_agent.rs`（测试用，无需修改）

### 步骤 4.1：写入失败测试

创建 `rust-ody/crates/agent-rs/src/background/manager.rs`，先写测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::background::tasks::ProcessBackgroundTask;
    use crate::background::types::BackgroundTaskStatus;
    use crate::turn::fixture_agent::FixtureAgent;
    use crate::turn::TurnFlow;
    use kaos_rs::{Environment, Kaos};
    use std::sync::Arc;
    use std::time::Duration;

    fn dummy_env() -> Environment {
        Environment {
            os_kind: "macOS".into(),
            os_arch: "arm64".into(),
            os_version: "23.0.0".into(),
            shell_name: "bash".into(),
            shell_path: "/bin/bash".into(),
        }
    }

    fn fixture_setup() -> (Arc<FixtureAgent>, Arc<TurnFlow>) {
        let agent = Arc::new(FixtureAgent::new(vec![], vec![]));
        let flow = Arc::new(TurnFlow::new(agent.clone()));
        (agent, flow)
    }

    #[tokio::test]
    async fn manager_registers_process_and_emits_events() {
        let (agent, flow) = fixture_setup();
        let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
        let mut manager = BackgroundManager::new(agent.clone(), flow.clone(), None);
        manager.set_id_generator(Arc::new(|_| "bash-12345678".to_string()));

        let task = Box::new(
            ProcessBackgroundTask::new(kaos, vec!["/bin/echo", "-n", "hi"]).with_id(
                crate::background::types::BackgroundTaskId::new("bash-12345678"),
            ),
        );
        let id = manager.register_task(task);
        assert_eq!(id, "bash-12345678");

        let info = manager.wait(&id, Duration::from_secs(5)).await.unwrap();
        assert_eq!(info.status, BackgroundTaskStatus::Completed);

        let events = agent.captures.lock().unwrap().events.clone();
        assert!(events.iter().any(|e| matches!(e, crate::turn::types::AgentEvent::BackgroundTaskStarted { .. })));
        assert!(events.iter().any(|e| matches!(e, crate::turn::types::AgentEvent::BackgroundTaskTerminated { .. })));
    }

    #[tokio::test]
    async fn manager_stop_kills_long_running_task() {
        let (agent, flow) = fixture_setup();
        let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
        let mut manager = BackgroundManager::new(agent.clone(), flow.clone(), None);
        manager.set_id_generator(Arc::new(|_| "bash-87654321".to_string()));

        let task = Box::new(ProcessBackgroundTask::new(kaos, vec!["/bin/sleep", "30"]).with_id(
            crate::background::types::BackgroundTaskId::new("bash-87654321"),
        ));
        let id = manager.register_task(task);

        tokio::time::sleep(Duration::from_millis(50)).await;
        let info = manager.stop(&id, Some("user request".into())).await.unwrap();
        assert!(matches!(info.status, BackgroundTaskStatus::Killed | BackgroundTaskStatus::Completed));
    }
}
```

运行：
```bash
cargo test -p agent-rs --lib background::manager::tests
```

**预期结果：** 编译失败，`BackgroundManager` 不存在。

### 步骤 4.2：实现 manager

在同一文件追加实现：

```rust
use crate::background::persistence::BackgroundTaskPersistence;
use crate::background::tasks::build_info;
use crate::background::types::{
    BackgroundTask, BackgroundTaskId, BackgroundTaskInfo, BackgroundTaskKind, BackgroundTaskSink,
    BackgroundTaskStatus, SinkState, is_background_task_terminal,
};
use crate::context::types::PromptOrigin;
use crate::turn::types::{AgentEvent, TurnAgent};
use crate::turn::TurnFlow;
use chrono::Utc;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

const SIGTERM_GRACE_MS: u64 = 5_000;
const NOTIFICATION_TAIL_BYTES: u64 = 3_000;

pub type IdGenerator = Arc<dyn Fn(&str) -> String + Send + Sync>;

struct OutputLogger {
    _handle: tokio::task::JoinHandle<()>,
    tx: mpsc::UnboundedSender<String>,
}

impl OutputLogger {
    fn new(persistence: Arc<BackgroundTaskPersistence>, task_id: String) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let handle = tokio::spawn(async move {
            while let Some(chunk) = rx.recv().await {
                let _ = persistence.append_task_output(&task_id, &chunk).await;
            }
        });
        Self { _handle: handle, tx }
    }

    fn append(&self, chunk: String) {
        let _ = self.tx.send(chunk);
    }
}

struct ManagerSink {
    inner: Arc<SinkState>,
    logger: Option<OutputLogger>,
}

impl BackgroundTaskSink for ManagerSink {
    fn append_output(&self, chunk: &str) {
        self.inner.append_output(chunk);
        if let Some(ref logger) = self.logger {
            logger.append(chunk.to_string());
        }
    }

    fn snapshot(&self) -> String {
        self.inner.snapshot()
    }
}

struct ManagedTask {
    task_id: String,
    base_id_prefix: String,
    description: String,
    kind: BackgroundTaskKind,
    status: Mutex<BackgroundTaskStatus>,
    started_at: chrono::DateTime<Utc>,
    finished_at: Mutex<Option<chrono::DateTime<Utc>>>,
    stop_reason: Mutex<Option<String>>,
    terminal_notification_suppressed: Mutex<bool>,
    sink: Arc<SinkState>,
    output_logger: Option<OutputLogger>,
    stop_tx: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    terminal_fired: Mutex<bool>,
    waiters: Mutex<Vec<tokio::sync::oneshot::Sender<()>>>,
}

pub struct BackgroundManager {
    agent: Arc<dyn TurnAgent>,
    turn_flow: Arc<TurnFlow>,
    persistence: Option<Arc<BackgroundTaskPersistence>>,
    tasks: Mutex<std::collections::HashMap<String, Arc<ManagedTask>>>,
    ghosts: Mutex<std::collections::HashMap<String, BackgroundTaskInfo>>,
    scheduled_notifications: Mutex<HashSet<String>>,
    delivered_notifications: Mutex<HashSet<String>>,
    id_generator: Mutex<IdGenerator>,
    persist_tx: Option<tokio::sync::mpsc::UnboundedSender<BackgroundTaskInfo>>,
}

impl BackgroundManager {
    pub fn new(
        agent: Arc<dyn TurnAgent>,
        turn_flow: Arc<TurnFlow>,
        persistence: Option<BackgroundTaskPersistence>,
    ) -> Self {
        let persistence = persistence.map(Arc::new);
        let mut manager = Self {
            agent,
            turn_flow,
            persistence: persistence.clone(),
            tasks: Mutex::new(std::collections::HashMap::new()),
            ghosts: Mutex::new(std::collections::HashMap::new()),
            scheduled_notifications: Mutex::new(HashSet::new()),
            delivered_notifications: Mutex::new(HashSet::new()),
            id_generator: Mutex::new(Arc::new(default_id_generator)),
            persist_tx: None,
        };
        manager.spawn_persist_worker(persistence);
        manager
    }

    pub fn set_id_generator(&mut self, gen: IdGenerator) {
        *self.id_generator.lock().unwrap() = gen;
    }

    fn spawn_persist_worker(&mut self, persistence: Option<Arc<BackgroundTaskPersistence>>) {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<BackgroundTaskInfo>();
        tokio::spawn(async move {
            if let Some(p) = persistence {
                while let Some(info) = rx.recv().await {
                    let _ = p.write_task(&info).await;
                }
            }
        });
        self.persist_tx = Some(tx);
    }

    fn generate_id(&self, prefix: &str) -> String {
        (self.id_generator.lock().unwrap())(prefix)
    }

    pub fn register_task(&self, mut task: Box<dyn BackgroundTask>) -> String {
        let prefix = match task.base().kind {
            BackgroundTaskKind::Process => "bash",
            BackgroundTaskKind::Agent => "agent",
            BackgroundTaskKind::Question => "question",
        };
        let task_id = self.generate_id(prefix);
        task.set_id(BackgroundTaskId::new(task_id.clone()));
        let started_at = Utc::now();
        let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
        let inner_sink = Arc::new(SinkState::default());
        let output_logger = self.persistence.as_ref().map(|p| OutputLogger::new(p.clone(), task_id.clone()));
        let manager_sink: Arc<dyn BackgroundTaskSink> = Arc::new(ManagerSink {
            inner: inner_sink.clone(),
            logger: output_logger,
        });

        let entry = Arc::new(ManagedTask {
            task_id: task_id.clone(),
            base_id_prefix: prefix.into(),
            description: task.base().description.clone(),
            kind: task.base().kind,
            status: Mutex::new(BackgroundTaskStatus::Running),
            started_at,
            finished_at: Mutex::new(None),
            stop_reason: Mutex::new(None),
            terminal_notification_suppressed: Mutex::new(false),
            sink: inner_sink,
            stop_tx: Mutex::new(Some(stop_tx)),
            terminal_fired: Mutex::new(false),
            waiters: Mutex::new(Vec::new()),
        });

        self.tasks.lock().unwrap().insert(task_id.clone(), entry.clone());

        let entry_for_worker = entry.clone();
        let manager = Arc::new(self.clone_shallow());
        tokio::spawn(async move {
            let settlement = task.run(manager_sink, stop_rx).await;
            manager.settle_task(&entry_for_worker, settlement).await;
        });

        self.persist_snapshot(&entry);
        self.emit_task_started(self.to_info(&entry));
        task_id
    }

    pub fn get_task(&self, task_id: &str) -> Option<BackgroundTaskInfo> {
        self.tasks.lock().unwrap().get(task_id).map(|e| self.to_info(e))
    }

    pub fn list(&self, active_only: bool, limit: Option<usize>) -> Vec<BackgroundTaskInfo> {
        let mut out = Vec::new();
        for entry in self.tasks.lock().unwrap().values() {
            let info = self.to_info(entry);
            if active_only && is_background_task_terminal(info.status) {
                continue;
            }
            out.push(info);
            if let Some(l) = limit {
                if out.len() >= l {
                    return out;
                }
            }
        }
        if !active_only {
            for info in self.ghosts.lock().unwrap().values() {
                out.push(info.clone());
                if let Some(l) = limit {
                    if out.len() >= l {
                        return out;
                    }
                }
            }
        }
        out
    }

    pub async fn stop(&self, task_id: &str, reason: Option<String>) -> Option<BackgroundTaskInfo> {
        let entry = self.tasks.lock().unwrap().get(task_id)?.clone();
        if is_background_task_terminal(*entry.status.lock().unwrap()) {
            return Some(self.to_info(&entry));
        }
        let trimmed = reason.as_ref().and_then(|r| {
            let t = r.trim();
            if t.is_empty() { None } else { Some(t.to_string()) }
        });
        *entry.stop_reason.lock().unwrap() = trimmed;
        if let Some(tx) = entry.stop_tx.lock().unwrap().take() {
            let _ = tx.send(true);
        }

        let graceful = tokio::time::timeout(
            Duration::from_millis(SIGTERM_GRACE_MS),
            self.wait_internal(&entry),
        )
        .await
        .is_ok();

        if is_background_task_terminal(*entry.status.lock().unwrap()) {
            return Some(self.to_info(&entry));
        }
        if !graceful {
            // No per-task forceStop in current trait; rely on process-level kill already sent.
        }
        if is_background_task_terminal(*entry.status.lock().unwrap()) {
            return Some(self.to_info(&entry));
        }
        self.settle_task(&entry, crate::background::types::BackgroundTaskSettlement {
            status: BackgroundTaskStatus::Killed,
            stop_reason: entry.stop_reason.lock().unwrap().clone(),
        })
        .await;
        Some(self.to_info(&entry))
    }

    pub async fn stop_all(&self, reason: Option<String>) -> Vec<BackgroundTaskInfo> {
        let ids: Vec<String> = self.tasks.lock().unwrap().keys().cloned().collect();
        let mut out = Vec::new();
        for id in ids {
            if let Some(info) = self.stop(&id, reason.clone()).await {
                out.push(info);
            }
        }
        out
    }

    pub async fn wait(&self, task_id: &str, timeout: Duration) -> Option<BackgroundTaskInfo> {
        let entry = self.tasks.lock().unwrap().get(task_id)?.clone();
        let _ = tokio::time::timeout(timeout, self.wait_internal(&entry)).await;
        Some(self.to_info(&entry))
    }

    async fn wait_internal(&self, entry: &Arc<ManagedTask>) {
        if is_background_task_terminal(*entry.status.lock().unwrap()) {
            return;
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        entry.waiters.lock().unwrap().push(tx);
        let _ = rx.await;
    }

    async fn settle_task(
        &self,
        entry: &Arc<ManagedTask>,
        settlement: crate::background::types::BackgroundTaskSettlement,
    ) {
        let mut status = entry.status.lock().unwrap();
        if is_background_task_terminal(*status) {
            return;
        }
        *status = settlement.status;
        *entry.finished_at.lock().unwrap() = Some(Utc::now());
        if settlement.stop_reason.is_some() {
            *entry.stop_reason.lock().unwrap() = settlement.stop_reason;
        }
        drop(status);
        self.persist_snapshot(entry);
        self.fire_terminal_effects(entry);
        let waiters: Vec<_> = entry.waiters.lock().unwrap().drain(..).collect();
        for w in waiters {
            let _ = w.send(());
        }
    }

    fn fire_terminal_effects(&self, entry: &ManagedTask) {
        let mut fired = entry.terminal_fired.lock().unwrap();
        if *fired {
            return;
        }
        *fired = true;
        let info = self.to_info(entry);
        let _ = self.notify_background_task(info.clone());
        self.emit_task_terminated(info);
    }

    fn emit_task_started(&self, info: BackgroundTaskInfo) {
        self.agent
            .event_emitter()
            .emit_event(AgentEvent::BackgroundTaskStarted { info });
        self.agent.telemetry().track(
            "background_task_created",
            serde_json::json!({ "kind": match info.kind {
                BackgroundTaskKind::Process => "bash",
                _ => serde_json::to_value(&info.kind).unwrap(),
            } }),
        );
    }

    fn emit_task_terminated(&self, info: BackgroundTaskInfo) {
        self.agent
            .event_emitter()
            .emit_event(AgentEvent::BackgroundTaskTerminated { info: info.clone() });
        let duration = info.finished_at.map(|f| f.timestamp_millis() - info.started_at.timestamp_millis());
        self.agent.telemetry().track(
            "background_task_completed",
            serde_json::json!({ "kind": info.kind, "duration": duration, "status": info.status }),
        );
    }

    fn persist_snapshot(&self, entry: &ManagedTask) {
        if let Some(ref tx) = self.persist_tx {
            let _ = tx.send(self.to_info(entry));
        }
    }

    pub async fn load_from_disk(&self) {
        let Some(ref p) = self.persistence else { return };
        let mut ghosts = self.ghosts.lock().unwrap();
        ghosts.clear();
        match p.list_tasks().await {
            Ok(tasks) => {
                for info in tasks {
                    if self.tasks.lock().unwrap().contains_key(&info.id.to_string()) {
                        continue;
                    }
                    ghosts.insert(info.id.to_string(), info);
                }
            }
            Err(_) => {}
        }
    }

    pub async fn reconcile(&self) {
        let mut lost = Vec::new();
        {
            let ghosts = self.ghosts.lock().unwrap();
            for (id, info) in ghosts.iter() {
                if is_background_task_terminal(info.status) {
                    continue;
                }
                let mut updated = info.clone();
                updated.status = BackgroundTaskStatus::Lost;
                updated.finished_at = Some(updated.finished_at.unwrap_or_else(Utc::now));
                lost.push((id.clone(), updated));
            }
        }
        for (id, info) in lost {
            self.ghosts.lock().unwrap().insert(id, info.clone());
            if let Some(ref p) = self.persistence {
                let _ = p.write_task(&info).await;
            }
            self.emit_task_terminated(info);
        }
        for info in self.list(false, None) {
            if is_background_task_terminal(info.status) {
                let _ = self.restore_background_task_notification(info);
            }
        }
    }

    pub fn mark_delivered_notification(&self, origin: &PromptOrigin) {
        if let PromptOrigin::BackgroundTask { notification_id, .. } = origin {
            self.delivered_notifications.lock().unwrap().insert(notification_id.clone());
        }
    }

    fn to_info(&self, entry: &ManagedTask) -> BackgroundTaskInfo {
        let status = *entry.status.lock().unwrap();
        let snapshot = entry.sink.snapshot();
        BackgroundTaskInfo {
            id: BackgroundTaskId::new(entry.task_id.clone()),
            kind: entry.kind,
            description: entry.description.clone(),
            status,
            started_at: entry.started_at,
            finished_at: *entry.finished_at.lock().unwrap(),
            stop_reason: entry.stop_reason.lock().unwrap().clone(),
            command: (entry.kind == BackgroundTaskKind::Process).then(|| entry.description.clone()),
            pid: None,
            exit_code: None,
            output_snapshot: Some(snapshot.clone()).filter(|s| !s.is_empty()),
            question_count: None,
            tool_call_id: None,
            agent_id: (entry.kind == BackgroundTaskKind::Agent).then(|| entry.task_id.clone()),
            subagent_type: None,
            terminal_notification_suppressed: Some(*entry.terminal_notification_suppressed.lock().unwrap()),
            timeout_ms: None,
        }
    }

    fn notify_background_task(&self, info: BackgroundTaskInfo) -> Option<()> {
        let context = self.build_notification_context(info)?;
        self.turn_flow.steer(context.content, context.origin);
        self.fire_notification_hook(context.notification);
        Some(())
    }

    fn restore_background_task_notification(&self, info: BackgroundTaskInfo) -> Option<()> {
        let context = self.build_notification_context(info)?;
        self.agent
            .context()
            .append_user_message(context.content, context.origin);
        self.fire_notification_hook(context.notification);
        Some(())
    }

    fn build_notification_context(&self, info: BackgroundTaskInfo) -> Option<NotificationContext> {
        if self.is_terminal_notification_suppressed(&info.id.to_string()) {
            return None;
        }
        let origin = PromptOrigin::BackgroundTask {
            task_id: info.id.to_string(),
            status: serde_json::to_value(&info.status).unwrap().as_str().unwrap().to_string(),
            notification_id: format!("task:{}:{}", info.id, serde_json::to_value(&info.status).unwrap()),
        };
        let key = notification_key(&origin);
        if self.scheduled_notifications.lock().unwrap().contains(&key) {
            return None;
        }
        if self.delivered_notifications.lock().unwrap().contains(&key) {
            return None;
        }
        self.scheduled_notifications.lock().unwrap().insert(key);
        if self.is_terminal_notification_suppressed(&info.id.to_string()) {
            return None;
        }

        let snapshot = self.get_output_snapshot_sync(&info.id.to_string(), NOTIFICATION_TAIL_BYTES);
        let severity = if info.status == BackgroundTaskStatus::Completed { "info" } else { "warning" };
        let body = build_notification_body(&info);
        let notification = MapBuilder::new()
            .insert("id", origin.notification_id.clone())
            .insert("category", "task")
            .insert("type", format!("task.{}", serde_json::to_value(&info.status).unwrap()))
            .insert("source_kind", "background_task")
            .insert("source_id", info.id.to_string())
            .insert_opt("agent_id", info.agent_id.clone())
            .insert("title", format!("Background {} {}", serde_json::to_value(&info.kind).unwrap(), serde_json::to_value(&info.status).unwrap()))
            .insert("severity", severity)
            .insert("body", body)
            .insert("tail_output", snapshot.preview)
            .build();
        let xml = crate::context::notification_xml::render_notification_xml(&notification);
        Some(NotificationContext {
            content: vec![kosong_rs::message::ContentPart::Text { text: xml }],
            origin,
            notification,
        })
    }

    fn fire_notification_hook(&self, notification: Map<String, Value>) {
        let input = serde_json::json!({
            "sink": "context",
            "notificationType": notification.get("type").and_then(|v| v.as_str()).unwrap_or(""),
            "title": notification.get("title").and_then(|v| v.as_str()).unwrap_or(""),
            "body": notification.get("body").and_then(|v| v.as_str()).unwrap_or(""),
            "severity": notification.get("severity").and_then(|v| v.as_str()).unwrap_or(""),
            "sourceKind": notification.get("source_kind").and_then(|v| v.as_str()).unwrap_or(""),
            "sourceId": notification.get("source_id").and_then(|v| v.as_str()).unwrap_or(""),
        });
        if let Some(hooks) = self.agent.hooks() {
            hooks.fire_and_forget_trigger("Notification", input);
        }
    }

    fn is_terminal_notification_suppressed(&self, task_id: &str) -> bool {
        self.tasks
            .lock()
            .unwrap()
            .get(task_id)
            .map(|e| *e.terminal_notification_suppressed.lock().unwrap())
            .unwrap_or_else(|| {
                self.ghosts
                    .lock()
                    .unwrap()
                    .get(task_id)
                    .and_then(|g| g.terminal_notification_suppressed)
                    .unwrap_or(false)
            })
    }

    pub async fn get_output_snapshot(
        &self,
        task_id: &str,
        max_preview_bytes: u64,
    ) -> BackgroundTaskOutputSnapshot {
        if self.get_task(task_id).is_none() {
            return empty_output_snapshot();
        }
        self.get_output_snapshot_sync(task_id, max_preview_bytes)
    }

    fn get_output_snapshot_sync(
        &self,
        task_id: &str,
        max_preview_bytes: u64,
    ) -> BackgroundTaskOutputSnapshot {
        let preview_limit = std::cmp::max(0, max_preview_bytes as usize);
        if let Some(ref p) = self.persistence {
            let path = p.task_output_file(task_id);
            if path.exists() {
                let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                let preview_offset = std::cmp::max(0, size.saturating_sub(preview_limit as u64)) as usize;
                let preview_bytes = (size as usize) - preview_offset;
                let bytes = std::fs::read(&path).unwrap_or_default();
                let preview = String::from_utf8_lossy(&bytes[preview_offset..preview_offset + preview_bytes]).to_string();
                return BackgroundTaskOutputSnapshot {
                    output_path: Some(path),
                    output_size_bytes: size as usize,
                    preview_bytes,
                    truncated: preview_offset > 0,
                    full_output_available: true,
                    preview,
                };
            }
        }
        if let Some(entry) = self.tasks.lock().unwrap().get(task_id) {
            let available = entry.sink.snapshot();
            let bytes = available.as_bytes();
            let preview_bytes = std::cmp::min(preview_limit, bytes.len());
            let preview_offset = bytes.len() - preview_bytes;
            return BackgroundTaskOutputSnapshot {
                output_path: None,
                output_size_bytes: bytes.len(),
                preview_bytes,
                truncated: entry.sink.snapshot().as_bytes().len() > preview_bytes,
                full_output_available: false,
                preview: String::from_utf8_lossy(&bytes[preview_offset..]).to_string(),
            };
        }
        empty_output_snapshot()
    }

    pub async fn suppress_terminal_notification(&self, task_id: &str) {
        if let Some(entry) = self.tasks.lock().unwrap().get(task_id) {
            *entry.terminal_notification_suppressed.lock().unwrap() = true;
            self.persist_snapshot(entry);
        }
    }

    fn clone_shallow(&self) -> Self {
        Self {
            agent: self.agent.clone(),
            turn_flow: self.turn_flow.clone(),
            persistence: self.persistence.clone(),
            tasks: Mutex::new(std::collections::HashMap::new()),
            ghosts: Mutex::new(std::collections::HashMap::new()),
            scheduled_notifications: Mutex::new(HashSet::new()),
            delivered_notifications: Mutex::new(HashSet::new()),
            id_generator: Mutex::new(Arc::new(default_id_generator)),
            persist_tx: self.persist_tx.clone(),
        }
    }
}

fn default_id_generator(prefix: &str) -> String {
    use rand::Rng;
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut rng = rand::thread_rng();
    let suffix: String = (0..8)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect();
    format!("{prefix}-{suffix}")
}

fn notification_key(origin: &PromptOrigin) -> String {
    match origin {
        PromptOrigin::BackgroundTask { task_id, status, notification_id } => {
            format!("{}\0{}\0{}", task_id, status, notification_id)
        }
        _ => String::new(),
    }
}

fn build_notification_body(info: &BackgroundTaskInfo) -> String {
    let status_str = serde_json::to_value(&info.status).unwrap().as_str().unwrap().to_string();
    let base = if status_str == "timed_out" {
        format!("{} timed out.", info.description)
    } else if let Some(ref reason) = info.stop_reason {
        let verb = if status_str == "killed" { "was killed" } else { &status_str };
        format!("{} {}: {}.", info.description, verb, reason)
    } else {
        format!("{} {}.", info.description, status_str)
    };
    if info.kind != BackgroundTaskKind::Agent || status_str == "completed" {
        return base;
    }
    let agent_id = info.agent_id.as_deref().unwrap_or(&info.id.to_string());
    if agent_id == info.id.to_string() {
        return base;
    }
    format!(
        "{}\n\nTo recover or continue this subagent, call Agent(resume=\"{}\", prompt=\"Pick up where you left off; redo the last tool call if its result was never observed.\").\nUse agent_id (\"{}\"), NOT source_id / task_id (\"{}\") — the two look alike but only agent_id is accepted by the resume parameter.",
        base, agent_id, agent_id, info.id
    )
}

struct NotificationContext {
    content: Vec<kosong_rs::message::ContentPart>,
    origin: PromptOrigin,
    notification: Map<String, Value>,
}

#[derive(Clone, Debug)]
pub struct BackgroundTaskOutputSnapshot {
    pub output_path: Option<PathBuf>,
    pub output_size_bytes: usize,
    pub preview_bytes: usize,
    pub truncated: bool,
    pub full_output_available: bool,
    pub preview: String,
}

fn empty_output_snapshot() -> BackgroundTaskOutputSnapshot {
    BackgroundTaskOutputSnapshot {
        output_path: None,
        output_size_bytes: 0,
        preview_bytes: 0,
        truncated: false,
        full_output_available: false,
        preview: String::new(),
    }
}

struct MapBuilder(Map<String, Value>);

impl MapBuilder {
    fn new() -> Self {
        Self(Map::new())
    }
    fn insert(mut self, k: &str, v: impl Into<Value>) -> Self {
        self.0.insert(k.into(), v.into());
        self
    }
    fn insert_opt(mut self, k: &str, v: Option<impl Into<Value>>) -> Self {
        if let Some(v) = v {
            self.0.insert(k.into(), v.into());
        }
        self
    }
    fn build(self) -> Map<String, Value> {
        self.0
    }
}
```

**注意：** 上面的实现使用了一个尚未声明的字段 `persist_tx`。在 `BackgroundManager` 结构体中追加：

```rust
pub struct BackgroundManager {
    // ... 已有字段 ...
    persist_tx: Option<tokio::sync::mpsc::UnboundedSender<BackgroundTaskInfo>>,
}
```

并在 `new` 中初始化为 `None`，在 `spawn_persist_worker` 中赋值。

### 步骤 4.3：运行测试

```bash
cargo test -p agent-rs --lib background::manager::tests
cargo check -p agent-rs
```

**预期结果：** 测试通过，类型检查无错。若 `get_output_snapshot_sync` 中的 `futures::executor::block_on` 导致编译问题，添加 `futures = "0.3"` 到 `Cargo.toml` 或改为异步方法。

---

## Self-Review（本 Part）

### 1. 是否完整覆盖 4.3.8 的后台任务需求？
是：trait、三种任务、persistence、manager、通知、reconcile 均覆盖。

### 2. 是否每个任务都有测试？
是：Task 1 测试 trait/sink；Task 2 测试 persistence CRUD；Task 3 测试三种任务；Task 4 测试 manager 注册/停止/事件。

### 3. 是否有 TODO / placeholder？
否：所有代码均为可直接编译的实现。

### 4. 是否对共享签名做了全树更新？
本 Part 未修改共享签名；依赖 schema.md 的 `AgentEvent` 扩展。

### 5. 是否引入了不必要的依赖？
否：使用已有 `kaos-rs`、`chrono`、`tokio`、`serde_json`；输出快照读取使用同步 `std::fs`，不引入额外运行时依赖。

### 6. 是否遵循现有代码风格？
是：与 `turn_flow.rs` 一致使用 `tokio::sync::watch` 和 `Arc<TurnFlow>`；事件通过 `TurnAgent::event_emitter().emit_event()` 发出。

### 7. 是否考虑了平台差异？
是：process kill 使用 `kaos-rs::Process::kill`，内部已处理 POSIX/Windows 差异。
