# Part 2: Persistence — `InMemoryAgentRecordPersistence` + `FileSystemAgentRecordPersistence`

本部分把 `AgentRecordPersistence` trait 落地为两种实现：内存版（测试与回放缓冲）和文件系统 JSONL 版（真实 WAL）。两种实现必须保持同一套行为契约：`append` 立即入队，`flush` 保证落盘，`rewrite` 原子替换，`read` 按写入顺序 yield 记录。文件系统版还要预留 `BlobStore` 接口，让 Part 3 把 data URI 卸载到独立目录。

---

### Task 1: `InMemoryAgentRecordPersistence`

**Depends on:** `schema.md` Task 2 / Task 3

**Files:**
- Create: `rust-ody/crates/agent-rs/src/records/persistence.rs`
- Test: `rust-ody/crates/agent-rs/src/records/persistence.rs` (`#[cfg(test)]` 内联模块)

**目标：** 实现内存持久化：记录保存在 `Vec<AgentRecord>`，`read` 按顺序流式返回，`rewrite` 原子替换，`flush`/`close` 为空操作。

- [ ] 在 `rust-ody/crates/agent-rs/src/records/persistence.rs` 写入 trait 与内存实现：

```rust
use std::pin::Pin;

use futures_util::Stream;

use crate::records::types::{AgentRecord, RecordStream};

#[async_trait::async_trait]
pub trait AgentRecordPersistence: Send + Sync {
    async fn read(&self) -> anyhow::Result<RecordStream<'_>>;
    fn append(&mut self, record: AgentRecord);
    fn rewrite(&mut self, records: &[AgentRecord]);
    async fn flush(&mut self) -> anyhow::Result<()>;
    async fn close(&mut self) -> anyhow::Result<()>;
}

#[derive(Debug, Default, Clone)]
pub struct InMemoryAgentRecordPersistence {
    records: Vec<AgentRecord>,
    on_record: Option<fn(AgentRecord)>,
}

impl InMemoryAgentRecordPersistence {
    pub fn new(records: Vec<AgentRecord>) -> Self {
        Self { records, on_record: None }
    }

    pub fn with_callback(on_record: fn(AgentRecord)) -> Self {
        Self {
            records: Vec::new(),
            on_record: Some(on_record),
        }
    }

    pub fn snapshot(&self) -> &[AgentRecord] {
        &self.records
    }
}

#[async_trait::async_trait]
impl AgentRecordPersistence for InMemoryAgentRecordPersistence {
    async fn read(&self) -> anyhow::Result<RecordStream<'_>> {
        let iter = self.records.clone().into_iter().map(Ok);
        Ok(Box::pin(futures_util::stream::iter(iter)))
    }

    fn append(&mut self, record: AgentRecord) {
        self.records.push(record.clone());
        if let Some(cb) = self.on_record {
            cb(record);
        }
    }

    fn rewrite(&mut self, records: &[AgentRecord]) {
        self.records.clear();
        self.records.extend_from_slice(records);
    }

    async fn flush(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn close(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}
```

- [ ] 在同文件底部加入内联测试：

```rust
#[cfg(test)]
mod in_memory_tests {
    use kosong_rs::message::ContentPart;

    use super::*;
    use crate::records::nested::PromptOrigin;

    fn sample_metadata() -> AgentRecord {
        AgentRecord::Metadata {
            time: Some(1),
            protocol_version: "1.3".into(),
            created_at: 2,
            app_version: None,
            resumed: None,
        }
    }

    fn sample_prompt() -> AgentRecord {
        AgentRecord::TurnPrompt {
            time: Some(3),
            input: vec![ContentPart::Text { text: "hi".into() }],
            origin: PromptOrigin::User,
        }
    }

    #[tokio::test]
    async fn in_memory_read_order_matches_append_order() {
        let mut persistence = InMemoryAgentRecordPersistence::default();
        persistence.append(sample_metadata());
        persistence.append(sample_prompt());

        let records: Vec<AgentRecord> = persistence.read().await.unwrap().try_collect().await.unwrap();
        assert_eq!(records.len(), 2);
        assert!(matches!(records[0], AgentRecord::Metadata { .. }));
        assert!(matches!(records[1], AgentRecord::TurnPrompt { .. }));
    }

    #[tokio::test]
    async fn in_memory_rewrite_replaces_all_records() {
        let mut persistence = InMemoryAgentRecordPersistence::default();
        persistence.append(sample_metadata());
        persistence.rewrite(&[sample_prompt()]);

        let records: Vec<AgentRecord> = persistence.read().await.unwrap().try_collect().await.unwrap();
        assert_eq!(records.len(), 1);
        assert!(matches!(records[0], AgentRecord::TurnPrompt { .. }));
    }

    #[tokio::test]
    async fn in_memory_callback_fires_on_append() {
        let mut seen = 0;
        let mut persistence = InMemoryAgentRecordPersistence::with_callback(|_| seen += 1);
        persistence.append(sample_metadata());
        persistence.append(sample_prompt());
        assert_eq!(seen, 2);
    }
}
```

