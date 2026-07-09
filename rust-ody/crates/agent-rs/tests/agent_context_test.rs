use std::sync::{Arc, Mutex};

use agent_rs::context::memory::ContextMemory;
use agent_rs::context::types::{
    BackgroundNotifications, Clock, ContextAgent, ContextMessage, ContextSwitchFlusher,
    InjectionLifecycle, MicroCompaction, PromptOrigin, RecordLog, ReplayBuilder, StatusEmitter,
};
use agent_rs::records::AgentRecord;
use kosong_rs::message::ContentPart;

pub const USER_PROMPT_ORIGIN: PromptOrigin = PromptOrigin::User;

struct DummyRecordLog(Mutex<Vec<AgentRecord>>);
impl RecordLog for DummyRecordLog {
    fn log_record(&self, r: AgentRecord) {
        self.0.lock().unwrap().push(r);
    }
    fn restoring_time(&self) -> Option<i64> {
        None
    }
}

struct DummyMicroCompaction;
impl MicroCompaction for DummyMicroCompaction {
    fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage> {
        messages.to_vec()
    }
    fn reset(&self, _max_cutoff: usize) {}
}

struct DummyInjection;
impl InjectionLifecycle for DummyInjection {
    fn on_context_clear(&self) {}
    fn on_context_compacted(&self, _compacted_count: usize) {}
    fn on_context_message_removed(&self, _index: usize) {}
}

struct DummyBackground;
impl BackgroundNotifications for DummyBackground {
    fn mark_delivered_notification(&self, _origin: &PromptOrigin) {}
}

struct DummyReplay;
impl ReplayBuilder for DummyReplay {
    fn push_message(&self, _message: &ContextMessage) {}
    fn remove_last_messages(&self, _messages: &[ContextMessage]) {}
}

struct DummyStatus;
impl StatusEmitter for DummyStatus {
    fn emit_status_updated(&self) {}
}

struct DummyFlusher;
impl ContextSwitchFlusher for DummyFlusher {
    fn flush_deferred_context_switch(&self) {}
}

struct DummyClock;
impl Clock for DummyClock {
    fn now_ms(&self) -> i64 {
        0
    }
}

struct DummyAgent {
    record_log: DummyRecordLog,
    micro_compaction: DummyMicroCompaction,
    injection: DummyInjection,
    background: DummyBackground,
    replay_builder: DummyReplay,
    status: DummyStatus,
    flusher: DummyFlusher,
    clock: DummyClock,
}

impl ContextAgent for DummyAgent {
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
fn context_memory_can_be_owned_by_agent_struct() {
    let agent: Arc<dyn ContextAgent> = Arc::new(DummyAgent {
        record_log: DummyRecordLog(Mutex::new(Vec::new())),
        micro_compaction: DummyMicroCompaction,
        injection: DummyInjection,
        background: DummyBackground,
        replay_builder: DummyReplay,
        status: DummyStatus,
        flusher: DummyFlusher,
        clock: DummyClock,
    });
    let mut memory = ContextMemory::new(agent.clone());
    memory.append_user_message(
        vec![ContentPart::Text { text: "hi".into() }],
        USER_PROMPT_ORIGIN,
    );
    assert_eq!(memory.history().len(), 1);
}
