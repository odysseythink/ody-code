use std::sync::{Arc, Mutex};

use agent_rs::agent_loop::events::{DefaultLoopEventDispatcher, LoopEvent, LoopRecordedEvent};
use agent_rs::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse};
use agent_rs::agent_loop::run_turn::run_turn;
use agent_rs::agent_loop::types::{LoopTurnStopReason, RunTurnInput};
use kosong_rs::message::Message;
use kosong_rs::provider::{AbortSignal, FinishReason};
use kosong_rs::usage::TokenUsage;

struct SingleTextLlm;

#[async_trait::async_trait]
impl Llm for SingleTextLlm {
    fn system_prompt(&self) -> &str {
        ""
    }

    fn model_name(&self) -> &str {
        "single"
    }

    async fn chat(&self, _params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        Ok(LlmChatResponse {
            tool_calls: vec![],
            provider_finish_reason: Some(FinishReason::Completed),
            raw_finish_reason: Some("stop".into()),
            usage: TokenUsage {
                input_other: 1,
                output: 1,
                input_cache_read: 0,
                input_cache_creation: 0,
            },
            stream_timing: None,
        })
    }
}

#[tokio::test]
async fn run_turn_completes_with_end_turn() {
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

    let result = run_turn(RunTurnInput {
        turn_id: "t1".into(),
        signal: AbortSignal::new(),
        llm: Box::new(SingleTextLlm),
        build_messages: Arc::new(|| Box::pin(async { Ok(vec![Message::user_text("go")]) })),
        dispatch_event: Arc::new(dispatcher),
        tools: None,
        hooks: None,
        max_steps: Some(5),
        max_retry_attempts: Some(1),
        record_step_usage: None,
    })
    .await
    .unwrap();

    assert_eq!(result.stop_reason, LoopTurnStopReason::EndTurn);
    assert_eq!(result.steps, 1);
}
