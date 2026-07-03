use serde::{Deserialize, Serialize};

pub use kosong_rs::ContentPart;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tools: Vec<ToolDefinition>,
    pub stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentPart>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Default)]
pub struct ChatDelta {
    pub index: usize,
    pub content: Option<String>,
    pub tool_call: Option<ToolCallDelta>,
}

#[derive(Debug, Clone, Default)]
pub struct ToolCallDelta {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishReason {
    Stop,
    ToolCalls,
    Length,
    ContentFilter,
    Other,
}

#[derive(Debug)]
pub enum LlmError {
    ApiError { status: u16, body: String },
    StreamParse { message: String },
    RequestFailed { source: reqwest::Error },
    Provider { message: String },
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmError::ApiError { status, body } => write!(f, "LLM API error {status}: {body}"),
            LlmError::StreamParse { message } => write!(f, "LLM stream parse error: {message}"),
            LlmError::RequestFailed { source } => write!(f, "LLM request failed: {source}"),
            LlmError::Provider { message } => write!(f, "LLM provider error: {message}"),
        }
    }
}

impl std::error::Error for LlmError {}

#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat_stream(
        &self,
        request: ChatRequest,
        on_delta: &mut (dyn FnMut(ChatDelta) + Send),
    ) -> Result<FinishReason, LlmError>;
}

pub mod chat_provider_adapter;
pub mod mock;
pub mod openai;
