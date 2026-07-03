use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use kosong_rs::message::{ContentPart, ToolCall};
use kosong_rs::provider::AbortSignal;
use lazy_static::lazy_static;
use serde_json::Value as JsonValue;

use crate::agent_loop::errors::error_message;
use crate::agent_loop::events::{LoopEventDispatcher, LoopLiveOnlyEvent, LoopRecordedEvent};
use crate::agent_loop::llm::Llm;
use crate::agent_loop::tool_access::{PathSecurityError, ToolAccesses};
use crate::agent_loop::tool_scheduler::{ToolCallTask, ToolScheduler};
use crate::agent_loop::types::{
    AuthorizeToolExecutionResult, ExecutableTool, ExecutableToolContext, ExecutableToolErrorResult,
    ExecutableToolOutput, ExecutableToolResult, ExecutableToolSuccessResult,
    FinalizeToolResultContext, LoopHooks, PrepareToolExecutionResult,
    ResolvedToolExecutionHookContext, RunnableToolExecution, ToolExecution,
    ToolExecutionHookContext,
};

const GRACE_TIMEOUT_MS: u64 = 2_000;
const TOOL_OUTPUT_EMPTY: &str = "Tool output is empty.";
const TOOL_OUTPUT_NON_TEXT: &str = "Tool returned non-text content.";

lazy_static! {
    static ref VALIDATORS: Mutex<HashMap<String, jsonschema::Validator>> =
        Mutex::new(HashMap::new());
}

pub struct ToolCallStepContext<'a> {
    pub tools: Option<Vec<Arc<dyn ExecutableTool>>>,
    pub hooks: Option<&'a LoopHooks>,
    pub dispatch_event: Arc<dyn LoopEventDispatcher>,
    pub llm: &'a dyn Llm,
    pub signal: AbortSignal,
    pub turn_id: String,
    pub current_step: u32,
    pub step_uuid: String,
}

#[derive(Debug, Clone, Default)]
pub struct ToolCallBatchResult {
    pub stop_turn: bool,
}

pub async fn run_tool_call_batch(
    step: &ToolCallStepContext<'_>,
    response: &crate::agent_loop::llm::LlmChatResponse,
) -> Result<ToolCallBatchResult, anyhow::Error> {
    if response.tool_calls.is_empty() {
        return Ok(ToolCallBatchResult { stop_turn: false });
    }

    let calls: Vec<PreflightedToolCall> = response
        .tool_calls
        .iter()
        .map(|tc| preflight_tool_call(step.tools.as_deref(), tc))
        .collect();

    let mut scheduler: ToolScheduler<Result<PendingToolResult, anyhow::Error>> =
        ToolScheduler::new();
    let mut pending_results: Vec<
        tokio::sync::oneshot::Receiver<Result<PendingToolResult, anyhow::Error>>,
    > = Vec::new();
    let mut stop_turn = false;

    for (index, call) in calls.into_iter().enumerate() {
        let prepared = prepare_tool_call(step, call).await?;
        pending_results.push(scheduler.add(prepared.task).await?);

        if prepared.stop_batch_after_this == Some(true) {
            stop_turn = true;
            for skipped in response.tool_calls.iter().skip(index + 1) {
                let skipped_preflight = preflight_tool_call(step.tools.as_deref(), skipped);
                let skipped_task = prepare_skipped_tool_call(step, skipped_preflight).await?;
                pending_results.push(scheduler.add(skipped_task).await?);
            }
            break;
        }
    }

    for pending in pending_results {
        let result = pending.await??;
        let finalized = finalize_pending_tool_result(step, result).await?;
        if finalized.stop_turn == Some(true) {
            stop_turn = true;
        }
        dispatch_tool_result_event(step, &finalized).await?;
    }

    Ok(ToolCallBatchResult { stop_turn })
}

enum PreflightedToolCall {
    Runnable {
        tool_call: ToolCall,
        tool_name: String,
        args: JsonValue,
    },
    Rejected {
        tool_call: ToolCall,
        tool_name: String,
        args: JsonValue,
        output: String,
    },
}

struct PendingToolResult {
    tool_call: ToolCall,
    tool_name: String,
    args: JsonValue,
    result: ExecutableToolResult,
    stop_turn: Option<bool>,
}

struct PreparedToolCallTask {
    task: ToolCallTask<Result<PendingToolResult, anyhow::Error>>,
    stop_batch_after_this: Option<bool>,
}

