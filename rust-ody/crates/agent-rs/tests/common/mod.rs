use std::sync::Mutex;

use agent_rs::context::*;
use agent_rs::records::nested::{ContextMessage, PromptOrigin};
use agent_rs::records::AgentRecord;

#[derive(Default)]
pub struct SpyRecordLog {
    pub records: Mutex<Vec<AgentRecord>>,
    pub restoring_time: Mutex<Option<i64>>,
}
impl SpyRecordLog {
    pub fn with_restoring_time(time: i64) -> Self {
        Self {
            records: Mutex::new(Vec::new()),
            restoring_time: Mutex::new(Some(time)),
        }
    }
}
impl RecordLog for SpyRecordLog {
    fn log_record(&self, record: AgentRecord) {
        self.records.lock().unwrap().push(record);
    }
    fn restoring_time(&self) -> Option<i64> {
        *self.restoring_time.lock().unwrap()
    }
}

pub struct SpyMicroCompaction(pub Mutex<Vec<usize>>);
impl MicroCompaction for SpyMicroCompaction {
    fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage> {
        messages.to_vec()
    }
    fn reset(&self, max_cutoff: usize) {
        self.0.lock().unwrap().push(max_cutoff);
    }
}

pub struct SpyInjection(pub Mutex<Vec<String>>);
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

pub struct NoopBackground;
impl BackgroundNotifications for NoopBackground {
    fn mark_delivered_notification(&self, _origin: &PromptOrigin) {}
}

pub struct SpyReplay(pub Mutex<Vec<Vec<ContextMessage>>>);
impl ReplayBuilder for SpyReplay {
    fn push_message(&self, message: &ContextMessage) {
        self.0.lock().unwrap().push(vec![message.clone()]);
    }
    fn remove_last_messages(&self, messages: &[ContextMessage]) {
        self.0.lock().unwrap().push(messages.to_vec());
    }
}

pub struct NoopStatus;
impl StatusEmitter for NoopStatus {
    fn emit_status_updated(&self) {}
}

pub struct NoopFlusher;
impl ContextSwitchFlusher for NoopFlusher {
    fn flush_deferred_context_switch(&self) {}
}

pub struct FixedClock(pub i64);
impl Clock for FixedClock {
    fn now_ms(&self) -> i64 {
        self.0
    }
}

pub struct TestAgent {
    pub record_log: SpyRecordLog,
    pub micro_compaction: SpyMicroCompaction,
    pub injection: SpyInjection,
    pub background: NoopBackground,
    pub replay_builder: SpyReplay,
    pub status: NoopStatus,
    pub flusher: NoopFlusher,
    pub clock: FixedClock,
}

impl TestAgent {
    pub fn new() -> Self {
        Self::with_restoring_time(None)
    }

    pub fn restoring(time: i64) -> Self {
        Self::with_restoring_time(Some(time))
    }

    fn with_restoring_time(time: Option<i64>) -> Self {
        Self {
            record_log: SpyRecordLog {
                records: Mutex::new(Vec::new()),
                restoring_time: Mutex::new(time),
            },
            micro_compaction: SpyMicroCompaction(Mutex::new(Vec::new())),
            injection: SpyInjection(Mutex::new(Vec::new())),
            background: NoopBackground,
            replay_builder: SpyReplay(Mutex::new(Vec::new())),
            status: NoopStatus,
            flusher: NoopFlusher,
            clock: FixedClock(12345),
        }
    }
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
