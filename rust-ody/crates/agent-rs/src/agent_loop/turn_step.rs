use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use kosong_rs::message::ContentPart;
use kosong_rs::provider::{AbortSignal, FinishReason};
use uuid::Uuid;

use crate::agent_loop::events::{LoopEventDispatcher, LoopLiveOnlyEvent, LoopRecordedEvent};
use crate::agent_loop::llm::{LlmChatParams, LlmChatResponse, TextPart, ThinkPart, ToolCallDelta};
use crate::agent_loop::retry::chat_with_retry;
use crate::agent_loop::tool_call::{run_tool_call_batch, ToolCallStepContext};
use crate::agent_loop::types::{
    AfterStepResult, BeforeStepResult, ExecuteLoopStepDeps, LoopStepHookContext,
    LoopStepStopReason, StepResult,
};

pub async fn execute_loop_step(deps: ExecuteLoopStepDeps<'_>) -> Result<StepResult, anyhow::Error> {
    let ExecuteLoopStepDeps {
        turn_id,
        signal,
        build_messages,
        dispatch_event,
        llm,
        tools,
        hooks,
        log: _,
        current_step,
        max_retry_attempts,
        record_usage,
    } = deps;

    if let Some(hooks) = hooks {
        if let Some(before) = &hooks.before_step {
            let ctx = LoopStepHookContext {
                turn_id: turn_id.as_str(),
                step_number: current_step,
                signal: signal.clone(),
                llm,
            };
            match before.before_step(ctx).await {
                Ok(Some(BeforeStepResult {
                    block: Some(true),
                    reason,
                })) => {
                    return Err(anyhow::anyhow!(
                        "{}",
                        reason.unwrap_or_else(|| format!("Step {} was blocked", current_step))
                    ));
                }
                Ok(_) => {}
                Err(e) => return Err(e),
            }
        }
    }

    signal.throw_if_aborted()?;
    let messages = build_messages().await?;
    signal.throw_if_aborted()?;

    let step_uuid = Uuid::new_v4().to_string();
    dispatch_event
        .dispatch_recorded(LoopRecordedEvent::StepBegin {
            uuid: step_uuid.clone(),
            turn_id: turn_id.clone(),
            step: current_step as i64,
        })
        .await?;

    let chat_params = LlmChatParams {
        messages,
        tools: tools
            .as_ref()
            .map(|t| {
                t.iter()
                    .map(|tool| kosong_rs::provider::Tool {
                        name: tool.name().into(),
                        description: tool.description().into(),
                        parameters: tool.parameters().clone(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        signal: signal.clone(),
        request_log_context: None,
        ..create_streaming_callbacks(
            dispatch_event.clone(),
            turn_id.clone(),
            current_step,
            step_uuid.clone(),
        )
    };

    let response = chat_with_retry(crate::agent_loop::retry::ChatWithRetryInput {
        llm,
        params: chat_params,
        dispatch_event: dispatch_event.clone(),
        turn_id: turn_id.clone(),
        current_step,
        step_uuid: step_uuid.clone(),
        max_attempts: max_retry_attempts,
    })
    .await?;

    let usage = response.usage.clone();
    let usage_result = record_usage(usage.clone()).await?;
    let stop_turn_after_usage = usage_result.and_then(|r| r.stop_turn).unwrap_or(false);
    let stop_reason = derive_step_stop_reason(&response);

    let mut effective_stop_reason =
        if stop_turn_after_usage && stop_reason == LoopStepStopReason::ToolUse {
            LoopStepStopReason::EndTurn
        } else {
            stop_reason
        };

    if effective_stop_reason == LoopStepStopReason::ToolUse {
        let step_ctx = ToolCallStepContext {
            tools,
            hooks,
            dispatch_event: dispatch_event.clone(),
            llm,
            signal: signal.clone(),
            turn_id: turn_id.clone(),
            current_step,
            step_uuid: step_uuid.clone(),
        };
        let batch_result = run_tool_call_batch(&step_ctx, &response).await?;
        if batch_result.stop_turn {
            effective_stop_reason = LoopStepStopReason::EndTurn;
        }
    }

    signal.throw_if_aborted()?;

    let diagnostics = step_end_provider_diagnostics(&response, effective_stop_reason);
    dispatch_event
        .dispatch_recorded(LoopRecordedEvent::StepEnd {
            uuid: step_uuid.clone(),
            turn_id: turn_id.clone(),
            step: current_step as i64,
            usage: Some(usage.clone()),
            finish_reason: Some(
                serde_json::to_string(&effective_stop_reason)
                    .unwrap()
                    .trim_matches('"')
                    .to_string(),
            ),
            llm_first_token_latency_ms: response
                .stream_timing
                .as_ref()
                .map(|t| t.first_token_latency_ms as i64),
            llm_stream_duration_ms: response
                .stream_timing
                .as_ref()
                .map(|t| t.stream_duration_ms as i64),
            provider_finish_reason: diagnostics.provider_finish_reason.map(|f| {
                serde_json::to_string(&f)
                    .unwrap()
                    .trim_matches('"')
                    .to_string()
            }),
            raw_finish_reason: diagnostics.raw_finish_reason.clone(),
        })
        .await?;

    let mut stop_turn_after_step = stop_turn_after_usage;
    if let Some(hooks) = hooks {
        if let Some(after) = &hooks.after_step {
            let ctx = crate::agent_loop::types::LoopAfterStepContext {
                turn_id: turn_id.as_str(),
                step_number: current_step,
                signal: signal.clone(),
                llm,
                usage: usage.clone(),
                stop_reason: effective_stop_reason,
            };
            match after.after_step(ctx).await {
                Ok(Some(AfterStepResult {
                    stop_turn: Some(true),
                })) => stop_turn_after_step = true,
                _ => {}
            }
        }
    }

    Ok(StepResult {
        usage,
        stop_reason: if stop_turn_after_step && effective_stop_reason == LoopStepStopReason::ToolUse
        {
            LoopStepStopReason::EndTurn
        } else {
            effective_stop_reason
        },
    })
}

fn derive_step_stop_reason(response: &LlmChatResponse) -> LoopStepStopReason {
    use LoopStepStopReason::*;
    match response.provider_finish_reason {
        Some(FinishReason::Truncated) => MaxTokens,
        Some(FinishReason::Filtered) => Filtered,
        Some(FinishReason::Paused) => Paused,
        Some(FinishReason::Other) => Unknown,
        Some(FinishReason::Completed) | None => {
            if response.tool_calls.is_empty() {
                EndTurn
            } else {
                ToolUse
            }
        }
        Some(FinishReason::ToolCalls) => {
            if response.tool_calls.is_empty() {
                Unknown
            } else {
                ToolUse
            }
        }
    }
}

#[derive(Default)]
struct StepEndDiagnostics {
    provider_finish_reason: Option<FinishReason>,
    raw_finish_reason: Option<String>,
}

fn step_end_provider_diagnostics(
    response: &LlmChatResponse,
    stop_reason: LoopStepStopReason,
) -> StepEndDiagnostics {
    let provider = response.provider_finish_reason;
    if matches!(
        (provider, stop_reason),
        (Some(FinishReason::Completed), LoopStepStopReason::EndTurn)
            | (Some(FinishReason::ToolCalls), LoopStepStopReason::ToolUse)
    ) {
        return StepEndDiagnostics::default();
    }
    StepEndDiagnostics {
        provider_finish_reason: provider,
        raw_finish_reason: response.raw_finish_reason.clone(),
    }
}

fn create_streaming_callbacks(
    dispatch_event: Arc<dyn LoopEventDispatcher>,
    turn_id: String,
    current_step: u32,
    step_uuid: String,
) -> LlmChatParams {
    let dispatch_event_text = dispatch_event.clone();
    let on_text_delta: std::sync::Arc<dyn Fn(String) + Send + Sync> =
        std::sync::Arc::new(move |delta| {
            dispatch_event_text.dispatch_live(LoopLiveOnlyEvent::TextDelta { delta });
        });

    let dispatch_event_think = dispatch_event.clone();
    let on_think_delta: std::sync::Arc<dyn Fn(String) + Send + Sync> =
        std::sync::Arc::new(move |delta| {
            dispatch_event_think.dispatch_live(LoopLiveOnlyEvent::ThinkingDelta { delta });
        });

    let dispatch_event_tool = dispatch_event.clone();
    let on_tool_call_delta: std::sync::Arc<dyn Fn(ToolCallDelta) + Send + Sync> =
        std::sync::Arc::new(move |delta| {
            dispatch_event_tool.dispatch_live(LoopLiveOnlyEvent::ToolCallDelta {
                tool_call_id: delta.tool_call_id,
                name: delta.name,
                arguments_part: delta.arguments_part,
            });
        });

    let dispatch_event_text_part = dispatch_event.clone();
    let turn_id_text_part = turn_id.clone();
    let step_uuid_text_part = step_uuid.clone();
    let on_text_part: std::sync::Arc<
        dyn Fn(TextPart) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync,
    > = std::sync::Arc::new(move |part| {
        let dispatch_event = dispatch_event_text_part.clone();
        let turn_id = turn_id_text_part.clone();
        let step_uuid = step_uuid_text_part.clone();
        Box::pin(async move {
            dispatch_event
                .dispatch_recorded(LoopRecordedEvent::ContentPartEvent {
                    uuid: Uuid::new_v4().to_string(),
                    turn_id,
                    step: current_step as i64,
                    step_uuid,
                    part: ContentPart::Text { text: part.text },
                })
                .await
                .ok();
        })
    });

    let dispatch_event_think_part = dispatch_event.clone();
    let turn_id_think_part = turn_id.clone();
    let step_uuid_think_part = step_uuid.clone();
    let on_think_part: std::sync::Arc<
        dyn Fn(ThinkPart) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync,
    > = std::sync::Arc::new(move |part| {
        let dispatch_event = dispatch_event_think_part.clone();
        let turn_id = turn_id_think_part.clone();
        let step_uuid = step_uuid_think_part.clone();
        Box::pin(async move {
            dispatch_event
                .dispatch_recorded(LoopRecordedEvent::ContentPartEvent {
                    uuid: Uuid::new_v4().to_string(),
                    turn_id,
                    step: current_step as i64,
                    step_uuid,
                    part: ContentPart::Think {
                        think: part.think,
                        encrypted: part.encrypted,
                    },
                })
                .await
                .ok();
        })
    });

    LlmChatParams {
        messages: vec![],
        tools: vec![],
        signal: AbortSignal::new(),
        request_log_context: None,
        on_text_delta: Some(on_text_delta),
        on_think_delta: Some(on_think_delta),
        on_tool_call_delta: Some(on_tool_call_delta),
        on_text_part: Some(on_text_part),
        on_think_part: Some(on_think_part),
    }
}
