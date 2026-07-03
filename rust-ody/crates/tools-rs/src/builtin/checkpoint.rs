/// Minimal trait — the tool only needs `checkpointNow()`.
/// Real impl in agent-rs coordinates with backup/index/integrity.
pub trait CheckpointCoordinator: Send + Sync {
    fn checkpoint_now(&self) -> Result<(), CheckpointError>;
}

#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    #[error("checkpoint coordinator is not enabled")]
    NotEnabled,
    #[error("{0}")]
    Other(String),
}

/// Mock for golden testing.
pub struct MockCheckpointCoordinator {
    pub saved: std::sync::Mutex<bool>,
}

impl MockCheckpointCoordinator {
    pub fn new() -> Self {
        Self {
            saved: std::sync::Mutex::new(false),
        }
    }
}

impl CheckpointCoordinator for MockCheckpointCoordinator {
    fn checkpoint_now(&self) -> Result<(), CheckpointError> {
        *self.saved.lock().unwrap() = true;
        Ok(())
    }
}

use crate::builtin::{
    BuiltinTool, ExecutableToolContext, ExecutableToolResult, ToolError, ToolExecution,
};
use serde_json::Value;
use std::sync::Arc;

pub struct CheckpointTool {
    coordinator: Arc<dyn CheckpointCoordinator>,
}

impl CheckpointTool {
    pub fn new(coordinator: Arc<dyn CheckpointCoordinator>) -> Self {
        Self { coordinator }
    }
}

impl BuiltinTool for CheckpointTool {
    fn name(&self) -> &str {
        "Checkpoint"
    }
    fn description(&self) -> &str {
        "Force an immediate durable checkpoint save."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "reason": { "type": "string", "description": "Short reason for taking the checkpoint." }
            },
            "additionalProperties": false
        })
    }
    fn resolve_execution(&self, args: Value) -> Result<ToolExecution, ToolError> {
        let reason = args["reason"].as_str().map(|s| s.to_string());
        let description = if let Some(ref r) = reason {
            format!("Taking manual checkpoint: {}", r)
        } else {
            "Taking manual checkpoint".into()
        };
        let coord = Arc::clone(&self.coordinator);
        Ok(ToolExecution {
            accesses: Default::default(),
            description,
            approval_rule: "Checkpoint".into(),
            matches_rule: None,
            display: None,
            execute: Box::new(move |_ctx: ExecutableToolContext| {
                let coord = Arc::clone(&coord);
                Box::pin(async move {
                    match coord.checkpoint_now() {
                        Ok(()) => ExecutableToolResult::ok_text("Checkpoint saved.".into()),
                        Err(e) => ExecutableToolResult::error_text(e.to_string(), e.to_string()),
                    }
                })
            }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn checkpoint_saves() {
        let coord = Arc::new(MockCheckpointCoordinator::new());
        let tool = CheckpointTool::new(coord.clone());
        let args = serde_json::json!({"reason": "manual trigger"});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        }));
        assert!(!result.is_error);
        assert!(result.to_text().contains("Checkpoint saved"));
        assert!(*coord.saved.lock().unwrap());
    }

    #[test]
    fn checkpoint_errors_when_not_enabled() {
        struct DisabledCoordinator;
        impl CheckpointCoordinator for DisabledCoordinator {
            fn checkpoint_now(&self) -> Result<(), CheckpointError> {
                Err(CheckpointError::NotEnabled)
            }
        }
        let coord = Arc::new(DisabledCoordinator);
        let tool = CheckpointTool::new(coord);
        let args = serde_json::json!({});
        let exec = tool.resolve_execution(args).unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let result = rt.block_on((exec.execute)(crate::builtin::ExecutableToolContext {
            turn_id: "1".into(),
            tool_call_id: "call_1".into(),
            signal: crate::builtin::AbortSignal::new(),
            metadata: None,
        }));
        assert!(result.is_error);
        assert!(result.to_text().contains("not enabled"));
    }
}
