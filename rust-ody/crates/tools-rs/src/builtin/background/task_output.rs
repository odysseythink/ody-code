use serde_json::{json, Value};
use std::sync::Arc;

use super::{BackgroundManager, BackgroundTaskStatus};
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolOutput, ExecutableToolResult, ToolError,
    ToolExecution,
};

const OUTPUT_PREVIEW_BYTES: usize = 32 * 1024; // 32 KiB

pub struct TaskOutputTool<M: BackgroundManager + 'static> {
    manager: Arc<M>,
}

impl<M: BackgroundManager + 'static> TaskOutputTool<M> {
    pub fn new(manager: Arc<M>) -> Self {
        Self { manager }
    }
}

impl BackgroundTaskStatus {
    fn to_status_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
            Self::Killed => "killed",
            Self::Lost => "lost",
        }
    }
}

fn retrieval_status(info: &super::BackgroundTaskInfoData) -> String {
    match info.status {
        BackgroundTaskStatus::Killed => format!(
            "killed (stop_reason: {})",
            info.stop_reason.as_deref().unwrap_or("unknown")
        ),
        _ => info.status.to_status_str().to_string(),
    }
}

fn terminal_reason(info: &super::BackgroundTaskInfoData) -> Option<String> {
    if info.status.is_terminal() {
        Some(match info.status {
            BackgroundTaskStatus::TimedOut => "timed_out".to_string(),
            BackgroundTaskStatus::Killed | BackgroundTaskStatus::Failed => format!(
                "stopped ({})",
                info.stop_reason.as_deref().unwrap_or("unknown")
            ),
            _ => info.status.to_status_str().to_string(),
        })
    } else {
        None
    }
}

fn full_output_hint(snapshot: &super::BackgroundTaskOutputSnapshot) -> String {
    if snapshot.truncated && snapshot.full_output_available {
        let extra = snapshot
            .output_size_bytes
            .saturating_sub(snapshot.preview_bytes as u64);
        format!(
            "Output is truncated ({preview}B of {total}B shown). Use the Read tool with output_path ({path}) to read the full output ({extra}B remaining without truncation).",
            preview = snapshot.preview_bytes,
            total = snapshot.output_size_bytes,
            path = snapshot.output_path.as_deref().unwrap_or("<no path>"),
            extra = extra,
        )
    } else {
        String::new()
    }
}

impl<M: BackgroundManager + 'static> BuiltinTool for TaskOutputTool<M> {
    fn name(&self) -> &str {
        "TaskOutput"
    }

    fn description(&self) -> &str {
        "Retrieve output from a running or completed background task."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The background task ID to inspect."
                },
                "block": {
                    "type": "boolean",
                    "description": "Whether to wait for the task to finish before returning.",
                    "default": false
                },
                "timeout": {
                    "type": "integer",
                    "description": "Maximum number of seconds to wait when block=true.",
                    "minimum": 0,
                    "maximum": 3600,
                    "default": 30
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

        let block = args.get("block").and_then(|v| v.as_bool()).unwrap_or(false);

        let timeout_secs = args.get("timeout").and_then(|v| v.as_u64()).unwrap_or(30);

        let manager = Arc::clone(&self.manager);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: format!("Get output for task {}", task_id),
            matches_rule: None,
            display: None,
            approval_rule: "allow".to_string(),
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let manager = Arc::clone(&manager);
                let tid = task_id.clone();
                Box::pin(async move {
                    let info = match manager.get_task(&tid) {
                        Some(info) => info,
                        None => {
                            return ExecutableToolResult {
                                output: ExecutableToolOutput::Text(format!(
                                    "Task {} not found.",
                                    tid
                                )),
                                message: None,
                                is_error: true,
                                stop_turn: None,
                            };
                        }
                    };

                    // If blocking, wait for terminal
                    if block && !info.status.is_terminal() {
                        let _waited = manager.wait(&tid, Some(timeout_secs * 1000));
                    }

                    // Re-fetch info after potential wait
                    let info = manager.get_task(&tid).unwrap_or(info);

                    let snapshot = manager.get_output_snapshot(&tid, OUTPUT_PREVIEW_BYTES);

                    let mut output = String::new();

                    // Status line
                    output.push_str(&format!("retrieval_status: {}\n", retrieval_status(&info)));

                    if let Some(reason) = terminal_reason(&info) {
                        output.push_str(&format!("terminal_reason: {}\n", reason));
                    }

                    if let Some(ref snap) = snapshot {
                        output.push_str(&format!(
                            "outputPath: {}\n",
                            snap.output_path.as_deref().unwrap_or("<none>")
                        ));
                        output.push_str(&format!("outputSizeBytes: {}\n", snap.output_size_bytes));
                        output.push_str(&format!("outputTruncated: {}\n", snap.truncated));
                        output.push_str(&format!(
                            "fullOutputAvailable: {}\n",
                            snap.full_output_available
                        ));

                        // Output hint
                        let hint = full_output_hint(snap);
                        if !hint.is_empty() {
                            output.push_str(&format!("fullOutputHint: {}\n", hint));
                        }

                        output.push_str("[output]\n");
                        output.push_str(&snap.preview);
                    } else {
                        output.push_str("[output]\n(no output available)");
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
