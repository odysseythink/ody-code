use async_trait::async_trait;

use super::{ApprovalClient, ApprovalDecision, ApprovalRequest, Tool, ToolError, ToolResult};

pub struct BashTool;

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

    async fn execute(&self, args: serde_json::Value, approval: &dyn ApprovalClient) -> Result<ToolResult, ToolError> {
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::ExecutionFailed {
                message: "missing 'command' argument".to_string(),
                source: Box::new(std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing command")),
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

        let output = tokio::process::Command::new("bash")
            .arg("-c")
            .arg(command)
            .output()
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                message: "failed to execute bash command".to_string(),
                source: Box::new(e),
            })?;

        Ok(serde_json::json!({
            "status": if output.status.success() { "success" } else { "error" },
            "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
            "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
            "exit_code": output.status.code().unwrap_or(-1),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::ApprovalResponse;

    struct MockApprovalClient {
        decision: ApprovalDecision,
    }

    #[async_trait]
    impl ApprovalClient for MockApprovalClient {
        async fn request(&self, _request: ApprovalRequest) -> Result<ApprovalResponse, ToolError> {
            Ok(ApprovalResponse { decision: self.decision })
        }
    }

    #[tokio::test]
    async fn bash_tool_approved_executes_command() {
        let tool = BashTool;
        let args = serde_json::json!({"command": "echo hello"});
        let result = tool.execute(args, &MockApprovalClient { decision: ApprovalDecision::Approved }).await.unwrap();
        assert_eq!(result["status"], "success");
        assert!(result["stdout"].as_str().unwrap().contains("hello"));
    }

    #[tokio::test]
    async fn bash_tool_rejected_returns_cancelled() {
        let tool = BashTool;
        let args = serde_json::json!({"command": "echo hello"});
        let result = tool.execute(args, &MockApprovalClient { decision: ApprovalDecision::Rejected }).await.unwrap();
        assert_eq!(result["status"], "cancelled");
    }
}
