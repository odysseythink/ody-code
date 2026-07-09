use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use agent_rs::records::persistence::{
    FileSystemAgentRecordPersistence, FileSystemAgentRecordPersistenceOptions, RecordBlobStore,
};
use agent_rs::records::types::AgentRecord;
use agent_rs::records::types::AgentRecordPersistence;
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
    let records: Vec<AgentRecord> = persistence
        .read()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap();
    assert_eq!(records.len(), 1);
}
