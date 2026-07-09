use std::sync::{Arc, Mutex};

use agent_rs::agent_loop::events::{DefaultLoopEventDispatcher, LoopEvent, LoopRecordedEvent};
use agent_rs::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse};
use agent_rs::agent_loop::tool_access::ToolAccesses;
use agent_rs::agent_loop::tool_call::{run_tool_call_batch, ToolCallStepContext};
use agent_rs::agent_loop::types::{
    ExecutableTool, ExecutableToolOutput, ExecutableToolResult, ExecutableToolSuccessResult,
    RunnableToolExecution, ToolExecution, ToolUpdate,
};

use kosong_rs::message::ToolCall;
use kosong_rs::provider::{AbortSignal, FinishReason};
use kosong_rs::usage::TokenUsage;
use serde_json::json;

struct AddTool;

#[async_trait::async_trait]
impl ExecutableTool for AddTool {
    fn name(&self) -> &str {
        "add"
    }

    fn description(&self) -> &str {
        "adds two numbers"
    }

    fn parameters(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "a": {"type": "number"},
                "b": {"type": "number"}
            },
            "required": ["a", "b"]
        })
    }

    async fn resolve_execution(
        &self,
        input: serde_json::Value,
    ) -> Result<ToolExecution, anyhow::Error> {
        Ok(ToolExecution::Runnable(RunnableToolExecution {
            approval_rule: "auto".into(),
            execute: Box::new(move |ctx| {
                let input = input.clone();
                Box::pin(async move {
                    let a = input["a"].as_f64().unwrap_or(0.0);
                    let b = input["b"].as_f64().unwrap_or(0.0);
                    (ctx.on_update.as_ref().unwrap())(ToolUpdate {
                        kind: "text".into(),
                        text: Some(format!("{} + {} = {}", a, b, a + b)),
                        percent: None,
                        custom_kind: None,
                        custom_data: None,
                    });
                    Ok(ExecutableToolResult::Success(ExecutableToolSuccessResult {
                        output: ExecutableToolOutput::Text(format!("{}", a + b)),
                        is_error: None,
                        stop_turn: None,
                        message: None,
                    }))
                })
            }),
            is_error: None,
            accesses: Some(ToolAccesses::none()),
            description: None,
            display: None,
            stop_batch_after_this: None,
            matches_rule: None,
        }))
    }
}

struct NoopLlm;

#[async_trait::async_trait]
impl Llm for NoopLlm {
    fn system_prompt(&self) -> &str {
        ""
    }

    fn model_name(&self) -> &str {
        "noop"
    }

    async fn chat(&self, _params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        Ok(LlmChatResponse {
            tool_calls: vec![],
            provider_finish_reason: None,
            raw_finish_reason: None,
            usage: TokenUsage::default(),
            stream_timing: None,
        })
    }
}

#[tokio::test]
async fn single_tool_call_emits_paired_call_and_result() {
    let events: Arc<Mutex<Vec<LoopEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let e = events.clone();
    let dispatcher = DefaultLoopEventDispatcher::new(
        move |event: LoopRecordedEvent| {
            let e = e.clone();
            async move {
                e.lock().unwrap().push(LoopEvent::Recorded(event));
                Ok::<_, anyhow::Error>(())
            }
        },
        Some(Box::new(move |event| {
            events.lock().unwrap().push(event);
        })),
    );

    let noop = NoopLlm;
    let step_ctx = ToolCallStepContext {
        tools: Some(vec![Arc::new(AddTool) as Arc<dyn ExecutableTool>]),
        hooks: None,
        dispatch_event: Arc::new(dispatcher),
        llm: &noop,
        signal: AbortSignal::new(),
        turn_id: "t1".into(),
        current_step: 1,
        step_uuid: "s1".into(),
    };

    let response = LlmChatResponse {
        tool_calls: vec![ToolCall {
            call_type: "function".into(),
            id: "tc1".into(),
            name: "add".into(),
            arguments: Some("{\"a\":1,\"b\":2}".into()),
            extras: None,
            stream_index: None,
        }],
        provider_finish_reason: Some(FinishReason::ToolCalls),
        raw_finish_reason: Some("tool_calls".into()),
        usage: TokenUsage::default(),
        stream_timing: None,
    };

    let result = run_tool_call_batch(&step_ctx, &response).await.unwrap();
    assert!(!result.stop_turn);
}