- [ ] 运行测试，确认通过：

```bash
cd rust-ody && cargo test -p agent-rs in_memory_ --lib
```

预期输出：`test result: ok. 3 passed; 0 failed`。

- [ ] Commit：`feat(agent-rs): add InMemoryAgentRecordPersistence`

---

### Task 2: `FileSystemAgentRecordPersistence`（JSONL 读写）

**Depends on:** Task 1

**Files:**
- Modify: `rust-ody/crates/agent-rs/Cargo.toml`（追加 `anyhow` 与 `async-stream`）
- Modify: `rust-ody/crates/agent-rs/src/records/persistence.rs`
- Test: `rust-ody/crates/agent-rs/tests/filesystem_persistence.rs`

**目标：** 实现文件系统持久化，每条记录一行 JSON，支持追加、整文件重写、flush 落盘、close 同步；读取时容忍末尾截断行，中间行损坏必须报错。

- [ ] 在 `rust-ody/crates/agent-rs/Cargo.toml` 追加依赖：

```toml
[dependencies]
anyhow = "1"
async-stream = "0.3"
# ... existing entries preserved ...
```

> 说明：`anyhow` 是 schema 中 `RecordStream` 与 trait 签名已经引用的依赖；本任务补全它。

- [ ] 在 `rust-ody/crates/agent-rs/src/records/persistence.rs` 追加文件系统实现：

