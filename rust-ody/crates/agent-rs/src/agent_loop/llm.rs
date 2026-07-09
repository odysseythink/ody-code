use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use kosong_rs::message::{Message, ToolCall};
use kosong_rs::provider::{AbortSignal, FinishReason, ModelCapability, Tool};
use kosong_rs::usage::TokenUsage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default)]
pub struct ToolCallDelta {
    pub tool_call_id: String,
    pub name: Option<String>,
    pub arguments_part: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TextPart {
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct ThinkPart {
    pub think: String,
    pub encrypted: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmRequestLogContext {
    pub turn_id: Option<String>,
    pub step: Option<u32>,
    pub step_uuid: Option<String>,
    pub attempt: Option<u32>,
    pub max_attempts: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmStreamTiming {
    pub first_token_latency_ms: u64,
    pub stream_duration_ms: u64,
}

pub type TextDeltaCallback = Arc<dyn Fn(String) + Send + Sync>;
pub type ThinkDeltaCallback = Arc<dyn Fn(String) + Send + Sync>;
pub type ToolCallDeltaCallback = Arc<dyn Fn(ToolCallDelta) + Send + Sync>;
pub type TextPartCallback =
    Arc<dyn Fn(TextPart) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;
pub type ThinkPartCallback =
    Arc<dyn Fn(ThinkPart) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

#[derive(Clone)]
pub struct LlmChatParams {
    pub messages: Vec<Message>,
    pub tools: Vec<Tool>,
    pub signal: AbortSignal,
    pub request_log_context: Option<LlmRequestLogContext>,
    pub on_text_delta: Option<TextDeltaCallback>,
    pub on_think_delta: Option<ThinkDeltaCallback>,
    pub on_tool_call_delta: Option<ToolCallDeltaCallback>,
    pub on_text_part: Option<TextPartCallback>,
    pub on_think_part: Option<ThinkPartCallback>,
}

#[derive(Debug, Clone, Default)]
pub struct LlmChatResponse {
    pub tool_calls: Vec<ToolCall>,
    pub provider_finish_reason: Option<FinishReason>,
    pub raw_finish_reason: Option<String>,
    pub usage: TokenUsage,
    pub stream_timing: Option<LlmStreamTiming>,
}

#[derive(Debug, Clone, Default)]
pub struct LlmFactoryConfig {
    pub model_name: String,
    pub system_prompt: String,
    pub capability: Option<ModelCapability>,
}

#[async_trait::async_trait]
pub trait Llm: Send + Sync {
    fn system_prompt(&self) -> &str;
    fn model_name(&self) -> &str;
    fn capability(&self) -> Option<&ModelCapability> {
        None
    }
    fn is_retryable_error(&self, _error: &anyhow::Error) -> bool {
        false
    }
    async fn chat(&self, params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error>;
}
