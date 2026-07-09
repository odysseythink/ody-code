use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BackgroundTaskId(pub String);

impl BackgroundTaskId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl fmt::Display for BackgroundTaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTaskStatus {
    Running,
    Completed,
    Failed,
    TimedOut,
    Killed,
    Lost,
}

pub const TERMINAL_BACKGROUND_STATUSES: &[BackgroundTaskStatus] = &[
    BackgroundTaskStatus::Completed,
    BackgroundTaskStatus::Failed,
    BackgroundTaskStatus::TimedOut,
    BackgroundTaskStatus::Killed,
    BackgroundTaskStatus::Lost,
];

pub fn is_background_task_terminal(status: BackgroundTaskStatus) -> bool {
    TERMINAL_BACKGROUND_STATUSES.contains(&status)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundTaskInfo {
    #[serde(rename = "taskId")]
    pub id: BackgroundTaskId,
    pub kind: BackgroundTaskKind,
    pub description: String,
    pub status: BackgroundTaskStatus,
    pub started_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub question_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_notification_suppressed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundTaskKind {
    Process,
    Agent,
    Question,
}

pub const BACKGROUND_TASK_STARTED_EVENT: &str = "background_task_started";
pub const BACKGROUND_TASK_TERMINATED_EVENT: &str = "background_task_terminated";

// ---------------------------------------------------------------------------
// Task types: trait + sink + base
// ---------------------------------------------------------------------------

use async_trait::async_trait;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct BackgroundTaskBase {
    pub id: BackgroundTaskId,
    pub kind: BackgroundTaskKind,
    pub description: String,
    pub timeout_ms: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct BackgroundTaskSettlement {
    pub status: BackgroundTaskStatus,
    pub stop_reason: Option<String>,
}

#[async_trait]
pub trait BackgroundTask: Send + Sync {
    fn base(&self) -> &BackgroundTaskBase;
    fn set_id(&mut self, id: BackgroundTaskId);
    async fn run(
        &self,
        sink: Arc<dyn BackgroundTaskSink>,
        stop: tokio::sync::watch::Receiver<bool>,
    ) -> BackgroundTaskSettlement;
}

pub trait BackgroundTaskSink: Send + Sync {
    fn append_output(&self, chunk: &str);
    fn snapshot(&self) -> String;
}

#[derive(Default)]
pub struct SinkState {
    chunks: Mutex<Vec<String>>,
}

impl BackgroundTaskSink for SinkState {
    fn append_output(&self, chunk: &str) {
        if chunk.is_empty() {
            return;
        }
        let mut guard = self.chunks.lock().unwrap();
        guard.push(chunk.to_string());
        const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
        let mut total: usize = guard.iter().map(|s| s.as_bytes().len()).sum();
        while total > MAX_OUTPUT_BYTES && guard.len() > 1 {
            let removed = guard.remove(0);
            total -= removed.as_bytes().len();
        }
    }

    fn snapshot(&self) -> String {
        self.chunks.lock().unwrap().join("")
    }
}

pub fn info_from_base(
    base: &BackgroundTaskBase,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    stop_reason: Option<String>,
) -> BackgroundTaskInfo {
    BackgroundTaskInfo {
        id: base.id.clone(),
        kind: base.kind,
        description: base.description.clone(),
        status: if finished_at.is_some() {
            BackgroundTaskStatus::Running
        } else {
            BackgroundTaskStatus::Running
        },
        started_at,
        finished_at,
        stop_reason,
        command: None,
        pid: None,
        exit_code: None,
        output_snapshot: None,
        question_count: None,
        tool_call_id: None,
        agent_id: None,
        subagent_type: None,
        terminal_notification_suppressed: None,
        timeout_ms: base.timeout_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct DummyTask {
        base: BackgroundTaskBase,
    }

    #[async_trait::async_trait]
    impl BackgroundTask for DummyTask {
        fn base(&self) -> &BackgroundTaskBase {
            &self.base
        }
        fn set_id(&mut self, id: BackgroundTaskId) {
            self.base.id = id;
        }

        async fn run(
            &self,
            sink: Arc<dyn BackgroundTaskSink>,
            mut stop: tokio::sync::watch::Receiver<bool>,
        ) -> BackgroundTaskSettlement {
            sink.append_output("hello");
            let _ = stop.changed().await;
            BackgroundTaskSettlement {
                status: BackgroundTaskStatus::Killed,
                stop_reason: Some("stopped".into()),
            }
        }
    }

    #[tokio::test]
    async fn trait_is_implementable_and_sink_collects_output() {
        let task = DummyTask {
            base: BackgroundTaskBase {
                id: BackgroundTaskId::new("bash-12345678"),
                kind: BackgroundTaskKind::Process,
                description: "dummy".into(),
                timeout_ms: None,
            },
        };
        let sink = Arc::new(SinkState::default());
        let (tx, rx) = tokio::sync::watch::channel(false);
        let handle = tokio::spawn({
            let sink = sink.clone();
            async move { task.run(sink, rx).await }
        });
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert_eq!(sink.snapshot(), "hello");
        tx.send(true).unwrap();
        let settlement = handle.await.unwrap();
        assert_eq!(settlement.status, BackgroundTaskStatus::Killed);
    }
}
