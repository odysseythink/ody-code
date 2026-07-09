use std::sync::{Arc, Mutex};

use crate::background::manager::BackgroundManager;
use crate::background::types::{
    BackgroundTask, BackgroundTaskBase, BackgroundTaskId, BackgroundTaskKind,
    BackgroundTaskSettlement, BackgroundTaskSink, BackgroundTaskStatus,
};
use tools_rs::builtin::collaboration::{
    AgentCompletion, AgentTaskOptions, BackgroundError, BackgroundRegistrar, QuestionRunFn,
    QuestionTaskOptions,
};

pub struct AgentBackgroundRegistrar {
    manager: Mutex<Option<Arc<BackgroundManager>>>,
}

impl AgentBackgroundRegistrar {
    pub fn new(manager: Option<Arc<BackgroundManager>>) -> Self {
        Self {
            manager: Mutex::new(manager),
        }
    }
}

#[async_trait::async_trait]
impl BackgroundRegistrar for AgentBackgroundRegistrar {
    async fn register_question_task(
        &self,
        description: String,
        run: QuestionRunFn,
        options: QuestionTaskOptions,
    ) -> Result<String, BackgroundError> {
        let manager = self.manager.lock().unwrap().clone();
        let Some(manager) = manager else {
            return Err(BackgroundError::Unavailable);
        };

        let task: Box<dyn BackgroundTask> = Box::new(QuestionBackgroundTask {
            base: BackgroundTaskBase {
                id: BackgroundTaskId::new(""),
                kind: BackgroundTaskKind::Question,
                description,
                timeout_ms: None,
            },
            run,
            _options: options,
        });
        Ok(manager.register_task(task))
    }

    async fn register_agent_task(
        &self,
        completion: AgentCompletion,
        description: String,
        options: AgentTaskOptions,
    ) -> Result<String, BackgroundError> {
        let manager = self.manager.lock().unwrap().clone();
        let Some(manager) = manager else {
            return Err(BackgroundError::Unavailable);
        };

        let abort = options.abort.clone();
        let task: Box<dyn BackgroundTask> = Box::new(AgentBackgroundTask {
            base: BackgroundTaskBase {
                id: BackgroundTaskId::new(""),
                kind: BackgroundTaskKind::Agent,
                description,
                timeout_ms: options.timeout_ms,
            },
            completion: Mutex::new(Some(completion)),
            abort,
        });
        Ok(manager.register_task(task))
    }
}

struct QuestionBackgroundTask {
    base: BackgroundTaskBase,
    run: QuestionRunFn,
    _options: QuestionTaskOptions,
}

#[async_trait::async_trait]
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
        let signal = tools_rs::builtin::AbortSignal::new();
        let signal_for_stop = signal.clone();
        let run = self.run.clone();
        let result = tokio::select! {
            biased;
            _ = stop.changed() => {
                signal_for_stop.abort();
                Ok(tools_rs::builtin::ExecutableToolResult::error_text(
                    "Cancelled".into(),
                    "Cancelled".into(),
                ))
            }
            r = run(signal) => r,
        };
        let output = match result {
            Ok(res) => res.to_text(),
            Err(e) => format!("error: {}", e),
        };
        sink.append_output(&output);
        BackgroundTaskSettlement {
            status: BackgroundTaskStatus::Completed,
            stop_reason: None,
        }
    }
}

struct AgentBackgroundTask {
    base: BackgroundTaskBase,
    completion: Mutex<Option<AgentCompletion>>,
    abort: Arc<dyn Fn() + Send + Sync>,
}

#[async_trait::async_trait]
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
        mut stop: tokio::sync::watch::Receiver<bool>,
    ) -> BackgroundTaskSettlement {
        let abort = self.abort.clone();
        let completion = self.completion.lock().unwrap().take().unwrap();
        let result = tokio::select! {
            biased;
            _ = stop.changed() => {
                abort();
                Ok(tools_rs::builtin::collaboration::SubagentResult {
                    result: "Cancelled".into(),
                    usage: None,
                })
            }
            r = completion => r,
        };
        match result {
            Ok(res) => {
                sink.append_output(&res.result);
                BackgroundTaskSettlement {
                    status: BackgroundTaskStatus::Completed,
                    stop_reason: None,
                }
            }
            Err(e) => BackgroundTaskSettlement {
                status: BackgroundTaskStatus::Failed,
                stop_reason: Some(format!("{:?}", e)),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn registrar_without_manager_returns_unavailable() {
        let registrar = AgentBackgroundRegistrar::new(None);
        let completion: AgentCompletion = Box::pin(async move {
            Ok(tools_rs::builtin::collaboration::SubagentResult {
                result: "x".into(),
                usage: None,
            })
        });
        let result = registrar
            .register_agent_task(
                completion,
                "desc".into(),
                AgentTaskOptions {
                    timeout_ms: None,
                    agent_id: "a".into(),
                    subagent_type: "coder".into(),
                    abort: Arc::new(|| {}),
                },
            )
            .await;
        assert!(matches!(result, Err(BackgroundError::Unavailable)));
    }
}
