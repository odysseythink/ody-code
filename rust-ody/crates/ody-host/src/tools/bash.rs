use std::sync::Arc;

use async_trait::async_trait;
use kaos_rs::kaos::Kaos;

use super::{ApprovalClient, ApprovalDecision, ApprovalRequest, Tool, ToolError, ToolResult};

pub struct BashTool {
    kaos: Arc<Kaos>,
}

impl BashTool {
    pub fn new(kaos: Arc<Kaos>) -> Self {
        Self { kaos }
    }
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn description(&self) -> &str {
        "Execute a bash command. Requires approval."
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The bash command to execute" },
                "description": { "type": "string", "description": "A description of what the command does" }
            },
            "required": ["command"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        approval: &dyn ApprovalClient,
    ) -> Result<ToolResult, ToolError> {
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::ExecutionFailed {
                message: "missing 'command' argument".to_string(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "missing command",
                )),
            })?;

        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or(command);

        let approval_request = ApprovalRequest {
            tool_call_id: uuid::Uuid::now_v7().to_string(),
            tool_name: "bash".to_string(),
            action: "Execute bash command".to_string(),
            display: serde_json::json!({ "command": command, "description": description }),
        };

        let approval_response = approval.request(approval_request).await?;

        match approval_response.decision {
            ApprovalDecision::Rejected | ApprovalDecision::Cancelled => {
                return Ok(serde_json::json!({
                    "status": "cancelled",
                    "message": "Command execution was rejected by the user."
                }));
            }
            ApprovalDecision::Approved => {}
        }

        let proc = self
            .kaos
            .exec(&["bash", "-c", command])
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                message: "failed to execute bash command".to_string(),
                source: Box::new(e),
            })?;

        let exit_code = proc.wait().await;
        let stdout = String::from_utf8_lossy(&proc.stdout().await).to_string();
        let stderr = String::from_utf8_lossy(&proc.stderr().await).to_string();

        Ok(serde_json::json!({
            "status": if exit_code == 0 { "success" } else { "error" },
            "stdout": stdout,
            "stderr": stderr,
            "exit_code": exit_code,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ApprovalResponse;
    use kaos_rs::environment::detect_environment_from_node;

    fn make_tool() -> BashTool {
        let env = detect_environment_from_node();
        BashTool::new(Arc::new(Kaos::new(env, std::env::current_dir().unwrap())))
    }

    struct MockApprovalClient {
        decision: ApprovalDecision,
    }

    #[async_trait]
    impl ApprovalClient for MockApprovalClient {
        async fn request(&self, _request: ApprovalRequest) -> Result<ApprovalResponse, ToolError> {
            Ok(ApprovalResponse {
                decision: self.decision,
            })
        }
    }

    #[tokio::test]
    async fn bash_tool_approved_executes_command() {
        let tool = make_tool();
        let args = serde_json::json!({"command": "echo hello"});
        let result = tool
            .execute(
                args,
                &MockApprovalClient {
                    decision: ApprovalDecision::Approved,
                },
            )
            .await
            .unwrap();
        assert_eq!(result["status"], "success");
        assert!(result["stdout"].as_str().unwrap().contains("hello"));
    }

    #[tokio::test]
    async fn bash_tool_rejected_returns_cancelled() {
        let tool = make_tool();
        let args = serde_json::json!({"command": "echo hello"});
        let result = tool
            .execute(
                args,
                &MockApprovalClient {
                    decision: ApprovalDecision::Rejected,
                },
            )
            .await
            .unwrap();
        assert_eq!(result["status"], "cancelled");
    }
}
