use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Map, Value};

use crate::capability_registry::get_kimi_model_capability;
use crate::chat_completions_stream::{
    parse_non_stream_response, parse_stream_response_with_usage_extractor,
};
use crate::errors::ChatProviderError;
use crate::generate::StreamedMessage;
use crate::http_client::{HttpClient, HttpResponse, ReqwestClient};
use crate::kimi_schema::normalize_kimi_tool_schema;
use crate::message::{ContentPart, Message, Role};
use crate::openai_common::{convert_content_part, thinking_effort_to_reasoning_effort};
use crate::provider::{
    ChatProvider, GenerateOptions, ModelCapability, ProviderRequestAuth, ThinkingEffort, Tool,
};
use crate::request_auth::require_provider_api_key;
use crate::tool_call_id::{
    normalize_tool_call_ids_for_provider, sanitize_tool_call_id, ToolCallIdPolicy,
};

// ── Kimi usage extractor ─────────────────────────────────────────────────

pub fn kimi_usage_extractor(value: &Value) -> Option<Value> {
    value
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("usage"))
        .cloned()
}

// ── Options ──────────────────────────────────────────────────────────────

pub struct KimiOptions {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: String,
    pub stream: Option<bool>,
    pub default_headers: Option<HashMap<String, String>>,
    pub generation_kwargs: Option<HashMap<String, Value>>,
    pub http_client: Option<Arc<dyn HttpClient>>,
    pub reasoning_key: Option<String>,
}

// ── OpenAIMessage types ──────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
struct OpenAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<Value>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
    #[serde(skip_serializing_if = "Vec::is_empty", rename = "tool_calls", default)]
    tool_calls: Vec<OpenAIToolCall>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct OpenAIToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAIFunctionCall,
}

#[derive(Debug, Clone, serde::Serialize)]
struct OpenAIFunctionCall {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<String>,
}

// ── Provider struct ──────────────────────────────────────────────────────

pub struct KimiChatProvider {
    model: String,
    stream: bool,
    api_key: Option<String>,
    base_url: String,
    default_headers: Option<HashMap<String, String>>,
    generation_kwargs: HashMap<String, Value>,
    reasoning_effort: Option<String>,
    extra_body: Option<Map<String, Value>>,
    http_client: Arc<dyn HttpClient>,
}

impl KimiChatProvider {
    pub fn new(options: KimiOptions) -> Self {
        let api_key = options
            .api_key
            .or_else(|| std::env::var("KIMI_API_KEY").ok())
            .filter(|k| !k.is_empty());
        let base_url = options
            .base_url
            .or_else(|| std::env::var("KIMI_BASE_URL").ok())
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| "https://api.moonshot.ai/v1".into());
        let http_client = options
            .http_client
            .unwrap_or_else(|| Arc::new(ReqwestClient::new(reqwest::Client::new())));

        let mut generation_kwargs = options.generation_kwargs.unwrap_or_default();

        // Extract reasoning_effort from generation_kwargs
        let reasoning_effort = generation_kwargs
            .remove("reasoning_effort")
            .and_then(|v| v.as_str().map(|s| s.to_string()));

        // Extract extra_body from generation_kwargs
        let extra_body = generation_kwargs
            .remove("extra_body")
            .and_then(|v| v.as_object().cloned());

