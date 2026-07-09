use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use indexmap::IndexMap;
use kosong_rs::message::{ContentPart, Message};
use tokio::sync::Mutex;

use super::nested::{
    ContextMessage, ExecutableToolOutput, ExecutableToolResult, LoopRecordedEvent,
};
use super::persistence::RecordBlobStore;
use super::types::AgentRecord;

const BLOBREF_PROTOCOL: &str = "blobref:";

/// Configuration for [`BlobStore`].
#[derive(Debug, Clone)]
pub struct BlobStoreOptions {
    /// Directory where blob files are written.
    pub blob_dir: PathBuf,
    /// Size threshold in bytes. Base64 payloads shorter than this are left inline.
    pub threshold_bytes: usize,
    /// Maximum decoded payload bytes to keep in memory.
    pub max_cache_size: usize,
}

impl Default for BlobStoreOptions {
    fn default() -> Self {
        Self {
            blob_dir: PathBuf::from(".ody/blobs"),
            threshold_bytes: 4096,
            max_cache_size: 50 * 1024 * 1024,
        }
    }
}

/// On-disk content-addressed blob store with LRU rehydration cache.
///
/// Mirrors the TypeScript implementation in `packages/agent-core/src/agent/records/blobref.ts`:
/// large `data:` URIs embedded in media content parts are replaced by
/// `blobref:<mime>;<sha256>` references, and restored on rehydration.
pub struct BlobStore {
    options: BlobStoreOptions,
    /// Mutex-protected LRU cache keyed by blob hash.
    cache: Mutex<IndexMap<String, Vec<u8>>>,
    /// Current decoded payload size held in `cache`.
    cache_size: Mutex<usize>,
}

impl BlobStore {
    /// Create a new blob store.
    pub fn new(options: BlobStoreOptions) -> Arc<Self> {
        Arc::new(Self {
            options,
            cache: Mutex::new(IndexMap::new()),
            cache_size: Mutex::new(0),
        })
    }

    /// Apply offloading to all large payloads in a record.
    pub async fn offload(&self, record: AgentRecord) -> AgentRecord {
        match record {
            AgentRecord::TurnPrompt {
                time,
                input,
                origin,
            } => AgentRecord::TurnPrompt {
                time,
                input: self.offload_parts(input).await,
                origin,
            },
            AgentRecord::TurnSteer {
                time,
                input,
                origin,
            } => AgentRecord::TurnSteer {
                time,
                input: self.offload_parts(input).await,
                origin,
            },
            AgentRecord::ContextAppendMessage { time, message } => {
                AgentRecord::ContextAppendMessage {
                    time,
                    message: self.offload_context_message(message).await,
                }
            }
            AgentRecord::ContextAppendLoopEvent { time, event } => {
                AgentRecord::ContextAppendLoopEvent {
                    time,
                    event: self.offload_loop_event(event).await,
                }
            }
            other => other,
        }
    }

    /// Rehydrate `blobref:` URLs inside a record back to inline `data:` URIs.
    pub async fn rehydrate(&self, record: AgentRecord) -> Result<AgentRecord> {
        Ok(match record {
            AgentRecord::TurnPrompt {
                time,
                input,
                origin,
            } => AgentRecord::TurnPrompt {
                time,
                input: self.rehydrate_parts(input).await?,
                origin,
            },
            AgentRecord::TurnSteer {
                time,
                input,
                origin,
            } => AgentRecord::TurnSteer {
                time,
                input: self.rehydrate_parts(input).await?,
                origin,
            },
            AgentRecord::ContextAppendMessage { time, message } => {
                AgentRecord::ContextAppendMessage {
                    time,
                    message: self.rehydrate_context_message(message).await?,
                }
            }
            AgentRecord::ContextAppendLoopEvent { time, event } => {
                AgentRecord::ContextAppendLoopEvent {
                    time,
                    event: self.rehydrate_loop_event(event).await?,
                }
            }
            other => other,
        })
    }