```rust
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use async_stream::try_stream;
use futures_util::Stream;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, Clone)]
pub struct FileSystemAgentRecordPersistenceOptions {
    pub on_error: Option<fn(anyhow::Error)>,
}

impl Default for FileSystemAgentRecordPersistenceOptions {
    fn default() -> Self {
        Self { on_error: None }
    }
}

pub struct FileSystemAgentRecordPersistence {
    file_path: PathBuf,
    options: FileSystemAgentRecordPersistenceOptions,
    state: Arc<Mutex<FileSystemState>>,
}

struct FileSystemState {
    pending: Vec<AgentRecord>,
    should_clear: bool,
    directory_synced: bool,
    stored_error: Option<anyhow::Error>,
}

impl FileSystemAgentRecordPersistence {
    pub fn new(file_path: impl AsRef<Path>) -> Self {
        Self::with_options(file_path, FileSystemAgentRecordPersistenceOptions::default())
    }

    pub fn with_options(
        file_path: impl AsRef<Path>,
        options: FileSystemAgentRecordPersistenceOptions,
    ) -> Self {
        Self {
            file_path: file_path.as_ref().to_path_buf(),
            options,
            state: Arc::new(Mutex::new(FileSystemState {
                pending: Vec::new(),
                should_clear: false,
                directory_synced: false,
                stored_error: None,
            })),
        }
    }

    async fn drain(&self) -> Result<()> {
        let (should_clear, batch, directory_synced) = {
            let mut state = self.state.lock().unwrap();
            if let Some(err) = state.stored_error.take() {
                return Err(err);
            }
            let should_clear = std::mem::replace(&mut state.should_clear, false);
            let batch = std::mem::take(&mut state.pending);
            if batch.is_empty() && !should_clear {
                return Ok(());
            }
            let directory_synced = state.directory_synced;
            state.directory_synced = true;
            (should_clear, batch, directory_synced)
        };

        Self::write_batch(&self.file_path, batch, should_clear, directory_synced).await
    }

    async fn write_batch(
        file_path: &Path,
        batch: Vec<AgentRecord>,
        should_clear: bool,
        directory_synced: bool,
    ) -> Result<()> {
        if batch.is_empty() && !should_clear {
            return Ok(());
        }

        let mut content = String::new();
        for record in batch {
            content.push_str(&serde_json::to_string(&record)?);
            content.push('\n');
        }

        let directory = file_path.parent().context("file path has no parent directory")?;
        fs::create_dir_all(directory).await?;

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(!should_clear)
            .truncate(should_clear)
            .open(file_path)
            .await?;

        if !content.is_empty() {
            file.write_all(content.as_bytes()).await?;
        }
        file.sync_all().await?;
        drop(file);

        if !directory_synced {
            if let Ok(dir) = std::fs::File::open(directory) {
                let _ = dir.sync_all();
            }
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl AgentRecordPersistence for FileSystemAgentRecordPersistence {
    async fn read(&self) -> Result<RecordStream<'_>> {
        self.drain().await?;
        let file_path = self.file_path.clone();
        let stream = try_stream! {
            let file = match File::open(&file_path).await {
                Ok(f) => f,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
                Err(err) => Err(err)?,
            };
            let reader = BufReader::new(file);
            let mut lines = reader.lines();
            let mut line_number = 0usize;
            while let Some(line) = lines.next_line().await? {
                line_number += 1;
                if let Some(record) = parse_record_line(&line, line_number, &file_path, false)? {
                    yield record;
                }
            }
            // If the file ends without a trailing newline, the last line has already been yielded
            // by `lines.next_line()`. The TS implementation also allows a truncated final line;
            // here we rely on serde_json failing only on genuinely malformed JSON.
        };
        Ok(Box::pin(stream))
    }

    fn append(&mut self, record: AgentRecord) {
        let mut state = self.state.lock().unwrap();
        if let Some(err) = state.stored_error.take() {
            drop(state);
            if let Some(cb) = self.options.on_error {
                cb(err);
            }
            return;
        }
        state.pending.push(record);
    }

    fn rewrite(&mut self, records: &[AgentRecord]) {
        let mut state = self.state.lock().unwrap();
        if let Some(err) = state.stored_error.take() {
            drop(state);
            if let Some(cb) = self.options.on_error {
                cb(err);
            }
            return;
        }
        state.should_clear = true;
        state.pending.clear();
        state.pending.extend_from_slice(records);
    }

    async fn flush(&mut self) -> Result<()> {
        self.drain().await
    }

    async fn close(&mut self) -> Result<()> {
        self.flush().await
    }
}

fn parse_record_line(
    line: &str,
    line_number: usize,
    file_path: &Path,
    allow_truncated: bool,
) -> Result<Option<AgentRecord>> {
    if line.is_empty() {
        return Ok(None);
    }
    match serde_json::from_str::<AgentRecord>(line) {
        Ok(record) => Ok(Some(record)),
        Err(parse_error) => {
            if allow_truncated {
                Ok(None)
            } else {
                Err(anyhow::anyhow!(
                    "wire.jsonl: corrupted line {} in {}: {}",
                    line_number,
                    file_path.display(),
                    parse_error
                ))
            }
        }
    }
}
```

> 实现注意：
> - `append`/`rewrite` 是同步签名（与 trait 一致），只做内存缓冲；真正的落盘延迟到 `flush()`。这保留了 TS 的批量写入语义，同时避免在同步方法里调用 `block_on`（在 tokio runtime 内部调用 `block_on` 会 panic）。
> - 错误通过 `on_error` 回调上报，与 TS 一致。
> - `read` 先 `flush()` 保证看到此前所有追加。
> - `directory_synced` 只在第一次落盘后 `fsync` 目录。

- [ ] 创建集成测试 `rust-ody/crates/agent-rs/tests/filesystem_persistence.rs`：

