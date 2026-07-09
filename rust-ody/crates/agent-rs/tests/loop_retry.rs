use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use agent_rs::agent_loop::events::{
    DefaultLoopEventDispatcher, LoopEvent, LoopLiveOnlyEvent, LoopRecordedEvent,
};
use agent_rs::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse};
use agent_rs::agent_loop::retry::{chat_with_retry, ChatWithRetryInput};
use kosong_rs::provider::{AbortSignal, FinishReason};
use kosong_rs::usage::TokenUsage;

struct FlakyLlm {
    fails_left: AtomicUsize,
}

#[async_trait::async_trait]
impl Llm for FlakyLlm {
    fn system_prompt(&self) -> &str {
        ""
    }

    fn model_name(&self) -> &str {
        "flaky"
    }

    fn is_retryable_error(&self, err: &anyhow::Error) -> bool {
        err.to_string().contains("retryable")
    }

    async fn chat(&self, _params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        let left = self.fails_left.fetch_sub(1, Ordering::SeqCst);
        if left > 0 {
            Err(anyhow::anyhow!("retryable failure"))
        } else {
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
}

#[tokio::test]
async fn retry_succeeds_after_one_failure_and_emits_retrying_event() {
    let llm = FlakyLlm {
        fails_left: AtomicUsize::new(1),
    };
    let live = Arc::new(std::sync::Mutex::new(Vec::new()));
    let l = live.clone();
    let dispatcher = DefaultLoopEventDispatcher::new(
        |_event: LoopRecordedEvent| async move { Ok::<_, anyhow::Error>(()) },
        Some(Box::new(move |event| {
            if let LoopEvent::Live(LoopLiveOnlyEvent::StepRetrying(ev)) = event {
                l.lock().unwrap().push(ev);
            }
        })),
    );
    let input = ChatWithRetryInput {
        llm: &llm,
        params: LlmChatParams {
            messages: vec![],
            tools: vec![],
            signal: AbortSignal::new(),
            request_log_context: None,
            on_text_delta: None,
            on_think_delta: None,
            on_tool_call_delta: None,
            on_text_part: None,
            on_think_part: None,
        },
        dispatch_event: Arc::new(dispatcher),
        turn_id: "t1".into(),
        current_step: 1,
        step_uuid: "s1".into(),
        max_attempts: Some(3),
    };
    let response = chat_with_retry(input).await.unwrap();
    assert_eq!(
        response.provider_finish_reason,
        Some(FinishReason::Completed)
    );
    let retry_events = live.lock().unwrap();
    assert_eq!(retry_events.len(), 1);
    assert_eq!(retry_events[0].failed_attempt, 1);
    assert_eq!(retry_events[0].next_attempt, 2);
}
