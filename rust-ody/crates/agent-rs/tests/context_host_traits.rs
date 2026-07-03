use std::sync::Mutex;

use agent_rs::context::*;
use agent_rs::records::nested::{ContextMessage, PromptOrigin};
use agent_rs::records::AgentRecord;

struct SpyRecordLog(Mutex<Vec<String>>);
impl RecordLog for SpyRecordLog {
    fn log_record(&self, record: AgentRecord) {
        self.0
            .lock()
            .unwrap()
            .push(record.record_type().to_string());
    }
    fn restoring_time(&self) -> Option<i64> {
        None
    }
}

struct SpyMicroCompaction(Mutex<Vec<usize>>);
impl MicroCompaction for SpyMicroCompaction {
    fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage> {
        messages.to_vec()
    }
    fn reset(&self, max_cutoff: usize) {
        self.0.lock().unwrap().push(max_cutoff);
    }
}

struct SpyInjection(Mutex<Vec<String>>);
impl InjectionLifecycle for SpyInjection {
    fn on_context_clear(&self) {
        self.0.lock().unwrap().push("clear".into());
    }
    fn on_context_compacted(&self, compacted_count: usize) {
        self.0
            .lock()
            .unwrap()
            .push(format!("compacted:{}", compacted_count));
    }
    fn on_context_message_removed(&self, index: usize) {
        self.0.lock().unwrap().push(format!("removed:{}", index));
    }
}

struct NoopBackground;
impl BackgroundNotifications for NoopBackground {
    fn mark_delivered_notification(&self, _origin: &PromptOrigin) {}
}

struct SpyReplay(Mutex<Vec<String>>);
impl ReplayBuilder for SpyReplay {
    fn push_message(&self, _message: &ContextMessage) {
        self.0.lock().unwrap().push("push".into());
    }
    fn remove_last_messages(&self, _messages: &[ContextMessage]) {
        self.0.lock().unwrap().push("remove".into());
    }
}

struct NoopStatus;
impl StatusEmitter for NoopStatus {
    fn emit_status_updated(&self) {}
}

struct NoopFlusher;
impl ContextSwitchFlusher for NoopFlusher {
    fn flush_deferred_context_switch(&self) {}
}

struct FixedClock(i64);
impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        self.0
    }
}

struct TestAgent {
    record_log: SpyRecordLog,
    micro_compaction: SpyMicroCompaction,
    injection: SpyInjection,
    background: NoopBackground,
    replay_builder: SpyReplay,
    status: NoopStatus,
    flusher: NoopFlusher,
    clock: FixedClock,
}

impl ContextAgent for TestAgent {
    fn record_log(&self) -> &dyn RecordLog {
        &self.record_log
    }
    fn micro_compaction(&self) -> &dyn MicroCompaction {
        &self.micro_compaction
    }
    fn injection(&self) -> &dyn InjectionLifecycle {
        &self.injection
    }
    fn background(&self) -> &dyn BackgroundNotifications {
        &self.background
    }
    fn replay_builder(&self) -> &dyn ReplayBuilder {
        &self.replay_builder
    }
    fn status_emitter(&self) -> &dyn StatusEmitter {
        &self.status
    }
    fn context_switch_flusher(&self) -> &dyn ContextSwitchFlusher {
        &self.flusher
    }
    fn clock(&self) -> &dyn Clock {
        &self.clock
    }
}

#[test]
fn context_agent_trait_routes_to_all_subsystems() {
    let agent = TestAgent {
        record_log: SpyRecordLog(Mutex::new(vec![])),
        micro_compaction: SpyMicroCompaction(Mutex::new(vec![])),
        injection: SpyInjection(Mutex::new(vec![])),
        background: NoopBackground,
        replay_builder: SpyReplay(Mutex::new(vec![])),
        status: NoopStatus,
        flusher: NoopFlusher,
        clock: FixedClock(12345),
    };

    agent
        .record_log()
        .log_record(AgentRecord::ContextClear { time: None });
    agent.injection().on_context_clear();
    agent.micro_compaction().reset(7);

    assert_eq!(
        agent.record_log.0.lock().unwrap().as_slice(),
        &["context.clear"]
    );
    assert_eq!(agent.injection.0.lock().unwrap().as_slice(), &["clear"]);
    assert_eq!(agent.micro_compaction.0.lock().unwrap().as_slice(), &[7]);
    assert_eq!(agent.clock().now_ms(), 12345);
}
