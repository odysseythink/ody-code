use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use crate::capability_registry;
use crate::errors::{
    is_context_overflow_error_code, APIContextOverflowError, APIMissingApiKeyError, AbortError,
    ChatProviderError,
};
use crate::generate::StreamedMessage;
use crate::http_client::{HttpClient, ReqwestClient};
use crate::message::{ContentPart, Message, StreamedMessagePart, ToolCall, ToolCallPart};
use crate::openai_common::{
    convert_openai_error, convert_tool_message_content, reasoning_effort_to_thinking_effort,
    thinking_effort_to_reasoning_effort, ToolMessageConversion,
};
use crate::provider::{
    ChatProvider, FinishReason, GenerateOptions, ModelCapability, ThinkingEffort, Tool,
};
use crate::request_auth::merge_request_headers;
use crate::tool_call_id::{
    normalize_tool_call_ids_for_provider, sanitize_openai_responses_call_id, ToolCallIdPolicy,
};
use crate::usage::TokenUsage;
use futures_util::StreamExt;

#[derive(Debug, Clone, Serialize)]
struct ResponseInputItem {
    #[serde(rename = "type")]
    item_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "call_id")]
    call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "encrypted_content")]
    encrypted_content: Option<String>,
}

fn content_parts_to_input_items(parts: &[ContentPart]) -> Vec<Value> {
    let mut items = Vec::new();
    for part in parts {
        match part {
            ContentPart::Text { text } if !text.is_empty() => {
                items.push(serde_json::json!({"type": "input_text", "text": text}));
            }
            ContentPart::ImageUrl { image_url } => {
                items.push(serde_json::json!({"type": "input_image", "detail": "auto", "image_url": image_url.url}));
            }
            ContentPart::AudioUrl { audio_url } => {
                if let Some(mapped) = map_audio_url_to_input_item(&audio_url.url) {
                    items.push(mapped);
                }
            }
            _ => {}
        }
    }
    items
}

fn content_parts_to_output_items(parts: &[ContentPart]) -> Vec<Value> {
    let mut items = Vec::new();
    for part in parts {
        if let ContentPart::Text { text } = part {
            if !text.is_empty() {
                items.push(
                    serde_json::json!({"type": "output_text", "text": text, "annotations": []}),
                );
            }
        }
    }
    items
}

fn map_audio_url_to_input_item(url: &str) -> Option<Value> {
    if url.starts_with("data:audio/") {
        let parts: Vec<&str> = url.splitn(2, ',').collect();
        if parts.len() != 2 {
            return None;
        }
        let header = parts[0];
        let b64 = parts[1];
        let subtype_part = header.split('/').nth(1)?;
        let subtype_head = subtype_part.split(';').next()?;
        let subtype = subtype_head.to_lowercase();
        let ext = if subtype == "mp3" || subtype == "mpeg" {
            "mp3"
        } else if subtype == "wav" {
            "wav"
        } else {
            return None;
        };
        Some(
            serde_json::json!({"type": "input_file", "file_data": b64, "filename": format!("inline.{}", ext)}),
        )
    } else if url.starts_with("http://") || url.starts_with("https://") {
        Some(serde_json::json!({"type": "input_file", "file_url": url}))
    } else {
        None
    }
}

fn flush_parts(parts: &mut Vec<ContentPart>, role: &str, result: &mut Vec<ResponseInputItem>) {
    if parts.is_empty() {
        return;
    }
    let content = if role == "assistant" {
        content_parts_to_output_items(parts)
    } else {
        content_parts_to_input_items(parts)
    };
    if !content.is_empty() {
        result.push(ResponseInputItem {
            item_type: "message".into(),
            role: Some(role.into()),
            content: Some(Value::Array(content)),
            call_id: None,
            output: None,
            name: None,
            arguments: None,
            summary: None,
            encrypted_content: None,
        });
    }
    parts.clear();
}

fn convert_message(
    message: &Message,
    model_name: &str,
    tool_message_conversion: Option<ToolMessageConversion>,
) -> Vec<ResponseInputItem> {
    let mut role = format!("{:?}", message.role).to_lowercase();
    if role == "system" && capability_registry::uses_openai_responses_developer_role(model_name) {
        role = "developer".into();
    }

    if role == "tool" {
        let call_id = message.tool_call_id.clone().unwrap_or_default();
        let output = if tool_message_conversion == Some(ToolMessageConversion::ExtractText) {
            Some(Value::String(crate::message::extract_text(message, "")))
        } else {
            convert_tool_message_content(
                message,
                tool_message_conversion.unwrap_or(ToolMessageConversion::Standard),
            )
        };
        return vec![ResponseInputItem {
            item_type: "function_call_output".into(),
            role: None,
            content: None,
            call_id: Some(call_id),
            output,
            name: None,
            arguments: None,
            summary: None,
            encrypted_content: None,
        }];
    }

    let mut result = Vec::new();
    if !message.content.is_empty() {
        let mut pending_parts: Vec<ContentPart> = Vec::new();

        let mut i = 0;
        while i < message.content.len() {
            let part = &message.content[i];
            if let ContentPart::Think { think, encrypted } = part {
                flush_parts(&mut pending_parts, &role, &mut result);
                let encrypted_value = encrypted.clone();
                let mut summaries =
                    vec![serde_json::json!({"type": "summary_text", "text": think.as_str()})];
                i += 1;
                while i < message.content.len() {
                    if let ContentPart::Think {
                        think: t,
                        encrypted: e,
                    } = &message.content[i]
                    {
                        if e == &encrypted_value {
                            summaries.push(
                                serde_json::json!({"type": "summary_text", "text": t.as_str()}),
                            );
                            i += 1;
                            continue;
                        }
                    }
                    break;
                }
                result.push(ResponseInputItem {
                    item_type: "reasoning".into(),
                    role: None,
                    content: None,
                    call_id: None,
                    output: None,
                    name: None,
                    arguments: None,
                    summary: Some(Value::Array(summaries)),
                    encrypted_content: encrypted_value,
                });
            } else {
                pending_parts.push(part.clone());
                i += 1;
            }
        }
        flush_parts(&mut pending_parts, &role, &mut result);
    }

    for tc in &message.tool_calls {
        result.push(ResponseInputItem {
            item_type: "function_call".into(),
            role: None,
            content: None,
            call_id: Some(tc.id.clone()),
            output: None,
            name: Some(tc.name.clone()),
            arguments: Some(tc.arguments.clone().unwrap_or_else(|| "{}".into())),
            summary: None,
            encrypted_content: None,
        });
    }

    result
}

static FUNCTION_CALL_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

fn function_call_id(call_id: Option<&str>) -> String {
    match call_id {
        Some(s) if !s.is_empty() => s.into(),
        _ => format!(
            "call-{}",
            FUNCTION_CALL_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
        ),
    }
}

fn read_string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(String::from)
}

fn read_nullable_string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| {
        if v.is_null() {
            Some(String::new())
        } else {
            v.as_str().map(String::from)
        }
    })
}

fn read_number_field(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|v| v.as_i64())
}

fn read_object_field(value: &Value, key: &str) -> Option<Value> {
    value.get(key).cloned().filter(|v| v.is_object())
}

fn read_object_array_field(value: &Value, key: &str) -> Option<Vec<Value>> {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .cloned()
        .map(|arr| arr.into_iter().filter(|v| v.is_object()).collect())
}

