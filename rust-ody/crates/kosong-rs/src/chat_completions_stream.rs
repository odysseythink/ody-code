use crate::errors::ChatProviderError;
use crate::message::{ContentPart, StreamedMessagePart, ToolCall, ToolCallPart};
use crate::openai_common::{extract_usage, normalize_openai_finish_reason};
use crate::provider::FinishReason;
use crate::usage::TokenUsage;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct BufferedChatCompletionToolCall {
    pub id: Option<String>,
    pub arguments: String,
    pub emitted: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct ChatCompletionChunk {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(default)]
    choices: Vec<ChatCompletionChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<Value>,
}
#[derive(Debug, Deserialize, Serialize)]
struct ChatCompletionChoice {
    #[serde(default)]
    index: i64,
    delta: ChatCompletionDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    finish_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<Value>,
}
#[derive(Debug, Deserialize, Serialize, Default)]
struct ChatCompletionDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ChatCompletionToolCallDelta>>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}
#[derive(Debug, Deserialize, Serialize)]
struct ChatCompletionToolCallDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    index: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "type")]
    call_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    function: Option<ChatCompletionToolFunctionDelta>,
}
#[derive(Debug, Deserialize, Serialize)]
struct ChatCompletionToolFunctionDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<String>,
}

const KNOWN_REASONING_KEYS: &[&str] = &["reasoning_content", "reasoning_details", "reasoning"];

fn extract_reasoning_content(source: &Value, explicit_key: Option<&str>) -> Option<String> {
    let obj = source.as_object()?;
    let keys: Vec<&str> = explicit_key
        .map(|k| vec![k])
        .unwrap_or_else(|| KNOWN_REASONING_KEYS.to_vec());
    for key in keys {
        if let Some(Value::String(s)) = obj.get(key) {
            if !s.is_empty() {
                return Some(s.clone());
            }
        }
    }
    None
}

fn convert_chat_completion_stream_tool_call(
    tool_call: &ChatCompletionToolCallDelta,
    buffered_by_index: &mut HashMap<String, BufferedChatCompletionToolCall>,
) -> Vec<StreamedMessagePart> {
    let function = match &tool_call.function {
        Some(f) => f,
        None => return vec![],
    };
    let stream_index = tool_call.index.as_ref().map(|v| v.to_string());
    let name = function.name.as_deref().filter(|s| !s.is_empty());
    let arguments = function.arguments.as_deref().filter(|s| !s.is_empty());
    if stream_index.is_none() {
        if let Some(name) = name {
            return vec![StreamedMessagePart::ToolCall(ToolCall {
                call_type: "function".into(),
                id: tool_call.id.clone().unwrap_or_else(|| "tc".into()),
                name: name.into(),
                arguments: arguments.map(Into::into),
                extras: None,
                stream_index: None,
            })];
        }
        if let Some(args) = arguments {
            return vec![StreamedMessagePart::ToolCallPart(ToolCallPart {
                part_type: "tool_call_part".into(),
                arguments_part: Some(args.into()),
                index: None,
            })];
        }
        return vec![];
    }
    let idx = stream_index.unwrap();
    let buffered = buffered_by_index
        .entry(idx.clone())
        .or_insert_with(BufferedChatCompletionToolCall::default);
    if let Some(id) = &tool_call.id {
        buffered.id = Some(id.clone());
    }
    if !buffered.emitted {
        if name.is_none() {
            if let Some(args) = arguments {
                buffered.arguments.push_str(args);
            }
            return vec![];
        }
        buffered.emitted = true;
        let initial_args = if buffered.arguments.is_empty() {
            arguments.map(Into::into)
        } else {
            Some(format!("{}{}", buffered.arguments, arguments.unwrap_or("")))
        };
        let tool_call_id = buffered
            .id
            .clone()
            .or_else(|| tool_call.id.clone())
            .unwrap_or_else(|| "tc".into());
        buffered.arguments.clear();
        return vec![StreamedMessagePart::ToolCall(ToolCall {
            call_type: "function".into(),
            id: tool_call_id,
            name: name.unwrap().into(),
            arguments: initial_args,
            extras: None,
            stream_index: Some(tool_call.index.clone().unwrap()),
        })];
    }
    if let Some(args) = arguments {
        return vec![StreamedMessagePart::ToolCallPart(ToolCallPart {
            part_type: "tool_call_part".into(),
            arguments_part: Some(args.into()),
            index: tool_call.index.clone(),
        })];
    }
    vec![]
}

