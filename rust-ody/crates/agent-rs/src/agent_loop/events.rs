use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

use crate::records::nested::ToolUpdate;

pub use crate::records::nested::LoopRecordedEvent;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepRetryingEvent {
    #[serde(rename = "turnId")]
    pub turn_id: String,
    pub step: u32,
    #[serde(rename = "stepUuid")]
    pub step_uuid: String,
    #[serde(rename = "failedAttempt")]
    pub failed_attempt: u32,
    #[serde(rename = "nextAttempt")]
    pub next_attempt: u32,
    #[serde(rename = "maxAttempts")]
    pub max_attempts: u32,
    #[serde(rename = "delayMs")]
    pub delay_ms: u64,
    #[serde(rename = "errorName")]
    pub error_name: String,
    #[serde(rename = "errorMessage")]
    pub error_message: String,
    #[serde(rename = "statusCode", skip_serializing_if = "Option::is_none")]
    pub status_code: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum LoopLiveOnlyEvent {
    #[serde(rename = "turn.interrupted")]
    TurnInterrupted {
        reason: LoopInterruptReason,
        #[serde(rename = "attemptedSteps")]
        attempted_steps: u32,
        #[serde(rename = "activeStep", skip_serializing_if = "Option::is_none")]
        active_step: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    #[serde(rename = "step.retrying")]
    StepRetrying(StepRetryingEvent),
    #[serde(rename = "text.delta")]
    TextDelta { delta: String },
    #[serde(rename = "thinking.delta")]
    ThinkingDelta { delta: String },
    #[serde(rename = "tool.call.delta")]
    ToolCallDelta {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(rename = "argumentsPart", skip_serializing_if = "Option::is_none")]
        arguments_part: Option<String>,
    },
    #[serde(rename = "tool.progress")]
    ToolProgress {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        update: ToolUpdate,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopInterruptReason {
    Aborted,
    MaxSteps,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LoopEvent {
    Recorded(LoopRecordedEvent),
    Live(LoopLiveOnlyEvent),
}

pub type LiveEventEmitter = Box<dyn Fn(LoopEvent) + Send + Sync>;

#[async_trait::async_trait]
pub trait LoopEventDispatcher: Send + Sync {
    async fn dispatch_recorded(&self, event: LoopRecordedEvent) -> Result<(), anyhow::Error>;
    fn dispatch_live(&self, event: LoopLiveOnlyEvent);
}

type AppendRecordFn = Box<
    dyn Fn(LoopRecordedEvent) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send>>
        + Send
        + Sync,
>;

pub struct DefaultLoopEventDispatcher {
    append_record: AppendRecordFn,
    emit_live: Option<LiveEventEmitter>,
}

impl DefaultLoopEventDispatcher {
    pub fn new<F, Fut>(append_record: F, emit_live: Option<LiveEventEmitter>) -> Self
    where
        F: Fn(LoopRecordedEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), anyhow::Error>> + Send + 'static,
    {
        Self {
            append_record: Box::new(move |event| Box::pin(append_record(event))),
            emit_live,
        }
    }
}

#[async_trait::async_trait]
impl LoopEventDispatcher for DefaultLoopEventDispatcher {
    async fn dispatch_recorded(&self, event: LoopRecordedEvent) -> Result<(), anyhow::Error> {
        (self.append_record)(event.clone()).await?;
        if let Some(emit) = &self.emit_live {
            emit(LoopEvent::Recorded(event));
        }
        Ok(())
    }

    fn dispatch_live(&self, event: LoopLiveOnlyEvent) {
        if let Some(emit) = &self.emit_live {
            emit(LoopEvent::Live(event));
        }
    }
}