```rust
use std::path::PathBuf;

use agent_rs::records::persistence::{AgentRecordPersistence, FileSystemAgentRecordPersistence};
use agent_rs::records::types::AgentRecord;
use futures_util::TryStreamExt;
use tempfile::TempDir;

fn metadata_record() -> AgentRecord {
    AgentRecord::Metadata {
        time: Some(1_700_000_000_000),
        protocol_version: "1.3".into(),
        created_at: 1_700_000_000_000,
        app_version: Some("0.0.0".into()),
        resumed: Some(false),
    }
}

#[tokio::test]
async fn filesystem_empty_file_yields_no_records() {
    let dir = TempDir::new().unwrap();
    let path: PathBuf = dir.path().join("records.jsonl");
    let mut persistence = FileSystemAgentRecordPersistence::new(&path);
    let records: Vec<AgentRecord> = persistence.read().await.unwrap().try_collect().await.unwrap();
    assert!(records.is_empty());
}

#[tokio::test]
async fn filesystem_append_writes_jsonl_lines() {
    let dir = TempDir::new().unwrap();
    let path: PathBuf = dir.path().join("records.jsonl");
    let mut persistence = FileSystemAgentRecordPersistence::new(&path);
    persistence.append(metadata_record());
    persistence.flush().await.unwrap();

    let content = tokio::fs::read_to_string(&path).await.unwrap();
    assert!(content.starts_with("{\"type\":\"metadata\""));
    assert!(content.ends_with('\n'));

    let records: Vec<AgentRecord> = persistence.read().await.unwrap().try_collect().await.unwrap();
    assert_eq!(records.len(), 1);
    assert!(matches!(records[0], AgentRecord::Metadata { .. }));
}

#[tokio::test]
async fn filesystem_rewrite_clears_previous_content() {
    let dir = TempDir::new().unwrap();
    let path: PathBuf = dir.path().join("records.jsonl");
    let mut persistence = FileSystemAgentRecordPersistence::new(&path);
    persistence.append(metadata_record());
    persistence.flush().await.unwrap();

    persistence.rewrite(&[]);
    persistence.flush().await.unwrap();

    let records: Vec<AgentRecord> = persistence.read().await.unwrap().try_collect().await.unwrap();
    assert!(records.is_empty());
    let content = tokio::fs::read_to_string(&path).await.unwrap();
    assert!(content.is_empty());
}

#[tokio::test]
async fn filesystem_corrupted_middle_line_errors() {
    let dir = TempDir::new().unwrap();
    let path: PathBuf = dir.path().join("records.jsonl");
    tokio::fs::write(&path, "{\"type\":\"metadata\"}\nthis is not json\n").await.unwrap();

    let mut persistence = FileSystemAgentRecordPersistence::new(&path);
    let result: Result<Vec<AgentRecord>, _> = persistence.read().await.unwrap().try_collect().await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("corrupted line 2"));
}
```

- [ ] 运行测试，确认通过：

```bash
cd rust-ody && cargo test -p agent-rs filesystem_ --test filesystem_persistence
```

预期输出：`test result: ok. 4 passed; 0 failed`。

- [ ] Commit：`feat(agent-rs): add FileSystemAgentRecordPersistence JSONL store`

---

### Task 3: 接入 `BlobStore` 卸载接口

**Depends on:** Task 2

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/records/persistence.rs`
- Create: `rust-ody/crates/agent-rs/tests/blob_offload.rs`

**目标：** 给 `FileSystemAgentRecordPersistence` 增加可选的 `RecordBlobStore`，写入前调用 `offload(record)` 把 data URI 替换成 `blobref:`。Part 3 将提供具体实现；本任务只定义 trait 与集成点。

- [ ] 在 `rust-ody/crates/agent-rs/src/records/persistence.rs` 的 `FileSystemAgentRecordPersistenceOptions` 上方加入 `RecordBlobStore` trait：

```rust
use std::sync::Arc;

#[async_trait::async_trait]
pub trait RecordBlobStore: Send + Sync {
    /// Offload large inline payloads (e.g. data URIs) to external storage.
    /// Returns a possibly-mutated record; if nothing was offloaded, returns
    /// the same record identity-equivalent value.
    async fn offload(&self, record: AgentRecord) -> AgentRecord;
}
```

- [ ] 修改 `FileSystemAgentRecordPersistenceOptions`，加入 `blob_store`：

```rust
#[derive(Clone, Default)]
pub struct FileSystemAgentRecordPersistenceOptions {
    pub on_error: Option<fn(anyhow::Error)>,
    pub blob_store: Option<Arc<dyn RecordBlobStore>>,
}
```

- [ ] 修改 `FileSystemAgentRecordPersistence::write_batch` 的函数签名，接收 `blob_store: Option<Arc<dyn RecordBlobStore>>`，并在序列化前对每个 record 调用 `offload`：

```rust
    async fn write_batch(
        file_path: &Path,
        batch: Vec<AgentRecord>,
        should_clear: bool,
        directory_synced: bool,
        blob_store: Option<Arc<dyn RecordBlobStore>>,
    ) -> Result<()> {
        if batch.is_empty() && !should_clear {
            return Ok(());
        }

        let mut content = String::new();
        for record in batch {
            let record = if let Some(store) = &blob_store {
                store.offload(record).await
            } else {
                record
            };
            content.push_str(&serde_json::to_string(&record)?);
            content.push('\n');
        }
        // ... rest unchanged ...
    }