pub async fn parse_stream_response_with_usage_extractor(
    body: Vec<u8>,
    reasoning_key: Option<&str>,
    usage_extractor: fn(&serde_json::Value) -> Option<Value>,
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
    let text = String::from_utf8_lossy(&body);
    let mut parts = Vec::new();
    let mut id: Option<String> = None;
    let mut usage: Option<TokenUsage> = None;
    let mut finish_reason: Option<FinishReason> = None;
    let mut raw_finish_reason: Option<String> = None;
    let mut buffered_tool_calls: HashMap<String, BufferedChatCompletionToolCall> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || !line.starts_with("data: ") {
            continue;
        }
        let data = &line[6..];
        if data == "[DONE]" {
            break;
        }
        let chunk: ChatCompletionChunk = match serde_json::from_str(data) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Some(cid) = chunk.id.clone() {
            id = Some(cid);
        }
        let raw_usage = chunk
            .usage
            .clone()
            .or_else(|| usage_extractor(&serde_json::to_value(&chunk).unwrap()));
        if let Some(u) = raw_usage {
            usage = extract_usage(&u);
        }
        for choice in chunk.choices {
            if let Some(raw) = &choice.finish_reason {
                let (fr, rfr) = normalize_openai_finish_reason(Some(raw.as_str()));
                finish_reason = fr.or(finish_reason);
                raw_finish_reason = rfr.or(raw_finish_reason);
            }
            let dv = serde_json::to_value(&choice.delta).unwrap();
            if let Some(reasoning) = extract_reasoning_content(&dv, reasoning_key) {
                parts.push(StreamedMessagePart::Content(ContentPart::Think {
                    think: reasoning,
                    encrypted: None,
                }));
            }
            if let Some(content) = choice.delta.content {
                parts.push(StreamedMessagePart::text(content));
            }
            if let Some(tool_calls) = choice.delta.tool_calls {
                for tc in tool_calls {
                    parts.extend(convert_chat_completion_stream_tool_call(
                        &tc,
                        &mut buffered_tool_calls,
                    ));
                }
            }
        }
    }
    Ok((parts, id, usage, finish_reason, raw_finish_reason))
}

pub async fn parse_stream_response(
    body: Vec<u8>,
    reasoning_key: Option<&str>,
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
    parse_stream_response_with_usage_extractor(body, reasoning_key, |_value| None).await
}

#[derive(Debug, Deserialize)]
struct ChatCompletion {
    id: String,
    #[serde(default)]
    choices: Vec<ChatCompletionNonStreamChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<Value>,
}
#[derive(Debug, Deserialize)]
struct ChatCompletionNonStreamChoice {
    message: ChatCompletionMessage,
    #[serde(skip_serializing_if = "Option::is_none")]
    finish_reason: Option<String>,
}
#[derive(Debug, Deserialize, Serialize)]
struct ChatCompletionMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
    #[serde(default, rename = "tool_calls")]
    tool_calls: Vec<ChatCompletionNonStreamToolCall>,
}
#[derive(Debug, Deserialize, Serialize)]
struct ChatCompletionNonStreamToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: ChatCompletionNonStreamFunction,
}
#[derive(Debug, Deserialize, Serialize)]
struct ChatCompletionNonStreamFunction {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<String>,
}

