use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::capability_registry;
use crate::generate::StreamedMessage;
use crate::http_client::HttpClient;
use crate::message::Message;
use crate::openai_common::ToolMessageConversion;
use crate::openai_legacy::{OpenAILegacyChatProvider, OpenAILegacyOptions};
use crate::provider::{ChatProvider, GenerateOptions, ModelCapability, ThinkingEffort, Tool};
use crate::ChatProviderError;

#[derive(Clone)]
pub struct DeepSeekOptions {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: String,
    pub stream: Option<bool>,
    pub max_tokens: Option<i64>,
    pub reasoning_key: Option<String>,
    pub http_client: Option<Arc<dyn HttpClient>>,
    pub default_headers: Option<HashMap<String, String>>,
    pub tool_message_conversion: Option<ToolMessageConversion>,
}

pub struct DeepSeekChatProvider {
    delegate: Arc<Box<dyn ChatProvider>>,
}

impl std::fmt::Debug for DeepSeekChatProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeepSeekChatProvider")
            .field("name", &self.name())
            .field("model", &self.model_name())
            .finish_non_exhaustive()
    }
}

impl Clone for DeepSeekChatProvider {
    fn clone(&self) -> Self {
        Self {
            delegate: Arc::clone(&self.delegate),
        }
    }
}

impl DeepSeekChatProvider {
    pub fn new(options: DeepSeekOptions) -> Self {
        let api_key = options
            .api_key
            .or_else(|| std::env::var("DEEPSEEK_API_KEY").ok())
            .unwrap_or_default();
        // Explicitly pass empty string so delegate doesn't fall back to OPENAI_API_KEY
        let resolved_api_key = if api_key.is_empty() {
            "".into()
        } else {
            api_key
        };
        let base_url = options
            .base_url
            .unwrap_or_else(|| "https://api.deepseek.com/v1".into());
        let delegate = OpenAILegacyChatProvider::new(OpenAILegacyOptions {
            api_key: Some(resolved_api_key),
            base_url: Some(base_url),
            model: options.model,
            stream: options.stream,
            max_tokens: options.max_tokens,
            reasoning_key: options.reasoning_key,
            default_headers: options.default_headers,
            tool_message_conversion: options.tool_message_conversion,
            http_client: options.http_client,
        });
        Self {
            delegate: Arc::new(Box::new(delegate)),
        }
    }
}

#[async_trait]
impl ChatProvider for DeepSeekChatProvider {
    fn name(&self) -> &str {
        "deepseek"
    }

    fn model_name(&self) -> &str {
        self.delegate.model_name()
    }

    fn thinking_effort(&self) -> Option<ThinkingEffort> {
        self.delegate.thinking_effort()
    }

    fn get_capability(&self, model: Option<&str>) -> ModelCapability {
        capability_registry::get_deepseek_model_capability(model.unwrap_or(self.model_name()))
    }

    async fn generate(
        &self,
        system_prompt: &str,
        tools: &[Tool],
        history: &[Message],
        options: Option<GenerateOptions>,
    ) -> Result<StreamedMessage, ChatProviderError> {
        self.delegate
            .generate(system_prompt, tools, history, options)
            .await
    }

    fn with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider> {
        let new_delegate = self.delegate.with_thinking(effort);
        Box::new(Self {
            delegate: Arc::new(new_delegate),
        })
    }

    fn with_max_completion_tokens(
        &self,
        max_completion_tokens: i64,
    ) -> Option<Box<dyn ChatProvider>> {
        let new_delegate = self
            .delegate
            .with_max_completion_tokens(max_completion_tokens)?;
        Some(Box::new(Self {
            delegate: Arc::new(new_delegate),
        }))
    }
}

#[cfg(test)]
mod provider_shell_tests {
    use super::*;

    fn test_provider(model: &str) -> DeepSeekChatProvider {
        DeepSeekChatProvider::new(DeepSeekOptions {
            api_key: Some("sk-test".into()),
            base_url: None,
            model: model.into(),
            stream: None,
            max_tokens: None,
            reasoning_key: None,
            http_client: None,
            default_headers: None,
            tool_message_conversion: None,
        })
    }

    #[test]
    fn name_and_model() {
        let p = test_provider("deepseek-chat");
        assert_eq!(p.name(), "deepseek");
        assert_eq!(p.model_name(), "deepseek-chat");
    }

