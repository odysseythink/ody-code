use serde::{Deserialize, Serialize};

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
pub struct UserToolRegistration {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum McpCollisionTarget {
    #[serde(rename = "same_server")]
    SameServer {
        #[serde(rename = "toolName")]
        tool_name: String,
    },
    #[serde(rename = "other_server")]
    OtherServer {
        #[serde(rename = "serverName")]
        server_name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolCollision {
    pub qualified: String,
    pub tool_name: String,
    pub collides_with: McpCollisionTarget,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerRegistrationResult {
    pub registered: Vec<String>,
    pub collisions: Vec<McpToolCollision>,
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
        assert_eq!(
            serde_json::to_string(&ToolSource::User).unwrap(),
            "\"user\""
        );
        assert_eq!(serde_json::to_string(&ToolSource::Mcp).unwrap(), "\"mcp\"");
    }

    #[test]
    fn mcp_collision_round_trips() {
        let c = McpToolCollision {
            qualified: "mcp__a__b".into(),
            tool_name: "b".into(),
            collides_with: McpCollisionTarget::OtherServer {
                server_name: "x".into(),
            },
        };
        let json = serde_json::to_string(&c).unwrap();
        assert!(json.contains("\"toolName\""));
        assert!(json.contains("\"kind\":\"other_server\""));
        assert!(json.contains("\"serverName\""));
        let round: McpToolCollision = serde_json::from_str(&json).unwrap();
        assert_eq!(round, c);
    }
}