fn require_string_field(
    value: &Value,
    key: &str,
    context: &str,
) -> Result<String, ChatProviderError> {
    read_string_field(value, key).ok_or_else(|| {
        ChatProviderError::Other(format!(
            "OpenAI Responses decode error: {}.{} must be a string.",
            context, key
        ))
    })
}

fn require_object_field(
    value: &Value,
    key: &str,
    context: &str,
) -> Result<Value, ChatProviderError> {
    read_object_field(value, key).ok_or_else(|| {
        ChatProviderError::Other(format!(
            "OpenAI Responses decode error: {}.{} must be an object.",
            context, key
        ))
    })
}

fn normalize_responses_finish_reason(
    status: Option<&str>,
    incomplete_reason: Option<&str>,
) -> (Option<FinishReason>, Option<String>) {
    match status {
        Some("completed") => (Some(FinishReason::Completed), Some("completed".into())),
        Some("incomplete") => match incomplete_reason {
            Some("max_output_tokens") => (
                Some(FinishReason::Truncated),
                Some("max_output_tokens".into()),
            ),
            Some("content_filter") => (Some(FinishReason::Filtered), Some("content_filter".into())),
            _ => (
                Some(FinishReason::Other),
                Some(incomplete_reason.unwrap_or("incomplete").into()),
            ),
        },
        Some("failed") => (Some(FinishReason::Other), Some("failed".into())),
        _ => (None, None),
    }
}

fn extract_responses_usage(usage: &Value) -> Option<TokenUsage> {
    if !usage.is_object() {
        return None;
    }
    let input_tokens = read_number_field(usage, "input_tokens").unwrap_or(0);
    let output_tokens = read_number_field(usage, "output_tokens").unwrap_or(0);
    let cached = read_number_field(usage, "cached_tokens")
        .or_else(|| {
            read_object_field(usage, "input_tokens_details")
                .and_then(|d| read_number_field(&d, "cached_tokens"))
        })
        .unwrap_or(0);
    Some(TokenUsage {
        input_other: input_tokens - cached,
        output: output_tokens,
        input_cache_read: cached,
        input_cache_creation: 0,
    })
}

#[derive(Debug, Clone)]
enum ResponseOutputItemView {
    Message {
        content: Vec<Value>,
    },
    FunctionCall {
        item_id: Option<String>,
        call_id: Option<String>,
        name: Option<String>,
        arguments: Option<String>,
    },
    Reasoning {
        encrypted_content: Option<String>,
        summary: Vec<Value>,
    },
    Other,
}

fn read_response_output_item(
    value: &Value,
    context: &str,
) -> Result<ResponseOutputItemView, ChatProviderError> {
    let item_type = require_string_field(value, "type", context)?;
    match item_type.as_str() {
        "message" => Ok(ResponseOutputItemView::Message {
            content: read_object_array_field(value, "content").unwrap_or_default(),
        }),
        "function_call" => Ok(ResponseOutputItemView::FunctionCall {
            item_id: read_string_field(value, "id"),
            call_id: read_string_field(value, "call_id"),
            name: read_string_field(value, "name"),
            arguments: read_nullable_string_field(value, "arguments"),
        }),
        "reasoning" => Ok(ResponseOutputItemView::Reasoning {
            encrypted_content: read_string_field(value, "encrypted_content"),
            summary: read_object_array_field(value, "summary").unwrap_or_default(),
        }),
        _ => Ok(ResponseOutputItemView::Other),
    }
}

pub fn parse_responses_non_stream(
    body: &[u8],
) -> Result<
    (
        Vec<StreamedMessagePart>,
        Option<String>,
        Option<TokenUsage>,
        Option<FinishReason>,
        Option<String>,
    ),
    ChatProviderError,
> {
    let response: Value = serde_json::from_slice(body)
        .map_err(|e| ChatProviderError::Other(format!("OpenAI Responses decode error: {}", e)))?;
    if !response.is_object() {
        return Err(ChatProviderError::Other(
            "OpenAI Responses decode error: response must be an object".into(),
        ));
    }

    let id = read_string_field(&response, "id");
    let usage = read_object_field(&response, "usage").and_then(|u| extract_responses_usage(&u));
    let status = read_string_field(&response, "status");
    let status_ref = status.as_deref();
    let incomplete = read_object_field(&response, "incomplete_details");
    let incomplete_reason = incomplete
        .as_ref()
        .and_then(|v| read_string_field(v, "reason"));
    let incomplete_reason_ref = incomplete_reason.as_deref();
    let (finish_reason, raw_finish_reason) =
        normalize_responses_finish_reason(status_ref, incomplete_reason_ref);

    let mut parts = Vec::new();
    let output = read_object_array_field(&response, "output").unwrap_or_default();
    for item in output {
        match read_response_output_item(&item, "response.output item")? {
            ResponseOutputItemView::Message { content } => {
                for content_item in content {
                    if read_string_field(&content_item, "type").as_deref() == Some("output_text") {
                        if let Some(text) = read_string_field(&content_item, "text") {
                            parts.push(StreamedMessagePart::text(text));
                        }
                    }
                }
            }
            ResponseOutputItemView::FunctionCall {
                call_id,
                name,
                arguments,
                ..
            } => {
                let name = name.ok_or_else(|| {
                    ChatProviderError::Other(
                        "OpenAI Responses function_call item is missing a name.".into(),
                    )
                })?;
                parts.push(StreamedMessagePart::ToolCall(ToolCall {
                    call_type: "function".into(),
                    id: function_call_id(call_id.as_deref()),
                    name,
                    arguments,
                    extras: None,
                    stream_index: None,
                }));
            }
            ResponseOutputItemView::Reasoning {
                encrypted_content,
                summary,
            } => {
                for summary_item in summary {
                    if read_string_field(&summary_item, "type").as_deref() == Some("summary_text") {
                        if let Some(text) = read_string_field(&summary_item, "text") {
                            parts.push(StreamedMessagePart::Content(ContentPart::Think {
                                think: text,
                                encrypted: encrypted_content.clone(),
                            }));
                        }
                    }
                }
            }
            ResponseOutputItemView::Other => {}
        }
    }

    Ok((parts, id, usage, finish_reason, raw_finish_reason))
}

#[derive(Default)]
struct StreamParserState {
    id: Option<String>,
    usage: Option<TokenUsage>,
    finish_reason: Option<FinishReason>,
    raw_finish_reason: Option<String>,
    function_call_args: HashMap<Value, String>,
    unindexed_args: Option<String>,
}

impl StreamParserState {
    fn has_args(&self, index: Option<&Value>) -> bool {
        match index {
            None => self.unindexed_args.is_some(),
            Some(v) => self.function_call_args.contains_key(v),
        }
    }

    fn get_args(&self, index: Option<&Value>) -> &str {
        match index {
            None => self.unindexed_args.as_deref().unwrap_or(""),
            Some(v) => self
                .function_call_args
                .get(v)
                .map(String::as_str)
                .unwrap_or(""),
        }
    }

    fn set_args(&mut self, index: Option<Value>, value: String) {
        match index {
            None => self.unindexed_args = Some(value),
            Some(v) => {
                self.function_call_args.insert(v, value);
            }
        }
    }

    fn append_args(
        &mut self,
        index: Option<&Value>,
        part: &str,
        context: &str,
    ) -> Result<(), ChatProviderError> {
        if !self.has_args(index) {
            return Err(ChatProviderError::Other(format!(
                "OpenAI Responses decode error: {} received function-call arguments for unknown stream index {}.",
                context,
                format_response_stream_index(index)
            )));
        }
        let new_value = format!("{}{}", self.get_args(index), part);
        self.set_args(index.cloned(), new_value);
        Ok(())
    }

