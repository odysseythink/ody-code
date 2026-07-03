use kosong_rs::message::ContentPart;
use serde::{Deserialize, Serialize};

pub use crate::records::nested::{ContextMessage, PromptOrigin};
use crate::records::AgentRecord;

/// 与 TS `USER_PROMPT_ORIGIN` 对应的只读常量。
pub const USER_PROMPT_ORIGIN: PromptOrigin = PromptOrigin::User;

/// 用户消息落盘/重放的轻量记录形状。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserMessageRecord {
    pub content: Vec<ContentPart>,
    pub origin: PromptOrigin,
}

/// system-reminder 落盘/重放的轻量记录形状。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemReminderRecord {
    pub content: String,
    pub origin: PromptOrigin,
}

/// `ContextMemory::data()` 的返回值；与 TS `AgentContextData` 对齐。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentContextData {
    pub history: Vec<ContextMessage>,
    pub token_count: i64,
}

/// 记录日志抽象。隐藏 `AgentRecords` 的泛型，使 `ContextMemory` 可测可替换。
pub trait RecordLog: Send + Sync {
    fn log_record(&self, record: AgentRecord);
    /// 若当前处于 record 恢复流程，返回恢复时间戳；否则返回 `None`。
    fn restoring_time(&self) -> Option<i64>;
}

/// Micro-compaction 最小接口。4.3.6 的真实实现与当前 test double 都满足此契约。
pub trait MicroCompaction: Send + Sync {
    fn compact(&self, messages: &[ContextMessage]) -> Vec<ContextMessage>;
    fn reset(&self, max_cutoff: usize);
}

/// Injection 生命周期回调。4.3.7 的 `InjectionManager` 将实现此 trait。
pub trait InjectionLifecycle: Send + Sync {
    fn on_context_clear(&self);
    fn on_context_compacted(&self, compacted_count: usize);
    fn on_context_message_removed(&self, index: usize);
}

/// 后台任务通知投递标记。4.3.8 的 `BackgroundManager` 将实现此 trait。
pub trait BackgroundNotifications: Send + Sync {
    fn mark_delivered_notification(&self, origin: &PromptOrigin);
}

/// ReplayBuilder 最小接口。4.3.7 的真实实现负责维护 resume 校验所需消息序列。
pub trait ReplayBuilder: Send + Sync {
    fn push_message(&self, message: &ContextMessage);
    fn remove_last_messages(&self, messages: &[ContextMessage]);
}

pub trait StatusEmitter: Send + Sync {
    fn emit_status_updated(&self);
}

pub trait ContextSwitchFlusher: Send + Sync {
    fn flush_deferred_context_switch(&self);
}

pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
}

/// `ContextMemory` 所需的全部 host 能力。真实 Agent 与 test double 都实现此 trait。
pub trait ContextAgent: Send + Sync {
    fn record_log(&self) -> &dyn RecordLog;
    fn micro_compaction(&self) -> &dyn MicroCompaction;
    fn injection(&self) -> &dyn InjectionLifecycle;
    fn background(&self) -> &dyn BackgroundNotifications;
    fn replay_builder(&self) -> &dyn ReplayBuilder;
    fn status_emitter(&self) -> &dyn StatusEmitter;
    fn context_switch_flusher(&self) -> &dyn ContextSwitchFlusher;
    fn clock(&self) -> &dyn Clock;
}
