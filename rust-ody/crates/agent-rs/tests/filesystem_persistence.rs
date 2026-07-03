use std::path::PathBuf;

use agent_rs::records::persistence::FileSystemAgentRecordPersistence;
use agent_rs::records::types::AgentRecord;
use agent_rs::records::types::AgentRecordPersistence;
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
    let persistence = FileSystemAgentRecordPersistence::new(&path);
    let records: Vec<AgentRecord> = persistence
        .read()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap();
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

    let records: Vec<AgentRecord> = persistence
        .read()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap();
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

    let records: Vec<AgentRecord> = persistence
        .read()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap();
    assert!(records.is_empty());
    let content = tokio::fs::read_to_string(&path).await.unwrap();
    assert!(content.is_empty());
}

#[tokio::test]
async fn filesystem_corrupted_middle_line_errors() {
    let dir = TempDir::new().unwrap();
    let path: PathBuf = dir.path().join("records.jsonl");
    tokio::fs::write(
        &path,
        "{\"type\":\"metadata\",\"protocol_version\":\"1.3\",\"created_at\":1700000000000}\nthis is not json\n",
    )
    .await
    .unwrap();

    let persistence = FileSystemAgentRecordPersistence::new(&path);
    let result: Result<Vec<AgentRecord>, _> = persistence.read().await.unwrap().try_collect().await;
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("corrupted line 2"));
}

#[tokio::test]
async fn filesystem_read_raw_returns_untyped_json() {
    let dir = TempDir::new().unwrap();
    let path: PathBuf = dir.path().join("records.jsonl");
    let mut persistence = FileSystemAgentRecordPersistence::new(&path);
    persistence.append(metadata_record());
    persistence.flush().await.unwrap();

    let values: Vec<serde_json::Value> = persistence
        .read_raw()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap();
    assert_eq!(values.len(), 1);
    assert_eq!(values[0]["type"], "metadata");
}
