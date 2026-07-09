use std::cell::RefCell;

use agent_rs::records::AgentRecord;
use agent_rs::usage::{UsageRecordScope, UsageRecorder, UsageRecorderContext};
use kosong_rs::usage::TokenUsage;

#[derive(Debug, Default)]
struct MockContext {
    records: RefCell<Vec<AgentRecord>>,
    status_emits: RefCell<usize>,
}

impl UsageRecorderContext for MockContext {
    fn log_record(&mut self, record: AgentRecord) {
        self.records.borrow_mut().push(record);
    }

    fn emit_status_updated(&mut self) {
        *self.status_emits.borrow_mut() += 1;
    }
}

#[test]
fn empty_recorder_returns_no_status() {
    let recorder = UsageRecorder::new(MockContext::default());
    assert_eq!(recorder.status(), None);
    let data = recorder.data();
    assert!(data.by_model.is_none());
    assert!(data.total.is_none());
    assert!(data.current_turn.is_none());
}

#[test]
fn session_record_accumulates_by_model_and_logs() {
    let mut recorder = UsageRecorder::new(MockContext::default());
    let usage = TokenUsage {
        input_other: 10,
        output: 5,
        input_cache_read: 2,
        input_cache_creation: 1,
    };
    recorder.record("kimi-k2", usage, UsageRecordScope::Session);

    let data = recorder.data();
    let by_model = data.by_model.unwrap();
    let model_usage = by_model.get("kimi-k2").unwrap();
    assert_eq!(model_usage.input_other, 10);
    assert_eq!(model_usage.output, 5);
    assert_eq!(data.total.unwrap().grand_total(), 18);

    let ctx = recorder.into_inner();
    assert_eq!(ctx.records.borrow().len(), 1);
    assert_eq!(*ctx.status_emits.borrow(), 1);
}

#[test]
fn turn_record_accumulates_current_turn() {
    let mut recorder = UsageRecorder::new(MockContext::default());
    recorder.record(
        "kimi-k2",
        TokenUsage {
            input_other: 4,
            output: 2,
            input_cache_read: 0,
            input_cache_creation: 0,
        },
        UsageRecordScope::Turn,
    );
    recorder.record(
        "kimi-k2",
        TokenUsage {
            input_other: 1,
            output: 1,
            input_cache_read: 0,
            input_cache_creation: 0,
        },
        UsageRecordScope::Turn,
    );

    let data = recorder.data();
    assert_eq!(data.current_turn.unwrap().input_other, 5);
    assert_eq!(data.current_turn.unwrap().output, 3);
    let by_model = data.by_model.unwrap();
    assert_eq!(by_model.get("kimi-k2").unwrap().input_other, 5);
}

#[test]
fn begin_turn_resets_current_turn_but_keeps_session_total() {
    let mut recorder = UsageRecorder::new(MockContext::default());
    recorder.record(
        "kimi-k2",
        TokenUsage {
            input_other: 1,
            output: 1,
            input_cache_read: 0,
            input_cache_creation: 0,
        },
        UsageRecordScope::Turn,
    );
    assert!(recorder.data().current_turn.is_some());
    recorder.begin_turn();
    assert!(recorder.data().current_turn.is_none());
    assert!(recorder.data().by_model.is_some());
}

#[test]
fn multiple_models_tracked_separately() {
    let mut recorder = UsageRecorder::new(MockContext::default());
    recorder.record(
        "kimi-k2",
        TokenUsage {
            input_other: 1,
            output: 0,
            input_cache_read: 0,
            input_cache_creation: 0,
        },
        UsageRecordScope::Session,
    );
    recorder.record(
        "gpt-4o",
        TokenUsage {
            input_other: 0,
            output: 2,
            input_cache_read: 0,
            input_cache_creation: 0,
        },
        UsageRecordScope::Session,
    );
    let data = recorder.data();
    assert_eq!(data.by_model.as_ref().unwrap().len(), 2);
    assert_eq!(data.total.unwrap().output, 2);
}
