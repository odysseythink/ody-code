use crate::llm::{ChatDelta, ChatRequest, FinishReason, LlmError, LlmProvider};

/// A deterministic mock provider for cross-language testing.
///
/// It echoes the last user message back as the assistant response, or a fixed
/// fallback when the request contains no user message. When the
/// `ODY_MOCK_RESPONSE` environment variable is set, its whitespace-separated
/// tokens are emitted as individual deltas, allowing benchmarks to inject a
/// known token count.
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
        let content = std::env::var("ODY_MOCK_RESPONSE")
            .ok()
            .or_else(|| {
                request
                    .messages
                    .iter()
                    .rev()
                    .find(|m| matches!(m.role, crate::llm::Role::User))
                    .map(|m| {
                        let text: String = m
                            .content
                            .iter()
                            .filter_map(|p| match p {
                                crate::llm::ContentPart::Text { text } => Some(text.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join(" ");
                        format!("echo: {}", text)
                    })
            })
            .unwrap_or_else(|| "echo".to_string());

        let tokens: Vec<&str> = content.split_whitespace().collect();
        if tokens.is_empty() {
            on_delta(ChatDelta {
                index: 0,
                content: Some(content),
                tool_call: None,
            });
        } else {
            for (index, token) in tokens.into_iter().enumerate() {
                on_delta(ChatDelta {
                    index,
                    content: Some(format!("{token} ")),
                    tool_call: None,
                });
            }
        }
        Ok(FinishReason::Stop)
    }
}
