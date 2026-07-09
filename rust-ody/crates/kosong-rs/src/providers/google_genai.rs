use crate::capability_registry::get_google_genai_model_capability;
use crate::provider::{ChatProvider, ModelCapability, ThinkingEffort};

pub struct GoogleGenAIChatProvider {
    model: String,
    api_key: Option<String>,
    vertexai: bool,
    project: Option<String>,
    location: Option<String>,
    stream: bool,
    base_url: Option<String>,
    generation_kwargs: serde_json::Value,
}

impl GoogleGenAIChatProvider {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            api_key: None,
            vertexai: false,
            project: None,
            location: None,
            stream: true,
            base_url: None,
            generation_kwargs: serde_json::Value::Object(Default::default()),
        }
    }

    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        let key = api_key.into();
        self.api_key = if key.is_empty() { None } else { Some(key) };
        self
    }

    pub fn with_vertexai(
        mut self,
        project: impl Into<String>,
        location: impl Into<String>,
    ) -> Self {
        self.vertexai = true;
        self.project = Some(project.into());
        self.location = Some(location.into());
        self
    }

    pub fn with_stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        let url = base_url.into();
        self.base_url = if url.is_empty() { None } else { Some(url) };
        self
    }
}

impl Clone for GoogleGenAIChatProvider {
    fn clone(&self) -> Self {
        Self {
            model: self.model.clone(),
            api_key: self.api_key.clone(),
            vertexai: self.vertexai,
            project: self.project.clone(),
            location: self.location.clone(),
            stream: self.stream,
            base_url: self.base_url.clone(),
            generation_kwargs: self.generation_kwargs.clone(),
        }
    }
}

// --- Conversion: Message → Google contents ---

use crate::message::{ContentPart, Message, Role, ToolCall};
use crate::provider::Tool;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GooglePart {
    Text {
        text: String,
    },
    InlineData {
        mime_type: String,
        data: String,
    },
    FileData {
        file_uri: String,
        mime_type: String,
    },
    FunctionCall {
        name: String,
        args: serde_json::Value,
        thought_signature: Option<String>,
    },
    FunctionResponse {
        name: String,
        response: serde_json::Value,
        parts: Vec<serde_json::Value>,
    },
}

