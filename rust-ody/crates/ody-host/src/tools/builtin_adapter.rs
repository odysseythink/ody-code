use std::sync::Arc;

use async_trait::async_trait;
use tools_rs::builtin::{BuiltinTool, ExecutableToolContext};

use super::{ApprovalClient, ApprovalDecision, ApprovalRequest, Tool, ToolError, ToolResult};

pub struct BuiltinToolAdapter<T: BuiltinTool> {
    inner: Arc<T>,
}

impl<T: BuiltinTool> BuiltinToolAdapter<T> {
    pub fn new(inner: Arc<T>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl<T: BuiltinTool> Tool for BuiltinToolAdapter<T> {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters(&self) -> serde_json::Value {
        self.inner.parameters()
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        approval: &dyn ApprovalClient,
    ) -> Result<ToolResult, ToolError> {
        let exec = self
            .inner
            .resolve_execution(args)
            .map_err(|e| ToolError::ExecutionFailed {
                message: "invalid tool arguments".to_string(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    e.to_string(),
                )),
            })?;

        if let Some(_matches_rule) = &exec.matches_rule {
            let request = ApprovalRequest {
                tool_call_id: uuid::Uuid::now_v7().to_string(),
                tool_name: self.inner.name().to_string(),
                action: exec.description.clone(),
                display: exec.display.clone().unwrap_or(serde_json::Value::Null),
            };
            let response = approval.request(request).await?;
            if response.decision != ApprovalDecision::Approved {
                return Ok(serde_json::json!({
                    "output": "Tool execution was not approved.",
                    "isError": true,
                }));
            }
        }

        let ctx = ExecutableToolContext {
            turn_id: uuid::Uuid::now_v7().to_string(),
            tool_call_id: uuid::Uuid::now_v7().to_string(),
            signal: tools_rs::builtin::AbortSignal::new(),
            metadata: None,
        };
        let result = (exec.execute)(ctx).await;
        Ok(
            serde_json::to_value(result).map_err(|e| ToolError::ExecutionFailed {
                message: "failed to serialize tool result".to_string(),
                source: Box::new(e),
            })?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{ApprovalClient, ApprovalDecision, ApprovalRequest, ApprovalResponse, Tool};

    struct AlwaysApprove;

    #[async_trait::async_trait]
    impl ApprovalClient for AlwaysApprove {
        async fn request(
            &self,
            _request: ApprovalRequest,
        ) -> Result<ApprovalResponse, crate::tools::ToolError> {
            Ok(ApprovalResponse {
                decision: ApprovalDecision::Approved,
            })
        }
    }

    #[tokio::test]
    async fn adapter_executes_builtin_tool() {
        use tools_rs::builtin::checkpoint::{CheckpointTool, MockCheckpointCoordinator};
        let coord = std::sync::Arc::new(MockCheckpointCoordinator::new());
        let tool = CheckpointTool::new(coord);
        let adapter = BuiltinToolAdapter::new(std::sync::Arc::new(tool));
        assert_eq!(adapter.name(), "Checkpoint");
    }
}
