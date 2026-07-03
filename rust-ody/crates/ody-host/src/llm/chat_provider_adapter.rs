use kosong_rs::provider::ChatProvider;
use kosong_rs::{
    ContentPart, GenerateOptions, Message as KosongMessage, Role as KosongRole,
    StreamedMessagePart, Tool as KosongTool,
};

use crate::llm::{ChatDelta, ChatRequest, FinishReason, LlmError, LlmProvider, ToolCallDelta};

pub struct ChatProviderLlmAdapter {
    inner: Box<dyn ChatProvider>,
}

impl ChatProviderLlmAdapter {
    pub fn new(inner: Box<dyn ChatProvider>) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl LlmProvider for ChatProviderLlmAdapter {
    async fn chat_stream(
        &self,
        request: ChatRequest,
        on_delta: &mut (dyn FnMut(ChatDelta) + Send),
    ) -> Result<FinishReason, LlmError> {
        let system_prompt = ""; // ody-host currently does not carry system prompt in ChatRequest
        let tools: Vec<KosongTool> = request
            .tools
            .into_iter()
            .map(|t| KosongTool {
                name: t.name,
                description: t.description,
                parameters: t.parameters,
            })
            .collect();
        let history: Vec<KosongMessage> = request
            .messages
            .into_iter()
            .map(|m| KosongMessage {
                role: match m.role {
                    crate::llm::Role::System => KosongRole::System,
                    crate::llm::Role::User => KosongRole::User,
                    crate::llm::Role::Assistant => KosongRole::Assistant,
                },
                name: None,
                content: m.content,
                tool_calls: Vec::new(),
                tool_call_id: None,
                partial: None,
            })
            .collect();

        let mut stream = self
            .inner
            .generate(
                system_prompt,
                &tools,
                &history,
                Some(GenerateOptions::default()),
            )
            .await
            .map_err(|e| LlmError::Provider {
                message: e.to_string(),
            })?;

        use futures_util::StreamExt;
        while let Some(part) = stream.next().await {
            match part {
                StreamedMessagePart::Content(ContentPart::Text { text }) => {
                    on_delta(ChatDelta {
                        index: 0,
                        content: Some(text),
                        tool_call: None,
                    });
                }
                StreamedMessagePart::ToolCall(tool_call) => {
                    on_delta(ChatDelta {
                        index: 0,
                        content: None,
                        tool_call: Some(ToolCallDelta {
                            id: tool_call.id,
                            name: tool_call.name,
                            arguments: tool_call
                                .arguments
                                .and_then(|s| serde_json::from_str(&s).ok())
                                .unwrap_or(serde_json::Value::Null),
                        }),
                    });
                }
                _ => {}
            }
        }

        Ok(map_finish_reason(stream.finish_reason()))
    }
}

fn map_finish_reason(reason: Option<kosong_rs::provider::FinishReason>) -> FinishReason {
    match reason {
        Some(kosong_rs::provider::FinishReason::Completed) => FinishReason::Stop,
        Some(kosong_rs::provider::FinishReason::ToolCalls) => FinishReason::ToolCalls,
        Some(kosong_rs::provider::FinishReason::Truncated) => FinishReason::Length,
        Some(kosong_rs::provider::FinishReason::Filtered) => FinishReason::ContentFilter,
        Some(kosong_rs::provider::FinishReason::Paused) => FinishReason::Other,
        Some(kosong_rs::provider::FinishReason::Other) => FinishReason::Other,
        None => FinishReason::Stop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{ChatRequest, ContentPart, LlmProvider, Message, Role};
    use kosong_rs::{MockProvider, StreamedMessagePart};

    #[tokio::test]
    async fn adapter_forwards_text_and_tool_call() {
        let chat = MockProvider::new("mock", "m1").with_parts(vec![
            StreamedMessagePart::text("hello"),
            StreamedMessagePart::tool_call("read_1", "read", Some(r#"{"path":"/tmp"}"#)),
        ]);
        let provider = ChatProviderLlmAdapter::new(Box::new(chat));
        let request = ChatRequest {
            model: "m1".into(),
            messages: vec![Message {
                role: Role::User,
                content: vec![ContentPart::Text { text: "hi".into() }],
            }],
            tools: vec![],
            stream: true,
        };
        let mut deltas = Vec::new();
        let reason = provider
            .chat_stream(request, &mut |d| deltas.push(d.clone()))
            .await
            .unwrap();
        assert_eq!(deltas.len(), 2);
        assert_eq!(deltas[0].content.as_deref(), Some("hello"));
        assert_eq!(deltas[1].tool_call.as_ref().unwrap().name, "read");
        assert!(matches!(reason, FinishReason::Stop));
    }

    #[tokio::test]
    async fn adapter_preserves_multimedia_content() {
        use kosong_rs::message::UrlPayload;
        use kosong_rs::provider::{ChatProvider, GenerateOptions, Tool};
        use std::sync::{Arc, Mutex};

        struct CaptureProvider {
            history: Arc<Mutex<Vec<KosongMessage>>>,
        }

        #[async_trait::async_trait]
        impl ChatProvider for CaptureProvider {
            fn name(&self) -> &str {
                "capture"
            }
            fn model_name(&self) -> &str {
                "m1"
            }
            fn thinking_effort(&self) -> Option<kosong_rs::provider::ThinkingEffort> {
                None
            }
            fn get_capability(&self, _model: Option<&str>) -> kosong_rs::provider::ModelCapability {
                kosong_rs::provider::ModelCapability::unknown()
            }
            async fn generate(
                &self,
                _system_prompt: &str,
                _tools: &[Tool],
                history: &[KosongMessage],
                _options: Option<GenerateOptions>,
            ) -> Result<kosong_rs::generate::StreamedMessage, kosong_rs::ChatProviderError>
            {
                *self.history.lock().unwrap() = history.to_vec();
                Ok(kosong_rs::MockProvider::new("capture", "m1")
                    .with_parts(vec![StreamedMessagePart::text("ok")])
                    .generate("", &[], &[], None)
                    .await?)
            }
            fn with_thinking(
                &self,
                _effort: kosong_rs::provider::ThinkingEffort,
            ) -> Box<dyn ChatProvider> {
                Box::new(CaptureProvider {
                    history: Arc::clone(&self.history),
                })
            }
            fn with_max_completion_tokens(&self, _max: i64) -> Option<Box<dyn ChatProvider>> {
                Some(Box::new(CaptureProvider {
                    history: Arc::clone(&self.history),
                }))
            }
        }

        let history = Arc::new(Mutex::new(Vec::new()));
        let provider = ChatProviderLlmAdapter::new(Box::new(CaptureProvider {
            history: Arc::clone(&history),
        }));
        let request = ChatRequest {
            model: "m1".into(),
            messages: vec![Message {
                role: Role::User,
                content: vec![
                    ContentPart::Text {
                        text: "describe".into(),
                    },
                    ContentPart::ImageUrl {
                        image_url: UrlPayload {
                            url: "https://example.com/img.png".into(),
                            id: None,
                        },
                    },
                ],
            }],
            tools: vec![],
            stream: true,
        };
        let _ = provider.chat_stream(request, &mut |_d| {}).await.unwrap();

        let captured = history.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].content.len(), 2);
        assert!(matches!(
            captured[0].content[1],
            kosong_rs::ContentPart::ImageUrl { .. }
        ));
    }
}
