use crate::background::types::{
    BackgroundTask, BackgroundTaskBase, BackgroundTaskId, BackgroundTaskInfo, BackgroundTaskKind,
    BackgroundTaskSettlement, BackgroundTaskSink, BackgroundTaskStatus,
};
use crate::records::nested::{ExecutableToolOutput, ExecutableToolResult};
use async_trait::async_trait;
use chrono::Utc;
use kaos_rs::kaos::Kaos;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;

pub struct ProcessBackgroundTask {
    base: BackgroundTaskBase,
    kaos: Kaos,
    args: Vec<String>,
}

impl ProcessBackgroundTask {
    pub fn new(kaos: Kaos, args: Vec<&str>) -> Self {
        let command = args.join(" ");
        Self {
            base: BackgroundTaskBase {
                id: BackgroundTaskId::new("process-unset"),
                kind: BackgroundTaskKind::Process,
                description: command.clone(),
                timeout_ms: None,
            },
            kaos,
            args: args.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    pub fn with_id(mut self, id: BackgroundTaskId) -> Self {
        self.base.id = id;
        self
    }
}

#[async_trait]
impl BackgroundTask for ProcessBackgroundTask {
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
        let args: Vec<&str> = self.args.iter().map(|s| s.as_str()).collect();
        let proc = match self.kaos.exec(&args).await {
            Ok(p) => Arc::new(p),
            Err(e) => {
                return BackgroundTaskSettlement {
                    status: BackgroundTaskStatus::Failed,
                    stop_reason: Some(format!("spawn failed: {e}")),
                };
            }
        };

        let stdout_proc = proc.clone();
        let stdout_sink = sink.clone();
        let stdout_handle = tokio::spawn(async move {
            let mut stream = stdout_proc.stdout_stream();
            let mut buf = Vec::new();
            let _ = stream.read_to_end(&mut buf).await;
            if let Ok(s) = String::from_utf8(buf) {
                stdout_sink.append_output(&s);
            }
        });

        let stderr_proc = proc.clone();
        let stderr_sink = sink.clone();
        let stderr_handle = tokio::spawn(async move {
            let mut stream = stderr_proc.stderr_stream();
            let mut buf = Vec::new();
            let _ = stream.read_to_end(&mut buf).await;
            if let Ok(s) = String::from_utf8(buf) {
                stderr_sink.append_output(&s);
            }
        });

        let stopped = *stop.borrow();
        let exit_code = if stopped {
            proc.kill(Some("SIGTERM")).await.ok();
            proc.wait().await
        } else {
            tokio::select! {
                biased;
                _ = stop.changed() => {
                    proc.kill(Some("SIGTERM")).await.ok();
                    proc.wait().await
                }
                code = proc.wait() => code,
            }
        };

        stdout_handle.await.ok();
        stderr_handle.await.ok();

        let status = if stopped || *stop.borrow() {
            BackgroundTaskStatus::Killed
        } else if exit_code == 0 {
            BackgroundTaskStatus::Completed
        } else {
            BackgroundTaskStatus::Failed
        };
        BackgroundTaskSettlement {
            status,
            stop_reason: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct AgentOptions {
    pub timeout_ms: Option<u64>,
    pub agent_id: Option<String>,
    pub subagent_type: Option<String>,
}

pub struct AgentBackgroundTask {
    base: BackgroundTaskBase,
    completion:
        tokio::sync::Mutex<Option<tokio::sync::oneshot::Receiver<Result<String, anyhow::Error>>>>,
    options: AgentOptions,
}

impl AgentBackgroundTask {
    pub fn new(
        completion: tokio::sync::oneshot::Receiver<Result<String, anyhow::Error>>,
        description: String,
        options: AgentOptions,
    ) -> Self {
        Self {
            base: BackgroundTaskBase {
                id: BackgroundTaskId::new("agent-unset"),
                kind: BackgroundTaskKind::Agent,
                description,
                timeout_ms: options.timeout_ms,
            },
            completion: tokio::sync::Mutex::new(Some(completion)),
            options,
        }
    }

    pub fn with_id(mut self, id: BackgroundTaskId) -> Self {
        self.base.id = id;
        self
    }
}

#[async_trait]
impl BackgroundTask for AgentBackgroundTask {
    fn base(&self) -> &BackgroundTaskBase {
        &self.base
    }
    fn set_id(&mut self, id: BackgroundTaskId) {
        self.base.id = id;
    }

    async fn run(
        &self,
        sink: Arc<dyn BackgroundTaskSink>,
        _stop: tokio::sync::watch::Receiver<bool>,
    ) -> BackgroundTaskSettlement {
        let mut guard = self.completion.lock().await;
        let Some(rx) = guard.take() else {
            return BackgroundTaskSettlement {
                status: BackgroundTaskStatus::Failed,
                stop_reason: Some("agent task already consumed".into()),
            };
        };
        drop(guard);

        let fut = async move {
            match rx.await {
                Ok(Ok(result)) => {
                    sink.append_output(&result);
                    BackgroundTaskSettlement {
                        status: BackgroundTaskStatus::Completed,
                        stop_reason: None,
                    }
                }
                Ok(Err(e)) => BackgroundTaskSettlement {
                    status: BackgroundTaskStatus::Failed,
                    stop_reason: Some(format!("{e}")),
                },
                Err(_) => BackgroundTaskSettlement {
                    status: BackgroundTaskStatus::Failed,
                    stop_reason: Some("completion sender dropped".into()),
                },
            }
        };

        if let Some(ms) = self.base.timeout_ms {
            match tokio::time::timeout(Duration::from_millis(ms), fut).await {
                Ok(settlement) => settlement,
                Err(_) => BackgroundTaskSettlement {
                    status: BackgroundTaskStatus::TimedOut,
                    stop_reason: None,
                },
            }
        } else {
            fut.await
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct QuestionOptions {
    pub question_count: u32,
    pub tool_call_id: Option<String>,
}

type QuestionRunFn = Box<
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<ExecutableToolResult, anyhow::Error>> + Send>>
        + Send
        + Sync,
>;

pub struct QuestionBackgroundTask {
    base: BackgroundTaskBase,
    run: QuestionRunFn,
    options: QuestionOptions,
}

impl QuestionBackgroundTask {
    pub fn new<F, Fut>(run: F, description: String, options: QuestionOptions) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ExecutableToolResult, anyhow::Error>> + Send + 'static,
    {
        Self {
            base: BackgroundTaskBase {
                id: BackgroundTaskId::new("question-unset"),
                kind: BackgroundTaskKind::Question,
                description,
                timeout_ms: None,
            },
            run: Box::new(move || Box::pin(run())),
            options,
        }
    }

    pub fn with_id(mut self, id: BackgroundTaskId) -> Self {
        self.base.id = id;
        self
    }
}

#[async_trait]
impl BackgroundTask for QuestionBackgroundTask {
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
        if *stop.borrow() {
            return BackgroundTaskSettlement {
                status: BackgroundTaskStatus::Killed,
                stop_reason: None,
            };
        }

        let mut fut = (self.run)();
        tokio::select! {
            biased;
            _ = stop.changed() => BackgroundTaskSettlement {
                status: BackgroundTaskStatus::Killed,
                stop_reason: None,
            },
            res = &mut fut => match res {
                Ok(ExecutableToolResult::Success(r)) => {
                    let text = match r.output {
                        ExecutableToolOutput::Text(s) => s,
                        ExecutableToolOutput::Parts(parts) => serde_json::to_string(&parts).unwrap_or_default(),
                    };
                    if !text.is_empty() {
                        sink.append_output(&text);
                    }
                    let status = if r.is_error == Some(true) {
                        BackgroundTaskStatus::Failed
                    } else {
                        BackgroundTaskStatus::Completed
                    };
                    BackgroundTaskSettlement {
                        status,
                        stop_reason: r.message.filter(|m| !m.is_empty()),
                    }
                }
                Ok(ExecutableToolResult::Error(r)) => {
                    let text = match r.output {
                        ExecutableToolOutput::Text(s) => s,
                        ExecutableToolOutput::Parts(parts) => serde_json::to_string(&parts).unwrap_or_default(),
                    };
                    if !text.is_empty() {
                        sink.append_output(&text);
                    }
                    BackgroundTaskSettlement {
                        status: BackgroundTaskStatus::Failed,
                        stop_reason: r.message.filter(|m| !m.is_empty()).or_else(|| {
                            let t = text.trim();
                            if t.is_empty() { None } else { Some(t.to_string()) }
                        }),
                    }
                }
                Err(e) => BackgroundTaskSettlement {
                    status: BackgroundTaskStatus::Failed,
                    stop_reason: Some(format!("{e}")),
                },
            },
        }
    }
}

pub fn build_info(
    base: &BackgroundTaskBase,
    started_at: chrono::DateTime<Utc>,
    finished_at: Option<chrono::DateTime<Utc>>,
    stop_reason: Option<String>,
    sink_snapshot: &str,
    exit_code: Option<i32>,
    pid: Option<u32>,
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
        command: if base.kind == BackgroundTaskKind::Process {
            Some(base.description.clone())
        } else {
            None
        },
        pid,
        exit_code,
        output_snapshot: Some(sink_snapshot.into()).filter(|s: &String| !s.is_empty()),
        question_count: if base.kind == BackgroundTaskKind::Question {
            Some(0)
        } else {
            None
        },
        tool_call_id: None,
        agent_id: (base.kind == BackgroundTaskKind::Agent).then(|| base.id.to_string()),
        subagent_type: None,
        terminal_notification_suppressed: None,
        timeout_ms: base.timeout_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::background::types::{
        BackgroundTask, BackgroundTaskSink, BackgroundTaskStatus, SinkState,
    };
    use crate::records::nested::{
        ExecutableToolOutput, ExecutableToolResult, ExecutableToolSuccessResult,
    };
    use kaos_rs::environment::Environment;
    use kaos_rs::kaos::Kaos;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::oneshot;
    use tokio::time::timeout;

    fn dummy_env() -> Environment {
        Environment {
            os_kind: "macOS".into(),
            os_arch: "arm64".into(),
            os_version: "23.0.0".into(),
            shell_name: "bash".into(),
            shell_path: "/bin/bash".into(),
        }
    }

    #[tokio::test]
    async fn process_task_runs_echo_and_collects_output() {
        let kaos = Kaos::new(dummy_env(), std::env::current_dir().unwrap());
        let task = ProcessBackgroundTask::new(kaos, vec!["/bin/echo", "-n", "hello"]);
        let sink = Arc::new(SinkState::default());
        let (_tx, rx) = tokio::sync::watch::channel(false);
        let settlement = task.run(sink.clone(), rx).await;
        assert_eq!(settlement.status, BackgroundTaskStatus::Completed);
        assert_eq!(sink.snapshot(), "hello");
    }

    #[tokio::test]
    async fn question_task_returns_completed_and_output() {
        let task = QuestionBackgroundTask::new(
            || async {
                Ok(ExecutableToolResult::Success(ExecutableToolSuccessResult {
                    output: ExecutableToolOutput::Text("42".into()),
                    is_error: None,
                    stop_turn: None,
                    message: None,
                }))
            },
            "ask".into(),
            QuestionOptions {
                question_count: 1,
                tool_call_id: None,
            },
        );
        let sink = Arc::new(SinkState::default());
        let (_tx, rx) = tokio::sync::watch::channel(false);
        let settlement = task.run(sink.clone(), rx).await;
        assert_eq!(settlement.status, BackgroundTaskStatus::Completed);
        assert_eq!(sink.snapshot(), "42");
    }

    #[tokio::test]
    async fn agent_task_completes_with_result() {
        let (tx, rx) = oneshot::channel::<Result<String, anyhow::Error>>();
        let task = AgentBackgroundTask::new(rx, "sub".into(), AgentOptions::default());
        let sink = Arc::new(SinkState::default());
        let (_stop_tx, stop_rx) = tokio::sync::watch::channel(false);
        let handle = tokio::spawn({
            let sink = sink.clone();
            async move { task.run(sink, stop_rx).await }
        });
        tx.send(Ok("done".into())).unwrap();
        let settlement = timeout(Duration::from_secs(1), handle)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(settlement.status, BackgroundTaskStatus::Completed);
        assert_eq!(sink.snapshot(), "done");
    }
}
