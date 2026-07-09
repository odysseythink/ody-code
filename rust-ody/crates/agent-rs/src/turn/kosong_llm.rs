use std::sync::Arc;

use async_trait::async_trait;
use kosong_rs::generate::generate;
use kosong_rs::message::{ContentPart, StreamedMessagePart};
use kosong_rs::provider::{ChatProvider, GenerateCallbacks, GenerateOptions, ModelCapability};

use crate::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse, LlmStreamTiming};

/// Optional completion-budget configuration. Not yet wired into provider selection
/// in Rust; kept for API parity with the TypeScript implementation.
#[derive(Debug, Clone, Default)]
pub struct CompletionBudgetConfig {
    /// Maximum output tokens allowed for a single request.
    pub max_tokens: Option<i64>,
}

pub struct KosongLLMConfig {
    pub provider: Box<dyn ChatProvider>,
    pub model_name: String,
    pub system_prompt: String,
    pub capability: Option<ModelCapability>,
    pub completion_budget_config: Option<CompletionBudgetConfig>,
}

pub struct KosongLLM {
    provider: Box<dyn ChatProvider>,
    model_name: String,
    system_prompt: String,
    capability: Option<ModelCapability>,
    _completion_budget_config: Option<CompletionBudgetConfig>,
}

impl KosongLLM {
    pub fn new(config: KosongLLMConfig) -> Self {
        Self {
            provider: config.provider,
            model_name: config.model_name,
            system_prompt: config.system_prompt,
            capability: config.capability,
            _completion_budget_config: config.completion_budget_config,
        }
    }
}

#[async_trait]
impl Llm for KosongLLM {
    fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn capability(&self) -> Option<&ModelCapability> {
        self.capability.as_ref()
    }

    fn is_retryable_error(&self, error: &anyhow::Error) -> bool {
        if let Some(chat_provider_error) =
            error.downcast_ref::<kosong_rs::errors::ChatProviderError>()
        {
            kosong_rs::errors::is_retryable_generate_error(chat_provider_error)
        } else {
            false
        }
    }

    async fn chat(&self, params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        let request_started_at = Arc::new(std::sync::Mutex::new(now_ms()));
        let first_chunk_at: Arc<std::sync::Mutex<Option<u64>>> =
            Arc::new(std::sync::Mutex::new(None));
        let stream_ended_at: Arc<std::sync::Mutex<Option<u64>>> =
            Arc::new(std::sync::Mutex::new(None));

        let mark_request_start = {
            let started = request_started_at.clone();
            move || {
                *started.lock().unwrap() = now_ms();
            }
        };

        let mark_stream_end = {
            let ended = stream_ended_at.clone();
            move || {
                *ended.lock().unwrap() = Some(now_ms());
            }
        };

        let mark_stream_output = {
            let first = first_chunk_at.clone();
            move || {
                let mut guard = first.lock().unwrap();
                if guard.is_none() {
                    *guard = Some(now_ms());
                }
            }
        };

        let callbacks = build_kosong_callbacks(
            params.on_text_delta.clone(),
            params.on_think_delta.clone(),
            params.on_tool_call_delta.clone(),
            mark_stream_output,
        );

        let options = GenerateOptions {
            signal: Some(params.signal.clone()),
            on_request_start: Some(Arc::new(mark_request_start)),
            on_stream_end: Some(Arc::new(mark_stream_end)),
            ..Default::default()
        };

        let result = generate(
            self.provider.as_ref(),
            &self.system_prompt,
            &params.tools,
            &params.messages,
            Some(&callbacks),
            Some(&options),
        )
        .await?;

        // Replay merged content parts onto loop per-block callbacks after the stream drained.
        if params.on_text_part.is_some() || params.on_think_part.is_some() {
            for part in &result.message.content {
                match part {
                    ContentPart::Text { text } => {
                        if let Some(cb) = &params.on_text_part {
                            cb(crate::agent_loop::llm::TextPart { text: text.clone() }).await;
                        }
                    }
                    ContentPart::Think { think, encrypted } => {
                        if let Some(cb) = &params.on_think_part {
                            cb(crate::agent_loop::llm::ThinkPart {
                                think: think.clone(),
                                encrypted: encrypted.clone(),
                            })
                            .await;
                        }
                    }
                    _ => {}
                }
            }
        }

        let request_started_at = *request_started_at.lock().unwrap();
        let first_chunk_at = *first_chunk_at.lock().unwrap();
        let stream_ended_at = *stream_ended_at.lock().unwrap();

        Ok(LlmChatResponse {
            tool_calls: result.message.tool_calls,
            provider_finish_reason: result.finish_reason,
            raw_finish_reason: result.raw_finish_reason,
            usage: result.usage.unwrap_or_default(),
            stream_timing: build_stream_timing(request_started_at, first_chunk_at, stream_ended_at),
        })
    }
}