        Self {
            model: options.model,
            stream: options.stream.unwrap_or(true),
            api_key,
            base_url,
            default_headers: options.default_headers,
            generation_kwargs,
            reasoning_effort,
            extra_body,
            http_client,
        }
    }

    fn kimi_tool_call_id_policy() -> ToolCallIdPolicy {
        ToolCallIdPolicy::new(|id| sanitize_tool_call_id(id, Some(64)), Some(64))
    }

    fn resolve_api_key(
        &self,
        auth: Option<&ProviderRequestAuth>,
    ) -> Result<String, ChatProviderError> {
        require_provider_api_key("kimi", auth, self.api_key.as_deref())
    }

    fn build_headers(
        &self,
        auth: Option<&ProviderRequestAuth>,
    ) -> Result<HashMap<String, String>, ChatProviderError> {
        let mut headers = HashMap::new();
        headers.insert("content-type".into(), "application/json".into());
        headers.insert(
            "authorization".into(),
            format!("Bearer {}", self.resolve_api_key(auth)?),
        );

        if let Some(default) = &self.default_headers {
            for (k, v) in default {
                headers.insert(k.clone(), v.clone());
            }
        }
        if let Some(request_headers) = auth.and_then(|a| a.headers.as_ref()) {
            for (k, v) in request_headers {
                headers.insert(k.clone(), v.clone());
            }
        }
        Ok(headers)
    }

    /// Convert a `ContentPart` to its OpenAI value representation.
    fn content_part_to_value(part: &ContentPart) -> Option<Value> {
        match part {
            ContentPart::Think { .. } => None,
            ContentPart::Text { text } => Some(Value::String(text.clone())),
            _ => convert_content_part(part).map(|c| serde_json::to_value(c).unwrap_or(Value::Null)),
        }
    }

    /// Convert a `Message` to an `OpenAIMessage`.
    fn convert_message(message: &Message) -> Result<OpenAIMessage, ChatProviderError> {
        let role = match message.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };

        // Collect think parts as reasoning_content extra field
        let reasoning_content = message
            .content
            .iter()
            .filter_map(|p| match p {
                ContentPart::Think { think, .. } if !think.is_empty() => Some(think.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("");

        // Collect non-think content parts
        let content_values: Vec<Value> = message
            .content
            .iter()
            .filter_map(Self::content_part_to_value)
            .collect();

        // Build content field
        let content =
            if role == "assistant" && !message.tool_calls.is_empty() && content_values.is_empty() {
                // Empty content + tool_calls → content: None (omit entirely)
                None
            } else if content_values.is_empty() {
                // No content parts → use empty string or None
                if message.tool_calls.is_empty() {
                    Some(Value::String(String::new()))
                } else {
                    None
                }
            } else if content_values.len() == 1 && matches!(&content_values[0], Value::String(_)) {
                // Single text part → Value::String
                Some(content_values.into_iter().next().unwrap())
            } else {
                // Multiple parts → Value::Array
                Some(Value::Array(content_values))
            };

        // Build extra fields
        let mut extra = HashMap::new();
        if !reasoning_content.is_empty() {
            extra.insert("reasoning_content".into(), Value::String(reasoning_content));
        }
        if role == "tool" {
            if let Some(tool_call_id) = &message.tool_call_id {
                extra.insert("tool_call_id".into(), Value::String(tool_call_id.clone()));
            }
        }
        if let Some(name) = &message.name {
            extra.insert("name".into(), Value::String(name.clone()));
        }

        // Convert tool calls
        let tool_calls: Vec<OpenAIToolCall> = message
            .tool_calls
            .iter()
            .map(|tc| OpenAIToolCall {
                id: tc.id.clone(),
                call_type: "function".into(),
                function: OpenAIFunctionCall {
                    name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                },
            })
            .collect();

        Ok(OpenAIMessage {
            role: role.into(),
            content,
            extra,
            tool_calls,
        })
    }

    /// Convert a `Tool` to its Kimi-compatible JSON value.
    fn convert_tool(tool: &Tool) -> Value {
        if tool.name.starts_with('$') {
            // Builtin function tool: {"type": "builtin_function", "function": {"name": "..."}}
            let name = tool.name.trim_start_matches('$');
            serde_json::json!({
                "type": "builtin_function",
                "function": {
                    "name": name
                }
            })
        } else {
            // Regular function tool with normalized schema
            let mut normalized_schema = tool.parameters.clone();
            if let Some(obj) = normalized_schema.as_object() {
                normalized_schema = Value::Object(normalize_kimi_tool_schema(obj.clone()));
            }
            serde_json::json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": normalized_schema
                }
            })
        }
    }

    /// Build the chat completions request body.
    fn build_create_params(
        &self,
        system_prompt: &str,
        tools: &[Tool],
        history: &[Message],
        stream: bool,
    ) -> Result<Value, ChatProviderError> {
        let normalized_history =
            normalize_tool_call_ids_for_provider(history, &Self::kimi_tool_call_id_policy());

        // Build messages array
        let mut messages: Vec<Value> = Vec::new();

        // System prompt as a message
        if !system_prompt.trim().is_empty() {
            messages.push(serde_json::json!({
                "role": "system",
                "content": system_prompt.trim()
            }));
        }

        // History messages
        for msg in &normalized_history {
            let openai_msg = Self::convert_message(msg)?;
            messages.push(
                serde_json::to_value(&openai_msg)
                    .map_err(|e| ChatProviderError::Other(e.to_string()))?,
            );
        }

        // Tools
        let kimi_tools: Vec<Value> = tools.iter().map(Self::convert_tool).collect();

        // Build request body
        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": stream,
        });

        let body_obj = body.as_object_mut().unwrap();

        if !kimi_tools.is_empty() {
            body_obj.insert("tools".into(), Value::Array(kimi_tools));
        }

        // max_tokens → max_completion_tokens
        if let Some(max_tokens) = self.generation_kwargs.get("max_tokens") {
            body_obj.insert("max_completion_tokens".into(), max_tokens.clone());
        } else if let Some(max_completion_tokens) =
            self.generation_kwargs.get("max_completion_tokens")
        {
            body_obj.insert(
                "max_completion_tokens".into(),
                max_completion_tokens.clone(),
            );
        }

        // Copy other generation_kwargs (skip max_tokens and max_completion_tokens)
        for (k, v) in &self.generation_kwargs {
            if k == "max_tokens" || k == "max_completion_tokens" {
                continue;
            }
            body_obj.insert(k.clone(), v.clone());
        }

        // reasoning_effort
        if let Some(ref effort) = self.reasoning_effort {
            body_obj.insert("reasoning_effort".into(), Value::String(effort.clone()));
        }

        // extra_body flattening
        if let Some(ref extra) = self.extra_body {
            for (k, v) in extra {
                body_obj.insert(k.clone(), v.clone());
            }
        }

        // stream_options.include_usage when streaming
        if stream {
            body_obj.insert(
                "stream_options".into(),
                serde_json::json!({"include_usage": true}),
            );
        }

        Ok(Value::Object(body_obj.clone()))
    }
}

