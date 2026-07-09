use kosong_rs::message::ContentPart;
use kosong_rs::usage::TokenUsage;
use serde_json::Value as JsonValue;

use crate::agent_loop::events::{
    LoopEvent, LoopInterruptReason, LoopLiveOnlyEvent, LoopRecordedEvent,
};
use crate::records::nested::{ExecutableToolOutput, ExecutableToolResult};
use crate::turn::error::to_ody_error;
use crate::turn::types::{AgentEvent, StepRetryingEvent, TurnErrorSummary};

pub fn summarize_turn_error(error: &anyhow::Error, turn_id: i64) -> TurnErrorSummary {
    const LLM_NOT_SET_MESSAGE: &str = "LLM not set, send \"/login\" to login";
    let mut summary = if let Some(ody) = to_ody_error(error) {
        let mut details = ody
            .details
            .clone()
            .unwrap_or_else(|| JsonValue::Object(Default::default()));
        if let Some(obj) = details.as_object_mut() {
            obj.insert("turnId".into(), turn_id.into());
        }
        TurnErrorSummary {
            code: ody.code,
            name: ody.name,
            message: ody.message,
            retryable: ody.retryable,
            details: Some(details),
        }
    } else {
        TurnErrorSummary {
            code: "runtime.error".into(),
            name: error.to_string(),
            message: error.to_string(),
            retryable: false,
            details: Some(serde_json::json!({ "turnId": turn_id })),
        }
    };
    if summary.code == "model.not_configured" {
        summary.message = LLM_NOT_SET_MESSAGE.into();
    }
    summary
}

pub fn goal_failure_pause_reason(error: Option<&TurnErrorSummary>) -> Option<&'static str> {
    const GOAL_RATE_LIMIT_PAUSE_REASON: &str = "Paused after provider rate limit";
    if let Some(e) = error {
        if e.code == "provider.rate_limit" {
            return Some(GOAL_RATE_LIMIT_PAUSE_REASON);
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApiErrorClassification {
    pub error_type: String,
    pub status_code: Option<i32>,
}

pub fn classify_api_error(
    error: &anyhow::Error,
    summary: &TurnErrorSummary,
) -> ApiErrorClassification {
    let status_code = api_status_code(error).or_else(|| summary_status_code(summary));
    if let Some(sc) = status_code {
        if sc == 429 {
            return ApiErrorClassification {
                error_type: "rate_limit".into(),
                status_code: Some(sc),
            };
        }
        if sc == 401 || sc == 403 {
            return ApiErrorClassification {
                error_type: "auth".into(),
                status_code: Some(sc),
            };
        }
        if sc >= 500 {
            return ApiErrorClassification {
                error_type: "5xx_server".into(),
                status_code: Some(sc),
            };
        }
        if is_context_overflow_status_error(sc, &summary.message) {
            return ApiErrorClassification {
                error_type: "context_overflow".into(),
                status_code: Some(sc),
            };
        }
        if sc >= 400 {
            return ApiErrorClassification {
                error_type: "4xx_client".into(),
                status_code: Some(sc),
            };
        }
        return ApiErrorClassification {
            error_type: "api".into(),
            status_code: Some(sc),
        };
    }
    if summary.code == "provider.rate_limit" {
        return ApiErrorClassification {
            error_type: "rate_limit".into(),
            status_code: None,
        };
    }
    if summary.code == "provider.auth_error" {
        return ApiErrorClassification {
            error_type: "auth".into(),
            status_code: None,
        };
    }
    if summary.code == "context_overflow" {
        return ApiErrorClassification {
            error_type: "context_overflow".into(),
            status_code: None,
        };
    }
    if is_api_connection_error(error, summary) {
        return ApiErrorClassification {
            error_type: "network".into(),
            status_code: None,
        };
    }
    if is_api_timeout_error(error, summary) {
        return ApiErrorClassification {
            error_type: "timeout".into(),
            status_code: None,
        };
    }
    if is_api_empty_response_error(error, summary) {
        return ApiErrorClassification {
            error_type: "empty_response".into(),
            status_code: None,
        };
    }
    ApiErrorClassification {
        error_type: "other".into(),
        status_code: None,
    }
}

fn api_status_code(error: &anyhow::Error) -> Option<i32> {
    if let Some(ody) = to_ody_error(error) {
        return status_code_from_value(ody.details.as_ref());
    }
    None
}

fn summary_status_code(summary: &TurnErrorSummary) -> Option<i32> {
    status_code_from_value(summary.details.as_ref())
}

fn status_code_from_value(value: Option<&JsonValue>) -> Option<i32> {
    value?.get("statusCode")?.as_i64()?.try_into().ok()
}

fn is_context_overflow_status_error(status_code: i32, message: &str) -> bool {
    status_code == 413 || (status_code == 400 && message.to_lowercase().contains("context"))
}

fn is_api_connection_error(error: &anyhow::Error, summary: &TurnErrorSummary) -> bool {
    error.to_string().to_lowercase().contains("connection") || summary.name == "APIConnectionError"
}

fn is_api_timeout_error(error: &anyhow::Error, summary: &TurnErrorSummary) -> bool {
    let text = error.to_string().to_lowercase();
    text.contains("timeout") || summary.name == "APITimeoutError" || summary.name == "TimeoutError"
}

fn is_api_empty_response_error(error: &anyhow::Error, summary: &TurnErrorSummary) -> bool {
    error.to_string().to_lowercase().contains("empty response")
        || summary.name == "APIEmptyResponseError"
}

pub fn current_turn_input_tokens(usage: Option<&TokenUsage>) -> Option<i64> {
    usage.map(|u| u.input_total())
}

pub fn telemetry_tool_outcome(result: &ExecutableToolResult) -> &'static str {
    match result {
        ExecutableToolResult::Success(s) if s.is_error != Some(true) => "success",
        ExecutableToolResult::Success(s) => {
            let text = tool_output_text(&s.output).to_lowercase();
            if text.contains("aborted")
                || text.contains("cancelled")
                || text.contains("manually interrupted")
            {
                "cancelled"
            } else {
                "error"
            }
        }
        ExecutableToolResult::Error(_) => {
            let text = tool_result_text(result).to_lowercase();
            if text.contains("aborted")
                || text.contains("cancelled")
                || text.contains("manually interrupted")
            {
                "cancelled"
            } else {
                "error"
            }
        }
    }
}

pub fn telemetry_tool_error_type(result: &ExecutableToolResult) -> &'static str {
    let text = tool_result_text(result);
    if text.starts_with("Tool \"") && text.contains("\" not found") {
        return "ToolNotFound";
    }
    if text.starts_with("Invalid args for tool \"") {
        return "ToolInputError";
    }
    if text.contains("prepareToolExecution hook failed")
        || text.contains("finalizeToolResult hook failed")
    {
        return "HookError";
    }
    if text.contains("blocked") {
        return "ToolBlocked";
    }
    "ToolError"
}

