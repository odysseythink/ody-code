use std::{
    env, fs,
    sync::{Arc, Mutex},
};

use agent_rs::context::{
    drop_orphan_tool_results, estimate_tokens_for_messages, project, render_notification_xml,
    Clock, ContextAgent, ContextMemory, ContextSwitchFlusher, InjectionLifecycle, MicroCompaction,
    RecordLog, ReplayBuilder, StatusEmitter,
};
use agent_rs::records::nested::ContextMessage;
use agent_rs::records::AgentRecord;
use kosong_rs::message::Message;
use serde_json::{Map, Value};

#[derive(serde::Deserialize)]
#[serde(tag = "kind")]
enum Fixture {
    #[serde(rename = "project")]
    Project { history: Vec<ContextMessage> },
    #[serde(rename = "tokens")]
    Tokens { messages: Vec<Message> },
    #[serde(rename = "notification")]
    Notification { data: Map<String, Value> },
    #[serde(rename = "memory")]
    Memory { operations: Vec<AgentRecord> },
}

fn main() {
    let path = env::args().nth(1).expect("fixture path argument required");
    let raw = fs::read_to_string(&path).expect("read fixture");
    let fixture: Fixture = serde_json::from_str(&raw).expect("parse fixture");

    let output = match fixture {
        Fixture::Project { history } => {
            serde_json::json!({ "messages": drop_orphan_tool_results(project(&history)) })
        }
        Fixture::Tokens { messages } => {
            serde_json::json!({ "tokens": estimate_tokens_for_messages(&messages) })
        }
        Fixture::Notification { data } => {
            serde_json::json!({ "xml": render_notification_xml(&data) })
        }
        Fixture::Memory { operations } => run_memory(operations),
    };

    println!("{}", serde_json::to_string(&output).unwrap());
}

fn run_memory(operations: Vec<AgentRecord>) -> Value {
    let agent = Arc::new(ParityAgent::new());
    let agent_ctx: Arc<dyn ContextAgent> = agent.clone();
    let mut memory = ContextMemory::new(agent_ctx);
    for op in operations {
        replay(&mut memory, op);
    }
    serde_json::json!({
        "history": memory.history(),
        "messages": memory.messages(),
        "token_count": memory.token_count(),
        "token_count_with_pending": memory.token_count_with_pending(),
        "records": agent
            .record_log
            .records()
            .into_iter()
            .map(clean_record_value)
            .collect::<Vec<_>>(),
    })
}

fn replay(memory: &mut ContextMemory, op: AgentRecord) {
    match op {
        AgentRecord::ContextAppendMessage { message, .. } => memory.append_message(message),
        AgentRecord::ContextAppendLoopEvent { event, .. } => memory.append_loop_event(event),
        AgentRecord::ContextClear { .. } => memory.clear(),
        AgentRecord::ContextApplyCompaction { result, .. } => memory.apply_compaction(result),
        AgentRecord::ContextUndo { count, .. } => memory.undo(count),
        _ => {}
    }
}

struct ParityRecordLog(Mutex<Vec<AgentRecord>>);
impl RecordLog for ParityRecordLog {
    fn log_record(&self, record: AgentRecord) {
        self.0.lock().unwrap().push(record);
    }
    fn restoring_time(&self) -> Option<i64> {
        None
    }
}
impl ParityRecordLog {
    fn records(&self) -> Vec<AgentRecord> {
        self.0.lock().unwrap().clone()
    }
    fn new() -> Self {
        Self(Mutex::new(Vec::new()))
    }
}

struct ParityMicroCompaction;
impl MicroCompaction for ParityMicroCompaction {
    fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage> {
        messages.to_vec()
    }
    fn reset(&self, _max_cutoff: usize) {}
}

struct ParityInjection;
impl InjectionLifecycle for ParityInjection {
    fn on_context_clear(&self) {}
    fn on_context_compacted(&self, _compacted_count: usize) {}
    fn on_context_message_removed(&self, _index: usize) {}
}

struct ParityBackground;
impl agent_rs::context::BackgroundNotifications for ParityBackground {
    fn mark_delivered_notification(&self, _origin: &agent_rs::context::PromptOrigin) {}
}

struct ParityReplay;
impl ReplayBuilder for ParityReplay {
    fn push_message(&self, _message: &ContextMessage) {}
    fn remove_last_messages(&self, _messages: &[ContextMessage]) {}
}

struct ParityStatus;
impl StatusEmitter for ParityStatus {
    fn emit_status_updated(&self) {}
}

struct ParityFlusher;
impl ContextSwitchFlusher for ParityFlusher {
    fn flush_deferred_context_switch(&self) {}
}

struct ParityClock;
impl Clock for ParityClock {
    fn now_ms(&self) -> i64 {
        12345
    }
}

struct ParityAgent {
    record_log: ParityRecordLog,
    micro_compaction: ParityMicroCompaction,
    injection: ParityInjection,
    background: ParityBackground,
    replay_builder: ParityReplay,
    status: ParityStatus,
    flusher: ParityFlusher,
    clock: ParityClock,
}
impl ParityAgent {
    fn new() -> Self {
        Self {
            record_log: ParityRecordLog::new(),
            micro_compaction: ParityMicroCompaction,
            injection: ParityInjection,
            background: ParityBackground,
            replay_builder: ParityReplay,
            status: ParityStatus,
            flusher: ParityFlusher,
            clock: ParityClock,
        }
    }
}
fn clean_record_value(record: AgentRecord) -> Value {
    let mut value = serde_json::to_value(record).expect("serialize record");
    if let Some(obj) = value.as_object_mut() {
        if obj.get("time").map(|v| v.is_null()).unwrap_or(false) {
            obj.remove("time");
        }
    }
    value
}

impl ContextAgent for ParityAgent {
    fn record_log(&self) -> &dyn RecordLog {
        &self.record_log
    }
    fn micro_compaction(&self) -> &dyn MicroCompaction {
        &self.micro_compaction
    }
    fn injection(&self) -> &dyn InjectionLifecycle {
        &self.injection
    }
    fn background(&self) -> &dyn agent_rs::context::BackgroundNotifications {
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