fn build_stream_timing(
    request_started_at: u64,
    first_chunk_at: Option<u64>,
    stream_ended_at: Option<u64>,
) -> Option<LlmStreamTiming> {
    let first_chunk_at = first_chunk_at?;
    let output_ended_at = stream_ended_at.unwrap_or_else(now_ms);
    Some(LlmStreamTiming {
        first_token_latency_ms: first_chunk_at.saturating_sub(request_started_at),
        stream_duration_ms: output_ended_at.saturating_sub(first_chunk_at),
    })
}

fn build_kosong_callbacks(
    on_text_delta: Option<crate::agent_loop::llm::TextDeltaCallback>,
    on_think_delta: Option<crate::agent_loop::llm::ThinkDeltaCallback>,
    on_tool_call_delta: Option<crate::agent_loop::llm::ToolCallDeltaCallback>,
    mark_stream_output: impl Fn() + Send + Sync + Clone + 'static,
) -> GenerateCallbacks {
    GenerateCallbacks {
        on_message_part: Some(Box::new(move |part: StreamedMessagePart| {
            mark_stream_output();
            dispatch_part(&part, &on_text_delta, &on_think_delta, &on_tool_call_delta);
        })),
        on_tool_call: None,
    }
}

fn dispatch_part(
    part: &StreamedMessagePart,
    on_text_delta: &Option<crate::agent_loop::llm::TextDeltaCallback>,
    on_think_delta: &Option<crate::agent_loop::llm::ThinkDeltaCallback>,
    on_tool_call_delta: &Option<crate::agent_loop::llm::ToolCallDeltaCallback>,
) {
    match part {
        StreamedMessagePart::Content(ContentPart::Text { text }) => {
            if let Some(cb) = on_text_delta {
                cb(text.clone());
            }
        }
        StreamedMessagePart::Content(ContentPart::Think { think, .. }) => {
            if let Some(cb) = on_think_delta {
                cb(think.clone());
            }
        }
        StreamedMessagePart::ToolCall(tc) => {
            if let Some(cb) = on_tool_call_delta {
                cb(crate::agent_loop::llm::ToolCallDelta {
                    tool_call_id: tc.id.clone(),
                    name: Some(tc.name.clone()),
                    arguments_part: tc.arguments.clone(),
                });
            }
        }
        StreamedMessagePart::ToolCallPart(tc) => {
            if let Some(cb) = on_tool_call_delta {
                // Recover the tool identity from buffered state if available. The kosong
                // generator merges argument deltas into the matching ToolCall, so a lone
                // ToolCallPart without an identity is best-effort forwarded.
                cb(crate::agent_loop::llm::ToolCallDelta {
                    tool_call_id: String::new(),
                    name: None,
                    arguments_part: tc.arguments_part.clone(),
                });
            }
        }
        StreamedMessagePart::Content(_) => {
            // Image, audio, and video URL parts are not forwarded as streaming deltas.
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use kosong_rs::message::{Message, Role};
    use kosong_rs::mock::MockProvider;

    #[tokio::test]
    async fn chat_returns_tool_calls_from_provider() {
        let provider =
            MockProvider::new("mock", "m1").with_parts(vec![StreamedMessagePart::tool_call(
                "call-1",
                "read",
                Some(r#"{"path":"/a"}"#),
            )]);
        let llm = KosongLLM::new(KosongLLMConfig {
            provider: Box::new(provider),
            model_name: "m1".into(),
            system_prompt: "sys".into(),
            capability: None,
            completion_budget_config: None,
        });
        let response = llm
            .chat(LlmChatParams {
                messages: vec![Message {
                    role: Role::User,
                    name: None,
                    content: vec![ContentPart::Text { text: "hi".into() }],
                    tool_calls: vec![],
                    tool_call_id: None,
                    partial: None,
                }],
                tools: vec![],
                signal: kosong_rs::provider::AbortSignal::new(),
                request_log_context: None,
                on_text_delta: None,
                on_think_delta: None,
                on_tool_call_delta: None,
                on_text_part: None,
                on_think_part: None,
            })
            .await
            .unwrap();
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "call-1");
    }

    #[tokio::test]
    async fn is_retryable_error_detects_retryable_chat_provider_errors() {
        let provider = MockProvider::new("mock", "m1");
        let llm = KosongLLM::new(KosongLLMConfig {
            provider: Box::new(provider),
            model_name: "m1".into(),
            system_prompt: "sys".into(),
            capability: None,
            completion_budget_config: None,
        });
        let err = anyhow::anyhow!(kosong_rs::errors::ChatProviderError::Connection(
            kosong_rs::errors::APIConnectionError,
        ));
        assert!(llm.is_retryable_error(&err));
    }
}