pub fn tool_result_text(result: &ExecutableToolResult) -> String {
    match result {
        ExecutableToolResult::Success(s) => tool_output_text(&s.output),
        ExecutableToolResult::Error(e) => tool_output_text(&e.output),
    }
}

pub fn tool_output_text(output: &ExecutableToolOutput) -> String {
    match output {
        ExecutableToolOutput::Text(s) => s.clone(),
        ExecutableToolOutput::Parts(parts) => parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
    }
}

pub fn tool_input_record(args: &JsonValue) -> JsonValue {
    if args.is_object() {
        args.clone()
    } else {
        JsonValue::Object(Default::default())
    }
}

pub fn map_loop_event(event: &LoopEvent, turn_id: i64) -> Option<AgentEvent> {
    match event {
        LoopEvent::Recorded(LoopRecordedEvent::StepBegin { uuid, step, .. }) => {
            Some(AgentEvent::TurnStepStarted {
                turn_id,
                step: *step as u32,
                step_id: uuid.clone(),
            })
        }
        LoopEvent::Recorded(LoopRecordedEvent::StepEnd {
            uuid,
            step,
            usage,
            finish_reason,
            llm_first_token_latency_ms,
            llm_stream_duration_ms,
            provider_finish_reason,
            raw_finish_reason,
            ..
        }) => Some(AgentEvent::TurnStepCompleted {
            turn_id,
            step: *step as u32,
            step_id: uuid.clone(),
            usage: usage.clone().unwrap_or_default(),
            finish_reason: finish_reason.clone(),
            llm_first_token_latency_ms: *llm_first_token_latency_ms,
            llm_stream_duration_ms: *llm_stream_duration_ms,
            provider_finish_reason: provider_finish_reason.clone(),
            raw_finish_reason: raw_finish_reason.clone(),
        }),
        LoopEvent::Recorded(LoopRecordedEvent::ContentPartEvent { .. }) => None,
        LoopEvent::Recorded(LoopRecordedEvent::ToolCallEvent {
            tool_call_id,
            name,
            args,
            description,
            display,
            ..
        }) => Some(AgentEvent::ToolCallStarted {
            turn_id,
            tool_call_id: tool_call_id.clone(),
            name: name.clone(),
            args: args.clone(),
            description: description.clone(),
            display: display.clone(),
        }),
        LoopEvent::Recorded(LoopRecordedEvent::ToolResultEvent {
            tool_call_id,
            result,
            ..
        }) => {
            let is_error = match result {
                ExecutableToolResult::Success(s) => s.is_error,
                ExecutableToolResult::Error(_) => Some(true),
            };
            Some(AgentEvent::ToolResult {
                turn_id,
                tool_call_id: tool_call_id.clone(),
                output: match result {
                    ExecutableToolResult::Success(s) => s.output.clone(),
                    ExecutableToolResult::Error(e) => e.output.clone(),
                },
                is_error,
            })
        }
        LoopEvent::Live(LoopLiveOnlyEvent::TurnInterrupted {
            reason,
            active_step,
            message,
            ..
        }) => {
            let step = active_step.as_ref()?;
            let reason_str = match reason {
                LoopInterruptReason::Aborted => "aborted",
                LoopInterruptReason::MaxSteps => "max_steps",
                LoopInterruptReason::Error => "error",
            };
            Some(AgentEvent::TurnStepInterrupted {
                turn_id,
                step: *step,
                reason: reason_str.into(),
                message: message.clone(),
            })
        }
        LoopEvent::Live(LoopLiveOnlyEvent::StepRetrying(e)) => {
            Some(AgentEvent::TurnStepRetrying(StepRetryingEvent {
                turn_id,
                step: e.step,
                step_uuid: e.step_uuid.clone(),
                failed_attempt: e.failed_attempt,
                next_attempt: e.next_attempt,
                max_attempts: e.max_attempts,
                delay_ms: e.delay_ms,
                error_name: e.error_name.clone(),
                error_message: e.error_message.clone(),
                status_code: e.status_code,
            }))
        }
        LoopEvent::Live(LoopLiveOnlyEvent::TextDelta { delta }) => {
            Some(AgentEvent::AssistantDelta {
                turn_id,
                delta: delta.clone(),
            })
        }
        LoopEvent::Live(LoopLiveOnlyEvent::ThinkingDelta { delta }) => {
            Some(AgentEvent::ThinkingDelta {
                turn_id,
                delta: delta.clone(),
            })
        }
        LoopEvent::Live(LoopLiveOnlyEvent::ToolCallDelta {
            tool_call_id,
            name,
            arguments_part,
        }) => Some(AgentEvent::ToolCallDelta {
            turn_id,
            tool_call_id: tool_call_id.clone(),
            name: name.clone(),
            arguments_part: arguments_part.clone(),
        }),
        LoopEvent::Live(LoopLiveOnlyEvent::ToolProgress {
            tool_call_id,
            update,
        }) => Some(AgentEvent::ToolProgress {
            turn_id,
            tool_call_id: tool_call_id.clone(),
            update: update.clone(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn::error::OdyError;

    #[test]
    fn summarize_turn_error_replaces_model_not_configured_message() {
        let err = anyhow::anyhow!(OdyError::new(
            "model.not_configured",
            "OdyError",
            "Model not set",
        ));
        let summary = summarize_turn_error(&err, 7);
        assert_eq!(summary.code, "model.not_configured");
        assert_eq!(summary.message, "LLM not set, send \"/login\" to login");
        assert_eq!(summary.details.as_ref().unwrap()["turnId"], 7);
    }

    #[test]
    fn classify_api_error_buckets_status_codes() {
        let err = anyhow::anyhow!("boom");
        let summary = TurnErrorSummary {
            code: "provider.api".into(),
            name: "APIStatusError".into(),
            message: "Bad Request".into(),
            retryable: false,
            details: Some(serde_json::json!({ "statusCode": 429 })),
        };
        let c = classify_api_error(&err, &summary);
        assert_eq!(c.error_type, "rate_limit");
        assert_eq!(c.status_code, Some(429));
    }

    #[test]
    fn map_loop_event_maps_step_begin() {
        use crate::agent_loop::events::{LoopEvent, LoopRecordedEvent};
        let event = LoopEvent::Recorded(LoopRecordedEvent::StepBegin {
            uuid: "step-1".into(),
            turn_id: "42".into(),
            step: 3,
        });
        let mapped = map_loop_event(&event, 42).unwrap();
        match mapped {
            AgentEvent::TurnStepStarted {
                turn_id,
                step,
                step_id,
            } => {
                assert_eq!(turn_id, 42);
                assert_eq!(step, 3);
                assert_eq!(step_id, "step-1");
            }
            _ => panic!("expected TurnStepStarted"),
        }
    }
}