    // -------------------------------------------------------------------------
    // Content parts
    // -------------------------------------------------------------------------

    async fn offload_parts(&self, parts: Vec<ContentPart>) -> Vec<ContentPart> {
        let mut out = Vec::with_capacity(parts.len());
        for part in parts {
            out.push(self.offload_content_part(part).await);
        }
        out
    }

    async fn offload_content_part(&self, mut part: ContentPart) -> ContentPart {
        if let Some(url) = media_url_mut(&mut part) {
            if let Some(new_url) = self.maybe_offload_string(url).await {
                *url = new_url;
            }
        }
        part
    }

    async fn rehydrate_parts(&self, parts: Vec<ContentPart>) -> Result<Vec<ContentPart>> {
        let mut out = Vec::with_capacity(parts.len());
        for part in parts {
            out.push(self.rehydrate_content_part(part).await?);
        }
        Ok(out)
    }

    async fn rehydrate_content_part(&self, mut part: ContentPart) -> Result<ContentPart> {
        if let Some(url) = media_url_mut(&mut part) {
            if let Some(new_url) = self.rehydrate_blobref_url(url).await? {
                *url = new_url;
            }
        }
        Ok(part)
    }

    // -------------------------------------------------------------------------
    // Context messages and loop events
    // -------------------------------------------------------------------------

    async fn offload_context_message(&self, ctx: ContextMessage) -> ContextMessage {
        ContextMessage {
            message: self.offload_message(ctx.message).await,
            origin: ctx.origin,
            is_error: ctx.is_error,
        }
    }

    async fn offload_message(&self, message: Message) -> Message {
        Message {
            content: self.offload_parts(message.content).await,
            ..message
        }
    }

    async fn rehydrate_context_message(&self, ctx: ContextMessage) -> Result<ContextMessage> {
        Ok(ContextMessage {
            message: self.rehydrate_message(ctx.message).await?,
            origin: ctx.origin,
            is_error: ctx.is_error,
        })
    }

    async fn rehydrate_message(&self, message: Message) -> Result<Message> {
        Ok(Message {
            content: self.rehydrate_parts(message.content).await?,
            ..message
        })
    }

    async fn offload_loop_event(&self, event: LoopRecordedEvent) -> LoopRecordedEvent {
        match event {
            LoopRecordedEvent::ToolResultEvent {
                parent_uuid,
                tool_call_id,
                result,
            } => LoopRecordedEvent::ToolResultEvent {
                parent_uuid,
                tool_call_id,
                result: self.offload_tool_result(result).await,
            },
            other => other,
        }
    }

    async fn offload_tool_result(&self, result: ExecutableToolResult) -> ExecutableToolResult {
        match result {
            ExecutableToolResult::Success(mut r) => {
                if let ExecutableToolOutput::Parts(parts) = r.output {
                    r.output = ExecutableToolOutput::Parts(self.offload_parts(parts).await);
                }
                ExecutableToolResult::Success(r)
            }
            other => other,
        }
    }

    async fn rehydrate_loop_event(&self, event: LoopRecordedEvent) -> Result<LoopRecordedEvent> {
        match event {
            LoopRecordedEvent::ToolResultEvent {
                parent_uuid,
                tool_call_id,
                result,
            } => Ok(LoopRecordedEvent::ToolResultEvent {
                parent_uuid,
                tool_call_id,
                result: self.rehydrate_tool_result(result).await?,
            }),
            other => Ok(other),
        }
    }

    async fn rehydrate_tool_result(
        &self,
        result: ExecutableToolResult,
    ) -> Result<ExecutableToolResult> {
        match result {
            ExecutableToolResult::Success(mut r) => {
                if let ExecutableToolOutput::Parts(parts) = r.output {
                    r.output = ExecutableToolOutput::Parts(self.rehydrate_parts(parts).await?);
                }
                Ok(ExecutableToolResult::Success(r))
            }
            other => Ok(other),
        }
    }

