use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::errors::ChatProviderError;
use crate::generate::StreamedMessage;
use crate::message::{Message, StreamedMessagePart, ToolCall};
use crate::usage::TokenUsage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    Anthropic,
    #[serde(rename = "openai")]
    OpenAi,
    Kimi,
    #[serde(rename = "google-genai")]
    GoogleGenai,
    #[serde(rename = "openai_responses")]
    OpenAiResponses,
    Vertexai,
    Deepseek,
    Glm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingEffort {
    Off,
    Low,
    Medium,
    High,
    #[serde(rename = "xhigh")]
    Xhigh,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Completed,
    ToolCalls,
    Truncated,
    Filtered,
    Paused,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelCapability {
    pub image_in: bool,
    pub video_in: bool,
    pub audio_in: bool,
    pub thinking: bool,
    pub tool_use: bool,
    pub max_context_tokens: i64,
    pub max_output_tokens: i64,
}

impl ModelCapability {
    pub fn unknown() -> Self {
        Self {
            image_in: false,
            video_in: false,
            audio_in: false,
            thinking: false,
            tool_use: false,
            max_context_tokens: 0,
            max_output_tokens: 0,
        }
    }

    pub fn is_unknown(&self) -> bool {
        !self.image_in
            && !self.video_in
            && !self.audio_in
            && !self.thinking
            && !self.tool_use
            && self.max_context_tokens == 0
            && self.max_output_tokens == 0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRequestAuth {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone)]
pub struct AbortSignal {
    aborted: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl AbortSignal {
    pub fn new() -> Self {
        Self {
            aborted: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn inner(&self) -> &std::sync::Arc<std::sync::atomic::AtomicBool> {
        &self.aborted
    }

    pub fn abort(&self) {
        self.aborted
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_aborted(&self) -> bool {
        self.aborted.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn throw_if_aborted(&self) -> Result<(), AbortedError> {
        if self.is_aborted() {
            Err(AbortedError)
        } else {
            Ok(())
        }
    }
}

/// Error returned when an operation is aborted.
#[derive(Debug)]
pub struct AbortedError;

impl std::fmt::Display for AbortedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "operation aborted")
    }
}

impl std::error::Error for AbortedError {}

impl Default for AbortSignal {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct GenerateOptions {
    pub auth: Option<ProviderRequestAuth>,
    pub signal: Option<AbortSignal>,
    pub on_request_start: Option<std::sync::Arc<dyn Fn() + Send + Sync + 'static>>,
    pub on_stream_end: Option<std::sync::Arc<dyn Fn() + Send + Sync + 'static>>,
}

impl Default for GenerateOptions {
    fn default() -> Self {
        Self {
            auth: None,
            signal: None,
            on_request_start: None,
            on_stream_end: None,
        }
    }
}

impl std::fmt::Debug for GenerateOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GenerateOptions")
            .field("auth", &self.auth)
            .field("signal", &self.signal)
            .finish()
    }
}

#[derive(Default)]
pub struct GenerateCallbacks {
    pub on_message_part: Option<Box<dyn Fn(StreamedMessagePart) + Send + Sync + 'static>>,
    pub on_tool_call: Option<Box<dyn Fn(ToolCall) + Send + Sync + 'static>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateResult {
    pub id: Option<String>,
    pub message: Message,
    pub usage: Option<TokenUsage>,
    pub finish_reason: Option<FinishReason>,
    pub raw_finish_reason: Option<String>,
}

#[async_trait]
pub trait ChatProvider: Send + Sync {
    fn name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn thinking_effort(&self) -> Option<ThinkingEffort>;

    async fn generate(
        &self,
        system_prompt: &str,
        tools: &[Tool],
        history: &[Message],
        options: Option<GenerateOptions>,
    ) -> Result<StreamedMessage, ChatProviderError>;

    fn with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider>;

    fn with_max_completion_tokens(&self, _max_tokens: i64) -> Option<Box<dyn ChatProvider>> {
        None
    }

    fn get_capability(&self, _model: Option<&str>) -> ModelCapability {
        ModelCapability::unknown()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_reason_serializes_to_snake_case() {
        assert_eq!(
            serde_json::to_string(&FinishReason::ToolCalls).unwrap(),
            "\"tool_calls\""
        );
    }

    #[test]
    fn token_usage_serializes_to_camel_case() {
        let u = TokenUsage {
            input_other: 10,
            output: 5,
            input_cache_read: 2,
            input_cache_creation: 1,
        };
        let v = serde_json::to_value(&u).unwrap();
        assert_eq!(v["inputOther"], 10);
        assert_eq!(v["inputCacheRead"], 2);
        assert_eq!(v["inputCacheCreation"], 1);
    }

    #[test]
    fn unknown_capability_is_all_false() {
        let cap = ModelCapability::unknown();
        assert!(!cap.image_in);
        assert!(!cap.thinking);
        assert_eq!(cap.max_context_tokens, 0);
    }

    #[test]
    fn provider_type_serializes_to_kebab_case() {
        let v = serde_json::to_value(ProviderType::GoogleGenai).unwrap();
        assert_eq!(v, "google-genai");
        let round: ProviderType = serde_json::from_value(v).unwrap();
        assert_eq!(round, ProviderType::GoogleGenai);
    }
}
