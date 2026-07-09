use std::sync::Arc;
use std::time::Duration;

use kosong_rs::provider::AbortSignal;
use tokio::time::sleep;

use crate::agent_loop::events::{LoopEventDispatcher, LoopLiveOnlyEvent};
use crate::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse, LlmRequestLogContext};

pub const DEFAULT_MAX_RETRY_ATTEMPTS: u32 = 3;
const RETRY_MIN_TIMEOUT_MS: u64 = 300;
const RETRY_MAX_TIMEOUT_MS: u64 = 5000;
const RETRY_FACTOR: u64 = 2;

pub struct ChatWithRetryInput<'a> {
    pub llm: &'a dyn Llm,
    pub params: LlmChatParams,
    pub dispatch_event: Arc<dyn LoopEventDispatcher>,
    pub turn_id: String,
    pub current_step: u32,
    pub step_uuid: String,
    pub max_attempts: Option<u32>,
}

pub async fn chat_with_retry(
    input: ChatWithRetryInput<'_>,
) -> Result<LlmChatResponse, anyhow::Error> {
    let max_attempts = input.max_attempts.unwrap_or(DEFAULT_MAX_RETRY_ATTEMPTS);
    let effective_max = std::cmp::max(max_attempts, 1);

    if effective_max <= 1 {
        let params = params_for_attempt(&input, 1, effective_max);
        return input.llm.chat(params).await;
    }

    let delays = retry_backoff_delays(effective_max);

    for attempt in 1..=effective_max {
        let params = params_for_attempt(&input, attempt, effective_max);
        match input.llm.chat(params).await {
            Ok(response) => return Ok(response),
            Err(error) => {
                if attempt >= effective_max || !input.llm.is_retryable_error(&error) {
                    return Err(error);
                }
                input
                    .params
                    .signal
                    .throw_if_aborted()
                    .map_err(|_| anyhow::anyhow!("Aborted"))?;
                let delay_ms = delays.get((attempt - 1) as usize).copied().unwrap_or(0);
                input
                    .dispatch_event
                    .dispatch_live(LoopLiveOnlyEvent::StepRetrying(
                        crate::agent_loop::events::StepRetryingEvent {
                            turn_id: input.turn_id.clone(),
                            step: input.current_step,
                            step_uuid: input.step_uuid.clone(),
                            failed_attempt: attempt,
                            next_attempt: attempt + 1,
                            max_attempts: effective_max,
                            delay_ms,
                            error_name: error.root_cause().to_string(),
                            error_message: error.to_string(),
                            status_code: status_code_from_error(&error),
                        },
                    ));
                sleep_for_retry(delay_ms, &input.params.signal).await?;
            }
        }
    }

    unreachable!()
}

fn params_for_attempt(
    input: &ChatWithRetryInput<'_>,
    attempt: u32,
    max_attempts: u32,
) -> LlmChatParams {
    let mut params = input.params.clone();
    params.request_log_context = Some(LlmRequestLogContext {
        turn_id: Some(input.turn_id.clone()),
        step: Some(input.current_step),
        step_uuid: Some(input.step_uuid.clone()),
        attempt: Some(attempt),
        max_attempts: Some(max_attempts),
    });
    params
}

fn status_code_from_error(error: &anyhow::Error) -> Option<i32> {
    use kosong_rs::errors::ChatProviderError;
    error
        .downcast_ref::<ChatProviderError>()
        .and_then(|e| match e {
            ChatProviderError::Status(s) => Some(s.status_code as i32),
            ChatProviderError::ContextOverflow(c) => Some(c.status_code as i32),
            _ => None,
        })
}

pub fn retry_backoff_delays(max_attempts: u32) -> Vec<u64> {
    let mut delays = Vec::new();
    let mut current = RETRY_MIN_TIMEOUT_MS;
    for _ in 0..max_attempts.saturating_sub(1) {
        delays.push(std::cmp::min(current, RETRY_MAX_TIMEOUT_MS));
        current *= RETRY_FACTOR;
    }
    delays
}

pub async fn sleep_for_retry(delay_ms: u64, signal: &AbortSignal) -> Result<(), anyhow::Error> {
    if signal.is_aborted() {
        return Err(anyhow::anyhow!("Aborted"));
    }
    let sleep_future = sleep(Duration::from_millis(delay_ms));
    tokio::pin!(sleep_future);
    loop {
        tokio::select! {
            _ = &mut sleep_future => return Ok(()),
            _ = tokio::task::yield_now() => {
                if signal.is_aborted() {
                    return Err(anyhow::anyhow!("Aborted"));
                }
            }
        }
    }
}
