use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use kosong_rs::usage::TokenUsage;

use crate::agent_loop::errors::{
    create_max_steps_exceeded_error, is_abort_error, is_max_steps_exceeded_error,
};
use crate::agent_loop::events::{LoopInterruptReason, LoopLiveOnlyEvent};
use crate::agent_loop::turn_step::execute_loop_step;
use crate::agent_loop::types::{
    LoopStepStopReason, LoopTerminalStepStopReason, LoopTurnStopReason, RecordStepUsageResult,
    RunTurnInput, StepResult, TurnResult,
};

fn empty_usage() -> TokenUsage {
    TokenUsage::default()
}

fn add_usage(a: TokenUsage, b: TokenUsage) -> TokenUsage {
    TokenUsage {
        input_other: a.input_other + b.input_other,
        output: a.output + b.output,
        input_cache_read: a.input_cache_read + b.input_cache_read,
        input_cache_creation: a.input_cache_creation + b.input_cache_creation,
    }
}

pub async fn run_turn(input: RunTurnInput) -> Result<TurnResult, anyhow::Error> {
    let RunTurnInput {
        turn_id,
        signal,
        llm,
        build_messages,
        dispatch_event,
        tools,
        hooks,
        max_steps,
        max_retry_attempts,
        record_step_usage,
    } = input;

    let usage_shared = Arc::new(Mutex::new(empty_usage()));
    let mut steps: u32 = 0;
    let mut stop_reason: LoopTurnStopReason = LoopTurnStopReason::EndTurn;
    let mut active_step: Option<u32> = None;

    let record_usage: Arc<
        dyn Fn(
                TokenUsage,
            ) -> Pin<
                Box<
                    dyn Future<Output = Result<Option<RecordStepUsageResult>, anyhow::Error>>
                        + Send,
                >,
            > + Send
            + Sync,
    > = {
        let usage_shared = usage_shared.clone();
        let host_cb = record_step_usage.clone();
        Arc::new(move |step_usage: TokenUsage| {
            let usage_shared = usage_shared.clone();
            let host_cb = host_cb.clone();
            Box::pin(async move {
                {
                    let mut u = usage_shared.lock().unwrap();
                    *u = add_usage(u.clone(), step_usage.clone());
                }
                if let Some(cb) = host_cb {
                    cb(step_usage).await
                } else {
                    Ok(None)
                }
            })
        })
    };

    let result: Result<(), anyhow::Error> = async {
        loop {
            signal.throw_if_aborted()?;

            if let Some(max) = max_steps {
                if max > 0 && steps >= max {
                    return Err(create_max_steps_exceeded_error(max).into());
                }
            }

            steps += 1;
            active_step = Some(steps);

            let StepResult {
                usage: step_usage,
                stop_reason: step_stop,
            } = execute_loop_step(crate::agent_loop::types::ExecuteLoopStepDeps {
                turn_id: turn_id.clone(),
                signal: signal.clone(),
                build_messages: build_messages.clone(),
                dispatch_event: dispatch_event.clone(),
                llm: llm.as_ref(),
                tools: tools.clone(),
                hooks: hooks.as_ref(),
                log: None,
                current_step: steps,
                max_retry_attempts,
                record_usage: record_usage.clone(),
            })
            .await?;

            active_step = None;

            if step_stop == LoopStepStopReason::ToolUse {
                continue;
            }

            let terminal: LoopTerminalStepStopReason = step_stop;
            stop_reason = terminal.into();

            let should_continue = if let Some(hooks) = &hooks {
                if let Some(hook) = &hooks.should_continue_after_stop {
                    let ctx = crate::agent_loop::types::LoopStoppedStepContext {
                        turn_id: turn_id.as_str(),
                        step_number: steps,
                        signal: signal.clone(),
                        llm: llm.as_ref(),
                        usage: step_usage,
                        stop_reason: terminal,
                    };
                    hook.should_continue_after_stop(ctx)
                        .await?
                        .map(|r| r.continue_)
                        .unwrap_or(false)
                } else {
                    false
                }
            } else {
                false
            };

            if !should_continue {
                break;
            }
        }
        Ok(())
    }
    .await;

    let usage = usage_shared.lock().unwrap().clone();

    match result {
        Ok(()) => Ok(TurnResult {
            stop_reason,
            steps,
            usage,
        }),
        Err(err) => {
            if is_abort_error(&err) || signal.is_aborted() {
                dispatch_event.dispatch_live(LoopLiveOnlyEvent::TurnInterrupted {
                    reason: LoopInterruptReason::Aborted,
                    attempted_steps: steps,
                    active_step,
                    message: None,
                });
                return Ok(TurnResult {
                    stop_reason: LoopTurnStopReason::Aborted,
                    steps,
                    usage,
                });
            }
            if is_max_steps_exceeded_error(&err) {
                dispatch_event.dispatch_live(LoopLiveOnlyEvent::TurnInterrupted {
                    reason: LoopInterruptReason::MaxSteps,
                    attempted_steps: steps,
                    active_step,
                    message: Some(err.to_string()),
                });
            } else {
                dispatch_event.dispatch_live(LoopLiveOnlyEvent::TurnInterrupted {
                    reason: LoopInterruptReason::Error,
                    attempted_steps: steps,
                    active_step,
                    message: Some(err.to_string()),
                });
            }
            Err(err)
        }
    }
}
