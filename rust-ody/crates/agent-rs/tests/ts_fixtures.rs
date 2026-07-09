use std::path::PathBuf;

use agent_rs::records::persistence::FileSystemAgentRecordPersistence;
use agent_rs::records::types::AgentRecordPersistence;
use futures_util::TryStreamExt;

#[tokio::test]
async fn reads_ts_generated_fixture() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let fixture = manifest.join("fixtures").join("ts_records.jsonl");

    let persistence = FileSystemAgentRecordPersistence::new(&fixture);
    let records: Vec<_> = persistence
        .read()
        .await
        .unwrap()
        .try_collect()
        .await
        .unwrap();

    assert_eq!(records.len(), 8);
    assert_eq!(records[0].record_type(), "metadata");
    assert_eq!(records[1].record_type(), "turn.prompt");
    assert_eq!(records[2].record_type(), "turn.steer");
    assert_eq!(records[3].record_type(), "context.append_message");
    assert_eq!(records[4].record_type(), "context.append_loop_event");
    assert_eq!(records[5].record_type(), "permission.set_mode");
    assert_eq!(records[6].record_type(), "usage.record");
    assert_eq!(records[7].record_type(), "goal.create");

    let json = serde_json::to_value(&records[1]).unwrap();
    assert_eq!(json["input"][0]["text"], "Hello from TypeScript");
    assert_eq!(json["input"][1]["imageUrl"]["id"], "img_1");

    let msg = serde_json::to_value(&records[3]).unwrap();
    assert_eq!(msg["message"]["toolCalls"][0]["name"], "read");
    assert_eq!(msg["message"]["role"], "assistant");

    let usage = serde_json::to_value(&records[6]).unwrap();
    assert_eq!(usage["model"], "kimi-k2");
    assert_eq!(usage["usage"]["inputCacheRead"], 1);
}
