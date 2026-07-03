use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// Re-export the records-layer payload so ToolManager and the WAL use the same type.
pub use crate::records::nested::UserToolRegistration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolSource {
    Builtin,
    User,
    Mcp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub active: bool,
    pub source: ToolSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolCollision {
    pub qualified: String,
    pub tool_name: String,
    pub collides_with: McpCollisionTarget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum McpCollisionTarget {
    SameServer { tool_name: String },
    OtherServer { server_name: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpServerRegistrationResult {
    pub registered: Vec<String>,
    pub collisions: Vec<McpToolCollision>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutableTool {
    pub name: String,
    pub description: String,
    pub parameters: JsonValue,
}

#[derive(Debug, Clone)]
pub struct BuiltinToolProvisionContext {
    pub agent_type: crate::agent::AgentType,
    pub model_capabilities: kosong_rs::provider::ModelCapability,
    pub homedir: Option<PathBuf>,
    pub goal_command_enabled: bool,
    pub rpc_open_external: bool,
    pub rpc_request_question: bool,
    pub background_available: bool,
    pub cron_available: bool,
    pub has_invocable_skills: bool,
    pub subagent_host_available: bool,
    pub web_searcher_available: bool,
    pub url_fetcher_available: bool,
}

impl Default for BuiltinToolProvisionContext {
    fn default() -> Self {
        Self {
            agent_type: crate::agent::AgentType::Main,
            model_capabilities: kosong_rs::provider::ModelCapability::unknown(),
            homedir: None,
            goal_command_enabled: false,
            rpc_open_external: false,
            rpc_request_question: false,
            background_available: false,
            cron_available: false,
            has_invocable_skills: false,
            subagent_host_available: false,
            web_searcher_available: false,
            url_fetcher_available: false,
        }
    }
}

pub trait BuiltinToolsProvider: Send + Sync {
    fn provide(
        &self,
        ctx: BuiltinToolProvisionContext,
    ) -> Vec<Arc<dyn crate::agent_loop::types::ExecutableTool>>;
}

#[async_trait::async_trait]
impl crate::agent_loop::types::ExecutableTool for ExecutableTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> serde_json::Value {
        self.parameters.clone()
    }

    async fn resolve_execution(
        &self,
        _input: serde_json::Value,
    ) -> Result<crate::agent_loop::types::ToolExecution, anyhow::Error> {
        Ok(crate::agent_loop::types::ToolExecution::Error(
            crate::records::nested::ExecutableToolErrorResult {
                output: crate::records::nested::ExecutableToolOutput::Text(
                    "Tool execution is not wired for this stub".into(),
                ),
                is_error: true,
                stop_turn: None,
                message: Some("Tool execution is not wired for this stub".into()),
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_source_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&ToolSource::Builtin).unwrap(),
            "\"builtin\""
        );
        assert_eq!(serde_json::to_string(&ToolSource::Mcp).unwrap(), "\"mcp\"");
    }

    #[test]
    fn mcp_collision_uses_camel_case_and_tag() {
        let collision = McpToolCollision {
            qualified: "mcp__a__b".into(),
            tool_name: "b".into(),
            collides_with: McpCollisionTarget::OtherServer {
                server_name: "x".into(),
            },
        };
        let json = serde_json::to_string(&collision).unwrap();
        assert!(json.contains("\"toolName\""));
        assert!(json.contains("\"kind\":\"other_server\""));
        assert!(json.contains("\"server_name\""));

        let round: McpToolCollision = serde_json::from_str(&json).unwrap();
        assert_eq!(round, collision);
    }
}
