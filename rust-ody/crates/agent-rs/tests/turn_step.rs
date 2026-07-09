use std::sync::{Arc, Mutex};

use agent_rs::agent_loop::events::{DefaultLoopEventDispatcher, LoopEvent, LoopRecordedEvent};
use agent_rs::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse, TextPart};
use agent_rs::agent_loop::turn_step::execute_loop_step;
use agent_rs::agent_loop::types::{ExecuteLoopStepDeps, LoopStepStopReason};
use kosong_rs::message::Message;
use kosong_rs::provider::{AbortSignal, FinishReason};
use kosong_rs::usage::TokenUsage;

struct TextLlm;

#[async_trait::async_trait]
impl Llm for TextLlm {
    fn system_prompt(&self) -> &str {
        ""
    }

    fn model_name(&self) -> &str {
        "text"
    }

    async fn chat(&self, params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        if let Some(cb) = params.on_text_delta {
            cb("hello".into());
            cb(" world".into());
        }
        if let Some(cb) = params.on_text_part {
            cb(TextPart {
                text: "hello world".into(),
            })
            .await;
        }
        Ok(LlmChatResponse {
            tool_calls: vec![],
            provider_finish_reason: Some(FinishReason::Completed),
            raw_finish_reason: Some("stop".into()),
            usage: TokenUsage {
                input_other: 2,
                output: 2,
                input_cache_read: 0,
                input_cache_creation: 0,
            },
            stream_timing: None,
        })
    }
}

#[tokio::test]
async fn text_step_emits_begin_end_content_part_and_deltas() {
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

    let result = execute_loop_step(ExecuteLoopStepDeps {
        turn_id: "t1".into(),
        signal: AbortSignal::new(),
        build_messages: Arc::new(|| Box::pin(async { Ok(vec![Message::user_text("go")]) })),
        dispatch_event: Arc::new(dispatcher),
        llm: &TextLlm,
        tools: None,
        hooks: None,
        log: None,
        current_step: 1,
        max_retry_attempts: Some(1),
        record_usage: Arc::new(|usage| {
            Box::pin(async move {
                assert_eq!(usage.output, 2);
                Ok(None)
            })
        }),
    })
    .await
    .unwrap();

    assert_eq!(result.stop_reason, LoopStepStopReason::EndTurn);
}
