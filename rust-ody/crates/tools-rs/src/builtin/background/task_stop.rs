use serde_json::{json, Value};
use std::sync::Arc;

use super::BackgroundManager;
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolOutput, ExecutableToolResult, ToolError,
    ToolExecution,
};

pub struct TaskStopTool<M: BackgroundManager + 'static> {
    manager: Arc<M>,
}

impl<M: BackgroundManager + 'static> TaskStopTool<M> {
    pub fn new(manager: Arc<M>) -> Self {
        Self { manager }
    }
}

impl<M: BackgroundManager + 'static> BuiltinTool for TaskStopTool<M> {
    fn name(&self) -> &str {
        "TaskStop"
    }

    fn description(&self) -> &str {
        "Stop a running background task."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The background task ID to stop."
                },
                "reason": {
                    "type": "string",
                    "description": "Short reason recorded when the task is stopped.",
                    "default": "Stopped by TaskStop"
                }
            },
            "required": ["task_id"],
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let task_id = args["task_id"].as_str().unwrap_or("").to_string();
        if task_id.is_empty() {
            return Err(ToolError::InvalidArgs("task_id is required".into()));
        }

        let reason = args
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("Stopped by TaskStop")
            .to_string();

        let manager = Arc::clone(&self.manager);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: format!("Stop background task {}", task_id),
            matches_rule: None,
            display: None,
            approval_rule: "allow".to_string(),
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let manager = Arc::clone(&manager);
                let tid = task_id.clone();
                let reason = reason.clone();
                Box::pin(async move {
                    let info = match manager.get_task(&tid) {
                        Some(info) => info,
                        None => {
                            return ExecutableToolResult {
                                output: ExecutableToolOutput::Text(format!(
                                    "No background task found with id {}.",
                                    tid
                                )),
                                message: None,
                                is_error: true,
                                stop_turn: None,
                            };
                        }
                    };

                    if info.status.is_terminal() {
                        return ExecutableToolResult {
                            output: ExecutableToolOutput::Text(format!(
                                "Task {} is already terminal (status: {:?}).",
                                tid, info.status
                            )),
                            message: None,
                            is_error: false,
                            stop_turn: None,
                        };
                    }

                    // Suppress terminal notification before stopping
                    manager.suppress_terminal_notification(&tid);

                    match manager.stop(&tid, Some(reason)) {
                        Some(result) => ExecutableToolResult {
                            output: ExecutableToolOutput::Text(format!(
                                "Task {} stopped. Status: {:?}.",
                                result.task_id, result.status
                            )),
                            message: None,
                            is_error: false,
                            stop_turn: None,
                        },
                        None => ExecutableToolResult {
                            output: ExecutableToolOutput::Text(format!(
                                "Failed to stop task {}.",
                                tid
                            )),
                            message: None,
                            is_error: true,
                            stop_turn: None,
                        },
                    }
                })
            }),
        })
    }
}
