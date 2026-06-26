use crate::llm::{ChatDelta, ChatRequest, FinishReason, LlmError, LlmProvider};

/// A deterministic mock provider for cross-language testing.
///
/// It echoes the last user message back as the assistant response, or a fixed
/// fallback when the request contains no user message.
pub struct MockProvider;

impl MockProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl LlmProvider for MockProvider {
    async fn chat_stream(
        &self,
        request: ChatRequest,
        on_delta: &mut (dyn FnMut(ChatDelta) + Send),
    ) -> Result<FinishReason, LlmError> {
        let content = request
            .messages
            .iter()
            .rev()
            .find(|m| matches!(m.role, crate::llm::Role::User))
            .map(|m| format!("echo: {}", m.content))
            .unwrap_or_else(|| "echo".to_string());
        on_delta(ChatDelta {
            index: 0,
            content: Some(content),
            tool_call: None,
        });
        Ok(FinishReason::Stop)
    }
}