#[async_trait]
impl ChatProvider for KimiChatProvider {
    fn name(&self) -> &str {
        "kimi"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn thinking_effort(&self) -> Option<ThinkingEffort> {
        match self.reasoning_effort.as_deref() {
            None | Some("off") => None,
            Some("low") | Some("minimal") => Some(ThinkingEffort::Low),
            Some("medium") => Some(ThinkingEffort::Medium),
            Some("high") => Some(ThinkingEffort::High),
            Some("xhigh") | Some("max") => Some(ThinkingEffort::Xhigh),
            _ => None,
        }
    }

    fn with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider> {
        let reasoning = thinking_effort_to_reasoning_effort(effort);
        Box::new(KimiChatProvider {
            model: self.model.clone(),
            stream: self.stream,
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            default_headers: self.default_headers.clone(),
            generation_kwargs: self.generation_kwargs.clone(),
            reasoning_effort: reasoning,
            extra_body: self.extra_body.clone(),
            http_client: self.http_client.clone(),
        })
    }

    fn with_max_completion_tokens(
        &self,
        max_completion_tokens: i64,
    ) -> Option<Box<dyn ChatProvider>> {
        let mut kwargs = self.generation_kwargs.clone();
        kwargs.insert(
            "max_completion_tokens".into(),
            Value::Number(max_completion_tokens.into()),
        );
        Some(Box::new(KimiChatProvider {
            model: self.model.clone(),
            stream: self.stream,
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            default_headers: self.default_headers.clone(),
            generation_kwargs: kwargs,
            reasoning_effort: self.reasoning_effort.clone(),
            extra_body: self.extra_body.clone(),
            http_client: self.http_client.clone(),
        }))
    }

    fn get_capability(&self, model: Option<&str>) -> ModelCapability {
        get_kimi_model_capability(model.unwrap_or(&self.model))
    }

    async fn generate(
        &self,
        system_prompt: &str,
        tools: &[Tool],
        history: &[Message],
        options: Option<GenerateOptions>,
    ) -> Result<StreamedMessage, ChatProviderError> {
        let auth = options.as_ref().and_then(|o| o.auth.as_ref());
        let stream = self.stream;
        let create_params = self.build_create_params(system_prompt, tools, history, stream)?;
        let headers = self.build_headers(auth)?;

        let base = self.base_url.trim_end_matches('/');
        let url = format!("{}/chat/completions", base);

        let response: HttpResponse = self
            .http_client
            .post_json(&url, headers, create_params)
            .await?;

        let status = response.status();
        if !(200..300).contains(&status) {
            let body_bytes = futures_util::StreamExt::collect::<Vec<Result<bytes::Bytes, _>>>(
                response.bytes_stream(),
            )
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .fold(Vec::new(), |mut acc, b| {
                acc.extend_from_slice(&b);
                acc
            });
            let body_str = String::from_utf8_lossy(&body_bytes);
            let error_msg = serde_json::from_str::<Value>(&body_str)
                .ok()
                .and_then(|v| {
                    v.get("error")
                        .and_then(|e| e.get("message").or_else(|| e.get("message")))
                        .and_then(|m| m.as_str().map(|s| s.to_string()))
                })
                .or_else(|| {
                    serde_json::from_str::<Value>(&body_str).ok().and_then(|v| {
                        v.get("message")
                            .and_then(|m| m.as_str().map(|s| s.to_string()))
                    })
                })
                .unwrap_or_else(|| body_str.to_string());
            return Err(crate::openai_common::convert_openai_error(
                &error_msg,
                Some(status),
                None,
            ));
        }

        if stream {
            let body_bytes = futures_util::StreamExt::collect::<Vec<Result<bytes::Bytes, _>>>(
                response.bytes_stream(),
            )
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .fold(Vec::new(), |mut acc, b| {
                acc.extend_from_slice(&b);
                acc
            });

            let (parts, id, usage, finish_reason, raw_finish_reason) =
                parse_stream_response_with_usage_extractor(
                    body_bytes,
                    Some("reasoning_content"),
                    kimi_usage_extractor,
                )
                .await?;

            Ok(StreamedMessage::from_parts(
                parts,
                id,
                usage,
                finish_reason,
                raw_finish_reason,
            ))
        } else {
            let body_bytes = futures_util::StreamExt::collect::<Vec<Result<bytes::Bytes, _>>>(
                response.bytes_stream(),
            )
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .fold(Vec::new(), |mut acc, b| {
                acc.extend_from_slice(&b);
                acc
            });

            let (parts, id, usage, finish_reason, raw_finish_reason) =
                parse_non_stream_response(&body_bytes, Some("reasoning_content"))?;

            Ok(StreamedMessage::from_parts(
                parts,
                id,
                usage,
                finish_reason,
                raw_finish_reason,
            ))
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_client::MockHttpClient;
    use crate::message::{ContentPart, Message, Role, StreamedMessagePart, ToolCall};
    use crate::provider::Tool;
    use futures_util::StreamExt;
    use serde_json::json;

    // ── CaptureJsonClient ──────────────────────────────────────────────

    struct CaptureJsonClient {
        response_status: u16,
        response_body: Vec<u8>,
        captured_body: std::sync::Mutex<Option<Value>>,
    }

    impl CaptureJsonClient {
        fn new(status: u16, body: Vec<u8>) -> Self {
            Self {
                response_status: status,
                response_body: body,
                captured_body: std::sync::Mutex::new(None),
            }
        }

        #[allow(dead_code)]
        fn captured_body(&self) -> Option<Value> {
            self.captured_body.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl HttpClient for CaptureJsonClient {
        async fn post_json(
            &self,
            _url: &str,
            _headers: HashMap<String, String>,
            body: Value,
        ) -> Result<HttpResponse, ChatProviderError> {
            *self.captured_body.lock().unwrap() = Some(body.clone());
            let chunks = vec![self.response_body.clone()];
            let stream =
                futures_util::stream::iter(chunks.into_iter().map(|c| Ok(bytes::Bytes::from(c))))
                    .boxed();
            Ok(HttpResponse::new(self.response_status, stream))
        }

        async fn post_multipart(
            &self,
            _url: &str,
            _headers: HashMap<String, String>,
            _parts: Vec<crate::http_client::MultipartPart>,
            _fields: HashMap<String, String>,
        ) -> Result<HttpResponse, ChatProviderError> {
            Err(ChatProviderError::Other("not implemented".into()))
        }
    }

    // ── Helpers ────────────────────────────────────────────────────────

    fn make_provider(model: &str) -> KimiChatProvider {
        KimiChatProvider::new(KimiOptions {
            api_key: Some("sk-test".into()),
            base_url: None,
            model: model.into(),
            stream: Some(false),
            default_headers: None,
            generation_kwargs: None,
            http_client: None,
            reasoning_key: None,
        })
    }

    // ── Provider shell tests ───────────────────────────────────────────

    #[test]
    fn provider_name() {
        let p = make_provider("kimi-k2-0711");
        assert_eq!(p.name(), "kimi");
    }

    #[test]
    fn provider_model_name() {
        let p = make_provider("kimi-k2-0711");
        assert_eq!(p.model_name(), "kimi-k2-0711");
    }

    #[test]
    fn provider_get_capability() {
        let cap = make_provider("kimi-k2-0711").get_capability(None);
        assert!(cap.thinking);
        assert!(cap.tool_use);
        assert!(!cap.image_in);
        assert!(!cap.video_in);
    }

    #[test]
    fn provider_thinking_effort_none_by_default() {
        let p = make_provider("kimi-k2-0711");
        assert_eq!(p.thinking_effort(), None);
    }

    #[test]
    fn provider_with_thinking() {
        let p = make_provider("kimi-k2-0711").with_thinking(ThinkingEffort::High);
        assert_eq!(p.thinking_effort(), Some(ThinkingEffort::High));
    }

    #[test]
    fn provider_with_max_completion_tokens() {
        let p = make_provider("kimi-k2-0711")
            .with_max_completion_tokens(4096)
            .unwrap();
        // After with_max_completion_tokens, we don't have a public getter,
        // but we can verify it doesn't panic
        assert_eq!(p.name(), "kimi");
    }

    // ── Request body tests ─────────────────────────────────────────────

    #[test]
    fn omits_empty_assistant_content() {
        let msg = Message {
            role: Role::Assistant,
            name: None,
            content: vec![],
            tool_calls: vec![ToolCall {
                call_type: "function".into(),
                id: "tc1".into(),
                name: "read".into(),
                arguments: Some("{}".into()),
                extras: None,
                stream_index: None,
            }],
            tool_call_id: None,
            partial: None,
        };
        let openai_msg = KimiChatProvider::convert_message(&msg).unwrap();
        assert_eq!(openai_msg.role, "assistant");
        assert!(openai_msg.content.is_none());
        assert_eq!(openai_msg.tool_calls.len(), 1);
    }

    #[test]
    fn converts_think_parts_to_reasoning_content() {
        let msg = Message {
            role: Role::Assistant,
            name: None,
            content: vec![
                ContentPart::Think {
                    think: "step 1".into(),
                    encrypted: None,
                },
                ContentPart::Think {
                    think: " step 2".into(),
                    encrypted: None,
                },
                ContentPart::Text {
                    text: "answer".into(),
                },
            ],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let openai_msg = KimiChatProvider::convert_message(&msg).unwrap();
        assert_eq!(
            openai_msg.extra.get("reasoning_content").unwrap(),
            "step 1 step 2"
        );
        assert_eq!(openai_msg.content, Some(Value::String("answer".into())));
    }

    #[test]
    fn normalizes_max_tokens_to_max_completion_tokens() {
        let mut p = make_provider("kimi-k2-0711");
        p.generation_kwargs
            .insert("max_tokens".into(), Value::Number(4096.into()));

        let params = p
            .build_create_params("hello", &[], &[Message::user_text("hi")], false)
            .unwrap();
        let obj = params.as_object().unwrap();
        assert!(obj.contains_key("max_completion_tokens"));
        assert!(!obj.contains_key("max_tokens"));
        assert_eq!(obj["max_completion_tokens"], 4096);
    }

    #[test]
    fn flattens_extra_body() {
        let mut extra_body = Map::new();
        extra_body.insert("custom_param".into(), Value::String("value".into()));

        let p = KimiChatProvider::new(KimiOptions {
            api_key: Some("sk-test".into()),
            base_url: None,
            model: "kimi-k2-0711".into(),
            stream: Some(false),
            default_headers: None,
            generation_kwargs: Some({
                let mut m = HashMap::new();
                m.insert("extra_body".into(), Value::Object(extra_body));
                m
            }),
            http_client: None,
            reasoning_key: None,
        });

        let params = p
            .build_create_params("hello", &[], &[Message::user_text("hi")], false)
            .unwrap();
        let obj = params.as_object().unwrap();
        assert_eq!(obj["custom_param"], "value");
    }

    #[test]
    fn normalizes_tool_schema() {
        let tool = Tool {
            name: "my_tool".into(),
            description: "A test tool".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": { "description": "a name" }
                }
            }),
        };

        let mut p = make_provider("kimi-k2-0711");
        p.generation_kwargs
            .insert("max_tokens".into(), Value::Number(4096.into()));

        let non_stream_body = serde_json::json!({
            "id": "chat-1",
            "choices": [{
                "message": {
                    "content": "done"
                },
                "finish_reason": "stop"
            }]
        });

        let client = Arc::new(CaptureJsonClient::new(
            200,
            serde_json::to_vec(&non_stream_body).unwrap(),
        ));

        let _p = KimiChatProvider {
            http_client: client.clone(),
            ..p
        };

        // We don't need to actually run generate - we just want the captured body
        // But generate requires tokio runtime, so let's test convert_tool directly
        let tool_value = KimiChatProvider::convert_tool(&tool);
        assert_eq!(tool_value["type"], "function");
        assert_eq!(tool_value["function"]["name"], "my_tool");
        // Should have type inferred by normalize_kimi_tool_schema
        assert_eq!(
            tool_value["function"]["parameters"]["properties"]["name"]["type"],
            "string"
        );
    }

    #[test]
    fn converts_builtin_function_tool() {
        let tool = Tool {
            name: "$web_search".into(),
            description: "search the web".into(),
            parameters: json!({}),
        };
        let tool_value = KimiChatProvider::convert_tool(&tool);
        assert_eq!(tool_value["type"], "builtin_function");
        assert_eq!(tool_value["function"]["name"], "web_search");
    }

    #[test]
    fn stream_options_include_usage_when_streaming() {
        let p = make_provider("kimi-k2-0711");
        let params = p
            .build_create_params("hello", &[], &[Message::user_text("hi")], true)
            .unwrap();
        let obj = params.as_object().unwrap();
        assert_eq!(obj["stream_options"]["include_usage"], true);
    }

    #[test]
    fn no_stream_options_when_not_streaming() {
        let p = make_provider("kimi-k2-0711");
        let params = p
            .build_create_params("hello", &[], &[Message::user_text("hi")], false)
            .unwrap();
        let obj = params.as_object().unwrap();
        assert!(!obj.contains_key("stream_options"));
    }

    #[test]
    fn reasoning_effort_in_body_when_set() {
        let p = KimiChatProvider::new(KimiOptions {
            api_key: Some("sk-test".into()),
            base_url: None,
            model: "kimi-k2-0711".into(),
            stream: Some(false),
            default_headers: None,
            generation_kwargs: Some({
                let mut m = HashMap::new();
                m.insert("reasoning_effort".into(), Value::String("high".into()));
                m
            }),
            http_client: None,
            reasoning_key: None,
        });
        let params = p
            .build_create_params("hello", &[], &[Message::user_text("hi")], false)
            .unwrap();
        assert_eq!(params["reasoning_effort"], "high");
    }

    // ── Generate tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn generate_streams_text() {
        let sse = b"data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hello\"}}]}\n\ndata: {\"id\":\"2\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"}}]}\n\ndata: {\"id\":\"3\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";

        let client = Arc::new(MockHttpClient::new(200, sse));
        let p = KimiChatProvider {
            stream: true,
            http_client: client,
            ..make_provider("kimi-k2-0711")
        };

        let result = p
            .generate("hello", &[], &[Message::user_text("hi")], None)
            .await
            .unwrap();
        let collected: Vec<_> = futures_util::StreamExt::collect::<Vec<_>>(result).await;
        let texts: Vec<_> = collected
            .iter()
            .filter_map(|p| match p {
                StreamedMessagePart::Content(ContentPart::Text { text }) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, vec!["Hello", " world"]);
    }

    #[tokio::test]
    async fn generate_streams_reasoning() {
        let sse = b"data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"reasoning_content\":\"Let me think...\"}}]}\n\ndata: {\"id\":\"2\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"answer\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";

        let client = Arc::new(MockHttpClient::new(200, sse));
        let p = KimiChatProvider {
            stream: true,
            http_client: client,
            ..make_provider("kimi-k2-0711")
        };

        let result = p
            .generate("hello", &[], &[Message::user_text("hi")], None)
            .await
            .unwrap();
        let collected: Vec<_> = futures_util::StreamExt::collect::<Vec<_>>(result).await;

        let has_think = collected
            .iter()
            .any(|p| matches!(p, StreamedMessagePart::Content(ContentPart::Think { .. })));
        assert!(has_think);
    }

    #[tokio::test]
    async fn generate_streams_tool_call() {
        let sse = b"data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"tc1\",\"type\":\"function\",\"function\":{\"name\":\"read\",\"arguments\":\"{}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n\n";

        let client = Arc::new(MockHttpClient::new(200, sse));
        let p = KimiChatProvider {
            stream: true,
            http_client: client,
            ..make_provider("kimi-k2-0711")
        };

        let result = p
            .generate("hello", &[], &[Message::user_text("hi")], None)
            .await
            .unwrap();
        let collected: Vec<_> = futures_util::StreamExt::collect::<Vec<_>>(result).await;

        let has_tool_call = collected
            .iter()
            .any(|p| matches!(p, StreamedMessagePart::ToolCall(_)));
        assert!(has_tool_call);
    }

    #[tokio::test]
    async fn generate_reads_choice_usage() {
        let sse = b"data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"usage\":{\"prompt_tokens\":20,\"completion_tokens\":4,\"cached_tokens\":5}}]}\n\ndata: [DONE]\n\n";

        let client = Arc::new(MockHttpClient::new(200, sse));
        let p = KimiChatProvider {
            stream: true,
            http_client: client,
            ..make_provider("kimi-k2-0711")
        };

        let result = p
            .generate("hello", &[], &[Message::user_text("hi")], None)
            .await
            .unwrap();
        let usage = result.usage();
        let collected: Vec<_> = futures_util::StreamExt::collect::<Vec<_>>(result).await;
        assert!(!collected.is_empty());
        assert!(usage.is_some());
        let u = usage.unwrap();
        assert_eq!(u.input_other, 15);
        assert_eq!(u.input_cache_read, 5);
        assert_eq!(u.output, 4);
    }

    #[tokio::test]
    async fn generate_non_stream_with_tool_call() {
        let body = br#"{"id":"chat-1","choices":[{"message":{"content":"ok","tool_calls":[{"id":"tc1","type":"function","function":{"name":"read","arguments":"{}"}}]},"finish_reason":"tool_calls"}]}"#;

        let client = Arc::new(MockHttpClient::new(200, body));
        let p = KimiChatProvider {
            http_client: client,
            ..make_provider("kimi-k2-0711")
        };

        let result = p
            .generate("hello", &[], &[Message::user_text("hi")], None)
            .await
            .unwrap();
        let result_id = result.id();
        let collected: Vec<_> = futures_util::StreamExt::collect::<Vec<_>>(result).await;

        let has_tool_call = collected
            .iter()
            .any(|p| matches!(p, StreamedMessagePart::ToolCall(_)));
        assert!(has_tool_call);
        assert_eq!(result_id, Some("chat-1".into()));
    }

    #[tokio::test]
    async fn generate_rejects_empty_api_key() {
        let p = KimiChatProvider::new(KimiOptions {
            api_key: None,
            base_url: None,
            model: "kimi-k2-0711".into(),
            stream: Some(false),
            default_headers: None,
            generation_kwargs: None,
            http_client: None,
            reasoning_key: None,
        });

        // Clear the env var so the fallback also fails
        // We can't easily clear env vars in tests, but since we're not setting KIMI_API_KEY,
        // and api_key is None, it should fail.
        let err = p
            .generate("hello", &[], &[Message::user_text("hi")], None)
            .await
            .unwrap_err();
        assert!(matches!(err, ChatProviderError::MissingApiKey(_)));
    }

    #[tokio::test]
    async fn generate_uses_env_api_key() {
        // Temporarily set KIMI_API_KEY
        std::env::set_var("KIMI_API_KEY", "sk-env-test");

        let body =
            br#"{"id":"chat-1","choices":[{"message":{"content":"done"},"finish_reason":"stop"}]}"#;
        let client = Arc::new(MockHttpClient::new(200, body));

        let p = KimiChatProvider {
            http_client: client,
            ..KimiChatProvider::new(KimiOptions {
                api_key: None,
                base_url: None,
                model: "kimi-k2-0711".into(),
                stream: Some(false),
                default_headers: None,
                generation_kwargs: None,
                http_client: None,
                reasoning_key: None,
            })
        };

        let result = p
            .generate("hello", &[], &[Message::user_text("hi")], None)
            .await
            .unwrap();
        let collected: Vec<_> = futures_util::StreamExt::collect::<Vec<_>>(result).await;
        assert!(!collected.is_empty());
    }
}