type ToolCallDisplayFields = (Option<String>, Option<JsonValue>);

fn preflight_tool_call(
    tools: Option<&[Arc<dyn ExecutableTool>]>,
    tool_call: &ToolCall,
) -> PreflightedToolCall {
    let tool_name = tool_call.name.clone();
    let parsed = parse_tool_call_arguments(&tool_call.arguments);
    let args = parsed
        .as_ref()
        .map(|v| v.clone())
        .unwrap_or_else(|_| JsonValue::Object(Default::default()));
    let tool = tools.and_then(|list| list.iter().find(|t| t.name() == tool_name));

    match tool {
        None => {
            let output = format!(r#"Tool "{}" not found"#, tool_name);
            PreflightedToolCall::Rejected {
                tool_call: tool_call.clone(),
                tool_name,
                args,
                output,
            }
        }
        Some(tool) => {
            if let Err(err) = parsed {
                let output = format!(
                    r#"Invalid args for tool "{}": malformed JSON in arguments: {}"#,
                    tool_name, err
                );
                return PreflightedToolCall::Rejected {
                    tool_call: tool_call.clone(),
                    tool_name,
                    args,
                    output,
                };
            }
            let data = parsed.unwrap();
            if let Some(err) = validate_executable_tool_args(tool.as_ref(), &data) {
                let output = format!(r#"Invalid args for tool "{}": {}"#, tool_name, err);
                return PreflightedToolCall::Rejected {
                    tool_call: tool_call.clone(),
                    tool_name,
                    args: data,
                    output,
                };
            }
            PreflightedToolCall::Runnable {
                tool_call: tool_call.clone(),
                tool_name,
                args: data,
            }
        }
    }
}

fn parse_tool_call_arguments(raw: &Option<String>) -> Result<JsonValue, String> {
    match raw {
        None => Ok(JsonValue::Object(Default::default())),
        Some(s) if s.is_empty() => Ok(JsonValue::Object(Default::default())),
        Some(s) => serde_json::from_str(s).map_err(|e| error_message(&anyhow::anyhow!(e))),
    }
}

fn validate_executable_tool_args(tool: &dyn ExecutableTool, args: &JsonValue) -> Option<String> {
    let params = tool.parameters();
    let key = format!(
        "{}:{}",
        tool.name(),
        serde_json::to_string(&params).unwrap_or_default()
    );
    let mut cache = VALIDATORS.lock().unwrap();
    let schema = cache.entry(key).or_insert_with(|| {
        jsonschema::validator_for(&params).expect("tool parameter schema compiles")
    });
    let errors: Vec<_> = schema.iter_errors(args).collect();
    if errors.is_empty() {
        return None;
    }
    Some(format_validation_errors(&errors))
}

fn format_validation_errors(errors: &[jsonschema::ValidationError<'_>]) -> String {
    errors
        .iter()
        .map(|e| {
            let path = e.instance_path.to_string();
            if path.is_empty() {
                e.to_string()
            } else {
                format!("validation error at {}: {}", path, e)
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

async fn prepare_tool_call(
    step: &ToolCallStepContext<'_>,
    call: PreflightedToolCall,
) -> Result<PreparedToolCallTask, anyhow::Error> {
    async fn settle_error(
        step: &ToolCallStepContext<'_>,
        call: &PreflightedToolCall,
        args: JsonValue,
        output: String,
        display: ToolCallDisplayFields,
    ) -> Result<PreparedToolCallTask, anyhow::Error> {
        dispatch_tool_call_event(step, call, &args, display).await?;
        Ok(PreparedToolCallTask {
            task: make_resolved_task(make_error_tool_result(call, args, output)),
            stop_batch_after_this: None,
        })
    }

    async fn settle_synthetic(
        step: &ToolCallStepContext<'_>,
        call: &PreflightedToolCall,
        args: JsonValue,
        result: ExecutableToolResult,
        display: ToolCallDisplayFields,
    ) -> Result<PreparedToolCallTask, anyhow::Error> {
        let coerced = coerce_tool_result(result, call.tool_name());
        let stops = tool_result_stops_turn(&coerced);
        dispatch_tool_call_event(step, call, &args, display).await?;
        Ok(PreparedToolCallTask {
            task: make_resolved_task(make_tool_result(call, args, coerced)),
            stop_batch_after_this: Some(stops),
        })
    }

    if let PreflightedToolCall::Rejected { output, .. } = &call {
        return settle_error(
            step,
            &call,
            call.args().clone(),
            output.clone(),
            (None, None),
        )
        .await;
    }

    let decision = run_prepare_tool_execution_hook(step, &call).await?;
    if decision.block == Some(true) {
        return settle_error(
            step,
            &call,
            decision
                .updated_args
                .clone()
                .unwrap_or_else(|| call.args().clone()),
            decision
                .reason
                .unwrap_or_else(|| format!(r#"Tool call "{}" was blocked"#, call.tool_name())),
            (None, None),
        )
        .await;
    }
    if let Some(synthetic) = decision.synthetic_result {
        return settle_synthetic(
            step,
            &call,
            decision
                .updated_args
                .clone()
                .unwrap_or_else(|| call.args().clone()),
            synthetic,
            (None, None),
        )
        .await;
    }

    let effective_args = decision
        .updated_args
        .clone()
        .unwrap_or_else(|| call.args().clone());
    let tool =
        find_tool(step.tools.as_deref(), call.tool_name()).expect("runnable tool must exist");
    if let Some(err) = validate_executable_tool_args(tool, &effective_args) {
        return settle_error(
            step,
            &call,
            effective_args,
            format!(
                r#"Invalid args for tool "{}" after prepareToolExecution hook: {}"#,
                call.tool_name(),
                err
            ),
            (None, None),
        )
        .await;
    }

    if step.signal.is_aborted() {
        return settle_error(
            step,
            &call,
            effective_args,
            aborted_tool_output(call.tool_name(), &step.signal),
            (None, None),
        )
        .await;
    }

    let execution = match tool.resolve_execution(effective_args.clone()).await {
        Ok(ToolExecution::Runnable(exec)) => exec,
        Ok(ToolExecution::Error(result)) => {
            return settle_synthetic(
                step,
                &call,
                effective_args,
                ExecutableToolResult::Error(result),
                (None, None),
            )
            .await;
        }
        Err(error) => {
            let output = if let Some(pse) = error.downcast_ref::<PathSecurityError>() {
                pse.to_string()
            } else {
                format!(
                    r#"Tool "{}" failed to resolve execution: {}"#,
                    call.tool_name(),
                    error_message(&error)
                )
            };
            return settle_error(step, &call, effective_args, output, (None, None)).await;
        }
    };

    let display = tool_call_display_fields_from_execution(&execution);
    if step.signal.is_aborted() {
        return settle_error(
            step,
            &call,
            effective_args,
            aborted_tool_output(call.tool_name(), &step.signal),
            display,
        )
        .await;
    }
    if execution.is_error == Some(true) {
        let result = ExecutableToolResult::Error(ExecutableToolErrorResult {
            output: ExecutableToolOutput::Text(format!(
                r#"Tool "{}" returned an error execution"#,
                call.tool_name()
            )),
            is_error: true,
            stop_turn: None,
            message: None,
        });
        return settle_synthetic(step, &call, effective_args, result, display).await;
    }

    let authorization =
        run_authorize_tool_execution_hook(step, &call, &effective_args, &execution).await?;
    if step.signal.is_aborted() {
        return settle_error(
            step,
            &call,
            effective_args,
            aborted_tool_output(call.tool_name(), &step.signal),
            display,
        )
        .await;
    }
    if authorization.block == Some(true) {
        return settle_error(
            step,
            &call,
            effective_args,
            authorization
                .reason
                .unwrap_or_else(|| format!(r#"Tool call "{}" was blocked"#, call.tool_name())),
            display,
        )
        .await;
    }
    if let Some(synthetic) = authorization.synthetic_result {
        return settle_synthetic(step, &call, effective_args, synthetic, display).await;
    }

    dispatch_tool_call_event(step, &call, &effective_args, display.clone()).await?;
    let metadata = authorization
        .execution_metadata
        .or(decision.execution_metadata);
    let accesses = execution.accesses.clone().unwrap_or_else(ToolAccesses::all);
    let stop_batch = execution.stop_batch_after_this;
    let tool_call_id = call.tool_call().id.clone();
    let execute_fn = execution.execute;
    let dispatch_event = step.dispatch_event.clone();
    let turn_id = step.turn_id.clone();
    let signal = step.signal.clone();
    let tool_name = call.tool_name().to_string();
    Ok(PreparedToolCallTask {
        task: ToolCallTask {
            accesses,
            start: Box::new(move || {
                Box::pin(run_runnable_tool_call(
                    turn_id.clone(),
                    signal.clone(),
                    dispatch_event.clone(),
                    tool_call_id.clone(),
                    tool_name.clone(),
                    effective_args.clone(),
                    metadata.clone(),
                    execute_fn,
                ))
                    as Pin<
                        Box<dyn Future<Output = Result<PendingToolResult, anyhow::Error>> + Send>,
                    >
            }),
        },
        stop_batch_after_this: stop_batch,
    })
}

async fn prepare_skipped_tool_call(
    step: &ToolCallStepContext<'_>,
    call: PreflightedToolCall,
) -> Result<ToolCallTask<Result<PendingToolResult, anyhow::Error>>, anyhow::Error> {
    let output = "Tool skipped because a previous tool call stopped the turn.";
    dispatch_tool_call_event(step, &call, &call.args().clone(), (None, None)).await?;
    Ok(make_resolved_task(make_error_tool_result(
        &call,
        call.args().clone(),
        output.into(),
    )))
}

fn make_resolved_task(
    result: PendingToolResult,
) -> ToolCallTask<Result<PendingToolResult, anyhow::Error>> {
    ToolCallTask {
        accesses: ToolAccesses::none(),
        start: Box::new(move || Box::pin(async move { Ok(result) })),
    }
}

async fn run_prepare_tool_execution_hook(
    step: &ToolCallStepContext<'_>,
    call: &PreflightedToolCall,
) -> Result<PrepareToolExecutionResult, anyhow::Error> {
    let args = call.args();
    let tool_call = call.tool_call();
    let tool_name = call.tool_name();

    let hook = match step.hooks.and_then(|h| h.prepare_tool_execution.as_ref()) {
        Some(h) => h,
        None => return Ok(PrepareToolExecutionResult::default()),
    };

    let ctx = ToolExecutionHookContext {
        turn_id: step.turn_id.as_str(),
        step_number: step.current_step,
        signal: step.signal.clone(),
        llm: step.llm,
        tool_call,
        tool: find_tool(step.tools.as_deref(), tool_name),
        args: args.clone(),
    };

    match hook.prepare_tool_execution(ctx).await {
        Ok(Some(result)) => Ok(result),
        Ok(None) => Ok(PrepareToolExecutionResult::default()),
        Err(error) => {
            if step.signal.is_aborted() {
                Ok(PrepareToolExecutionResult {
                    block: Some(true),
                    reason: Some(format!(
                        r#"Tool "{}" was aborted during prepareToolExecution hook"#,
                        tool_name
                    )),
                    ..Default::default()
                })
            } else {
                Ok(PrepareToolExecutionResult {
                    block: Some(true),
                    reason: Some(format!(
                        r#"prepareToolExecution hook failed for "{}": {}"#,
                        tool_name,
                        error_message(&error)
                    )),
                    ..Default::default()
                })
            }
        }
    }
}

async fn run_authorize_tool_execution_hook(
    step: &ToolCallStepContext<'_>,
    call: &PreflightedToolCall,
    args: &JsonValue,
    execution: &RunnableToolExecution,
) -> Result<AuthorizeToolExecutionResult, anyhow::Error> {
    let hook = match step.hooks.and_then(|h| h.authorize_tool_execution.as_ref()) {
        Some(h) => h,
        None => return Ok(AuthorizeToolExecutionResult::default()),
    };

    let ctx = ResolvedToolExecutionHookContext {
        turn_id: step.turn_id.as_str(),
        step_number: step.current_step,
        signal: step.signal.clone(),
        llm: step.llm,
        tool_call: call.tool_call(),
        tool: find_tool(step.tools.as_deref(), call.tool_name()),
        args: args.clone(),
        execution,
    };

    match hook.authorize_tool_execution(ctx).await {
        Ok(Some(result)) => Ok(result),
        Ok(None) => Ok(AuthorizeToolExecutionResult::default()),
        Err(error) => {
            if step.signal.is_aborted() {
                Ok(AuthorizeToolExecutionResult {
                    block: Some(true),
                    reason: Some(format!(
                        r#"Tool "{}" was aborted during authorizeToolExecution hook"#,
                        call.tool_name()
                    )),
                    ..Default::default()
                })
            } else {
                Ok(AuthorizeToolExecutionResult {
                    block: Some(true),
                    reason: Some(format!(
                        r#"authorizeToolExecution hook failed for "{}": {}"#,
                        call.tool_name(),
                        error_message(&error)
                    )),
                    ..Default::default()
                })
            }
        }
    }
}

async fn run_finalize_tool_result_hook(
    step: &ToolCallStepContext<'_>,
    pending: &PendingToolResult,
) -> Result<ExecutableToolResult, anyhow::Error> {
    let hook = match step.hooks.and_then(|h| h.finalize_tool_result.as_ref()) {
        Some(h) => h,
        None => {
            return Ok(coerce_tool_result(
                pending.result.clone(),
                &pending.tool_name,
            ))
        }
    };

    let ctx = FinalizeToolResultContext {
        turn_id: step.turn_id.as_str(),
        step_number: step.current_step,
        signal: step.signal.clone(),
        llm: step.llm,
        tool_call: &pending.tool_call,
        tool: find_tool(step.tools.as_deref(), &pending.tool_name),
        args: pending.args.clone(),
        result: pending.result.clone(),
    };

    match hook.finalize_tool_result(ctx).await {
        Ok(Some(finalized)) => {
            let coerced = coerce_tool_result(finalized, &pending.tool_name);
            Ok(normalize_tool_result(coerced))
        }
        Ok(None) => Ok(normalize_tool_result(coerce_tool_result(
            pending.result.clone(),
            &pending.tool_name,
        ))),
        Err(error) => {
            let output = if step.signal.is_aborted() {
                format!(
                    r#"Tool "{}" aborted during finalizeToolResult hook."#,
                    pending.tool_name
                )
            } else {
                format!(
                    r#"finalizeToolResult hook failed for "{}": {}"#,
                    pending.tool_name,
                    error_message(&error)
                )
            };
            Ok(ExecutableToolResult::Error(ExecutableToolErrorResult {
                output: ExecutableToolOutput::Text(output),
                is_error: true,
                stop_turn: None,
                message: None,
            }))
        }
    }
}

async fn finalize_pending_tool_result(
    step: &ToolCallStepContext<'_>,
    pending: PendingToolResult,
) -> Result<PendingToolResult, anyhow::Error> {
    let finalized = run_finalize_tool_result_hook(step, &pending).await?;
    Ok(PendingToolResult {
        stop_turn: Some(pending.stop_turn == Some(true) || tool_result_stops_turn(&finalized)),
        result: finalized,
        ..pending
    })
}

async fn run_runnable_tool_call(
    turn_id: String,
    signal: AbortSignal,
    dispatch_event: Arc<dyn LoopEventDispatcher>,
    tool_call_id: String,
    tool_name: String,
    args: JsonValue,
    metadata: Option<JsonValue>,
    execute: Box<
        dyn Fn(
                ExecutableToolContext,
            )
                -> Pin<Box<dyn Future<Output = Result<ExecutableToolResult, anyhow::Error>> + Send>>
            + Send
            + Sync,
    >,
) -> Result<PendingToolResult, anyhow::Error> {
    if signal.is_aborted() {
        let output = aborted_tool_output(&tool_name, &signal);
        return Ok(make_error_tool_result_from_ids(
            tool_call_id,
            tool_name,
            args,
            output,
        ));
    }

    let dispatch_event_update = dispatch_event.clone();
    let tool_call_id_update = tool_call_id.clone();
    let ctx = ExecutableToolContext {
        turn_id,
        tool_call_id: tool_call_id.clone(),
        metadata,
        signal: signal.clone(),
        on_update: Some(Box::new(move |update| {
            dispatch_event_update.dispatch_live(LoopLiveOnlyEvent::ToolProgress {
                tool_call_id: tool_call_id_update.clone(),
                update,
            });
        })),
    };

    let result =
        match race_execute_with_grace_timeout(execute(ctx), signal.clone(), &tool_name).await {
            Ok(raw) => coerce_tool_result(raw, &tool_name),
            Err(error) => {
                let aborted = error
                    .downcast_ref::<crate::agent_loop::errors::LoopError>()
                    .map(|e| e.is_abort())
                    .unwrap_or(false)
                    || signal.is_aborted();
                let output = if aborted {
                    aborted_tool_output(&tool_name, &signal)
                } else {
                    format!(r#"Tool "{}" failed: {}"#, tool_name, error_message(&error))
                };
                ExecutableToolResult::Error(ExecutableToolErrorResult {
                    output: ExecutableToolOutput::Text(output),
                    is_error: true,
                    stop_turn: None,
                    message: None,
                })
            }
        };

    Ok(make_tool_result_from_ids(
        tool_call_id,
        tool_name,
        args,
        result,
    ))
}

async fn race_execute_with_grace_timeout(
    execute: Pin<Box<dyn Future<Output = Result<ExecutableToolResult, anyhow::Error>> + Send>>,
    signal: AbortSignal,
    tool_name: &str,
) -> Result<ExecutableToolResult, anyhow::Error> {
    if signal.is_aborted() {
        return Ok(error_result(aborted_tool_output(tool_name, &signal)));
    }

    let mut execute = execute;
    let abort_fut = async {
        loop {
            if signal.is_aborted() {
                tokio::time::sleep(Duration::from_millis(GRACE_TIMEOUT_MS)).await;
                return error_result(aborted_tool_output(tool_name, &signal));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    tokio::pin!(abort_fut);

    tokio::select! {
        res = &mut execute => res,
        res = &mut abort_fut => Ok(res),
    }
}

fn error_result(output: String) -> ExecutableToolResult {
    ExecutableToolResult::Error(ExecutableToolErrorResult {
        output: ExecutableToolOutput::Text(output),
        is_error: true,
        stop_turn: None,
        message: None,
    })
}

fn coerce_tool_result(value: ExecutableToolResult, _tool_name: &str) -> ExecutableToolResult {
    value
}

fn normalize_tool_result(r: ExecutableToolResult) -> ExecutableToolResult {
    let output = match &r {
        ExecutableToolResult::Success(s) => normalize_output(&s.output),
        ExecutableToolResult::Error(e) => normalize_output(&e.output),
    };
    match r {
        ExecutableToolResult::Success(s) => {
            ExecutableToolResult::Success(ExecutableToolSuccessResult {
                output,
                is_error: s.is_error,
                stop_turn: s.stop_turn,
                message: s.message,
            })
        }
        ExecutableToolResult::Error(e) => ExecutableToolResult::Error(ExecutableToolErrorResult {
            output,
            is_error: true,
            stop_turn: e.stop_turn,
            message: e.message,
        }),
    }
}

fn normalize_output(output: &ExecutableToolOutput) -> ExecutableToolOutput {
    match output {
        ExecutableToolOutput::Text(s) => {
            if s.is_empty() {
                ExecutableToolOutput::Text(TOOL_OUTPUT_EMPTY.into())
            } else {
                ExecutableToolOutput::Text(s.clone())
            }
        }
        ExecutableToolOutput::Parts(parts) => {
            if parts.is_empty() {
                return ExecutableToolOutput::Text(TOOL_OUTPUT_EMPTY.into());
            }
            if parts.iter().all(|p| is_media_content_part(p)) {
                if parts
                    .iter()
                    .any(|p| matches!(p, ContentPart::Text { text } if !text.is_empty()))
                {
                    ExecutableToolOutput::Parts(parts.clone())
                } else {
                    let mut out = vec![ContentPart::Text {
                        text: TOOL_OUTPUT_NON_TEXT.into(),
                    }];
                    out.extend(parts.iter().cloned());
                    ExecutableToolOutput::Parts(out)
                }
            } else {
                let joined: String = parts
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
                if joined.is_empty() {
                    ExecutableToolOutput::Text(TOOL_OUTPUT_EMPTY.into())
                } else {
                    ExecutableToolOutput::Text(joined)
                }
            }
        }
    }
}

fn is_media_content_part(part: &ContentPart) -> bool {
    matches!(
        part,
        ContentPart::ImageUrl { .. } | ContentPart::AudioUrl { .. } | ContentPart::VideoUrl { .. }
    )
}

fn tool_result_stops_turn(result: &ExecutableToolResult) -> bool {
    match result {
        ExecutableToolResult::Success(s) => s.stop_turn == Some(true),
        ExecutableToolResult::Error(e) => e.stop_turn == Some(true),
    }
}

fn make_tool_result(
    call: &PreflightedToolCall,
    args: JsonValue,
    result: ExecutableToolResult,
) -> PendingToolResult {
    PendingToolResult {
        tool_call: call.tool_call().clone(),
        tool_name: call.tool_name().into(),
        args,
        stop_turn: Some(tool_result_stops_turn(&result)),
        result,
    }
}

fn make_tool_result_from_ids(
    tool_call_id: String,
    tool_name: String,
    args: JsonValue,
    result: ExecutableToolResult,
) -> PendingToolResult {
    PendingToolResult {
        tool_call: ToolCall {
            call_type: "function".into(),
            id: tool_call_id,
            name: tool_name.clone(),
            arguments: None,
            extras: None,
            stream_index: None,
        },
        tool_name,
        args,
        stop_turn: Some(tool_result_stops_turn(&result)),
        result,
    }
}

fn make_error_tool_result(
    call: &PreflightedToolCall,
    args: JsonValue,
    output: String,
) -> PendingToolResult {
    make_tool_result(
        call,
        args,
        ExecutableToolResult::Error(ExecutableToolErrorResult {
            output: ExecutableToolOutput::Text(output),
            is_error: true,
            stop_turn: None,
            message: None,
        }),
    )
}

fn make_error_tool_result_from_ids(
    tool_call_id: String,
    tool_name: String,
    args: JsonValue,
    output: String,
) -> PendingToolResult {
    make_tool_result_from_ids(
        tool_call_id,
        tool_name,
        args,
        ExecutableToolResult::Error(ExecutableToolErrorResult {
            output: ExecutableToolOutput::Text(output),
            is_error: true,
            stop_turn: None,
            message: None,
        }),
    )
}

fn aborted_tool_output(tool_name: &str, _signal: &AbortSignal) -> String {
    format!(r#"Tool "{}" was aborted"#, tool_name)
}

fn tool_call_display_fields_from_execution(
    execution: &RunnableToolExecution,
) -> ToolCallDisplayFields {
    let description = execution.description.clone().filter(|d| !d.is_empty());
    (description, execution.display.clone())
}

async fn dispatch_tool_call_event(
    step: &ToolCallStepContext<'_>,
    call: &PreflightedToolCall,
    args: &JsonValue,
    display: ToolCallDisplayFields,
) -> Result<(), anyhow::Error> {
    step.dispatch_event
        .dispatch_recorded(LoopRecordedEvent::ToolCallEvent {
            uuid: call.tool_call().id.clone(),
            turn_id: step.turn_id.clone(),
            step: step.current_step as i64,
            step_uuid: step.step_uuid.clone(),
            tool_call_id: call.tool_call().id.clone(),
            name: call.tool_name().into(),
            args: args.clone(),
            description: display.0,
            display: display.1,
        })
        .await
}

async fn dispatch_tool_result_event(
    step: &ToolCallStepContext<'_>,
    pending: &PendingToolResult,
) -> Result<(), anyhow::Error> {
    // stop_turn is an internal loop-control hint; it must not be persisted in
    // the durable tool.result event.
    let event_result = match &pending.result {
        ExecutableToolResult::Success(s) => {
            ExecutableToolResult::Success(ExecutableToolSuccessResult {
                stop_turn: None,
                ..s.clone()
            })
        }
        ExecutableToolResult::Error(e) => ExecutableToolResult::Error(ExecutableToolErrorResult {
            stop_turn: None,
            ..e.clone()
        }),
    };
    step.dispatch_event
        .dispatch_recorded(LoopRecordedEvent::ToolResultEvent {
            parent_uuid: pending.tool_call.id.clone(),
            tool_call_id: pending.tool_call.id.clone(),
            result: event_result,
        })
        .await
}

fn find_tool<'a>(
    tools: Option<&'a [Arc<dyn ExecutableTool>]>,
    name: &str,
) -> Option<&'a dyn ExecutableTool> {
    tools.and_then(|list| list.iter().find(|t| t.name() == name).map(|t| t.as_ref()))
}

impl PreflightedToolCall {
    fn tool_call(&self) -> &ToolCall {
        match self {
            PreflightedToolCall::Runnable { tool_call, .. }
            | PreflightedToolCall::Rejected { tool_call, .. } => tool_call,
        }
    }

    fn tool_name(&self) -> &str {
        match self {
            PreflightedToolCall::Runnable { tool_name, .. }
            | PreflightedToolCall::Rejected { tool_name, .. } => tool_name,
        }
    }

    fn args(&self) -> &JsonValue {
        match self {
            PreflightedToolCall::Runnable { args, .. }
            | PreflightedToolCall::Rejected { args, .. } => args,
        }
    }
}