    fn yield_final_args_suffix(
        &mut self,
        index: Option<&Value>,
        final_args: &str,
        context: &str,
    ) -> Result<Option<String>, ChatProviderError> {
        if !self.has_args(index) {
            return Err(ChatProviderError::Other(format!(
                "OpenAI Responses decode error: {} received final function-call arguments for unknown stream index {}.",
                context,
                format_response_stream_index(index)
            )));
        }
        let accumulated = self.get_args(index).to_string();
        if final_args == accumulated {
            return Ok(None);
        }
        if !final_args.starts_with(&accumulated) {
            return Err(ChatProviderError::Other(format!(
                "OpenAI Responses final function-call arguments for stream index {} do not match the streamed argument deltas.",
                format_response_stream_index(index)
            )));
        }
        let suffix = &final_args[accumulated.len()..];
        self.set_args(index.cloned(), final_args.into());
        if suffix.is_empty() {
            Ok(None)
        } else {
            Ok(Some(suffix.into()))
        }
    }
}

fn format_response_stream_index(index: Option<&Value>) -> String {
    match index {
        None => "<unindexed>".into(),
        Some(v) => v.to_string(),
    }
}

fn response_stream_index(item_id: Option<&str>, output_index: Option<i64>) -> Option<Value> {
    item_id
        .map(|s| Value::String(s.into()))
        .or_else(|| output_index.map(|n| Value::Number(n.into())))
}

fn capture_finish_reason_from_response(state: &mut StreamParserState, response: &Value) {
    let status = read_string_field(response, "status");
    let status_ref = status.as_deref();
    let incomplete = read_object_field(response, "incomplete_details");
    let incomplete_reason = incomplete
        .as_ref()
        .and_then(|v| read_string_field(v, "reason"));
    let incomplete_reason_ref = incomplete_reason.as_deref();
    let (fr, rfr) = normalize_responses_finish_reason(status_ref, incomplete_reason_ref);
    if state.finish_reason.is_none() {
        state.finish_reason = fr;
    }
    if state.raw_finish_reason.is_none() {
        state.raw_finish_reason = rfr;
    }
}

fn process_stream_event(
    event: &Value,
    parts: &mut Vec<StreamedMessagePart>,
    state: &mut StreamParserState,
) -> Result<(), ChatProviderError> {
    let event_type = require_string_field(event, "type", "stream event")?;
    match event_type.as_str() {
        "response.output_text.delta" => {
            parts.push(StreamedMessagePart::text(require_string_field(
                event,
                "delta",
                "response.output_text.delta",
            )?));
        }
        "response.created" | "response.in_progress" => {
            let response = require_object_field(event, "response", &event_type)?;
            if let Some(resp_id) = read_string_field(&response, "id") {
                state.id = Some(resp_id);
            }
        }
        "response.output_item.added" => {
            let item = read_response_output_item(
                event.get("item").unwrap_or(&Value::Null),
                &format!("{}.item", event_type),
            )?;
            let output_index = read_number_field(event, "output_index");
            if let ResponseOutputItemView::FunctionCall {
                item_id,
                call_id,
                name,
                arguments,
            } = item
            {
                let stream_index = response_stream_index(item_id.as_deref(), output_index);
                state.set_args(stream_index.clone(), arguments.clone().unwrap_or_default());
                let name = name.ok_or_else(|| {
                    ChatProviderError::Other(
                        "OpenAI Responses function_call item is missing a name.".into(),
                    )
                })?;
                parts.push(StreamedMessagePart::ToolCall(ToolCall {
                    call_type: "function".into(),
                    id: function_call_id(call_id.as_deref()),
                    name,
                    arguments,
                    extras: None,
                    stream_index: stream_index.clone(),
                }));
            }
        }
        "response.output_item.done" => {
            let item = read_response_output_item(
                event.get("item").unwrap_or(&Value::Null),
                &format!("{}.item", event_type),
            )?;
            let output_index = read_number_field(event, "output_index");
            match item {
                ResponseOutputItemView::Reasoning {
                    encrypted_content, ..
                } => {
                    parts.push(StreamedMessagePart::Content(ContentPart::Think {
                        think: "".into(),
                        encrypted: encrypted_content,
                    }));
                }
                ResponseOutputItemView::FunctionCall {
                    item_id,
                    arguments: Some(args),
                    ..
                } => {
                    let stream_index = response_stream_index(item_id.as_deref(), output_index);
                    if let Some(suffix) =
                        state.yield_final_args_suffix(stream_index.as_ref(), &args, &event_type)?
                    {
                        parts.push(StreamedMessagePart::ToolCallPart(ToolCallPart {
                            part_type: "tool_call_part".into(),
                            arguments_part: Some(suffix),
                            index: stream_index,
                        }));
                    }
                }
                _ => {}
            }
        }
        "response.function_call_arguments.delta" => {
            let stream_index = response_stream_index(
                read_string_field(event, "item_id").as_deref(),
                read_number_field(event, "output_index"),
            );
            let delta =
                require_string_field(event, "delta", "response.function_call_arguments.delta")?;
            state.append_args(
                stream_index.as_ref(),
                &delta,
                "response.function_call_arguments.delta",
            )?;
            parts.push(StreamedMessagePart::ToolCallPart(ToolCallPart {
                part_type: "tool_call_part".into(),
                arguments_part: Some(delta),
                index: stream_index,
            }));
        }
        "response.function_call_arguments.done" => {
            let stream_index = response_stream_index(
                read_string_field(event, "item_id").as_deref(),
                read_number_field(event, "output_index"),
            );
            let args =
                require_string_field(event, "arguments", "response.function_call_arguments.done")?;
            if let Some(suffix) = state.yield_final_args_suffix(
                stream_index.as_ref(),
                &args,
                "response.function_call_arguments.done",
            )? {
                parts.push(StreamedMessagePart::ToolCallPart(ToolCallPart {
                    part_type: "tool_call_part".into(),
                    arguments_part: Some(suffix),
                    index: stream_index,
                }));
            }
        }
        "response.reasoning_summary_part.added" => {
            parts.push(StreamedMessagePart::think(""));
        }
        "response.reasoning_summary_text.delta" => {
            parts.push(StreamedMessagePart::think(require_string_field(
                event,
                "delta",
                "response.reasoning_summary_text.delta",
            )?));
        }
        "response.completed" | "response.incomplete" => {
            let response = require_object_field(event, "response", &event_type)?;
            if let Some(resp_id) = read_string_field(&response, "id") {
                state.id = Some(resp_id);
            }
            if let Some(usage) = read_object_field(&response, "usage") {
                state.usage = extract_responses_usage(&usage);
            }
            capture_finish_reason_from_response(state, &response);
        }
        "error" => {
            let message = require_string_field(event, "message", "error")?;
            let code = read_nullable_string_field(event, "code");
            let param = read_nullable_string_field(event, "param");
            return Err(error_from_openai_responses_event(
                "OpenAI Responses stream error",
                code.as_deref(),
                &message,
                param.as_deref(),
            ));
        }
        "response.failed" => {
            let response = require_object_field(event, "response", "response.failed")?;
            if let Some(error) = read_responses_failed_response_error(&response) {
                return Err(error_from_openai_responses_event(
                    "OpenAI Responses response.failed",
                    Some(&error.code),
                    &error.message,
                    None,
                ));
            }
            return Err(ChatProviderError::Other(format!(
                "OpenAI Responses response.failed: {}",
                format_responses_failed_response(&response)
            )));
        }
        _ => {}
    }
    Ok(())
}

