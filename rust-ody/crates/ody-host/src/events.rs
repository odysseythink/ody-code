use serde::{Deserialize, Serialize};

use crate::error::RpcError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AgentEvent {
    SessionCreated {
        session_id: String,
        work_dir: String,
    },
    SessionClosed {
        session_id: String,
    },
    Message {
        session_id: String,
        role: String,
        content: String,
    },
    ToolCall {
        session_id: String,
        tool_name: String,
        args: serde_json::Value,
    },
    ToolResult {
        session_id: String,
        tool_name: String,
        result: serde_json::Value,
    },
    Error {
        session_id: String,
        message: String,
    },
    Status {
        session_id: String,
        status: String,
    },
}

#[async_trait::async_trait]
pub trait EventSink: Send + Sync {
    async fn request(&self, method: &str, payload: Vec<u8>) -> Result<Vec<u8>, RpcError>;
    fn emit(&self, event: AgentEvent);
}
