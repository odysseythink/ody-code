use serde_json::{json, Value};
use std::sync::Arc;

use super::CronManager;
use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolOutput, ExecutableToolResult, ToolError,
    ToolExecution,
};

pub struct CronDeleteTool<M: CronManager + 'static> {
    manager: Arc<M>,
}

impl<M: CronManager + 'static> CronDeleteTool<M> {
    pub fn new(manager: Arc<M>) -> Self {
        Self { manager }
    }
}

/// Validate that a string is exactly 8 hex characters.
fn validate_id_format(id: &str) -> bool {
    if id.len() != 8 {
        return false;
    }
    id.chars().all(|c| c.is_ascii_hexdigit())
}

impl<M: CronManager + 'static> BuiltinTool for CronDeleteTool<M> {
    fn name(&self) -> &str {
        "CronDelete"
    }

    fn description(&self) -> &str {
        "Cancel a scheduled cron job by id."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "The 8-hex cron job id returned by CronCreate / CronList."
                }
            },
            "required": ["id"],
            "additionalProperties": false
        })
    }

    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let id = args["id"].as_str().unwrap_or("").to_string();

        // Validate ID format: exactly 8 hex chars
        if !validate_id_format(&id) {
            return Err(ToolError::InvalidArgs(format!(
                "Invalid cron job id: '{}'. Must be 8 hex characters.",
                id
            )));
        }

        let manager = Arc::clone(&self.manager);

        Ok(ToolExecution {
            accesses: Default::default(),
            description: format!("Delete cron job {}", id),
            matches_rule: None,
            display: None,
            approval_rule: "allow".to_string(),
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let manager = Arc::clone(&manager);
                let tid = id.clone();
                Box::pin(async move {
                    let removed = manager.remove_tasks(&[tid.clone()]);
                    if removed.is_empty() {
                        ExecutableToolResult {
                            output: ExecutableToolOutput::Text(format!(
                                "No cron job with id {}.",
                                tid
                            )),
                            message: None,
                            is_error: true,
                            stop_turn: None,
                        }
                    } else {
                        ExecutableToolResult {
                            output: ExecutableToolOutput::Text(format!(
                                "Cron job {} deleted.",
                                tid
                            )),
                            message: None,
                            is_error: false,
                            stop_turn: None,
                        }
                    }
                })
            }),
        })
    }
}
