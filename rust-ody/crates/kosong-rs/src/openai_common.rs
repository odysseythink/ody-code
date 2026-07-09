use crate::errors::{
    is_context_overflow_error_code, normalize_api_status_error, APIConnectionError,
    APIContextOverflowError, APITimeoutError, ChatProviderError,
};
use crate::message::{ContentPart, Message};
use crate::provider::{FinishReason, ThinkingEffort, Tool};
use crate::usage::TokenUsage;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolMessageConversion {
    ExtractText,
    Standard,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAIContentPart {
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "image_url")]
    pub image_url: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "audio_url")]
    pub audio_url: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "video_url")]
    pub video_url: Option<Value>,
}
pub fn convert_content_part(part: &ContentPart) -> Option<OpenAIContentPart> {
    match part {
        ContentPart::Text { text } => Some(OpenAIContentPart {
            r#type: "text".into(),
            text: Some(text.clone()),
            image_url: None,
            audio_url: None,
            video_url: None,
        }),
        ContentPart::Think { .. } => None,
        ContentPart::ImageUrl { image_url } => Some(OpenAIContentPart {
            r#type: "image_url".into(),
            text: None,
            image_url: Some(serde_json::to_value(image_url).unwrap()),
            audio_url: None,
            video_url: None,
        }),
        ContentPart::AudioUrl { audio_url } => Some(OpenAIContentPart {
            r#type: "audio_url".into(),
            text: None,
            image_url: None,
            audio_url: Some(serde_json::to_value(audio_url).unwrap()),
            video_url: None,
        }),
        ContentPart::VideoUrl { video_url } => Some(OpenAIContentPart {
            r#type: "video_url".into(),
            text: None,
            image_url: None,
            audio_url: None,
            video_url: Some(serde_json::to_value(video_url).unwrap()),
        }),
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct OpenAIToolParam {
    pub r#type: String,
    pub function: OpenAIToolFunction,
}
#[derive(Debug, Clone, Serialize)]
pub struct OpenAIToolFunction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
}
pub fn tool_to_openai(tool: &Tool) -> OpenAIToolParam {
    OpenAIToolParam {
        r#type: "function".into(),
        function: OpenAIToolFunction {
            name: tool.name.clone(),
            description: Some(tool.description.clone()).filter(|d| !d.is_empty()),
            parameters: Some(tool.parameters.clone()).filter(|p| !p.is_null()),
        },
    }
}
pub fn extract_usage(usage: &Value) -> Option<TokenUsage> {
    if !usage.is_object() {
        return None;
    }
    let prompt_tokens = usage
        .get("prompt_tokens")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let completion_tokens = usage
        .get("completion_tokens")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let cached = usage
        .get("cached_tokens")
        .and_then(|v| v.as_i64())
        .or_else(|| {
            usage
                .get("prompt_tokens_details")
                .and_then(|d| d.as_object())
                .and_then(|d| d.get("cached_tokens"))
                .and_then(|v| v.as_i64())
        })
        .unwrap_or(0);
    Some(TokenUsage {
        input_other: prompt_tokens - cached,
        output: completion_tokens,
        input_cache_read: cached,
        input_cache_creation: 0,
    })
}
pub fn normalize_openai_finish_reason(raw: Option<&str>) -> (Option<FinishReason>, Option<String>) {
    match raw {
        None => (None, None),
        Some("stop") => (Some(FinishReason::Completed), Some("stop".into())),
        Some("tool_calls") | Some("function_call") => {
            let s = raw.unwrap().to_string();
            (Some(FinishReason::ToolCalls), Some(s))
        }
        Some("length") => (Some(FinishReason::Truncated), Some("length".into())),
        Some("content_filter") => (Some(FinishReason::Filtered), Some("content_filter".into())),
        Some(other) => (Some(FinishReason::Other), Some(other.into())),
    }
}
pub fn thinking_effort_to_reasoning_effort(effort: ThinkingEffort) -> Option<String> {
    match effort {
        ThinkingEffort::Off => None,
        ThinkingEffort::Low => Some("low".into()),
        ThinkingEffort::Medium => Some("medium".into()),
        ThinkingEffort::High => Some("high".into()),
        ThinkingEffort::Xhigh | ThinkingEffort::Max => Some("xhigh".into()),
    }
}
pub fn reasoning_effort_to_thinking_effort(reasoning: Option<&str>) -> Option<ThinkingEffort> {
    match reasoning {
        None | Some("none") => None,
        Some("low") | Some("minimal") => Some(ThinkingEffort::Low),
        Some("medium") => Some(ThinkingEffort::Medium),
        Some("high") => Some(ThinkingEffort::High),
        Some("xhigh") | Some("max") => Some(ThinkingEffort::Xhigh),
        _ => Some(ThinkingEffort::Off),
    }
}
pub fn convert_tool_message_content(
    message: &Message,
    conversion: ToolMessageConversion,
) -> Option<Value> {
    let non_think: Vec<&ContentPart> = message
        .content
        .iter()
        .filter(|p| !matches!(p, ContentPart::Think { .. }))
        .collect();
    if non_think.is_empty() {
        return None;
    }
    if matches!(conversion, ToolMessageConversion::ExtractText)
        || non_think
            .iter()
            .any(|p| !matches!(p, ContentPart::Text { .. }))
    {
        return Some(Value::String(crate::message::extract_text(message, "")));
    }
    if non_think.len() == 1 {
        if let ContentPart::Text { text } = non_think[0] {
            return Some(Value::String(text.clone()));
        }
    }
    let parts: Vec<Value> = non_think
        .iter()
        .filter_map(|p| convert_content_part(p).map(|c| serde_json::to_value(c).unwrap()))
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(Value::Array(parts))
    }
}
pub fn convert_openai_error(
    message: &str,
    status_code: Option<u16>,
    error_code: Option<&str>,
) -> ChatProviderError {
    if let Some(code) = error_code {
        if is_context_overflow_error_code(Some(code)) {
            return ChatProviderError::ContextOverflow(APIContextOverflowError {
                status_code: status_code.unwrap_or(0),
                message: message.into(),
                request_id: None,
            });
        }
    }
    if let Some(status) = status_code {
        return normalize_api_status_error(status, message, None);
    }
    let re_network = Regex::new(r"(?i)network|connection|connect|disconnect|terminated").unwrap();
    let re_timeout = Regex::new(r"(?i)timed?\s*out|timeout|deadline").unwrap();
    if re_timeout.is_match(message) {
        ChatProviderError::Timeout(APITimeoutError)
    } else if re_network.is_match(message) {
        ChatProviderError::Connection(APIConnectionError)
    } else {
        ChatProviderError::Other(format!("Error: {message}"))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Message, Role};
    #[test]
    fn convert_text_part() {
        let part = ContentPart::Text { text: "hi".into() };
        let out = convert_content_part(&part).unwrap();
        assert_eq!(out.r#type, "text");
        assert_eq!(out.text, Some("hi".into()));
    }
    #[test]
    fn think_part_returns_none() {
        let part = ContentPart::Think {
            think: "x".into(),
            encrypted: None,
        };
        assert!(convert_content_part(&part).is_none());
    }
    #[test]
    fn extract_usage_prefers_top_level_cached_tokens() {
        let usage =
            serde_json::json!({"prompt_tokens":100,"completion_tokens":20,"cached_tokens":30});
        let u = extract_usage(&usage).unwrap();
        assert_eq!(u.input_other, 70);
        assert_eq!(u.output, 20);
        assert_eq!(u.input_cache_read, 30);
    }
    #[test]
    fn extract_usage_falls_back_to_prompt_tokens_details() {
        let usage = serde_json::json!({"prompt_tokens":100,"completion_tokens":20,"prompt_tokens_details":{"cached_tokens":25}});
        let u = extract_usage(&usage).unwrap();
        assert_eq!(u.input_cache_read, 25);
        assert_eq!(u.input_other, 75);
    }
    #[test]
    fn finish_reason_mappings() {
        assert_eq!(
            normalize_openai_finish_reason(Some("stop")),
            (Some(FinishReason::Completed), Some("stop".into()))
        );
        assert_eq!(
            normalize_openai_finish_reason(Some("tool_calls")),
            (Some(FinishReason::ToolCalls), Some("tool_calls".into()))
        );
        assert_eq!(
            normalize_openai_finish_reason(Some("length")),
            (Some(FinishReason::Truncated), Some("length".into()))
        );
        assert_eq!(normalize_openai_finish_reason(None), (None, None));
    }
    #[test]
    fn reasoning_effort_round_trip() {
        assert_eq!(
            reasoning_effort_to_thinking_effort(Some("xhigh")),
            Some(ThinkingEffort::Xhigh)
        );
        assert_eq!(
            thinking_effort_to_reasoning_effort(ThinkingEffort::Max),
            Some("xhigh".into())
        );
    }
    #[test]
    fn convert_tool_message_extracts_text() {
        let msg = Message {
            role: Role::Tool,
            name: None,
            content: vec![
                ContentPart::Text { text: "a".into() },
                ContentPart::Text { text: "b".into() },
            ],
            tool_calls: vec![],
            tool_call_id: Some("tc1".into()),
            partial: None,
        };
        let value = convert_tool_message_content(&msg, ToolMessageConversion::ExtractText);
        assert_eq!(value, Some(Value::String("ab".into())));
    }
    #[test]
    fn convert_openai_error_classifies_timeout() {
        let err = convert_openai_error("Request timed out", None, None);
        assert!(matches!(err, ChatProviderError::Timeout(_)));
    }
    #[test]
    fn convert_openai_error_classifies_context_overflow_code() {
        let err = convert_openai_error("too long", Some(400), Some("context_length_exceeded"));
        assert!(matches!(err, ChatProviderError::ContextOverflow(_)));
    }
}
