use serde_json::{json, Value};
use std::sync::Arc;

use super::{BackgroundManager, BackgroundTaskInfoData, BackgroundTaskStatus};
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolOutput, ExecutableToolResult, ToolError,
    ToolExecution,
};

pub struct TaskListTool<M: BackgroundManager + 'static> {
    manager: Arc<M>,
}

impl<M: BackgroundManager + 'static> TaskListTool<M> {
    pub fn new(manager: Arc<M>) -> Self {
        Self { manager }
    }

    fn format_task(info: &BackgroundTaskInfoData) -> String {
        let mut lines = Vec::new();
        lines.push(format!("task_id: {}", info.task_id));
        lines.push(format!("description: {}", info.description));
        lines.push(format!("status: {}", status_to_str(info.status)));
        if let Some(ended) = info.ended_at {
            lines.push(format!("ended_at: {}", ended));
        }
        lines.push(format!("started_at: {}", info.started_at));
        if let Some(ref reason) = info.stop_reason {
            lines.push(format!("stop_reason: {}", reason));
        }
        if info.terminal_notification_suppressed {
            lines.push("terminal_notification_suppressed: true".to_string());
        }
        lines.join("\n")
    }
}

fn status_to_str(status: BackgroundTaskStatus) -> &'static str {
    match status {
        BackgroundTaskStatus::Running => "running",
        BackgroundTaskStatus::Completed => "completed",
        BackgroundTaskStatus::Failed => "failed",
        BackgroundTaskStatus::TimedOut => "timed_out",
        BackgroundTaskStatus::Killed => "killed",
        BackgroundTaskStatus::Lost => "lost",
    }
}

impl<M: BackgroundManager + 'static> BuiltinTool for TaskListTool<M> {
    fn name(&self) -> &str {
        "TaskList"
    }

    fn description(&self) -> &str {
        "List background tasks and their current status."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "active_only": {
                    "type": "boolean",
                    "description": "Whether to list only non-terminal background tasks.",
                    "default": true
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of tasks to return.",
                    "minimum": 1,
                    "maximum": 100,
                    "default": 20
                }
            },
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let active_only = args
            .get("active_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let limit = args
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(20);

        let manager = Arc::clone(&self.manager);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: format!(
                "List {} background tasks",
                if active_only { "active" } else { "all" }
            ),
            matches_rule: None,
            display: None,
            approval_rule: "allow".to_string(),
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let manager = Arc::clone(&manager);
                Box::pin(async move {
                    // Get full list first for accurate total count, then truncate display
                    let all_tasks = manager.list(active_only, None);
                    let total = all_tasks.len();
                    let displayed: Vec<_> = all_tasks.into_iter().take(limit).collect();

                    let header = if active_only {
                        format!("active_background_tasks: {}", total)
                    } else {
                        format!("background_tasks: {}", total)
                    };

                    let mut output = header;
                    if displayed.is_empty() {
                        output.push_str("\nNo background tasks.");
                    } else {
                        for task in &displayed {
                            output.push_str("\n---\n");
                            output.push_str(&Self::format_task(task));
                        }
                        // Show limit note if there might be more
                        if total > limit {
                            output.push_str(&format!("\n---\n(showing {})", limit));
                        }
                    }

                    ExecutableToolResult {
                        output: ExecutableToolOutput::Text(output),
                        message: None,
                        is_error: false,
                        stop_turn: None,
                    }
                })
            }),
        })
    }
}
