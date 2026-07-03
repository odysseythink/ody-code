use crate::chat_completions_stream::{parse_non_stream_response, parse_stream_response};
use crate::errors::{APIMissingApiKeyError, ChatProviderError};
use crate::http_client::{HttpClient, ReqwestClient};
use crate::message::{ContentPart, Message, Role};
use crate::openai_common::{
    convert_content_part, convert_openai_error, convert_tool_message_content,
    reasoning_effort_to_thinking_effort, thinking_effort_to_reasoning_effort, tool_to_openai,
    ToolMessageConversion,
};
use crate::provider::{ChatProvider, GenerateOptions, ModelCapability, ThinkingEffort, Tool};
use crate::request_auth::merge_request_headers;
use crate::tool_call_id::{
    normalize_tool_call_ids_for_provider, sanitize_tool_call_id, ToolCallIdPolicy,
};
use crate::{capability_registry, generate::StreamedMessage};
use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

fn openai_chat_tool_call_id_policy() -> ToolCallIdPolicy {
    ToolCallIdPolicy::new(|id| sanitize_tool_call_id(id, Some(64)), Some(64))
}

#[derive(Clone)]
pub struct OpenAILegacyOptions {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: String,
    pub stream: Option<bool>,
    pub max_tokens: Option<i64>,
    pub reasoning_key: Option<String>,
    pub default_headers: Option<HashMap<String, String>>,
    pub tool_message_conversion: Option<ToolMessageConversion>,
    pub http_client: Option<Arc<dyn HttpClient>>,
}
impl std::fmt::Debug for OpenAILegacyOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAILegacyOptions")
            .field("model", &self.model)
            .field("stream", &self.stream)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Serialize)]
struct OpenAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "tool_calls")]
    tool_calls: Option<Vec<OpenAIToolCallOut>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "tool_call_id")]
    tool_call_id: Option<String>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}
#[derive(Debug, Clone, Serialize)]
struct OpenAIToolCallOut {
    #[serde(rename = "type")]
    call_type: String,
    id: String,
    function: OpenAIToolFunctionOut,
}
#[derive(Debug, Clone, Serialize)]
struct OpenAIToolFunctionOut {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<String>,
}

pub struct OpenAILegacyChatProvider {
    model: String,
    api_key: Option<String>,
    base_url: String,
    default_headers: Option<HashMap<String, String>>,
    reasoning_key: Option<String>,
    reasoning_effort: Option<String>,
    max_output_tokens_cap: Option<i64>,
    generation_kwargs: HashMap<String, Value>,
    tool_message_conversion: Option<ToolMessageConversion>,
    stream: bool,
    http_client: Arc<dyn HttpClient>,
}

impl OpenAILegacyChatProvider {
    pub fn new(options: OpenAILegacyOptions) -> Self {
        let api_key = options
            .api_key
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .filter(|k| !k.is_empty());
        let base_url = options
            .base_url
            .unwrap_or_else(|| "https://api.openai.com/v1".into());
        let reasoning_key = options
            .reasoning_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from);
        let mut generation_kwargs = HashMap::new();
        if let Some(max) = options.max_tokens {
            generation_kwargs.insert("max_tokens".into(), Value::Number(max.into()));
        }
        let http_client = options
            .http_client
            .unwrap_or_else(|| Arc::new(ReqwestClient::new(reqwest::Client::new())));
        Self {
            model: options.model,
            api_key,
            base_url,
            default_headers: options.default_headers,
            reasoning_key,
            reasoning_effort: None,
            max_output_tokens_cap: options.max_tokens,
            generation_kwargs,
            tool_message_conversion: options.tool_message_conversion,
            stream: options.stream.unwrap_or(true),
            http_client,
        }
    }
}
impl Clone for OpenAILegacyChatProvider {
    fn clone(&self) -> Self {
        Self {
            model: self.model.clone(),
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            default_headers: self.default_headers.clone(),
            reasoning_key: self.reasoning_key.clone(),
            reasoning_effort: self.reasoning_effort.clone(),
            max_output_tokens_cap: self.max_output_tokens_cap,
            generation_kwargs: self.generation_kwargs.clone(),
            tool_message_conversion: self.tool_message_conversion,
            stream: self.stream,
            http_client: Arc::clone(&self.http_client),
        }
    }
}