pub fn parse_non_stream_response(
    body: &[u8],
    reasoning_key: Option<&str>,
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
    let completion: ChatCompletion =
        serde_json::from_slice(body).map_err(|e| ChatProviderError::Other(e.to_string()))?;
    let mut parts = Vec::new();
    let choice = completion.choices.into_iter().next();
    let (finish_reason, raw_finish_reason) = choice
        .as_ref()
        .and_then(|c| c.finish_reason.as_deref())
        .map(|raw| normalize_openai_finish_reason(Some(raw)))
        .unwrap_or((None, None));
    if let Some(msg) = choice.map(|c| c.message) {
        let mv = serde_json::to_value(&msg).unwrap();
        if let Some(reasoning) = extract_reasoning_content(&mv, reasoning_key) {
            parts.push(StreamedMessagePart::Content(ContentPart::Think {
                think: reasoning,
                encrypted: None,
            }));
        }
        if let Some(content) = msg.content {
            if !content.is_empty() {
                parts.push(StreamedMessagePart::text(content));
            }
        }
        for tc in msg.tool_calls {
            parts.push(StreamedMessagePart::ToolCall(ToolCall {
                call_type: tc.call_type,
                id: tc.id,
                name: tc.function.name,
                arguments: tc.function.arguments,
                extras: None,
                stream_index: None,
            }));
        }
    }
    let usage = completion.usage.and_then(|u| extract_usage(&u));
    Ok((
        parts,
        Some(completion.id),
        usage,
        finish_reason,
        raw_finish_reason,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn parses_text_stream() {
        let sse = "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hello\"}}]}\n\ndata: {\"id\":\"2\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"}}]}\n\ndata: {\"id\":\"3\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
        let (parts, id, usage, finish, raw) =
            parse_stream_response(sse.into(), None).await.unwrap();
        assert_eq!(id, Some("3".into()));
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], StreamedMessagePart::text("Hello"));
        assert_eq!(parts[1], StreamedMessagePart::text(" world"));
        assert_eq!(finish, Some(FinishReason::Completed));
        assert_eq!(raw, Some("stop".into()));
        assert!(usage.is_none());
    }
    #[tokio::test]
    async fn routes_parallel_tool_call_deltas() {
        let sse = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"name\":\"read\"}}]}}]}\n\ndata: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":1,\"function\":{\"name\":\"write\"}}]}}]}\n\ndata: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"a\\\":1\"}}]}}]}\n\ndata: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":1,\"function\":{\"arguments\":\"{\\\"b\\\":2\"}}]}}]}\n\ndata: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"}\"}}]}}]}\n\ndata: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":1,\"function\":{\"arguments\":\"}\"}}]}}]}\n\n";
        let (parts, _, _, _, _) = parse_stream_response(sse.into(), None).await.unwrap();
        let calls: Vec<_> = parts
            .iter()
            .filter_map(|p| match p {
                StreamedMessagePart::ToolCall(tc) => Some(tc),
                _ => None,
            })
            .collect();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "read");
        assert_eq!(calls[1].name, "write");
    }
    #[test]
    fn parses_non_stream_with_tool_call() {
        let body = br#"{"id":"chat-1","choices":[{"message":{"content":"ok","tool_calls":[{"id":"tc1","type":"function","function":{"name":"read","arguments":"{}"}}]},"finish_reason":"tool_calls"}]}"#;
        let (_parts, id, _, finish, raw) = parse_non_stream_response(body, None).unwrap();
        assert_eq!(id, Some("chat-1".into()));
        assert_eq!(finish, Some(FinishReason::ToolCalls));
        assert_eq!(raw, Some("tool_calls".into()));
    }
}

#[cfg(test)]
mod usage_extractor_tests {
    use super::*;

    fn kimi_usage_extractor(value: &serde_json::Value) -> Option<Value> {
        value
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|c| c.get("usage"))
            .cloned()
    }

    #[tokio::test]
    async fn default_extractor_prefers_top_level_usage() {
        let sse = r#"data: {"id":"1","choices":[{"index":0,"delta":{"content":"hi"}}],"usage":{"prompt_tokens":10,"completion_tokens":2,"cached_tokens":3}}

data: [DONE]

"#;
        let (parts, _, usage, _, _) = parse_stream_response(sse.into(), None).await.unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0], StreamedMessagePart::text("hi"));
        let u = usage.unwrap();
        assert_eq!(u.input_other, 7);
        assert_eq!(u.input_cache_read, 3);
        assert_eq!(u.output, 2);
    }

    #[tokio::test]
    async fn kimi_extractor_reads_choice_usage() {
        let sse = r#"data: {"id":"1","choices":[{"index":0,"delta":{"content":"hi"},"usage":{"prompt_tokens":20,"completion_tokens":4,"cached_tokens":5}}]}

data: [DONE]

"#;
        let (parts, _, usage, _, _) =
            parse_stream_response_with_usage_extractor(sse.into(), None, kimi_usage_extractor)
                .await
                .unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0], StreamedMessagePart::text("hi"));
        let u = usage.unwrap();
        assert_eq!(u.input_other, 15);
        assert_eq!(u.input_cache_read, 5);
        assert_eq!(u.output, 4);
    }

    #[tokio::test]
    async fn top_level_usage_wins_over_choice_usage() {
        let sse = r#"data: {"id":"1","choices":[{"index":0,"delta":{"content":"hi"},"usage":{"prompt_tokens":20,"completion_tokens":4,"cached_tokens":5}}],"usage":{"prompt_tokens":100,"completion_tokens":10,"cached_tokens":30}}

data: [DONE]

"#;
        let (parts, _, usage, _, _) =
            parse_stream_response_with_usage_extractor(sse.into(), None, kimi_usage_extractor)
                .await
                .unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0], StreamedMessagePart::text("hi"));
        let u = usage.unwrap();
        assert_eq!(u.input_other, 70); // top-level wins
        assert_eq!(u.input_cache_read, 30);
        assert_eq!(u.output, 10);
    }
}
