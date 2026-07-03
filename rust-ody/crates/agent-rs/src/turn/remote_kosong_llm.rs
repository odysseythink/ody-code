use std::sync::Arc;

use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use kosong_rs::message::{Message, ToolCall};
use kosong_rs::provider::{FinishReason, ModelCapability};
use kosong_rs::usage::TokenUsage;
use serde::{Deserialize, Serialize};

use crate::agent_loop::llm::{
    Llm, LlmChatParams, LlmChatResponse, LlmRequestLogContext, LlmStreamTiming,
};

/// Request payload sent to the remote host to initiate a chat stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamRequest {
    pub model_name: String,
    pub system_prompt: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub capability: Option<ModelCapability>,
    pub request_log_context: Option<LlmRequestLogContext>,
    pub provider: serde_json::Value,
}

/// Model-visible tool definition crossing the RPC boundary. Runtime state such
/// as executable closures is stripped; only the schema travels over RPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Final result of a remote chat stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamResult {
    pub tool_calls: Vec<ToolCall>,
    pub provider_finish_reason: Option<FinishReason>,
    pub raw_finish_reason: Option<String>,
    pub usage: TokenUsage,
    pub stream_timing: Option<LlmStreamTiming>,
}

/// A delta emitted by the remote host during a chat stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StreamDelta {
    Text {
        text: String,
    },
    Think {
        think: String,
    },
    ToolCallPart {
        tool_call_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        arguments_part: Option<String>,
    },
}

/// Events that a remote kosong LLM client streams back to the consumer.
#[derive(Debug, Clone)]
pub enum RemoteChatStreamEvent {
    Delta(StreamDelta),
    End(ChatStreamResult),
    Error(Arc<anyhow::Error>),
}

/// Client transport for remote kosong LLM streaming. Implementations bridge to
/// the host-side LLM proxy (e.g. node-sdk RPC) and are responsible for
/// registering/unregistering stream handlers so no deltas are dropped.
#[async_trait]
pub trait RemoteKosongLlmClient: Send + Sync {
    /// Generate a unique stream id. The LLM owns the id so the local handler can
    /// be registered before the host starts emitting deltas.
    fn generate_stream_id(&self) -> String;

    /// Initiate a stream and return a channel of events from the host.
    async fn chat_stream_init(
        &self,
        request: ChatStreamRequest,
        stream_id: &str,
    ) -> Result<Box<dyn Stream<Item = RemoteChatStreamEvent> + Send + Unpin>, anyhow::Error>;

    /// Cancel an in-flight stream.
    fn chat_stream_cancel(&self, stream_id: &str);
}

pub struct RemoteKosongLLM {
    client: Box<dyn RemoteKosongLlmClient>,
    model_name: String,
    system_prompt: String,
    capability: Option<ModelCapability>,
}

impl RemoteKosongLLM {
    pub fn new(
        client: Box<dyn RemoteKosongLlmClient>,
        model_name: String,
        system_prompt: String,
        capability: Option<ModelCapability>,
    ) -> Self {
        Self {
            client,
            model_name,
            system_prompt,
            capability,
        }
    }
}

#[async_trait]
impl Llm for RemoteKosongLLM {
    fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn capability(&self) -> Option<&ModelCapability> {
        self.capability.as_ref()
    }

    fn is_retryable_error(&self, _error: &anyhow::Error) -> bool {
        // The remote host is responsible for classifying provider errors. Retry
        // signals from the host surface as explicit StreamDelta::Error events.
        false
    }

    async fn chat(&self, params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        let stream_id = self.client.generate_stream_id();
        let request = self.build_request(&params);

        params
            .signal
            .throw_if_aborted()
            .map_err(|_| anyhow::anyhow!("Stream cancelled"))?;

        let mut stream = self.client.chat_stream_init(request, &stream_id).await?;

        while let Some(event) = stream.next().await {
            params
                .signal
                .throw_if_aborted()
                .map_err(|_| anyhow::anyhow!("Stream cancelled"))?;
            match event {
                RemoteChatStreamEvent::Delta(delta) => forward_delta(&delta, &params),
                RemoteChatStreamEvent::End(result) => {
                    return Ok(self.to_llm_chat_response(result));
                }
                RemoteChatStreamEvent::Error(error) => return Err(anyhow::anyhow!("{error}")),
            }
        }

        Err(anyhow::anyhow!("Remote chat stream ended without result"))
    }
}

impl RemoteKosongLLM {
    fn build_request(&self, params: &LlmChatParams) -> ChatStreamRequest {
        ChatStreamRequest {
            model_name: self.model_name.clone(),
            system_prompt: self.system_prompt.clone(),
            messages: params.messages.clone(),
            tools: params
                .tools
                .iter()
                .map(|t| ToolDefinition {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                })
                .collect(),
            capability: self.capability.clone(),
            request_log_context: params.request_log_context.clone(),
            provider: serde_json::Value::Null,
        }
    }

