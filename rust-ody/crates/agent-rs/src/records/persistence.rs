use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use async_stream::try_stream;
use serde_json::Value as JsonValue;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::records::types::{AgentRecord, AgentRecordPersistence, RawRecordStream, RecordStream};

#[async_trait::async_trait]
pub trait RecordBlobStore: Send + Sync {
    /// Offload large inline payloads (e.g. data URIs) to external storage.
    /// Returns a possibly-mutated record; if nothing was offloaded, returns
    /// the same record identity-equivalent value.
    async fn offload(&self, record: AgentRecord) -> AgentRecord;
}

#[derive(Clone, Default)]
pub struct FileSystemAgentRecordPersistenceOptions {
    pub on_error: Option<fn(anyhow::Error)>,
    pub blob_store: Option<Arc<dyn RecordBlobStore>>,
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
        Self::with_options(
            file_path,
            FileSystemAgentRecordPersistenceOptions::default(),
        )
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

        Self::write_batch(
            &self.file_path,
            batch,
            should_clear,
            directory_synced,
            blob_store,
        )
        .await
    }

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

        let directory = file_path
            .parent()
            .context("file path has no parent directory")?;
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

    async fn read_raw_jsonl(&self) -> Result<RawRecordStream<'static>> {
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
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<JsonValue>(&line) {
                    Ok(value) => yield value,
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
        };
        Ok(Box::pin(stream))
    }

    async fn read_raw(&self) -> Result<RawRecordStream<'_>> {
        self.read_raw_jsonl().await
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

#[derive(Default, Clone)]
pub struct InMemoryAgentRecordPersistence {
    records: Vec<AgentRecord>,
    raw_lines: Vec<JsonValue>,
    on_record: Option<Arc<dyn Fn(AgentRecord) + Send + Sync>>,
}

impl InMemoryAgentRecordPersistence {
    pub fn new(records: Vec<AgentRecord>) -> Self {
        let raw_lines = records
            .iter()
            .map(|r| serde_json::to_value(r).unwrap())
            .collect();
        Self {
            records,
            raw_lines,
            on_record: None,
        }
    }

    pub fn with_callback(on_record: impl Fn(AgentRecord) + Send + Sync + 'static) -> Self {
        Self {
            records: Vec::new(),
            raw_lines: Vec::new(),
            on_record: Some(Arc::new(on_record)),
        }
    }

    /// Append an already-serialized raw record. Useful for injecting legacy
    /// wire-format records in tests.
    pub fn append_raw(&mut self, value: JsonValue) {
        self.raw_lines.push(value);
    }

    pub fn snapshot(&self) -> &[AgentRecord] {
        &self.records
    }
}

#[async_trait::async_trait]
impl AgentRecordPersistence for InMemoryAgentRecordPersistence {
    async fn read(&self) -> Result<RecordStream<'_>> {
        let iter = self.records.clone().into_iter().map(Ok);
        Ok(Box::pin(futures_util::stream::iter(iter)))
    }

    async fn read_raw(&self) -> Result<RawRecordStream<'_>> {
        let values = self.raw_lines.clone();
        Ok(Box::pin(futures_util::stream::iter(
            values.into_iter().map(Ok),
        )))
    }

    fn append(&mut self, record: AgentRecord) {
        self.raw_lines.push(serde_json::to_value(&record).unwrap());
        self.records.push(record.clone());
        if let Some(cb) = &self.on_record {
            cb(record);
        }
    }

    fn rewrite(&mut self, records: &[AgentRecord]) {
        self.records.clear();
        self.records.extend_from_slice(records);
        self.raw_lines.clear();
        self.raw_lines
            .extend(records.iter().map(|r| serde_json::to_value(r).unwrap()));
    }

    async fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod in_memory_tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use futures_util::TryStreamExt;
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

        let records: Vec<AgentRecord> = persistence
            .read()
            .await
            .unwrap()
            .try_collect()
            .await
            .unwrap();
        assert_eq!(records.len(), 2);
        assert!(matches!(records[0], AgentRecord::Metadata { .. }));
        assert!(matches!(records[1], AgentRecord::TurnPrompt { .. }));
    }

    #[tokio::test]
    async fn in_memory_rewrite_replaces_all_records() {
        let mut persistence = InMemoryAgentRecordPersistence::default();
        persistence.append(sample_metadata());
        persistence.rewrite(&[sample_prompt()]);

        let records: Vec<AgentRecord> = persistence
            .read()
            .await
            .unwrap()
            .try_collect()
            .await
            .unwrap();
        assert_eq!(records.len(), 1);
        assert!(matches!(records[0], AgentRecord::TurnPrompt { .. }));
    }

    #[tokio::test]
    async fn in_memory_callback_fires_on_append() {
        let seen = Arc::new(AtomicUsize::new(0));
        let seen2 = Arc::clone(&seen);
        let mut persistence = InMemoryAgentRecordPersistence::with_callback(move |_| {
            seen2.fetch_add(1, Ordering::SeqCst);
        });
        persistence.append(sample_metadata());
        persistence.append(sample_prompt());
        assert_eq!(seen.load(Ordering::SeqCst), 2);
    }
}
