use crate::errors::ChatProviderError;
use crate::generate::StreamedMessage;
use crate::message::{Message, StreamedMessagePart};
use crate::provider::{ChatProvider, FinishReason, GenerateOptions, ThinkingEffort, Tool};

pub struct MockProvider {
    name: String,
    model_name: String,
    parts: Vec<StreamedMessagePart>,
    id: Option<String>,
    usage: Option<crate::usage::TokenUsage>,
    finish_reason: Option<FinishReason>,
    raw_finish_reason: Option<String>,
    thinking_effort: Option<ThinkingEffort>,
}

impl MockProvider {
    pub fn new(name: impl Into<String>, model_name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            model_name: model_name.into(),
            parts: vec![],
            id: None,
            usage: None,
            finish_reason: None,
            raw_finish_reason: None,
            thinking_effort: None,
        }
    }

    pub fn with_parts(mut self, parts: Vec<StreamedMessagePart>) -> Self {
        self.parts = parts;
        self
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn with_usage(mut self, usage: crate::usage::TokenUsage) -> Self {
        self.usage = Some(usage);
        self
    }

    pub fn with_finish_reason(mut self, reason: FinishReason) -> Self {
        self.finish_reason = Some(reason);
        self
    }

    pub fn with_raw_finish_reason(mut self, reason: impl Into<String>) -> Self {
        self.raw_finish_reason = Some(reason.into());
        self
    }
}

#[async_trait::async_trait]
impl ChatProvider for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn thinking_effort(&self) -> Option<ThinkingEffort> {
        self.thinking_effort
    }

    async fn generate(
        &self,
        _system_prompt: &str,
        _tools: &[Tool],
        _history: &[Message],
        _options: Option<GenerateOptions>,
    ) -> Result<StreamedMessage, ChatProviderError> {
        Ok(StreamedMessage::from_parts(
            self.parts.clone(),
            self.id.clone(),
            self.usage,
            self.finish_reason,
            self.raw_finish_reason.clone(),
        ))
    }

    fn with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider> {
        let mut clone = self.clone();
        clone.thinking_effort = Some(effort);
        Box::new(clone)
    }
}

impl Clone for MockProvider {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            model_name: self.model_name.clone(),
            parts: self.parts.clone(),
            id: self.id.clone(),
            usage: self.usage,
            finish_reason: self.finish_reason,
            raw_finish_reason: self.raw_finish_reason.clone(),
            thinking_effort: self.thinking_effort,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_provider_yields_configured_parts() {
        let provider = MockProvider::new("mock", "m1")
            .with_parts(vec![
                StreamedMessagePart::text("hello"),
                StreamedMessagePart::text(" world"),
            ])
            .with_finish_reason(FinishReason::Completed);

        let stream = provider.generate("sys", &[], &[], None).await.unwrap();
        let collected: Vec<_> = futures_util::StreamExt::collect::<Vec<_>>(stream).await;
        assert_eq!(collected.len(), 2);
        assert_eq!(collected[0], StreamedMessagePart::text("hello"));
        assert_eq!(collected[1], StreamedMessagePart::text(" world"));
    }
}
