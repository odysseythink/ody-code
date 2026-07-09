use agent_rs::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse, ToolCallDelta};
use agent_rs::agent_loop::types::LoopStepStopReason;
use kosong_rs::message::{ContentPart, Message, Role, ToolCall};
use kosong_rs::provider::{AbortSignal, FinishReason, ModelCapability};
use kosong_rs::usage::TokenUsage;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct MockLlm {
    response: LlmChatResponse,
}

#[async_trait::async_trait]
impl Llm for MockLlm {
    fn system_prompt(&self) -> &str {
        "sys"
    }
    fn model_name(&self) -> &str {
        "mock"
    }
    fn capability(&self) -> Option<&ModelCapability> {
        None
    }

    async fn chat(&self, _params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        Ok(self.response.clone())
    }
}

#[tokio::test]
async fn mock_llm_chat_returns_tool_call_response() {
    let response = LlmChatResponse {
        tool_calls: vec![ToolCall {
            call_type: "function".into(),
            id: "tc1".into(),
            name: "echo".into(),
            arguments: Some("{\"x\":\"hi\"}".into()),
            extras: None,
            stream_index: None,
        }],
        provider_finish_reason: Some(FinishReason::ToolCalls),
        raw_finish_reason: Some("tool_calls".into()),
        usage: TokenUsage {
            input_other: 3,
            output: 2,
            input_cache_read: 0,
            input_cache_creation: 0,
        },
        stream_timing: None,
    };
    let llm = MockLlm {
        response: response.clone(),
    };
    let params = LlmChatParams {
        messages: vec![Message {
            role: Role::User,
            name: None,
            content: vec![ContentPart::Text { text: "go".into() }],
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
    };
    let got = llm.chat(params).await.unwrap();
    assert_eq!(got.tool_calls.len(), 1);
    assert_eq!(got.tool_calls[0].id, "tc1");
    assert_eq!(got.usage.output, 2);
}