    #[test]
    fn get_capability_reasoner() {
        let cap = test_provider("deepseek-reasoner").get_capability(None);
        assert!(cap.thinking);
        assert!(!cap.tool_use);
    }

    #[test]
    fn get_capability_chat() {
        let cap = test_provider("deepseek-chat").get_capability(None);
        assert!(!cap.thinking);
        assert!(cap.tool_use);
    }

    #[test]
    fn get_capability_v4() {
        let cap = test_provider("deepseek-v4-0320").get_capability(None);
        assert!(cap.thinking);
        assert!(cap.tool_use);
        assert_eq!(cap.max_context_tokens, 1_000_000);
        assert_eq!(cap.max_output_tokens, 384_000);
    }

    #[test]
    fn get_capability_for_specific_model() {
        let cap = test_provider("deepseek-chat").get_capability(Some("deepseek-reasoner"));
        assert!(cap.thinking);
        assert!(!cap.tool_use);
    }

    #[test]
    fn with_thinking_returns_new_instance() {
        let p = test_provider("deepseek-chat");
        let q = p.with_thinking(ThinkingEffort::High);
        assert_eq!(q.thinking_effort(), Some(ThinkingEffort::High));
        // Original should be unchanged
        assert_eq!(p.thinking_effort(), None);
    }

    #[test]
    fn with_thinking_off_then_on() {
        let p = test_provider("deepseek-chat")
            .with_thinking(ThinkingEffort::High)
            .with_thinking(ThinkingEffort::Off);
        // OpenAILegacy represents Off as reasoning_effort=None, so thinking_effort() returns None
        assert_eq!(p.thinking_effort(), None);
    }

    #[test]
    fn with_max_completion_tokens_returns_provider() {
        let p = test_provider("deepseek-chat");
        let q = p.with_max_completion_tokens(1024);
        assert!(q.is_some());
        assert_eq!(q.unwrap().name(), "deepseek");
    }

    #[test]
    fn thinking_effort_none_when_not_configured() {
        assert_eq!(test_provider("deepseek-chat").thinking_effort(), None);
    }
}

#[cfg(test)]
mod generate_tests {
    use super::*;
    use crate::http_client::MockHttpClient;

    fn text_sse_bytes() -> Vec<u8> {
        br#"data: {"id":"1","choices":[{"index":0,"delta":{"role":"assistant","content":"Hello"}}]}

data: {"id":"2","choices":[{"index":0,"delta":{"content":" from DeepSeek"}}]}

data: {"id":"3","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]

"#
        .to_vec()
    }

    fn provider_with_body(status: u16, body: Vec<u8>) -> DeepSeekChatProvider {
        DeepSeekChatProvider::new(DeepSeekOptions {
            api_key: Some("sk-test".into()),
            base_url: Some("http://mock".into()),
            model: "deepseek-chat".into(),
            stream: Some(true),
            max_tokens: None,
            reasoning_key: None,
            http_client: Some(Arc::new(MockHttpClient::new(status, body))),
            default_headers: None,
            tool_message_conversion: None,
        })
    }

    #[tokio::test]
    async fn generate_delegates_to_openai_legacy_stream() {
        let provider = provider_with_body(200, text_sse_bytes());
        let stream = provider.generate("", &[], &[], None).await.unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(stream).await;
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], crate::message::StreamedMessagePart::text("Hello"));
        assert_eq!(
            parts[1],
            crate::message::StreamedMessagePart::text(" from DeepSeek")
        );
    }

    #[tokio::test]
    async fn empty_api_key_does_not_fallback_to_openai_key() {
        // Temporarily unset both env vars to ensure isolation
        std::env::remove_var("DEEPSEEK_API_KEY");
        std::env::remove_var("OPENAI_API_KEY");
        let provider = DeepSeekChatProvider::new(DeepSeekOptions {
            api_key: None,
            base_url: Some("http://mock".into()),
            model: "deepseek-chat".into(),
            stream: Some(true),
            max_tokens: None,
            reasoning_key: None,
            http_client: Some(Arc::new(MockHttpClient::new(200, vec![]))),
            default_headers: None,
            tool_message_conversion: None,
        });
        let result = provider.generate("", &[], &[], None).await;
        // Should fail because empty API key is passed — delegate should not fall back to OPENAI_API_KEY
        assert!(result.is_err());
    }
}