pub fn parse_responses_stream(
    body: &[u8],
) -> Result<
    (
        Vec<StreamedMessagePart>,
        Option<String>,
        Option<TokenUsage>,
        Option<FinishReason>,
        Option<String>,
    ),
    ChatProviderError,
> {
    let text = std::str::from_utf8(body).map_err(|e| {
        ChatProviderError::Other(format!("OpenAI Responses stream is not valid UTF-8: {}", e))
    })?;
    let mut parts = Vec::new();
    let mut state = StreamParserState::default();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let event: Value = serde_json::from_str(line).map_err(|e| {
            ChatProviderError::Other(format!("OpenAI Responses decode error: {}", e))
        })?;
        process_stream_event(&event, &mut parts, &mut state)?;
    }

    Ok((
        parts,
        state.id,
        state.usage,
        state.finish_reason,
        state.raw_finish_reason,
    ))
}

pub struct OpenAIResponsesStreamedMessage;

impl OpenAIResponsesStreamedMessage {
    pub fn from_non_stream_bytes(body: Vec<u8>) -> Result<StreamedMessage, ChatProviderError> {
        let (parts, id, usage, finish_reason, raw_finish_reason) =
            parse_responses_non_stream(&body)?;
        Ok(StreamedMessage::from_parts(
            parts,
            id,
            usage,
            finish_reason,
            raw_finish_reason,
        ))
    }

    pub fn from_stream_bytes(body: Vec<u8>) -> Result<StreamedMessage, ChatProviderError> {
        let (parts, id, usage, finish_reason, raw_finish_reason) = parse_responses_stream(&body)?;
        Ok(StreamedMessage::from_parts(
            parts,
            id,
            usage,
            finish_reason,
            raw_finish_reason,
        ))
    }