    // -------------------------------------------------------------------------
    // String-level offload / rehydrate
    // -------------------------------------------------------------------------

    async fn maybe_offload_string(&self, url: &str) -> Option<String> {
        if is_blob_ref(url) {
            return None;
        }
        let (mime, payload) = parse_data_uri(url)?;
        if payload.len() < self.options.threshold_bytes {
            return None;
        }
        let hash = sha256_hex(payload.as_bytes());
        let blob_path = self.options.blob_dir.join(&hash);
        if let Err(e) = self.write_blob_file(&blob_path, payload).await {
            // Best-effort: keep inline if we cannot persist.
            eprintln!("blobstore: failed to write blob {}: {}", hash, e);
            return None;
        }
        if let Ok(bytes) = BASE64.decode(payload) {
            self.set_cache(hash.clone(), bytes).await;
        }
        Some(format!("{}{};{}", BLOBREF_PROTOCOL, mime, hash))
    }

    async fn rehydrate_blobref_url(&self, url: &str) -> Result<Option<String>> {
        if !is_blob_ref(url) {
            return Ok(None);
        }
        let (mime, hash) =
            parse_blobref_url(url).ok_or_else(|| anyhow!("invalid blobref URL: {}", url))?;
        let bytes = self.read_blob(hash).await?;
        let payload = BASE64.encode(&bytes);
        Ok(Some(format!("data:{};base64,{}", mime, payload)))
    }

    // -------------------------------------------------------------------------
    // Blob IO and cache
    // -------------------------------------------------------------------------