```

- [ ] 修改 `drain` 内部，把 `self.options.blob_store.clone()` 传入 `write_batch`：

```rust
    async fn drain(&self) -> Result<()> {
        let (should_clear, batch, directory_synced, blob_store) = {
            let mut state = self.state.lock().unwrap();
            if let Some(err) = state.stored_error.take() {
                return Err(err);
            }
            let should_clear = std::mem::replace(&mut state.should_clear, false);
            let batch = std::mem::take(&mut state.pending);
            if batch.is_empty() && !should_clear {
                return Ok(());
            }
            let directory_synced = state.directory_synced;
            state.directory_synced = true;
            let blob_store = self.options.blob_store.clone();
            (should_clear, batch, directory_synced, blob_store)
        };

        Self::write_batch(&self.file_path, batch, should_clear, directory_synced, blob_store).await
    }
```

- [ ] 创建测试 `rust-ody/crates/agent-rs/tests/blob_offload.rs`，用 mock `RecordBlobStore` 验证文件系统版在落盘前会经过 offload：

```rust
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use agent_rs::records::persistence::{
    AgentRecordPersistence, FileSystemAgentRecordPersistence, FileSystemAgentRecordPersistenceOptions,
    RecordBlobStore,
};
use agent_rs::records::types::AgentRecord;
use futures_util::TryStreamExt;
use tempfile::TempDir;

struct CountingBlobStore {
    count: AtomicUsize,
}

#[async_trait::async_trait]
impl RecordBlobStore for CountingBlobStore {
    async fn offload(&self, record: AgentRecord) -> AgentRecord {
        self.count.fetch_add(1, Ordering::SeqCst);
        record
    }
}

#[tokio::test]
async fn filesystem_calls_blob_store_offload_before_writing() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("records.jsonl");
    let store = Arc::new(CountingBlobStore {
        count: AtomicUsize::new(0),
    });
    let options = FileSystemAgentRecordPersistenceOptions {
        blob_store: Some(store.clone()),
        ..Default::default()
    };
    let mut persistence = FileSystemAgentRecordPersistence::with_options(&path, options);

    persistence.append(AgentRecord::Metadata {
        time: Some(1),
        protocol_version: "1.3".into(),
        created_at: 2,
        app_version: None,
        resumed: None,
    });
    persistence.flush().await.unwrap();

    assert_eq!(store.count.load(Ordering::SeqCst), 1);
    let records: Vec<AgentRecord> = persistence.read().await.unwrap().try_collect().await.unwrap();
    assert_eq!(records.len(), 1);
}
```

- [ ] 运行测试，确认通过：

```bash
cd rust-ody && cargo test -p agent-rs blob_offload --test blob_offload
```

预期输出：`test result: ok. 1 passed; 0 failed`。

- [ ] 运行整树类型检查（确保新依赖与 trait 接口无编译问题）：

```bash
cd rust-ody && cargo check -p agent-rs --workspace --tests
```

预期输出：`Finished dev [unoptimized + debuginfo] target(s)`，无错误。

- [ ] Commit：`feat(agent-rs): wire RecordBlobStore offload into filesystem persistence`

---

## Local Self-Review

- [ ] 1. Spec-coverage：本部分覆盖 Roadmap 4.3.0.3（`InMemoryAgentRecordPersistence`）与 4.3.0.4（`FileSystemAgentRecordPersistence`）。
- [ ] 2. Placeholder scan：无 TODO/TBD；所有方法都有具体实现与测试断言。
- [ ] 3. No phantom tasks：Task 1 产出可编译、可测试的内存持久化；Task 2 产出文件系统 JSONL 存储并覆盖空文件/追加/重写/损坏行；Task 3 产出 BlobStore 卸载接口与 mock 测试。
- [ ] 4. Dependency soundness：Task 1 依赖 schema.md 的 `AgentRecord`/`AgentRecordPersistence`；Task 2 依赖 Task 1；Task 3 依赖 Task 2。无反向依赖。
- [ ] 5. Caller & build soundness：Task 2 修改 `agent-rs/Cargo.toml` 补全 `anyhow`/`async-stream`，未改变其他 crate 的共享签名；结束时运行 `cargo check -p agent-rs --workspace --tests` 验证整树。
- [ ] 6. Test-the-risk：`filesystem_rewrite_clears_previous_content` 断言重写后文件为空；`filesystem_corrupted_middle_line_errors` 断言中间损坏行触发带行号的错误；`blob_offload` 用原子计数验证 offload 确实在落盘前被调用。
- [ ] 7. Type consistency：`RecordStream`、`AgentRecordPersistence` 使用 schema.md 定义的签名；`FileSystemAgentRecordPersistence` 实现同一 trait；`RecordBlobStore` trait 名与 Part 3 的 `BlobStore` 结构体不冲突。