    pub fn from_bytes(
        body: Vec<u8>,
        is_stream: bool,
    ) -> Result<StreamedMessage, ChatProviderError> {
        if is_stream {
            Self::from_stream_bytes(body)
        } else {
            Self::from_non_stream_bytes(body)
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ResponseToolParam {
    #[serde(rename = "type")]
    tool_type: String,
    name: String,
    description: String,
    parameters: Value,
    strict: bool,
}

fn convert_tool(tool: &Tool) -> ResponseToolParam {
    ResponseToolParam {
        tool_type: "function".into(),
        name: tool.name.clone(),
        description: tool.description.clone(),
        parameters: tool.parameters.clone(),
        strict: false,
    }
}

#[derive(Clone)]
pub struct OpenAIResponsesOptions {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: String,
    pub max_output_tokens: Option<i64>,
    pub default_headers: Option<HashMap<String, String>>,
    pub tool_message_conversion: Option<ToolMessageConversion>,
    pub http_client: Option<Arc<dyn HttpClient>>,
}

impl std::fmt::Debug for OpenAIResponsesOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAIResponsesOptions")
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

pub struct OpenAIResponsesChatProvider {
    model: String,
    api_key: Option<String>,
    base_url: String,
    default_headers: Option<HashMap<String, String>>,
    generation_kwargs: HashMap<String, Value>,
    tool_message_conversion: Option<ToolMessageConversion>,
    http_client: Arc<dyn HttpClient>,
}

impl OpenAIResponsesChatProvider {
    pub fn new(options: OpenAIResponsesOptions) -> Self {
        let api_key = options
            .api_key
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .filter(|k| !k.is_empty());
        let base_url = options
            .base_url
            .unwrap_or_else(|| "https://api.openai.com/v1".into());
        let mut generation_kwargs = HashMap::new();
        if let Some(max) = options.max_output_tokens {
            generation_kwargs.insert("max_output_tokens".into(), Value::Number(max.into()));
        }
        let http_client = options
            .http_client
            .unwrap_or_else(|| Arc::new(ReqwestClient::new(reqwest::Client::new())));
        Self {
            model: options.model,
            api_key,
            base_url,
            default_headers: options.default_headers,
            generation_kwargs,
            tool_message_conversion: options.tool_message_conversion,
            http_client,
        }
    }

    fn build_request_body(
        &self,
        system_prompt: &str,
        tools: &[Tool],
        history: &[Message],
    ) -> Result<Value, ChatProviderError> {
        let mut input: Vec<Value> = Vec::new();
        if !system_prompt.is_empty() {
            let role = if capability_registry::uses_openai_responses_developer_role(&self.model) {
                "developer"
            } else {
                "system"
            };
            input.push(serde_json::json!({"role": role, "content": system_prompt}));
        }

        let normalized_history =
            normalize_tool_call_ids_for_provider(history, &openai_responses_tool_call_id_policy());
        for msg in &normalized_history {
            for item in convert_message(msg, &self.model, self.tool_message_conversion) {
                input.push(serde_json::to_value(item).unwrap());
            }
        }

        let mut kwargs = self.generation_kwargs.clone();
        let reasoning_effort = kwargs
            .remove("reasoning_effort")
            .and_then(|v| v.as_str().map(String::from));
        if let Some(effort) = reasoning_effort {
            kwargs.insert(
                "reasoning".into(),
                serde_json::json!({"effort": effort, "summary": "auto"}),
            );
            kwargs.insert(
                "include".into(),
                Value::Array(vec![Value::String("reasoning.encrypted_content".into())]),
            );
        }

        let mut body = serde_json::Map::new();
        body.insert("model".into(), Value::String(self.model.clone()));
        body.insert("input".into(), Value::Array(input));
        body.insert("store".into(), Value::Bool(false));
        body.insert("stream".into(), Value::Bool(true));
        for (k, v) in kwargs {
            if !v.is_null() {
                body.insert(k, v);
            }
        }
        if !tools.is_empty() {
            body.insert(
                "tools".into(),
                Value::Array(
                    tools
                        .iter()
                        .map(|t| serde_json::to_value(convert_tool(t)).unwrap())
                        .collect(),
                ),
            );
        }
        Ok(Value::Object(body))
    }
}

impl Clone for OpenAIResponsesChatProvider {
    fn clone(&self) -> Self {
        Self {
            model: self.model.clone(),
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            default_headers: self.default_headers.clone(),
            generation_kwargs: self.generation_kwargs.clone(),
            tool_message_conversion: self.tool_message_conversion,
            http_client: Arc::clone(&self.http_client),
        }
    }
}

fn openai_responses_tool_call_id_policy() -> ToolCallIdPolicy {
    ToolCallIdPolicy::new(
        |id| sanitize_openai_responses_call_id(id, Some(64)),
        Some(64),
    )
}

#[async_trait]
impl ChatProvider for OpenAIResponsesChatProvider {
    fn name(&self) -> &str {
        "openai-responses"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn thinking_effort(&self) -> Option<ThinkingEffort> {
        self.generation_kwargs
            .get("reasoning_effort")
            .and_then(|v| v.as_str())
            .and_then(|s| reasoning_effort_to_thinking_effort(Some(s)))
    }

    fn get_capability(&self, model: Option<&str>) -> ModelCapability {
        capability_registry::get_openai_responses_model_capability(model.unwrap_or(&self.model))
    }

    async fn generate(
        &self,
        system_prompt: &str,
        tools: &[Tool],
        history: &[Message],
        options: Option<GenerateOptions>,
    ) -> Result<StreamedMessage, ChatProviderError> {
        if let Some(signal) = options.as_ref().and_then(|o| o.signal.as_ref()) {
            if signal.is_aborted() {
                return Err(ChatProviderError::Aborted(AbortError));
            }
        }

        let auth = options.as_ref().and_then(|o| o.auth.clone());
        let api_key = auth
            .as_ref()
            .and_then(|a| a.api_key.clone())
            .or_else(|| self.api_key.clone())
            .filter(|k| !k.is_empty())
            .ok_or_else(|| {
                ChatProviderError::MissingApiKey(APIMissingApiKeyError {
                    provider: self.name().to_string(),
                })
            })?;

        let merged_headers = merge_request_headers(
            self.default_headers.as_ref(),
            auth.as_ref().and_then(|a| a.headers.as_ref()),
        );
        let mut headers = HashMap::new();
        headers.insert("Authorization".into(), format!("Bearer {api_key}"));
        headers.insert("Content-Type".into(), "application/json".into());
        if let Some(m) = merged_headers {
            headers.extend(m);
        }

        let body = self.build_request_body(system_prompt, tools, history)?;
        let url = format!("{}/responses", self.base_url.trim_end_matches('/'));
        let response = self.http_client.post_json(&url, headers, body).await?;
        let status = response.status();
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            bytes.extend_from_slice(chunk?.as_ref());
        }

        if status < 200 || status >= 300 {
            let body_str = String::from_utf8_lossy(&bytes);
            let (msg, code) = parse_error_body(&body_str);
            return Err(convert_openai_error(&msg, Some(status), code.as_deref()));
        }

        OpenAIResponsesStreamedMessage::from_bytes(bytes, true)
    }

    fn with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider> {
        let mut clone = self.clone();
        let reasoning_effort = thinking_effort_to_reasoning_effort(effort);
        if let Some(re) = reasoning_effort {
            clone
                .generation_kwargs
                .insert("reasoning_effort".into(), Value::String(re));
        } else {
            clone.generation_kwargs.remove("reasoning_effort");
        }
        Box::new(clone)
    }

    fn with_max_completion_tokens(
        &self,
        max_completion_tokens: i64,
    ) -> Option<Box<dyn ChatProvider>> {
        let mut clone = self.clone();
        clone.generation_kwargs.insert(
            "max_output_tokens".into(),
            Value::Number(max_completion_tokens.into()),
        );
        Some(Box::new(clone))
    }
}

fn parse_error_body(body: &str) -> (String, Option<String>) {
    let value: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let message = value
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .or_else(|| value.get("message").and_then(|m| m.as_str()))
        .unwrap_or(body)
        .to_string();
    let code = value
        .get("error")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_str())
        .map(String::from);
    (message, code)
}

fn format_responses_error_event(code: Option<&str>, message: &str, param: Option<&str>) -> String {
    let code = code.unwrap_or("unknown");
    let param_text = match param {
        Some(p) => format!(" (param: {})", p),
        None => String::new(),
    };
    format!("{}: {}{}", code, message, param_text)
}

fn error_from_openai_responses_event(
    prefix: &str,
    code: Option<&str>,
    message: &str,
    param: Option<&str>,
) -> ChatProviderError {
    let formatted = format_responses_error_event(code, message, param);
    let full_message = format!("{}: {}", prefix, formatted);
    if is_context_overflow_error_code(code) {
        ChatProviderError::ContextOverflow(APIContextOverflowError {
            status_code: 400,
            message: full_message,
            request_id: None,
        })
    } else {
        ChatProviderError::Other(full_message)
    }
}

#[derive(Debug, Clone)]
struct FailedResponseError {
    code: String,
    message: String,
}

fn read_responses_failed_response_error(response: &Value) -> Option<FailedResponseError> {
    read_object_field(response, "error").map(|error| {
        let code = read_nullable_string_field(&error, "code").unwrap_or_else(|| "unknown".into());
        let message = read_string_field(&error, "message").unwrap_or_else(|| "no message".into());
        FailedResponseError { code, message }
    })
}

fn format_responses_failed_response(response: &Value) -> String {
    if let Some(error) = read_responses_failed_response_error(response) {
        return format_responses_error_event(Some(&error.code), &error.message, None);
    }
    if let Some(incomplete) = read_object_field(response, "incomplete_details") {
        if let Some(reason) = read_string_field(&incomplete, "reason") {
            return format!("incomplete: {}", reason);
        }
    }
    "Unknown error (no error details in response)".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ContentPart, Role, UrlPayload};

    #[test]
    fn provider_exposes_name_model_and_capability() {
        let provider = OpenAIResponsesChatProvider::new(OpenAIResponsesOptions {
            api_key: Some("sk-test".into()),
            base_url: None,
            model: "gpt-4o-mini".into(),
            max_output_tokens: None,
            default_headers: None,
            tool_message_conversion: None,
            http_client: None,
        });
        assert_eq!(provider.name(), "openai-responses");
        assert_eq!(provider.model_name(), "gpt-4o-mini");
        let cap = provider.get_capability(None);
        assert!(cap.tool_use);
        assert!(cap.image_in);
        assert!(!cap.thinking);
    }

    #[test]
    fn thinking_effort_round_trips_via_reasoning_effort() {
        let provider = OpenAIResponsesChatProvider::new(OpenAIResponsesOptions {
            api_key: Some("sk-test".into()),
            base_url: None,
            model: "o3-mini".into(),
            max_output_tokens: None,
            default_headers: None,
            tool_message_conversion: None,
            http_client: None,
        })
        .with_thinking(ThinkingEffort::High);
        assert_eq!(provider.thinking_effort(), Some(ThinkingEffort::High));
    }

    #[test]
    fn with_max_completion_tokens_sets_max_output_tokens() {
        let provider = OpenAIResponsesChatProvider::new(OpenAIResponsesOptions {
            api_key: Some("sk-test".into()),
            base_url: None,
            model: "gpt-4o-mini".into(),
            max_output_tokens: None,
            default_headers: None,
            tool_message_conversion: None,
            http_client: None,
        });
        let cloned = provider.with_max_completion_tokens(123).unwrap();
        assert_eq!(cloned.model_name(), "gpt-4o-mini");
    }

    #[test]
    fn convert_user_text_message() {
        let msg = Message::user_text("Hello");
        let items = convert_message(&msg, "gpt-4o-mini", None);
        assert_eq!(items.len(), 1);
        let v = serde_json::to_value(&items[0]).unwrap();
        assert_eq!(v["type"], "message");
        assert_eq!(v["role"], "user");
        assert_eq!(v["content"][0]["type"], "input_text");
        assert_eq!(v["content"][0]["text"], "Hello");
    }

    #[test]
    fn convert_system_to_developer_for_reasoning_model() {
        let msg = Message {
            role: Role::System,
            name: None,
            content: vec![ContentPart::Text { text: "sys".into() }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let items = convert_message(&msg, "o3-mini", None);
        assert_eq!(items.len(), 1);
        let v = serde_json::to_value(&items[0]).unwrap();
        assert_eq!(v["role"], "developer");
    }

    #[test]
    fn convert_assistant_with_tool_call() {
        let msg = Message {
            role: Role::Assistant,
            name: None,
            content: vec![],
            tool_calls: vec![crate::message::ToolCall {
                call_type: "function".into(),
                id: "call_1".into(),
                name: "read".into(),
                arguments: Some("{}".into()),
                extras: None,
                stream_index: None,
            }],
            tool_call_id: None,
            partial: None,
        };
        let items = convert_message(&msg, "gpt-4o-mini", None);
        assert_eq!(items.len(), 1);
        let v = serde_json::to_value(&items[0]).unwrap();
        assert_eq!(v["type"], "function_call");
        assert_eq!(v["call_id"], "call_1");
        assert_eq!(v["name"], "read");
        assert_eq!(v["arguments"], "{}");
    }

    #[test]
    fn convert_tool_result_with_extract_text() {
        let msg = Message {
            role: Role::Tool,
            name: None,
            content: vec![
                ContentPart::Text { text: "a".into() },
                ContentPart::Text { text: "b".into() },
            ],
            tool_calls: vec![],
            tool_call_id: Some("call_1".into()),
            partial: None,
        };
        let items = convert_message(
            &msg,
            "gpt-4o-mini",
            Some(ToolMessageConversion::ExtractText),
        );
        assert_eq!(items.len(), 1);
        let v = serde_json::to_value(&items[0]).unwrap();
        assert_eq!(v["type"], "function_call_output");
        assert_eq!(v["output"], "ab");
    }

    #[test]
    fn convert_user_image_url() {
        let msg = Message {
            role: Role::User,
            name: None,
            content: vec![ContentPart::ImageUrl {
                image_url: UrlPayload {
                    url: "https://example.com/img.png".into(),
                    id: None,
                },
            }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let items = convert_message(&msg, "gpt-4o-mini", None);
        let v = serde_json::to_value(&items[0]).unwrap();
        assert_eq!(v["content"][0]["type"], "input_image");
        assert_eq!(v["content"][0]["image_url"], "https://example.com/img.png");
    }

    #[test]
    fn convert_think_parts_to_reasoning_item() {
        let msg = Message {
            role: Role::User,
            name: None,
            content: vec![
                ContentPart::Think {
                    think: "step1".into(),
                    encrypted: None,
                },
                ContentPart::Think {
                    think: "step2".into(),
                    encrypted: None,
                },
            ],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let items = convert_message(&msg, "gpt-4o-mini", None);
        assert_eq!(items.len(), 1);
        let v = serde_json::to_value(&items[0]).unwrap();
        assert_eq!(v["type"], "reasoning");
        assert_eq!(v["summary"][0]["text"], "step1");
        assert_eq!(v["summary"][1]["text"], "step2");
    }

    #[test]
    fn convert_tool_to_response_function_param() {
        let tool = Tool {
            name: "add".into(),
            description: "Add two integers.".into(),
            parameters: serde_json::json!({"type": "object", "properties": {"a": {"type": "integer"}}}),
        };
        let v = serde_json::to_value(convert_tool(&tool)).unwrap();
        assert_eq!(v["type"], "function");
        assert_eq!(v["name"], "add");
        assert_eq!(v["description"], "Add two integers.");
        assert_eq!(v["strict"], false);
    }

    #[test]
    fn build_request_body_includes_model_input_tools_and_reasoning() {
        let provider = OpenAIResponsesChatProvider {
            model: "o3-mini".into(),
            api_key: Some("sk-test".into()),
            base_url: "https://api.openai.com/v1".into(),
            default_headers: None,
            generation_kwargs: {
                let mut m = HashMap::new();
                m.insert("reasoning_effort".into(), Value::String("high".into()));
                m.insert("max_output_tokens".into(), Value::Number(100.into()));
                m
            },
            tool_message_conversion: None,
            http_client: Arc::new(ReqwestClient::new(reqwest::Client::new())),
        };

        let tool = Tool {
            name: "add".into(),
            description: "Add.".into(),
            parameters: serde_json::json!({"type": "object"}),
        };
        let history = vec![Message::user_text("1+1")];
        let body = provider
            .build_request_body("sys", &[tool], &history)
            .unwrap();

        assert_eq!(body["model"], "o3-mini");
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert!(body["input"].as_array().unwrap().len() >= 2); // system + user
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["reasoning"]["effort"], "high");
        assert_eq!(body["reasoning"]["summary"], "auto");
        assert!(body["include"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("reasoning.encrypted_content")));
        assert_eq!(body["max_output_tokens"], 100);
    }

    #[test]
    fn build_request_body_uses_developer_role_for_reasoning_model() {
        let provider = OpenAIResponsesChatProvider::new(OpenAIResponsesOptions {
            api_key: Some("sk-test".into()),
            base_url: None,
            model: "o3-mini".into(),
            max_output_tokens: None,
            default_headers: None,
            tool_message_conversion: None,
            http_client: None,
        });
        let body = provider.build_request_body("sys", &[], &[]).unwrap();
        let input = body["input"].as_array().unwrap();
        assert_eq!(input[0]["role"], "developer");
    }

    use crate::http_client::{HttpClient, HttpResponse};
    use crate::provider::ProviderRequestAuth;
    use std::sync::{Arc, Mutex};

    struct RecordingMockHttpClient {
        status: u16,
        body: Vec<u8>,
        requests: Arc<Mutex<Vec<(String, HashMap<String, String>, Value)>>>,
    }

    #[async_trait]
    impl HttpClient for RecordingMockHttpClient {
        async fn post_json(
            &self,
            url: &str,
            headers: HashMap<String, String>,
            body: Value,
        ) -> Result<HttpResponse, ChatProviderError> {
            self.requests
                .lock()
                .unwrap()
                .push((url.to_string(), headers, body));
            let chunks = vec![self.body.clone()];
            let stream =
                futures_util::stream::iter(chunks.into_iter().map(|c| Ok(bytes::Bytes::from(c))))
                    .boxed();
            Ok(HttpResponse::new(self.status, stream))
        }

        async fn post_multipart(
            &self,
            _url: &str,
            _headers: HashMap<String, String>,
            _parts: Vec<crate::http_client::MultipartPart>,
            _fields: HashMap<String, String>,
        ) -> Result<HttpResponse, ChatProviderError> {
            let chunks = vec![self.body.clone()];
            let stream =
                futures_util::stream::iter(chunks.into_iter().map(|c| Ok(bytes::Bytes::from(c))))
                    .boxed();
            Ok(HttpResponse::new(self.status, stream))
        }
    }

    #[tokio::test]
    async fn generate_posts_to_responses_endpoint() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let client = Arc::new(RecordingMockHttpClient {
            status: 200,
            body: br#"{"type":"response.created","response":{"id":"resp-1"}}
{"type":"response.completed","response":{"id":"resp-1","status":"completed","output":[],"usage":{"input_tokens":1,"output_tokens":1}}}"#.to_vec(),
            requests: Arc::clone(&requests),
        });
        let provider = OpenAIResponsesChatProvider::new(OpenAIResponsesOptions {
            api_key: Some("sk-test".into()),
            base_url: Some("http://mock".into()),
            model: "gpt-4o-mini".into(),
            max_output_tokens: None,
            default_headers: None,
            tool_message_conversion: None,
            http_client: Some(client),
        });
        let _ = provider
            .generate("sys", &[], &[Message::user_text("hi")], None)
            .await
            .unwrap();

        let reqs = requests.lock().unwrap();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].0, "http://mock/responses");
        assert_eq!(reqs[0].1["Authorization"], "Bearer sk-test");
        assert_eq!(reqs[0].2["model"], "gpt-4o-mini");
        assert_eq!(reqs[0].2["stream"], true);
    }

    #[tokio::test]
    async fn generate_rejects_missing_api_key() {
        let provider = OpenAIResponsesChatProvider::new(OpenAIResponsesOptions {
            api_key: None,
            base_url: Some("http://mock".into()),
            model: "gpt-4o-mini".into(),
            max_output_tokens: None,
            default_headers: None,
            tool_message_conversion: None,
            http_client: Some(Arc::new(crate::http_client::MockHttpClient::new(
                200,
                vec![],
            ))),
        });
        let err = provider.generate("", &[], &[], None).await.unwrap_err();
        assert!(matches!(err, ChatProviderError::MissingApiKey(_)));
    }

    #[tokio::test]
    async fn generate_prefers_request_auth_over_default() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let client = Arc::new(RecordingMockHttpClient {
            status: 200,
            body: br#"{"type":"response.created","response":{"id":"resp-1"}}
{"type":"response.completed","response":{"id":"resp-1","status":"completed","output":[],"usage":{"input_tokens":1,"output_tokens":1}}}"#.to_vec(),
            requests: Arc::clone(&requests),
        });
        let provider = OpenAIResponsesChatProvider::new(OpenAIResponsesOptions {
            api_key: Some("sk-default".into()),
            base_url: Some("http://mock".into()),
            model: "gpt-4o-mini".into(),
            max_output_tokens: None,
            default_headers: None,
            tool_message_conversion: None,
            http_client: Some(client),
        });
        let auth = ProviderRequestAuth {
            api_key: Some("sk-req".into()),
            headers: None,
        };
        let options = GenerateOptions {
            auth: Some(auth),
            ..Default::default()
        };
        let _ = provider
            .generate("", &[], &[Message::user_text("hi")], Some(options))
            .await
            .unwrap();
        let reqs = requests.lock().unwrap();
        assert_eq!(reqs[0].1["Authorization"], "Bearer sk-req");
    }

    #[tokio::test]
    async fn parses_non_stream_text_response() {
        let body = br#"{"id":"resp-1","status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"Hello world"}]}],"usage":{"input_tokens":10,"output_tokens":2}}"#;
        let msg = OpenAIResponsesStreamedMessage::from_non_stream_bytes(body.to_vec()).unwrap();
        assert_eq!(msg.id(), Some("resp-1".into()));
        assert_eq!(msg.finish_reason(), Some(FinishReason::Completed));
        assert_eq!(msg.raw_finish_reason(), Some("completed".into()));
        let usage = msg.usage().unwrap();
        assert_eq!(usage.input_other, 10);
        assert_eq!(usage.output, 2);
        let parts: Vec<_> = futures_util::StreamExt::collect(msg).await;
        assert_eq!(parts, vec![StreamedMessagePart::text("Hello world")]);
    }

    #[tokio::test]
    async fn parses_non_stream_function_call() {
        let body = br#"{"id":"resp-2","status":"completed","output":[{"type":"function_call","id":"fc-1","call_id":"call_1","name":"read","arguments":"{}"}]}"#;
        let msg = OpenAIResponsesStreamedMessage::from_non_stream_bytes(body.to_vec()).unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(msg).await;
        assert_eq!(parts.len(), 1);
        match &parts[0] {
            StreamedMessagePart::ToolCall(tc) => {
                assert_eq!(tc.id, "call_1");
                assert_eq!(tc.name, "read");
                assert_eq!(tc.arguments.as_deref(), Some("{}"));
            }
            _ => panic!("expected ToolCall"),
        }
    }

    #[tokio::test]
    async fn parses_non_stream_reasoning() {
        let body = br#"{"id":"resp-3","status":"completed","output":[{"type":"reasoning","encrypted_content":"enc","summary":[{"type":"summary_text","text":"step1"},{"type":"summary_text","text":"step2"}]}]}"#;
        let msg = OpenAIResponsesStreamedMessage::from_non_stream_bytes(body.to_vec()).unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(msg).await;
        assert_eq!(parts.len(), 2);
        match &parts[0] {
            StreamedMessagePart::Content(ContentPart::Think { think, encrypted }) => {
                assert_eq!(think, "step1");
                assert_eq!(encrypted.as_deref(), Some("enc"));
            }
            _ => panic!("expected Think"),
        }
        match &parts[1] {
            StreamedMessagePart::Content(ContentPart::Think { think, encrypted }) => {
                assert_eq!(think, "step2");
                assert_eq!(encrypted.as_deref(), Some("enc"));
            }
            _ => panic!("expected Think"),
        }
    }

    #[tokio::test]
    async fn parses_non_stream_incomplete_max_output_tokens() {
        let body = br#"{"id":"resp-4","status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},"output":[]}"#;
        let msg = OpenAIResponsesStreamedMessage::from_non_stream_bytes(body.to_vec()).unwrap();
        assert_eq!(msg.finish_reason(), Some(FinishReason::Truncated));
        assert_eq!(msg.raw_finish_reason(), Some("max_output_tokens".into()));
    }

    #[tokio::test]
    async fn parses_stream_text_deltas() {
        let body = br#"{"type":"response.created","response":{"id":"resp-stream-1"}}
{"type":"response.output_text.delta","delta":"Hello"}
{"type":"response.output_text.delta","delta":" world"}
{"type":"response.completed","response":{"id":"resp-stream-1","usage":{"input_tokens":5,"output_tokens":2},"status":"completed"}}"#;
        let msg = OpenAIResponsesStreamedMessage::from_stream_bytes(body.to_vec()).unwrap();
        assert_eq!(msg.id(), Some("resp-stream-1".into()));
        assert_eq!(msg.finish_reason(), Some(FinishReason::Completed));
        let parts: Vec<_> = futures_util::StreamExt::collect(msg).await;
        assert_eq!(
            parts,
            vec![
                StreamedMessagePart::text("Hello"),
                StreamedMessagePart::text(" world")
            ]
        );
    }

    #[tokio::test]
    async fn parses_stream_incomplete_max_output_tokens() {
        let body = br#"{"type":"response.incomplete","response":{"id":"resp-inc","status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},"usage":{"input_tokens":1,"output_tokens":1}}}"#;
        let msg = OpenAIResponsesStreamedMessage::from_stream_bytes(body.to_vec()).unwrap();
        assert_eq!(msg.finish_reason(), Some(FinishReason::Truncated));
        assert_eq!(msg.raw_finish_reason(), Some("max_output_tokens".into()));
    }

    #[tokio::test]
    async fn parses_stream_error_event() {
        let body = br#"{"type":"error","code":"rate_limit_exceeded","message":"Rate limited","param":null}"#;
        let err = OpenAIResponsesStreamedMessage::from_stream_bytes(body.to_vec()).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("OpenAI Responses stream error"));
        assert!(msg.contains("rate_limit_exceeded"));
        assert!(msg.contains("Rate limited"));
    }