    async fn write_blob_file(&self, path: &std::path::Path, base64_payload: &str) -> Result<()> {
        tokio::fs::create_dir_all(&self.options.blob_dir)
            .await
            .with_context(|| format!("creating blob dir {:?}", self.options.blob_dir))?;

        let bytes = BASE64
            .decode(base64_payload)
            .with_context(|| "decoding base64 payload for blob")?;

        // `create_new` provides deduplication: identical hashes map to identical paths.
        match tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .await
        {
            Ok(mut file) => {
                use tokio::io::AsyncWriteExt;
                file.write_all(&bytes).await?;
                file.flush().await?;
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    async fn read_blob(&self, hash: &str) -> Result<Vec<u8>> {
        {
            let mut cache = self.cache.lock().await;
            if let Some(entry) = cache.shift_remove(hash) {
                cache.insert(hash.to_string(), entry.clone());
                return Ok(entry);
            }
        }

        let path = self.options.blob_dir.join(hash);
        let bytes = tokio::fs::read(&path)
            .await
            .with_context(|| format!("reading blob {}", hash))?;
        self.set_cache(hash.to_string(), bytes.clone()).await;
        Ok(bytes)
    }

    async fn set_cache(&self, hash: String, payload: Vec<u8>) {
        let size = payload.len();
        let mut cache = self.cache.lock().await;
        let mut cache_size = self.cache_size.lock().await;

        if let Some(old) = cache.shift_remove(&hash) {
            *cache_size = cache_size.saturating_sub(old.len());
        } else {
            if size > self.options.max_cache_size {
                return;
            }
            while *cache_size + size > self.options.max_cache_size && !cache.is_empty() {
                if let Some((_, old)) = cache.shift_remove_index(0) {
                    *cache_size = cache_size.saturating_sub(old.len());
                }
            }
        }

        *cache_size += size;
        cache.insert(hash, payload);
    }
}

#[async_trait]
impl RecordBlobStore for BlobStore {
    async fn offload(&self, record: AgentRecord) -> AgentRecord {
        self.offload(record).await
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn is_blob_ref(url: &str) -> bool {
    url.starts_with(BLOBREF_PROTOCOL)
}

/// Parse a data URI into (mime_type, base64_payload).
fn parse_data_uri(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("data:")?;
    let (mime, payload) = rest.split_once(";base64,")?;
    Some((mime, payload))
}

/// Parse a `blobref:<mime>;<hash>` URL into (mime_type, hash).
fn parse_blobref_url(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix(BLOBREF_PROTOCOL)?;
    let (mime, hash) = rest.split_once(';')?;
    if hash.is_empty() {
        return None;
    }
    Some((mime, hash))
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
async fn count_dir_entries(path: &std::path::Path) -> usize {
    let mut dir = tokio::fs::read_dir(path).await.unwrap();
    let mut count = 0;
    while let Some(entry) = dir.next_entry().await.unwrap() {
        let _ = entry;
        count += 1;
    }
    count
}

#[cfg(test)]
fn media_url(part: &ContentPart) -> Option<&String> {
    match part {
        ContentPart::ImageUrl { image_url } => Some(&image_url.url),
        ContentPart::AudioUrl { audio_url } => Some(&audio_url.url),
        ContentPart::VideoUrl { video_url } => Some(&video_url.url),
        _ => None,
    }
}

fn media_url_mut(part: &mut ContentPart) -> Option<&mut String> {
    match part {
        ContentPart::ImageUrl { image_url } => Some(&mut image_url.url),
        ContentPart::AudioUrl { audio_url } => Some(&mut audio_url.url),
        ContentPart::VideoUrl { video_url } => Some(&mut video_url.url),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::nested::{ExecutableToolOutput, ExecutableToolSuccessResult, PromptOrigin};
    use super::*;
    use kosong_rs::message::{ContentPart, Message, Role, UrlPayload};

    fn small_store(dir: &tempfile::TempDir) -> Arc<BlobStore> {
        BlobStore::new(BlobStoreOptions {
            blob_dir: dir.path().to_path_buf(),
            threshold_bytes: 4096,
            max_cache_size: 10 * 1024 * 1024,
        })
    }

    fn tiny_data_uri() -> String {
        "data:image/png;base64,SGVsbG8=".to_string()
    }

    fn large_data_uri() -> String {
        let zeros = vec![0u8; 5 * 1024];
        format!("data:image/png;base64,{}", BASE64.encode(&zeros))
    }

    fn image_part(url: String) -> ContentPart {
        ContentPart::ImageUrl {
            image_url: UrlPayload { url, id: None },
        }
    }

    fn turn_prompt_record(input: Vec<ContentPart>) -> AgentRecord {
        AgentRecord::TurnPrompt {
            time: Some(1),
            input,
            origin: PromptOrigin::User,
        }
    }

    #[tokio::test]
    async fn leaves_small_payload_inline() {
        let dir = tempfile::tempdir().unwrap();
        let store = small_store(&dir);
        let record = turn_prompt_record(vec![image_part(tiny_data_uri())]);
        let out = store.offload(record).await;

        let AgentRecord::TurnPrompt { input, .. } = out else {
            panic!("unexpected variant")
        };
        assert_eq!(media_url(&input[0]).unwrap(), &tiny_data_uri());
        let blob_count = count_dir_entries(dir.path()).await;
        assert_eq!(blob_count, 0);
    }

    #[tokio::test]
    async fn offloads_large_image_url() {
        let dir = tempfile::tempdir().unwrap();
        let store = small_store(&dir);
        let url = large_data_uri();
        let record = turn_prompt_record(vec![image_part(url.clone())]);
        let out = store.offload(record).await;

        let AgentRecord::TurnPrompt { ref input, .. } = out else {
            panic!("unexpected variant")
        };
        let new_url = media_url(&input[0]).unwrap();
        assert!(is_blob_ref(new_url));
        assert!(new_url.starts_with("blobref:image/png;"));

        let blob_count = count_dir_entries(dir.path()).await;
        assert_eq!(blob_count, 1);

        // Rehydrate restores the data URI.
        let hydrated = store.rehydrate(out).await.unwrap();
        let AgentRecord::TurnPrompt {
            input: hydrated_input,
            ..
        } = hydrated
        else {
            panic!("unexpected variant")
        };
        let restored = media_url(&hydrated_input[0]).unwrap();
        assert!(restored.starts_with("data:image/png;base64,"));
        let (_, payload) = parse_data_uri(restored).unwrap();
        assert_eq!(payload.len(), BASE64.encode(&vec![0u8; 5 * 1024]).len());
    }

    #[tokio::test]
    async fn rehydrate_keeps_non_blob_url_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let store = small_store(&dir);
        let url = "https://example.com/image.png".to_string();
        let record = turn_prompt_record(vec![image_part(url.clone())]);
        let hydrated = store.rehydrate(record).await.unwrap();
        let AgentRecord::TurnPrompt { input, .. } = hydrated else {
            panic!("unexpected variant")
        };
        assert_eq!(media_url(&input[0]).unwrap(), &url);
    }

    #[tokio::test]
    async fn offload_is_idempotent_for_blobref() {
        let dir = tempfile::tempdir().unwrap();
        let store = small_store(&dir);
        let url = large_data_uri();
        let record = turn_prompt_record(vec![image_part(url)]);
        let off1 = store.offload(record).await;
        let AgentRecord::TurnPrompt { input, .. } = off1.clone() else {
            panic!()
        };
        let record2 = turn_prompt_record(input);
        let off2 = store.offload(record2).await;
        assert_eq!(off1, off2);
    }

    #[tokio::test]
    async fn caches_rehydrated_blobs() {
        let dir = tempfile::tempdir().unwrap();
        let store = small_store(&dir);
        let url = large_data_uri();
        let record = turn_prompt_record(vec![image_part(url)]);
        let off1 = store.offload(record).await;
        let hydrated1 = store.rehydrate(off1.clone()).await.unwrap();
        // Second rehydration should hit cache; result identical.
        let hydrated2 = store.rehydrate(off1).await.unwrap();
        assert_eq!(hydrated1, hydrated2);
    }

    #[tokio::test]
    async fn offloads_tool_result_parts() {
        let dir = tempfile::tempdir().unwrap();
        let store = small_store(&dir);
        let url = large_data_uri();
        let record = AgentRecord::ContextAppendLoopEvent {
            time: Some(2),
            event: LoopRecordedEvent::ToolResultEvent {
                parent_uuid: "p1".into(),
                tool_call_id: "tc1".into(),
                result: ExecutableToolResult::Success(ExecutableToolSuccessResult {
                    output: ExecutableToolOutput::Parts(vec![image_part(url)]),
                    is_error: None,
                    stop_turn: None,
                    message: None,
                }),
            },
        };
        let out = store.offload(record).await;
        let AgentRecord::ContextAppendLoopEvent { event, .. } = out else {
            panic!()
        };
        let LoopRecordedEvent::ToolResultEvent { result, .. } = event else {
            panic!()
        };
        let ExecutableToolResult::Success(r) = result else {
            panic!()
        };
        let ExecutableToolOutput::Parts(parts) = r.output else {
            panic!()
        };
        assert!(is_blob_ref(media_url(&parts[0]).unwrap()));
    }

    #[tokio::test]
    async fn context_append_message_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let store = small_store(&dir);
        let url = large_data_uri();
        let record = AgentRecord::ContextAppendMessage {
            time: Some(3),
            message: ContextMessage {
                message: Message {
                    role: Role::User,
                    name: None,
                    content: vec![image_part(url)],
                    tool_calls: vec![],
                    tool_call_id: None,
                    partial: None,
                },
                origin: Some(PromptOrigin::User),
                is_error: None,
            },
        };
        let off = store.offload(record).await;
        let hydrated = store.rehydrate(off).await.unwrap();
        let AgentRecord::ContextAppendMessage { message, .. } = hydrated else {
            panic!()
        };
        let restored = media_url(&message.message.content[0]).unwrap();
        assert!(restored.starts_with("data:image/png;base64,"));
    }
}