fn convert_message(
    message: &Message,
    reasoning_key: Option<&str>,
    tool_message_conversion: Option<ToolMessageConversion>,
) -> OpenAIMessage {
    let mut reasoning_content = String::new();
    let non_think: Vec<&ContentPart> = message
        .content
        .iter()
        .filter(|p| {
            if let ContentPart::Think { think, .. } = p {
                reasoning_content.push_str(think);
                false
            } else {
                true
            }
        })
        .collect();
    let content = if message.role == Role::Tool {
        let has_non_text = non_think
            .iter()
            .any(|p| !matches!(p, ContentPart::Text { .. }));
        let effective = if has_non_text {
            Some(ToolMessageConversion::ExtractText)
        } else {
            tool_message_conversion
        };
        convert_tool_message_content(
            &Message {
                role: message.role,
                name: message.name.clone(),
                content: non_think.into_iter().cloned().collect(),
                tool_calls: message.tool_calls.clone(),
                tool_call_id: message.tool_call_id.clone(),
                partial: message.partial,
            },
            effective.unwrap_or(ToolMessageConversion::Standard),
        )
    } else {
        if non_think.is_empty() {
            None
        } else if non_think.len() == 1 {
            if let ContentPart::Text { text } = non_think[0] {
                Some(Value::String(text.clone()))
            } else {
                convert_content_part(non_think[0]).map(|c| serde_json::to_value(c).unwrap())
            }
        } else {
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
    };
    let tool_calls = if message.tool_calls.is_empty() {
        None
    } else {
        Some(
            message
                .tool_calls
                .iter()
                .map(|tc| OpenAIToolCallOut {
                    call_type: tc.call_type.clone(),
                    id: tc.id.clone(),
                    function: OpenAIToolFunctionOut {
                        name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                    },
                })
                .collect(),
        )
    };
    let mut extra = HashMap::new();
    if !reasoning_content.is_empty() {
        extra.insert(
            reasoning_key.unwrap_or("reasoning_content").to_string(),
            Value::String(reasoning_content),
        );
    }
    OpenAIMessage {
        role: format!("{:?}", message.role).to_lowercase(),
        content,
        name: message.name.clone(),
        tool_calls,
        tool_call_id: message.tool_call_id.clone(),
        extra,
    }
}

async fn read_body_bytes(
    stream: &mut crate::http_client::ByteStream,
) -> Result<Vec<u8>, ChatProviderError> {
    use futures_util::StreamExt;
    let mut buf = Vec::new();
    while let Some(chunk) = stream.next().await {
        buf.extend_from_slice(chunk?.as_ref());
    }
    Ok(buf)
}

#[async_trait]
impl ChatProvider for OpenAILegacyChatProvider {
    fn name(&self) -> &str {
        "openai"
    }
    fn model_name(&self) -> &str {
        &self.model
    }
    fn thinking_effort(&self) -> Option<ThinkingEffort> {
        reasoning_effort_to_thinking_effort(self.reasoning_effort.as_deref())
    }
    fn get_capability(&self, model: Option<&str>) -> ModelCapability {
        capability_registry::get_openai_legacy_model_capability(model.unwrap_or(&self.model))
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
                return Err(ChatProviderError::Aborted(crate::errors::AbortError));
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
        let normalized_history =
            normalize_tool_call_ids_for_provider(history, &openai_chat_tool_call_id_policy());
        let mut messages: Vec<OpenAIMessage> = Vec::new();
        if !system_prompt.is_empty() {
            messages.push(OpenAIMessage {
                role: "system".into(),
                content: Some(Value::String(system_prompt.into())),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                extra: HashMap::new(),
            });
        }
        for msg in &normalized_history {
            messages.push(convert_message(
                msg,
                self.reasoning_key.as_deref(),
                self.tool_message_conversion,
            ));
        }
        let mut create_params = serde_json::Map::new();
        create_params.insert("model".into(), Value::String(self.model.clone()));
        create_params.insert("messages".into(), serde_json::to_value(&messages).unwrap());
        create_params.insert("stream".into(), Value::Bool(self.stream));
        let mut kwargs = self.generation_kwargs.clone();
        let mut reasoning_effort = self.reasoning_effort.clone();
        if reasoning_effort.is_none() && !kwargs.contains_key("reasoning_effort") {
            let has_think = history.iter().any(|m| {
                m.content
                    .iter()
                    .any(|p| matches!(p, ContentPart::Think { .. }))
            });
            if has_think {
                reasoning_effort = Some("medium".into());
            }
        }
        kwargs.retain(|_, v| !v.is_null());
        for (k, v) in kwargs {
            create_params.insert(k, v);
        }
        if !tools.is_empty() {
            let tools_json: Vec<Value> = tools
                .iter()
                .map(|t| serde_json::to_value(tool_to_openai(t)).unwrap())
                .collect();
            create_params.insert("tools".into(), Value::Array(tools_json));
        }
        if self.stream {
            create_params.insert(
                "stream_options".into(),
                serde_json::json!({"include_usage": true}),
            );
        }
        if let Some(re) = reasoning_effort {
            create_params.insert("reasoning_effort".into(), Value::String(re));
        }
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let response = self
            .http_client
            .post_json(&url, headers, Value::Object(create_params))
            .await?;
        let status = response.status();
        let mut body_stream = response.bytes_stream();
        if status < 200 || status >= 300 {
            let body_bytes = read_body_bytes(&mut body_stream).await.unwrap_or_default();
            let body = String::from_utf8_lossy(&body_bytes);
            let (msg, code) = parse_error_body(&body);
            return Err(convert_openai_error(&msg, Some(status), code.as_deref()));
        }
        let body_bytes = read_body_bytes(&mut body_stream).await?;
        let (parts, id, usage, finish_reason, raw_finish_reason) = if self.stream {
            parse_stream_response(body_bytes, self.reasoning_key.as_deref()).await?
        } else {
            parse_non_stream_response(&body_bytes, self.reasoning_key.as_deref())?
        };
        Ok(StreamedMessage::from_parts(
            parts,
            id,
            usage,
            finish_reason,
            raw_finish_reason,
        ))
    }
    fn with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider> {
        let mut clone = self.clone();
        clone.reasoning_effort = thinking_effort_to_reasoning_effort(effort);
        Box::new(clone)
    }
    fn with_max_completion_tokens(
        &self,
        max_completion_tokens: i64,
    ) -> Option<Box<dyn ChatProvider>> {
        let effective = self
            .max_output_tokens_cap
            .map(|cap| std::cmp::min(max_completion_tokens, cap))
            .unwrap_or(max_completion_tokens);
        let mut clone = self.clone();
        clone
            .generation_kwargs
            .insert("max_tokens".into(), Value::Number(effective.into()));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_client::MockHttpClient;
    use crate::message::{Message, Role, StreamedMessagePart, ToolCall};
    fn text_sse_bytes() -> Vec<u8> {
        br#"data: {"id":"1","choices":[{"index":0,"delta":{"role":"assistant","content":"Hello"}}]}

data: {"id":"2","choices":[{"index":0,"delta":{"content":" world"}}]}

data: {"id":"3","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]

"#
        .to_vec()
    }
    fn provider_with_body(status: u16, body: Vec<u8>) -> OpenAILegacyChatProvider {
        OpenAILegacyChatProvider::new(OpenAILegacyOptions {
            api_key: Some("sk-test".into()),
            base_url: Some("http://mock".into()),
            model: "gpt-4o-mini".into(),
            stream: Some(true),
            max_tokens: None,
            reasoning_key: None,
            default_headers: None,
            tool_message_conversion: None,
            http_client: Some(Arc::new(MockHttpClient::new(status, body))),
        })
    }
    #[tokio::test]
    async fn generate_streams_text() {
        let provider = provider_with_body(200, text_sse_bytes());
        let stream = provider.generate("", &[], &[], None).await.unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(stream).await;
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], StreamedMessagePart::text("Hello"));
        assert_eq!(parts[1], StreamedMessagePart::text(" world"));
    }
    #[tokio::test]
    async fn generate_rejects_missing_api_key() {
        let provider = OpenAILegacyChatProvider::new(OpenAILegacyOptions {
            api_key: None,
            base_url: None,
            model: "gpt-4o".into(),
            stream: None,
            max_tokens: None,
            reasoning_key: None,
            default_headers: None,
            tool_message_conversion: None,
            http_client: Some(Arc::new(MockHttpClient::new(200, vec![]))),
        });
        let result = provider.generate("", &[], &[], None).await;
        assert!(result.is_err());
    }
    #[test]
    fn convert_message_serializes_tool_call_id() {
        let msg = Message {
            role: Role::Assistant,
            name: None,
            content: vec![],
            tool_calls: vec![ToolCall {
                call_type: "function".into(),
                id: "tc_1".into(),
                name: "read".into(),
                arguments: None,
                extras: None,
                stream_index: None,
            }],
            tool_call_id: None,
            partial: None,
        };
        let out = convert_message(&msg, None, None);
        assert_eq!(out.tool_calls.as_ref().unwrap()[0].id, "tc_1");
    }
    #[test]
    fn with_max_completion_tokens_clamps_to_cap() {
        let provider = OpenAILegacyChatProvider::new(OpenAILegacyOptions {
            api_key: Some("sk".into()),
            base_url: None,
            model: "gpt-4o".into(),
            stream: None,
            max_tokens: Some(100),
            reasoning_key: None,
            default_headers: None,
            tool_message_conversion: None,
            http_client: None,
        });
        let _limited = provider.with_max_completion_tokens(200).unwrap();
    }
}