impl GooglePart {
    pub(crate) fn text(s: impl Into<String>) -> Self {
        Self::Text { text: s.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct GoogleContent {
    pub role: String,
    pub parts: Vec<GooglePart>,
}

pub(crate) fn tool_to_google_genai(tool: &Tool) -> serde_json::Value {
    serde_json::json!({
        "functionDeclarations": [{
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.parameters,
        }]
    })
}

fn convert_media_url(url: &str, fallback_mime: &str) -> GooglePart {
    if let Some(rest) = url.strip_prefix("data:") {
        let comma = rest.find(',').unwrap_or(rest.len());
        let meta = &rest[..comma];
        let base64_offset = if comma < rest.len() { comma + 1 } else { comma };
        let data = if base64_offset <= rest.len() {
            &rest[base64_offset..]
        } else {
            ""
        };
        let mime = meta.split_once(';').map(|(m, _)| m).unwrap_or(meta);
        return GooglePart::InlineData {
            mime_type: mime.to_string(),
            data: data.to_string(),
        };
    }
    let mime = std::path::Path::new(url)
        .extension()
        .and_then(|e| e.to_str())
        .map(|ext| match ext.to_lowercase().as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "mp3" | "mpeg" => "audio/mpeg",
            "wav" => "audio/wav",
            "ogg" => "audio/ogg",
            "mp4" => "video/mp4",
            _ => fallback_mime,
        })
        .unwrap_or(fallback_mime);
    GooglePart::FileData {
        file_uri: url.to_string(),
        mime_type: mime.to_string(),
    }
}

fn content_part_to_google(part: &ContentPart) -> Option<GooglePart> {
    Some(match part {
        ContentPart::Text { text } => GooglePart::text(text.clone()),
        ContentPart::Think { .. } => return None,
        ContentPart::ImageUrl { image_url } => convert_media_url(&image_url.url, "image/jpeg"),
        ContentPart::AudioUrl { audio_url } => convert_media_url(&audio_url.url, "audio/mpeg"),
        ContentPart::VideoUrl { video_url } => convert_media_url(&video_url.url, "video/mp4"),
    })
}

fn parse_tool_arguments(
    arguments: Option<&str>,
) -> Result<serde_json::Value, crate::errors::ChatProviderError> {
    match arguments {
        None | Some("") => Ok(serde_json::Value::Object(Default::default())),
        Some(s) => {
            let v: serde_json::Value = serde_json::from_str(s).map_err(|_| {
                crate::errors::ChatProviderError::Other(format!(
                    "Tool call arguments must be valid JSON: {s}"
                ))
            })?;
            if !v.is_object() {
                return Err(crate::errors::ChatProviderError::Other(
                    "Tool call arguments must be a JSON object.".into(),
                ));
            }
            Ok(v)
        }
    }
}

fn tool_call_to_google(
    tool_call: &ToolCall,
) -> Result<GooglePart, crate::errors::ChatProviderError> {
    let args = parse_tool_arguments(tool_call.arguments.as_deref())?;
    Ok(GooglePart::FunctionCall {
        name: tool_call.name.clone(),
        args,
        thought_signature: tool_call
            .extras
            .as_ref()
            .and_then(|e| e.get("thought_signature_b64"))
            .and_then(|v| v.as_str())
            .map(Into::into),
    })
}

fn tool_call_id_to_name(
    id: &str,
    tool_name_by_id: &std::collections::HashMap<String, String>,
) -> String {
    if let Some(name) = tool_name_by_id.get(id) {
        return name.clone();
    }
    let re = regex::Regex::new(r"^(.+)_[^_]+$").unwrap();
    re.captures(id)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| id.to_string())
}

fn tool_message_to_parts(
    message: &Message,
    tool_name_by_id: &std::collections::HashMap<String, String>,
) -> Result<Vec<GooglePart>, crate::errors::ChatProviderError> {
    let id = message.tool_call_id.as_deref().ok_or_else(|| {
        crate::errors::ChatProviderError::Other("Tool response is missing toolCallId.".into())
    })?;
    let mut text = String::new();
    let mut media = Vec::new();
    for part in &message.content {
        match part {
            ContentPart::Text { text: t } => text.push_str(t),
            other => {
                if let Some(gp) = content_part_to_google(other) {
                    media.push(gp);
                }
            }
        }
    }
    let name = tool_call_id_to_name(id, tool_name_by_id);
    let mut parts = vec![GooglePart::FunctionResponse {
        name,
        response: serde_json::json!({ "output": text }),
        parts: vec![],
    }];
    parts.extend(media);
    Ok(parts)
}

pub(crate) fn messages_to_google_genai_contents(
    messages: &[Message],
) -> Result<Vec<GoogleContent>, crate::errors::ChatProviderError> {
    let mut contents = Vec::new();
    let mut tool_name_by_id = std::collections::HashMap::new();
    let mut i = 0;
    while i < messages.len() {
        let msg = &messages[i];
        if msg.role == Role::System {
            let text: String = msg
                .content
                .iter()
                .filter_map(|p| match p {
                    ContentPart::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            if !text.is_empty() {
                contents.push(GoogleContent {
                    role: "user".into(),
                    parts: vec![GooglePart::text(format!("<system>{text}</system>"))],
                });
            }
            i += 1;
            continue;
        }
        if msg.role == Role::Assistant && !msg.tool_calls.is_empty() {
            let mut parts = Vec::new();
            for part in &msg.content {
                if let Some(gp) = content_part_to_google(part) {
                    parts.push(gp);
                }
            }
            for tc in &msg.tool_calls {
                tool_name_by_id.insert(tc.id.clone(), tc.name.clone());
                parts.push(tool_call_to_google(tc)?);
            }
            contents.push(GoogleContent {
                role: "model".into(),
                parts,
            });

            let expected_ids: Vec<_> = msg.tool_calls.iter().map(|tc| tc.id.clone()).collect();
            let mut j = i + 1;
            let mut tool_messages = Vec::new();
            while j < messages.len() && messages[j].role == Role::Tool {
                tool_messages.push(&messages[j]);
                j += 1;
            }
            if !tool_messages.is_empty() {
                let mut by_id = std::collections::HashMap::new();
                let mut seen = std::collections::HashSet::new();
                for tm in &tool_messages {
                    let id = tm.tool_call_id.as_deref().ok_or_else(|| {
                        crate::errors::ChatProviderError::Other(
                            "Tool response is missing toolCallId.".into(),
                        )
                    })?;
                    if !seen.insert(id) {
                        return Err(crate::errors::ChatProviderError::Other(format!(
                            "Duplicate tool response for id: {id}"
                        )));
                    }
                    by_id.insert(id, *tm);
                }
                let mut sorted_parts = Vec::new();
                for expected in &expected_ids {
                    let tm = by_id.remove(expected.as_str()).ok_or_else(|| {
                        crate::errors::ChatProviderError::Other(format!(
                            "Missing tool responses for ids: {expected}"
                        ))
                    })?;
                    sorted_parts.extend(tool_message_to_parts(tm, &tool_name_by_id)?);
                }
                if !by_id.is_empty() {
                    return Err(crate::errors::ChatProviderError::Other(format!(
                        "Unexpected tool responses for ids: {:?}",
                        by_id.keys().collect::<Vec<_>>()
                    )));
                }
                contents.push(GoogleContent {
                    role: "user".into(),
                    parts: sorted_parts,
                });
                i = j;
                continue;
            }
            i += 1;
            continue;
        }
        if msg.role == Role::Tool {
            contents.push(GoogleContent {
                role: "user".into(),
                parts: tool_message_to_parts(msg, &tool_name_by_id)?,
            });
            i += 1;
            continue;
        }
        // user / assistant without tool calls
        let mut parts = Vec::new();
        for part in &msg.content {
            if let Some(gp) = content_part_to_google(part) {
                parts.push(gp);
            }
        }
        contents.push(GoogleContent {
            role: if msg.role == Role::Assistant {
                "model".into()
            } else {
                "user".into()
            },
            parts,
        });
        i += 1;
    }
    Ok(contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_name_and_model() {
        let p = GoogleGenAIChatProvider::new("gemini-2.0-flash").with_api_key("sk-test");
        assert_eq!(p.name(), "google_genai");
        assert_eq!(p.model_name(), "gemini-2.0-flash");
    }

    #[test]
    fn capability_for_gemini_flash() {
        let p = GoogleGenAIChatProvider::new("gemini-2.0-flash-exp");
        let cap = p.get_capability(None);
        assert!(cap.image_in);
        assert!(cap.video_in);
        assert!(cap.audio_in);
        assert!(cap.tool_use);
        assert!(!cap.thinking);
    }

    #[test]
    fn capability_for_gemini_thinking() {
        let p = GoogleGenAIChatProvider::new("gemini-2.5-pro-preview-05-06");
        let cap = p.get_capability(None);
        assert!(cap.thinking);
    }
}

#[cfg(test)]
mod conversion_tests {
    use super::*;
    use crate::message::{ContentPart, Message, Role, ToolCall, UrlPayload};

    #[test]
    fn user_text_message_to_content() {
        let msgs = vec![Message::user_text("hello")];
        let contents = messages_to_google_genai_contents(&msgs).unwrap();
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].role, "user");
        assert_eq!(contents[0].parts, vec![GooglePart::text("hello")]);
    }

    #[test]
    fn assistant_tool_call_to_content() {
        let msg = Message {
            role: Role::Assistant,
            name: None,
            content: vec![],
            tool_calls: vec![ToolCall {
                call_type: "function".into(),
                id: "tc_1".into(),
                name: "read".into(),
                arguments: Some(r#"{"path":"/a"}"#.into()),
                extras: None,
                stream_index: None,
            }],
            tool_call_id: None,
            partial: None,
        };
        let contents = messages_to_google_genai_contents(&[msg]).unwrap();
        assert_eq!(contents[0].parts.len(), 1);
        assert!(matches!(
            &contents[0].parts[0],
            GooglePart::FunctionCall { name, args, .. }
            if name == "read" && args.get("path") == Some(&serde_json::Value::String("/a".into()))
        ));
    }

    #[test]
    fn tool_results_sorted_by_assistant_tool_call_order() {
        let assistant = Message {
            role: Role::Assistant,
            name: None,
            content: vec![],
            tool_calls: vec![
                ToolCall {
                    call_type: "function".into(),
                    id: "id_b".into(),
                    name: "b".into(),
                    arguments: None,
                    extras: None,
                    stream_index: None,
                },
                ToolCall {
                    call_type: "function".into(),
                    id: "id_a".into(),
                    name: "a".into(),
                    arguments: None,
                    extras: None,
                    stream_index: None,
                },
            ],
            tool_call_id: None,
            partial: None,
        };
        let tool_b = Message {
            role: Role::Tool,
            name: None,
            content: vec![ContentPart::Text {
                text: "out_b".into(),
            }],
            tool_calls: vec![],
            tool_call_id: Some("id_b".into()),
            partial: None,
        };
        let tool_a = Message {
            role: Role::Tool,
            name: None,
            content: vec![ContentPart::Text {
                text: "out_a".into(),
            }],
            tool_calls: vec![],
            tool_call_id: Some("id_a".into()),
            partial: None,
        };
        let contents = messages_to_google_genai_contents(&[assistant, tool_a, tool_b]).unwrap();
        assert_eq!(contents.len(), 2);
        assert_eq!(contents[1].role, "user");
        let names: Vec<_> = contents[1]
            .parts
            .iter()
            .filter_map(|p| match p {
                GooglePart::FunctionResponse { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, vec!["b", "a"]);
    }

    #[test]
    fn media_url_to_inline_data() {
        let msg = Message {
            role: Role::User,
            name: None,
            content: vec![ContentPart::ImageUrl {
                image_url: UrlPayload {
                    url: "data:image/png;base64,ABC".into(),
                    id: None,
                },
            }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let contents = messages_to_google_genai_contents(&[msg]).unwrap();
        assert!(matches!(
            &contents[0].parts[0],
            GooglePart::InlineData { mime_type, data } if mime_type == "image/png" && data == "ABC"
        ));
    }

    #[test]
    fn system_message_in_history_wrapped_as_user() {
        let msg = Message {
            role: Role::System,
            name: None,
            content: vec![ContentPart::Text { text: "sys".into() }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let contents = messages_to_google_genai_contents(&[msg]).unwrap();
        assert_eq!(contents[0].role, "user");
        assert_eq!(
            contents[0].parts,
            vec![GooglePart::text("<system>sys</system>")]
        );
    }
}

// --- Finish reason normalization ---

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NormalizedFinishReason {
    pub finish_reason: Option<crate::provider::FinishReason>,
    pub raw_finish_reason: Option<String>,
}

pub(crate) fn normalize_google_genai_finish_reason(
    raw: &serde_json::Value,
) -> NormalizedFinishReason {
    let raw_string = match raw {
        serde_json::Value::String(s) => s.to_uppercase(),
        serde_json::Value::Number(n) => n.to_string().to_uppercase(),
        serde_json::Value::Bool(b) => b.to_string().to_uppercase(),
        _ => {
            return NormalizedFinishReason {
                finish_reason: None,
                raw_finish_reason: None,
            }
        }
    };
    if raw_string.is_empty() || raw_string == "FINISH_REASON_UNSPECIFIED" {
        return NormalizedFinishReason {
            finish_reason: None,
            raw_finish_reason: None,
        };
    }
    use crate::provider::FinishReason;
    let finish_reason = match raw_string.as_str() {
        "STOP" => Some(FinishReason::Completed),
        "MAX_TOKENS" => Some(FinishReason::Truncated),
        "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" | "IMAGE_SAFETY" => {
            Some(FinishReason::Filtered)
        }
        _ => Some(FinishReason::Other),
    };
    NormalizedFinishReason {
        finish_reason,
        raw_finish_reason: Some(raw_string),
    }
}

#[cfg(test)]
mod finish_reason_tests {
    use super::*;
    use crate::provider::FinishReason;

    #[test]
    fn maps_stop_to_completed() {
        assert_eq!(
            normalize_google_genai_finish_reason(&serde_json::json!("STOP")).finish_reason,
            Some(FinishReason::Completed)
        );
    }

    #[test]
    fn maps_max_tokens_to_truncated() {
        assert_eq!(
            normalize_google_genai_finish_reason(&serde_json::json!("MAX_TOKENS")).finish_reason,
            Some(FinishReason::Truncated)
        );
    }

    #[test]
    fn maps_safety_to_filtered() {
        for raw in [
            "SAFETY",
            "RECITATION",
            "BLOCKLIST",
            "PROHIBITED_CONTENT",
            "SPII",
            "IMAGE_SAFETY",
        ] {
            assert_eq!(
                normalize_google_genai_finish_reason(&serde_json::json!(raw)).finish_reason,
                Some(FinishReason::Filtered),
                "failed for {raw}"
            );
        }
    }

    #[test]
    fn maps_other_to_other() {
        assert_eq!(
            normalize_google_genai_finish_reason(&serde_json::json!("OTHER")).finish_reason,
            Some(FinishReason::Other)
        );
    }

    #[test]
    fn unspecified_returns_null() {
        let r =
            normalize_google_genai_finish_reason(&serde_json::json!("FINISH_REASON_UNSPECIFIED"));
        assert_eq!(r.finish_reason, None);
        assert_eq!(r.raw_finish_reason, None);
    }

    #[test]
    fn invalid_object_returns_null() {
        let r = normalize_google_genai_finish_reason(&serde_json::json!({"foo":1}));
        assert_eq!(r.finish_reason, None);
        assert_eq!(r.raw_finish_reason, None);
    }
}

// --- Response chunk parsing (parts / usage / id) ---

use crate::message::StreamedMessagePart;
use crate::usage::TokenUsage;
use serde_json::Map;

pub(crate) fn extract_id(response: &Map<String, serde_json::Value>) -> Option<String> {
    response
        .get("responseId")
        .and_then(|v| v.as_str())
        .map(Into::into)
}

pub(crate) fn extract_usage(response: &Map<String, serde_json::Value>) -> Option<TokenUsage> {
    let meta = response.get("usageMetadata")?.as_object()?;
    let prompt = meta
        .get("promptTokenCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as i64;
    let cached = meta
        .get("cachedContentTokenCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as i64;
    let output = meta
        .get("candidatesTokenCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as i64;
    Some(TokenUsage {
        input_other: (prompt - cached).max(0),
        output,
        input_cache_read: cached,
        input_cache_creation: 0,
    })
}

pub(crate) fn extract_chunk_parts(
    response: &Map<String, serde_json::Value>,
) -> Vec<StreamedMessagePart> {
    let mut out = Vec::new();
    let candidates = match response.get("candidates").and_then(|v| v.as_array()) {
        Some(c) => c,
        None => return out,
    };
    for candidate in candidates {
        let content = match candidate.get("content").and_then(|v| v.as_object()) {
            Some(c) => c,
            None => continue,
        };
        let parts = match content.get("parts").and_then(|v| v.as_array()) {
            Some(p) => p,
            None => continue,
        };
        for part in parts {
            let obj = match part.as_object() {
                Some(o) => o,
                None => continue,
            };
            if obj.get("thought").and_then(|v| v.as_bool()) == Some(true) {
                if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                    out.push(StreamedMessagePart::think(text));
                }
            } else if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                out.push(StreamedMessagePart::text(text));
            } else if let Some(fc) = obj
                .get("functionCall")
                .or_else(|| obj.get("function_call"))
                .and_then(|v| v.as_object())
            {
                let name = fc.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if name.is_empty() {
                    continue;
                }
                let id = fc.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let args = fc
                    .get("args")
                    .cloned()
                    .unwrap_or(serde_json::Value::Object(Default::default()));
                let thought_sig = obj
                    .get("thoughtSignature")
                    .or_else(|| obj.get("thought_signature"))
                    .and_then(|v| v.as_str());
                let extras = thought_sig.map(|s| serde_json::json!({"thought_signature_b64": s}));
                out.push(StreamedMessagePart::ToolCall(ToolCall {
                    call_type: "function".into(),
                    id: format!("{name}_{id}"),
                    name: name.into(),
                    arguments: Some(args.to_string()),
                    extras,
                    stream_index: None,
                }));
            }
        }
    }
    out
}

#[cfg(test)]
mod chunk_tests {
    use super::*;
    use crate::message::{StreamedMessagePart, ToolCall};
    use crate::usage::TokenUsage;

    fn chunk(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
        value.as_object().unwrap().clone()
    }

    #[test]
    fn extracts_text_part() {
        let parts = extract_chunk_parts(&chunk(serde_json::json!({
            "candidates": [{"content": {"parts": [{"text": "hello"}]}}]
        })));
        assert_eq!(parts, vec![StreamedMessagePart::text("hello")]);
    }

    #[test]
    fn extracts_thought_part() {
        let parts = extract_chunk_parts(&chunk(serde_json::json!({
            "candidates": [{"content": {"parts": [{"text": "think", "thought": true}]}}]
        })));
        assert_eq!(parts, vec![StreamedMessagePart::think("think")]);
    }

    #[test]
    fn extracts_function_call() {
        let parts = extract_chunk_parts(&chunk(serde_json::json!({
            "candidates": [{"content": {"parts": [{
                "functionCall": {"name": "read", "args": {"path": "/a"}, "id": "abc"}
            }]}}]
        })));
        assert_eq!(parts.len(), 1);
        assert!(matches!(
            &parts[0],
            StreamedMessagePart::ToolCall(ToolCall { id, name, arguments, .. })
            if id == "read_abc" && name == "read" && arguments.as_deref() == Some(r#"{"path":"/a"}"#)
        ));
    }

    #[test]
    fn extracts_usage() {
        let usage = extract_usage(&chunk(serde_json::json!({
            "usageMetadata": {
                "promptTokenCount": 100,
                "cachedContentTokenCount": 30,
                "candidatesTokenCount": 20
            }
        })));
        assert_eq!(
            usage,
            Some(TokenUsage {
                input_other: 70,
                output: 20,
                input_cache_read: 30,
                input_cache_creation: 0,
            })
        );
    }

    #[test]
    fn extracts_response_id() {
        assert_eq!(
            extract_id(&chunk(serde_json::json!({"responseId": "resp_1"}))),
            Some("resp_1".into())
        );
    }
}

// --- StreamedMessage adapter ---

use crate::generate::StreamedMessage;
use crate::provider::{AbortSignal, FinishReason};

pub(crate) struct GoogleGenAIStreamedMessage;

impl GoogleGenAIStreamedMessage {
    pub fn from_chunks<I>(chunks: I, signal: Option<AbortSignal>) -> StreamedMessage
    where
        I: IntoIterator<Item = serde_json::Value>,
    {
        let mut id: Option<String> = None;
        let mut usage: Option<TokenUsage> = None;
        let mut finish_reason: Option<FinishReason> = None;
        let mut raw_finish_reason: Option<String> = None;
        let mut parts = Vec::new();

        for chunk in chunks {
            if let Some(sig) = signal.as_ref() {
                if sig.is_aborted() {
                    break;
                }
            }
            if let Some(obj) = chunk.as_object() {
                if let Some(new_id) = extract_id(obj) {
                    id = Some(new_id);
                }
                if let Some(new_usage) = extract_usage(obj) {
                    usage = Some(new_usage);
                }
                // Check finishReason at top level or nested in first candidate
                let finish_val = obj
                    .get("finishReason")
                    .or_else(|| obj.get("finish_reason"))
                    .or_else(|| {
                        obj.get("candidates")
                            .and_then(|c| c.as_array()?.first())
                            .and_then(|cand| {
                                cand.get("finishReason")
                                    .or_else(|| cand.get("finish_reason"))
                            })
                    });
                let normalized = finish_val
                    .map(normalize_google_genai_finish_reason)
                    .unwrap_or(NormalizedFinishReason {
                        finish_reason: None,
                        raw_finish_reason: None,
                    });
                if normalized.finish_reason.is_some() || normalized.raw_finish_reason.is_some() {
                    finish_reason = normalized.finish_reason;
                    raw_finish_reason = normalized.raw_finish_reason;
                }
                parts.extend(extract_chunk_parts(obj));
            }
        }

        StreamedMessage::from_parts(parts, id, usage, finish_reason, raw_finish_reason)
    }

    pub fn from_response(
        response: serde_json::Value,
        signal: Option<AbortSignal>,
    ) -> StreamedMessage {
        Self::from_chunks(std::iter::once(response), signal)
    }
}

#[cfg(test)]
mod streamed_message_tests {
    use super::*;
    use crate::provider::FinishReason;
    use crate::usage::TokenUsage;
    use futures_util::StreamExt;

    #[tokio::test]
    async fn collects_text_from_stream() {
        let chunks = vec![
            serde_json::json!({"candidates": [{"content": {"parts": [{"text": "hello"}]}}]}),
            serde_json::json!({"candidates": [{"content": {"parts": [{"text": " world"}]}}]}),
        ];
        let msg = GoogleGenAIStreamedMessage::from_chunks(chunks, None);
        let msg_id = msg.id();
        let parts: Vec<_> = msg.collect().await;
        assert_eq!(
            parts,
            vec![
                StreamedMessagePart::text("hello"),
                StreamedMessagePart::text(" world")
            ]
        );
        assert_eq!(msg_id, None);
    }

    #[tokio::test]
    async fn collects_from_non_stream_response() {
        let resp = serde_json::json!({
            "responseId": "r1",
            "candidates": [{"content": {"parts": [{"text": "hi"}]}, "finishReason": "STOP"}],
            "usageMetadata": {"promptTokenCount": 5, "candidatesTokenCount": 1}
        });
        let msg = GoogleGenAIStreamedMessage::from_response(resp, None);
        let msg_id = msg.id();
        let msg_usage = msg.usage();
        let msg_finish = msg.finish_reason();
        let parts: Vec<_> = msg.collect().await;
        assert_eq!(parts, vec![StreamedMessagePart::text("hi")]);
        assert_eq!(msg_id, Some("r1".into()));
        assert_eq!(
            msg_usage,
            Some(TokenUsage {
                input_other: 5,
                output: 1,
                input_cache_read: 0,
                input_cache_creation: 0
            })
        );
        assert_eq!(msg_finish, Some(FinishReason::Completed));
    }

    #[tokio::test]
    async fn abort_mid_stream_breaks() {
        let signal = AbortSignal::new();
        let signal_clone = signal.clone();
        let chunks = vec![
            serde_json::json!({"candidates": [{"content": {"parts": [{"text": "a"}]}}]}),
            serde_json::json!({"candidates": [{"content": {"parts": [{"text": "b"}]}}]}),
        ];
        signal_clone.abort();
        let msg = GoogleGenAIStreamedMessage::from_chunks(chunks, Some(signal));
        let parts: Vec<_> = msg.collect().await;
        // After abort, only the first chunk before the abort check fires is processed.
        // Since we aborted before consuming, the stream may be empty or partial.
        assert!(parts.len() <= 1);
    }
}

// --- HTTP request / error conversion ---

use crate::errors::{APIConnectionError, APITimeoutError, ChatProviderError};
use crate::provider::GenerateOptions;
use crate::request_auth::require_provider_api_key;

const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

impl GoogleGenAIChatProvider {
    fn base_url(&self) -> &str {
        self.base_url.as_deref().unwrap_or(DEFAULT_BASE_URL)
    }

    pub(crate) fn build_url(&self, model: &str, stream: bool) -> String {
        if self.vertexai {
            let project = self.project.as_deref().unwrap_or("");
            let location = self.location.as_deref().unwrap_or("");
            let action = if stream {
                "streamGenerateContent"
            } else {
                "generateContent"
            };
            format!(
                "https://{}-aiplatform.googleapis.com/v1beta1/projects/{}/locations/{}/publishers/google/models/{}:{}",
                location, project, location, model, action
            )
        } else {
            let action = if stream {
                "streamGenerateContent"
            } else {
                "generateContent"
            };
            format!(
                "{}/models/{}:{}?key=__KEY__",
                self.base_url(),
                model,
                action
            )
        }
    }

    pub(crate) fn build_config(&self, system_prompt: &str, tools: &[Tool]) -> serde_json::Value {
        let mut config = self.generation_kwargs.clone();
        config["systemInstruction"] = serde_json::json!({ "parts": [{ "text": system_prompt }] });
        if !tools.is_empty() {
            config["tools"] =
                serde_json::Value::Array(tools.iter().map(tool_to_google_genai).collect());
        }
        config
    }
}

async fn abort_future(signal: Option<&AbortSignal>) -> Result<(), ChatProviderError> {
    match signal {
        None => futures_util::future::pending().await,
        Some(sig) => {
            while !sig.is_aborted() {
                tokio::task::yield_now().await;
            }
            Err(ChatProviderError::Aborted(crate::errors::AbortError))
        }
    }
}

pub(crate) fn convert_google_genai_error(
    error: reqwest::Error,
    status: Option<reqwest::StatusCode>,
    body: Option<String>,
) -> ChatProviderError {
    if error.is_timeout() {
        return ChatProviderError::Timeout(APITimeoutError);
    }
    if error.is_connect() || error.is_request() {
        return ChatProviderError::Connection(APIConnectionError);
    }
    if let Some(code) = status {
        let msg = body.unwrap_or_else(|| error.to_string());
        return crate::errors::normalize_api_status_error(code.as_u16(), msg, None);
    }
    ChatProviderError::Other(format!("GoogleGenAI error: {error}"))
}

// --- Updated ChatProvider impl ---

#[async_trait::async_trait]
impl ChatProvider for GoogleGenAIChatProvider {
    fn name(&self) -> &str {
        "google_genai"
    }
    fn model_name(&self) -> &str {
        &self.model
    }

    fn thinking_effort(&self) -> Option<ThinkingEffort> {
        let cfg = self.generation_kwargs.get("thinkingConfig")?;
        if let Some(level) = cfg.get("thinkingLevel").and_then(|v| v.as_str()) {
            return Some(match level {
                "MINIMAL"
                    if cfg.get("includeThoughts").and_then(|v| v.as_bool()) == Some(false) =>
                {
                    ThinkingEffort::Off
                }
                "MINIMAL" => ThinkingEffort::Low,
                "LOW" => ThinkingEffort::Low,
                "MEDIUM" => ThinkingEffort::Medium,
                "HIGH" => ThinkingEffort::High,
                _ => return None,
            });
        }
        if let Some(budget) = cfg.get("thinkingBudget").and_then(|v| v.as_i64()) {
            if cfg.get("includeThoughts").and_then(|v| v.as_bool()) == Some(false) {
                return Some(ThinkingEffort::Off);
            }
            return Some(match budget {
                0 => ThinkingEffort::Off,
                1..=1024 => ThinkingEffort::Low,
                1025..=4096 => ThinkingEffort::Medium,
                _ => ThinkingEffort::High,
            });
        }
        None
    }

    fn get_capability(&self, model: Option<&str>) -> ModelCapability {
        get_google_genai_model_capability(model.unwrap_or(&self.model))
    }

    async fn generate(
        &self,
        system_prompt: &str,
        tools: &[Tool],
        history: &[crate::message::Message],
        options: Option<GenerateOptions>,
    ) -> Result<StreamedMessage, ChatProviderError> {
        if let Some(ref opts) = options {
            if let Some(ref sig) = opts.signal {
                if sig.is_aborted() {
                    return Err(ChatProviderError::Aborted(crate::errors::AbortError));
                }
            }
        }

        let contents = messages_to_google_genai_contents(history)?;
        let config = self.build_config(system_prompt, tools);
        let body = serde_json::json!({ "contents": contents, "config": config });

        let api_key = if self.vertexai {
            None
        } else {
            let auth = options.as_ref().and_then(|o| o.auth.as_ref());
            Some(require_provider_api_key(
                "google_genai",
                auth,
                self.api_key.as_deref(),
            )?)
        };

        let url = self.build_url(&self.model, self.stream);
        let url = if let Some(key) = api_key {
            url.replace("__KEY__", &key)
        } else {
            url
        };

        let client = reqwest::Client::builder().no_proxy().build().map_err(|e| {
            ChatProviderError::Other(format!("failed to build reqwest client: {e}"))
        })?;

        let request_fut = client.post(&url).json(&body).send();
        let signal_ref = options.as_ref().and_then(|o| o.signal.as_ref());

        let response = match futures_util::future::select(
            std::pin::pin!(request_fut),
            std::pin::pin!(abort_future(signal_ref)),
        )
        .await
        {
            futures_util::future::Either::Left((res, _)) => {
                res.map_err(|e| convert_google_genai_error(e, None, None))?
            }
            futures_util::future::Either::Right((Ok(()), _)) => {
                unreachable!("abort_future should not resolve with Ok")
            }
            futures_util::future::Either::Right((Err(err), _)) => return Err(err),
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(crate::errors::normalize_api_status_error(
                status.as_u16(),
                body,
                None,
            ));
        }

        if self.stream {
            let bytes_fut = response.bytes();
            let signal_ref = options.as_ref().and_then(|o| o.signal.as_ref());
            let bytes = match futures_util::future::select(
                std::pin::pin!(bytes_fut),
                std::pin::pin!(abort_future(signal_ref)),
            )
            .await
            {
                futures_util::future::Either::Left((res, _)) => {
                    res.map_err(|e| convert_google_genai_error(e, None, None))?
                }
                futures_util::future::Either::Right((Ok(()), _)) => {
                    unreachable!("abort_future should not resolve with Ok")
                }
                futures_util::future::Either::Right((Err(err), _)) => return Err(err),
            };
            let text = String::from_utf8_lossy(&bytes);
            let mut chunks = Vec::new();
            for line in text.lines() {
                if let Some(sig) = options.as_ref().and_then(|o| o.signal.as_ref()) {
                    if sig.is_aborted() {
                        break;
                    }
                }
                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        break;
                    }
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(data) {
                        chunks.push(value);
                    }
                }
            }
            Ok(GoogleGenAIStreamedMessage::from_chunks(
                chunks,
                options.and_then(|o| o.signal),
            ))
        } else {
            let json: serde_json::Value = response
                .json()
                .await
                .map_err(|e| convert_google_genai_error(e, None, None))?;
            Ok(GoogleGenAIStreamedMessage::from_response(
                json,
                options.and_then(|o| o.signal),
            ))
        }
    }

    fn with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider> {
        self.cloned_with_thinking(effort)
    }

    fn with_max_completion_tokens(&self, max_tokens: i64) -> Option<Box<dyn ChatProvider>> {
        Some(self.cloned_with_max_completion_tokens(max_tokens))
    }
}

impl GoogleGenAIChatProvider {
    pub fn clustered_with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider> {
        let mut clone = self.clone();
        Self::apply_thinking_inner(&mut clone, effort);
        Box::new(clone)
    }

    pub fn with_thinking(mut self, effort: ThinkingEffort) -> Self {
        Self::apply_thinking_inner(&mut self, effort);
        self
    }

    fn apply_thinking_inner(this: &mut Self, effort: ThinkingEffort) {
        let is_gemini_3 = this.model.to_lowercase().contains("gemini-3");
        let mut cfg = serde_json::json!({ "includeThoughts": true });
        if is_gemini_3 {
            cfg["thinkingLevel"] = match effort {
                ThinkingEffort::Off => {
                    cfg["includeThoughts"] = serde_json::Value::Bool(false);
                    serde_json::Value::String("MINIMAL".into())
                }
                ThinkingEffort::Low => serde_json::Value::String("LOW".into()),
                ThinkingEffort::Medium => serde_json::Value::String("MEDIUM".into()),
                ThinkingEffort::High | ThinkingEffort::Xhigh | ThinkingEffort::Max => {
                    serde_json::Value::String("HIGH".into())
                }
            };
        } else {
            cfg["thinkingBudget"] = match effort {
                ThinkingEffort::Off => {
                    cfg["includeThoughts"] = serde_json::Value::Bool(false);
                    serde_json::Value::Number(0.into())
                }
                ThinkingEffort::Low => serde_json::Value::Number(1024.into()),
                ThinkingEffort::Medium => serde_json::Value::Number(4096.into()),
                ThinkingEffort::High | ThinkingEffort::Xhigh | ThinkingEffort::Max => {
                    serde_json::Value::Number(32000.into())
                }
            };
        }
        if let Some(obj) = this.generation_kwargs.as_object_mut() {
            obj.insert("thinkingConfig".into(), cfg);
        }
    }

    pub fn with_max_completion_tokens(mut self, max_tokens: i64) -> Self {
        Self::apply_max_completion_tokens_inner(&mut self, max_tokens);
        self
    }

    fn cloned_with_max_completion_tokens(&self, max_tokens: i64) -> Box<dyn ChatProvider> {
        let mut clone = self.clone();
        Self::apply_max_completion_tokens_inner(&mut clone, max_tokens);
        Box::new(clone)
    }

    fn cloned_with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider> {
        self.clustered_with_thinking(effort)
    }

    fn apply_max_completion_tokens_inner(this: &mut Self, max_tokens: i64) {
        if let Some(obj) = this.generation_kwargs.as_object_mut() {
            obj.insert(
                "maxOutputTokens".into(),
                serde_json::Value::Number(max_tokens.into()),
            );
        }
    }
}

#[cfg(test)]
mod generate_tests {
    use super::*;
    use futures_util::StreamExt;

    #[tokio::test]
    async fn streams_text_over_rest_sse() {
        let server = httptest::Server::run();
        server.expect(
            httptest::Expectation::matching(httptest::matchers::request::method_path("POST", "/v1beta/models/gemini-test:streamGenerateContent"))
                .respond_with(httptest::responders::status_code(200).body(
                    "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Hello\"}]}}]}\n\n\
                     data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\" world\"}]},\"finishReason\":\"STOP\"}]}\n\n",
                )),
        );

        let provider = GoogleGenAIChatProvider::new("gemini-test")
            .with_api_key("sk-test")
            .with_base_url(server.url_str("/v1beta"));

        let stream = provider.generate("sys", &[], &[], None).await.unwrap();
        let parts: Vec<_> = stream.collect().await;
        assert_eq!(
            parts,
            vec![
                StreamedMessagePart::text("Hello"),
                StreamedMessagePart::text(" world")
            ]
        );
    }

    #[tokio::test]
    async fn converts_api_error_to_status_error() {
        let server = httptest::Server::run();
        server.expect(
            httptest::Expectation::matching(httptest::matchers::request::method_path(
                "POST",
                "/v1beta/models/gemini-test:generateContent",
            ))
            .respond_with(
                httptest::responders::status_code(429)
                    .body(r#"{"error":{"message":"rate limit","code":429}}"#),
            ),
        );

        let provider = GoogleGenAIChatProvider::new("gemini-test")
            .with_api_key("sk-test")
            .with_stream(false)
            .with_base_url(server.url_str("/v1beta"));

        let err = provider.generate("sys", &[], &[], None).await.unwrap_err();
        assert!(format!("{err}").contains("429") || format!("{err}").contains("rate limit"));
    }
}

#[cfg(test)]
mod thinking_tests {
    use super::*;
    use crate::provider::ThinkingEffort;

    #[test]
    fn gemini_3_off_maps_to_minimal_without_thoughts() {
        let p = GoogleGenAIChatProvider::new("gemini-3-flash").with_thinking(ThinkingEffort::Off);
        assert_eq!(p.thinking_effort(), Some(ThinkingEffort::Off));
        let cfg = p.generation_kwargs.get("thinkingConfig").unwrap();
        assert_eq!(cfg["thinkingLevel"], "MINIMAL");
        assert_eq!(cfg["includeThoughts"], false);
    }

    #[test]
    fn gemini_3_high_maps_to_high() {
        let p = GoogleGenAIChatProvider::new("gemini-3-flash").with_thinking(ThinkingEffort::High);
        let cfg = p.generation_kwargs.get("thinkingConfig").unwrap();
        assert_eq!(cfg["thinkingLevel"], "HIGH");
        assert_eq!(cfg["includeThoughts"], true);
    }

    #[test]
    fn non_gemini_3_medium_uses_budget() {
        let p =
            GoogleGenAIChatProvider::new("gemini-2.5-pro").with_thinking(ThinkingEffort::Medium);
        assert_eq!(p.thinking_effort(), Some(ThinkingEffort::Medium));
        let cfg = p.generation_kwargs.get("thinkingConfig").unwrap();
        assert_eq!(cfg["thinkingBudget"], 4096);
        assert_eq!(cfg["includeThoughts"], true);
    }

    #[test]
    fn non_gemini_3_off_uses_zero_budget() {
        let p = GoogleGenAIChatProvider::new("gemini-2.5-pro").with_thinking(ThinkingEffort::Off);
        let cfg = p.generation_kwargs.get("thinkingConfig").unwrap();
        assert_eq!(cfg["thinkingBudget"], 0);
        assert_eq!(cfg["includeThoughts"], false);
    }

    #[test]
    fn max_completion_tokens_propagates() {
        let p = GoogleGenAIChatProvider::new("gemini-2.0-flash").with_max_completion_tokens(1024);
        assert_eq!(p.generation_kwargs["maxOutputTokens"], 1024);
    }
}

#[cfg(test)]
mod fixture_tests {
    use super::*;
    use futures_util::StreamExt;
    use std::fs;

    #[tokio::test]
    async fn text_stream_fixture() {
        let raw = fs::read_to_string("fixtures/google-genai/text_stream.json").unwrap();
        let fixture: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let chunks: Vec<serde_json::Value> =
            serde_json::from_value(fixture["chunks"].clone()).unwrap();
        let msg = GoogleGenAIStreamedMessage::from_chunks(chunks, None);
        let parts: Vec<_> = msg.collect().await;
        let expected_content: Vec<serde_json::Value> =
            serde_json::from_value(fixture["expected"]["content"].clone()).unwrap();
        // Compare serialized forms since StreamedMessagePart doesn't derive PartialEq with serde_json::Value
        assert_eq!(
            serde_json::to_value(&parts).unwrap(),
            serde_json::to_value(&expected_content).unwrap()
        );
    }

    #[tokio::test]
    async fn tool_call_stream_fixture() {
        let raw = fs::read_to_string("fixtures/google-genai/tool_call_stream.json").unwrap();
        let fixture: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let chunks: Vec<serde_json::Value> =
            serde_json::from_value(fixture["chunks"].clone()).unwrap();
        let msg = GoogleGenAIStreamedMessage::from_chunks(chunks, None);
        let parts: Vec<_> = msg.collect().await;
        if let StreamedMessagePart::ToolCall(tc) = &parts[0] {
            assert_eq!(tc.name, "read");
            assert_eq!(tc.id, "read_abc");
        } else {
            panic!("expected ToolCall");
        }
    }

    #[test]
    fn non_stream_fixture() {
        let raw = fs::read_to_string("fixtures/google-genai/non_stream.json").unwrap();
        let fixture: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let resp = fixture["response"].clone();
        let msg = GoogleGenAIStreamedMessage::from_response(resp, None);
        assert_eq!(msg.id(), Some("r1".into()));
        assert_eq!(msg.finish_reason(), Some(FinishReason::Completed));
    }

    #[test]
    fn vertexai_url_construction() {
        let raw = fs::read_to_string("fixtures/google-genai/vertexai_config.json").unwrap();
        let fixture: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let p = GoogleGenAIChatProvider::new(fixture["model"].as_str().unwrap()).with_vertexai(
            fixture["project"].as_str().unwrap(),
            fixture["location"].as_str().unwrap(),
        );
        let url = p.build_url("gemini-2.0-flash", true);
        assert!(url.ends_with(fixture["expectedUrlSuffix"].as_str().unwrap()));
    }
}