    #[tokio::test]
    async fn parses_stream_response_failed() {
        let body = br#"{"type":"response.failed","response":{"error":{"code":"server_error","message":"Internal error"}}}"#;
        let err = OpenAIResponsesStreamedMessage::from_stream_bytes(body.to_vec()).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("OpenAI Responses response.failed"));
        assert!(msg.contains("server_error"));
        assert!(msg.contains("Internal error"));
    }

    #[tokio::test]
    async fn parses_stream_single_function_call() {
        let body = br#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"fc-1","call_id":"call_1","name":"read","arguments":""}}
{"type":"response.function_call_arguments.delta","item_id":"fc-1","output_index":0,"delta":"{\"a\":1"}
{"type":"response.function_call_arguments.delta","item_id":"fc-1","output_index":0,"delta":"}"}
{"type":"response.function_call_arguments.done","item_id":"fc-1","output_index":0,"arguments":"{\"a\":1}"}
{"type":"response.completed","response":{"status":"completed"}}"#;
        let msg = OpenAIResponsesStreamedMessage::from_stream_bytes(body.to_vec()).unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(msg).await;
        assert_eq!(parts.len(), 3);
        match &parts[0] {
            StreamedMessagePart::ToolCall(tc) => {
                assert_eq!(tc.id, "call_1");
                assert_eq!(tc.name, "read");
                assert_eq!(tc.arguments.as_deref(), Some(""));
                assert_eq!(
                    tc.stream_index.as_ref().and_then(|v| v.as_str()),
                    Some("fc-1")
                );
            }
            _ => panic!("expected ToolCall"),
        }
        assert_eq!(
            parts[1],
            StreamedMessagePart::ToolCallPart(ToolCallPart {
                part_type: "tool_call_part".into(),
                arguments_part: Some("{\"a\":1".into()),
                index: Some(serde_json::json!("fc-1")),
            })
        );
        assert_eq!(
            parts[2],
            StreamedMessagePart::ToolCallPart(ToolCallPart {
                part_type: "tool_call_part".into(),
                arguments_part: Some("}".into()),
                index: Some(serde_json::json!("fc-1")),
            })
        );
    }

    #[tokio::test]
    async fn parses_stream_parallel_function_calls_by_item_id() {
        let body = br#"{"type":"response.output_item.added","item":{"type":"function_call","id":"fc-a","call_id":"call_a","name":"read","arguments":""}}
{"type":"response.output_item.added","item":{"type":"function_call","id":"fc-b","call_id":"call_b","name":"write","arguments":""}}
{"type":"response.function_call_arguments.delta","item_id":"fc-b","delta":"{\"b\":2}"}
{"type":"response.function_call_arguments.delta","item_id":"fc-a","delta":"{\"a\":1}"}
{"type":"response.function_call_arguments.done","item_id":"fc-b","arguments":"{\"b\":2}"}
{"type":"response.function_call_arguments.done","item_id":"fc-a","arguments":"{\"a\":1}"}
{"type":"response.completed","response":{"status":"completed"}}"#;
        let msg = OpenAIResponsesStreamedMessage::from_stream_bytes(body.to_vec()).unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(msg).await;
        let deltas: Vec<_> = parts
            .iter()
            .filter_map(|p| match p {
                StreamedMessagePart::ToolCallPart(tp) => tp.arguments_part.clone(),
                _ => None,
            })
            .collect();
        assert_eq!(
            deltas,
            vec!["{\"b\":2}".to_string(), "{\"a\":1}".to_string()]
        );
    }

    #[tokio::test]
    async fn rejects_stream_final_arguments_suffix_mismatch() {
        let body = br#"{"type":"response.output_item.added","item":{"type":"function_call","id":"fc-1","call_id":"call_1","name":"read","arguments":""}}
{"type":"response.function_call_arguments.delta","item_id":"fc-1","delta":"{\"a\":1"}
{"type":"response.function_call_arguments.done","item_id":"fc-1","arguments":"{\"b\":2}"}
{"type":"response.completed","response":{"status":"completed"}}"#;
        let err = OpenAIResponsesStreamedMessage::from_stream_bytes(body.to_vec()).unwrap_err();
        assert!(format!("{}", err).contains("do not match the streamed argument deltas"));
    }

    #[tokio::test]
    async fn parses_stream_output_item_done_with_final_arguments_suffix() {
        let body = br#"{"type":"response.output_item.added","item":{"type":"function_call","id":"fc-1","call_id":"call_1","name":"read","arguments":"{\"a\":1"}}
{"type":"response.output_item.done","item":{"type":"function_call","id":"fc-1","call_id":"call_1","name":"read","arguments":"{\"a\":123}"},"output_index":7}
{"type":"response.completed","response":{"status":"completed"}}"#;
        let msg = OpenAIResponsesStreamedMessage::from_stream_bytes(body.to_vec()).unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(msg).await;
        assert_eq!(parts.len(), 2);
        match &parts[0] {
            StreamedMessagePart::ToolCall(tc) => {
                assert_eq!(
                    tc.stream_index.as_ref().and_then(|v| v.as_str()),
                    Some("fc-1")
                );
            }
            _ => panic!("expected ToolCall"),
        }
        match &parts[1] {
            StreamedMessagePart::ToolCallPart(tp) => {
                assert_eq!(tp.arguments_part.as_deref(), Some("23}"));
                assert_eq!(tp.index.as_ref().and_then(|v| v.as_str()), Some("fc-1"));
            }
            _ => panic!("expected ToolCallPart"),
        }
    }

    #[tokio::test]
    async fn parses_stream_reasoning_summary() {
        let body = br#"{"type":"response.reasoning_summary_part.added"}
{"type":"response.reasoning_summary_text.delta","delta":"step1"}
{"type":"response.reasoning_summary_text.delta","delta":" step2"}
{"type":"response.output_item.done","item":{"type":"reasoning","encrypted_content":"enc","summary":[]}}
{"type":"response.completed","response":{"status":"completed"}}"#;
        let msg = OpenAIResponsesStreamedMessage::from_stream_bytes(body.to_vec()).unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(msg).await;
        assert_eq!(parts.len(), 4);
        match &parts[0] {
            StreamedMessagePart::Content(ContentPart::Think { think, encrypted }) => {
                assert_eq!(think, "");
                assert!(encrypted.is_none());
            }
            _ => panic!("expected Think"),
        }
        match &parts[1] {
            StreamedMessagePart::Content(ContentPart::Think { think, .. }) => {
                assert_eq!(think, "step1")
            }
            _ => panic!("expected Think"),
        }
        match &parts[2] {
            StreamedMessagePart::Content(ContentPart::Think { think, .. }) => {
                assert_eq!(think, " step2")
            }
            _ => panic!("expected Think"),
        }
        match &parts[3] {
            StreamedMessagePart::Content(ContentPart::Think { think, encrypted }) => {
                assert_eq!(think, "");
                assert_eq!(encrypted.as_deref(), Some("enc"));
            }
            _ => panic!("expected Think"),
        }
    }

    #[tokio::test]
    async fn generate_parses_stream_response() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let body = br#"{"type":"response.created","response":{"id":"resp-real"}}
{"type":"response.output_text.delta","delta":"Hi"}
{"type":"response.completed","response":{"id":"resp-real","status":"completed","usage":{"input_tokens":3,"output_tokens":1}}}"#;
        let client = Arc::new(RecordingMockHttpClient {
            status: 200,
            body: body.to_vec(),
            requests: Arc::clone(&requests),
        });
        let provider = OpenAIResponsesChatProvider::new(OpenAIResponsesOptions {
            api_key: Some("sk-test".into()),
            base_url: Some("http://mock".into()),
            model: "gpt-4o-mini".into(),
            max_output_tokens: None,
            default_headers: None,
            tool_message_conversion: None,
            http_client: Some(client),
        });
        let msg = provider
            .generate("sys", &[], &[Message::user_text("hi")], None)
            .await
            .unwrap();
        assert_eq!(msg.id(), Some("resp-real".into()));
        let parts: Vec<_> = futures_util::StreamExt::collect(msg).await;
        assert_eq!(parts, vec![StreamedMessagePart::text("Hi")]);
    }
}
