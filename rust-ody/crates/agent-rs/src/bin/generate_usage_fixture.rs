use std::{env, fs, path::PathBuf};

use agent_rs::records::AgentRecord;
use agent_rs::usage::{UsageRecordScope, UsageRecorder, UsageRecorderContext};
use kosong_rs::usage::TokenUsage;

struct NoopCtx;

impl UsageRecorderContext for NoopCtx {
    fn log_record(&mut self, _record: AgentRecord) {}
    fn emit_status_updated(&mut self) {}
}

fn main() {
    let mut recorder = UsageRecorder::new(NoopCtx);
    recorder.record(
        "kimi-k2",
        TokenUsage {
            input_other: 10,
            output: 5,
            input_cache_read: 2,
            input_cache_creation: 1,
        },
        UsageRecordScope::Session,
    );
    recorder.record(
        "kimi-k2",
        TokenUsage {
            input_other: 3,
            output: 2,
            input_cache_read: 0,
            input_cache_creation: 0,
        },
        UsageRecordScope::Turn,
    );
    let status = recorder.data();

    let out_dir = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .unwrap()
        .join("tests/fixtures");
    fs::create_dir_all(&out_dir).unwrap();
    fs::write(
        out_dir.join("usage-rust.json"),
        serde_json::to_string_pretty(&status).unwrap(),
    )
    .unwrap();
}
