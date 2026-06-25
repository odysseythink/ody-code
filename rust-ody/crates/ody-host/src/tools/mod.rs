use std::sync::Arc;

use async_trait::async_trait;

pub mod bash;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalDecision {
    Approved,
    Rejected,
    Cancelled,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub tool_call_id: String,
    pub tool_name: String,
    pub action: String,
    pub display: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApprovalResponse {
    pub decision: ApprovalDecision,
}

#[derive(Debug)]
pub enum ToolError {
    ExecutionFailed { message: String, source: Box<dyn std::error::Error + Send> },
    ApprovalFailed { source: Box<dyn std::error::Error + Send> },
    NotFound { tool_name: String },
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::ExecutionFailed { message, source } => write!(f, "tool exec failed: {message}: {source}"),
            ToolError::ApprovalFailed { source } => write!(f, "tool approval failed: {source}"),
            ToolError::NotFound { tool_name } => write!(f, "tool not found: {tool_name}"),
        }
    }
}

impl std::error::Error for ToolError {}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str {
        ""
    }
    fn parameters(&self) -> serde_json::Value {
        serde_json::Value::Null
    }
    async fn execute(&self, args: serde_json::Value, approval: &dyn ApprovalClient) -> Result<ToolResult, ToolError>;
}

#[async_trait]
pub trait ApprovalClient: Send + Sync {
    async fn request(&self, request: ApprovalRequest) -> Result<ApprovalResponse, ToolError>;
}

pub type ToolResult = serde_json::Value;

pub struct ToolRegistry {
    tools: Vec<Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.push(tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.iter().find(|t| t.name() == name).cloned()
    }

    pub fn all(&self) -> &[Arc<dyn Tool>] {
        &self.tools
    }

    pub async fn execute(&self, name: &str, args: serde_json::Value, approval: &dyn ApprovalClient) -> Result<ToolResult, ToolError> {
        let tool = self.get(name).ok_or_else(|| ToolError::NotFound { tool_name: name.to_string() })?;
        tool.execute(args, approval).await
    }

    pub fn tool_definitions(&self) -> Vec<crate::llm::ToolDefinition> {
        self.tools
            .iter()
            .map(|t| crate::llm::ToolDefinition {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters(),
            })
            .collect()
    }
}