    fn to_llm_chat_response(&self, result: ChatStreamResult) -> LlmChatResponse {
        LlmChatResponse {
            tool_calls: result.tool_calls,
            provider_finish_reason: result.provider_finish_reason,
            raw_finish_reason: result.raw_finish_reason,
            usage: result.usage,
            stream_timing: result.stream_timing,
        }
    }
}

fn forward_delta(delta: &StreamDelta, params: &LlmChatParams) {
    match delta {
        StreamDelta::Text { text } => {
            if let Some(cb) = &params.on_text_delta {
                cb(text.clone());
            }
        }
        StreamDelta::Think { think } => {
            if let Some(cb) = &params.on_think_delta {
                cb(think.clone());
            }
        }
        StreamDelta::ToolCallPart {
            tool_call_id,
            name,
            arguments_part,
        } => {
            if let Some(cb) = &params.on_tool_call_delta {
                cb(crate::agent_loop::llm::ToolCallDelta {
                    tool_call_id: tool_call_id.clone(),
                    name: name.clone(),
                    arguments_part: arguments_part.clone(),
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kosong_rs::message::Role;
    use kosong_rs::provider::AbortSignal;
    use std::sync::{Arc, Mutex};

    struct MockClient {
        events: Vec<RemoteChatStreamEvent>,
        stream_id_counter: Arc<Mutex<u32>>,
    }

    impl MockClient {
        fn new(events: Vec<RemoteChatStreamEvent>) -> Self {
            Self {
                events,
                stream_id_counter: Arc::new(Mutex::new(0)),
            }
        }
    }

    #[async_trait]
    impl RemoteKosongLlmClient for MockClient {
        fn generate_stream_id(&self) -> String {
            let mut guard = self.stream_id_counter.lock().unwrap();
            *guard += 1;
            format!("stream-{}", *guard)
        }

        async fn chat_stream_init(
            &self,
            _request: ChatStreamRequest,
            _stream_id: &str,
        ) -> Result<Box<dyn Stream<Item = RemoteChatStreamEvent> + Send + Unpin>, anyhow::Error>
        {
            let events = self.events.clone();
            Ok(Box::new(futures_util::stream::iter(events)))
        }

        fn chat_stream_cancel(&self, _stream_id: &str) {}
    }

    #[tokio::test]
    async fn chat_returns_end_result() {
        let client = MockClient::new(vec![RemoteChatStreamEvent::End(ChatStreamResult {
            tool_calls: vec![ToolCall {
                call_type: "function".into(),
                id: "call-1".into(),
                name: "read".into(),
                arguments: Some(r#"{"path":"/a"}"#.into()),
                extras: None,
                stream_index: None,
            }],
            provider_finish_reason: Some(FinishReason::Completed),
            raw_finish_reason: None,
            usage: TokenUsage::default(),
            stream_timing: None,
        })]);
        let llm = RemoteKosongLLM::new(Box::new(client), "m1".into(), "sys".into(), None);
        let response = llm
            .chat(LlmChatParams {
                messages: vec![Message {
                    role: Role::User,
                    name: None,
                    content: vec![kosong_rs::message::ContentPart::Text { text: "hi".into() }],
                    tool_calls: vec![],
                    tool_call_id: None,
                    partial: None,
                }],
                tools: vec![],
                signal: AbortSignal::new(),
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
    async fn chat_forwards_text_deltas() {
        let deltas = Arc::new(Mutex::new(Vec::new()));
        let deltas_cb = deltas.clone();
        let client = MockClient::new(vec![
            RemoteChatStreamEvent::Delta(StreamDelta::Text {
                text: "hello".into(),
            }),
            RemoteChatStreamEvent::Delta(StreamDelta::Text {
                text: " world".into(),
            }),
            RemoteChatStreamEvent::End(ChatStreamResult {
                tool_calls: vec![],
                provider_finish_reason: Some(FinishReason::Completed),
                raw_finish_reason: None,
                usage: TokenUsage::default(),
                stream_timing: None,
            }),
        ]);
        let llm = RemoteKosongLLM::new(Box::new(client), "m1".into(), "sys".into(), None);
        let _ = llm
            .chat(LlmChatParams {
                messages: vec![Message {
                    role: Role::User,
                    name: None,
                    content: vec![kosong_rs::message::ContentPart::Text { text: "hi".into() }],
                    tool_calls: vec![],
                    tool_call_id: None,
                    partial: None,
                }],
                tools: vec![],
                signal: AbortSignal::new(),
                request_log_context: None,
                on_text_delta: Some(Arc::new(move |s| {
                    deltas_cb.lock().unwrap().push(s);
                })),
                on_think_delta: None,
                on_tool_call_delta: None,
                on_text_part: None,
                on_think_part: None,
            })
            .await
            .unwrap();
        assert_eq!(deltas.lock().unwrap().as_slice(), &["hello", " world"]);
    }
}
