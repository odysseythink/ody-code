use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;

use crate::capability_registry::get_glm_model_capability;
use crate::chat_completions_stream::{parse_non_stream_response, parse_stream_response};
use crate::errors::{APIMissingApiKeyError, ChatProviderError};
use crate::generate::StreamedMessage;
use crate::http_client::{HttpClient, ReqwestClient};
use crate::message::{ContentPart, Message};
use crate::openai_common::{convert_openai_error, tool_to_openai, OpenAIContentPart};
use crate::provider::{ChatProvider, GenerateOptions, ModelCapability, ThinkingEffort, Tool};
use crate::request_auth::merge_request_headers;
use crate::tool_call_id::{
    normalize_tool_call_ids_for_provider, sanitize_tool_call_id, ToolCallIdPolicy,
};

// ── Options & kwargs ─────────────────────────────────────────────────────

#[derive(Clone)]
pub struct GLMOptions {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: String,
    pub stream: Option<bool>,
    pub max_tokens: Option<i64>,
    pub default_headers: Option<HashMap<String, String>>,
    pub http_client: Option<Arc<dyn HttpClient>>,
}

impl std::fmt::Debug for GLMOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GLMOptions")
            .field("model", &self.model)
            .field("stream", &self.stream)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Default, Serialize)]
pub struct GLMGenerationKwargs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Value>,
}

// ── GLM wire types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct GLMMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "tool_calls")]
    tool_calls: Option<Vec<GLMToolCallOut>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "tool_call_id")]
    tool_call_id: Option<String>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct GLMToolCallOut {
    #[serde(rename = "type")]
    call_type: String,
    id: String,
    function: GLMToolFunctionOut,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct GLMToolFunctionOut {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<String>,
}

// ── Message conversion ───────────────────────────────────────────────────

fn convert_glm_content_part(part: &ContentPart) -> Option<OpenAIContentPart> {
    match part {
        ContentPart::Text { text } => Some(OpenAIContentPart {
            r#type: "text".into(),
            text: Some(text.clone()),
            image_url: None,
            audio_url: None,
            video_url: None,
        }),
        // GLM silently drops Think, ImageUrl, AudioUrl, VideoUrl parts
        ContentPart::Think { .. }
        | ContentPart::ImageUrl { .. }
        | ContentPart::AudioUrl { .. }
        | ContentPart::VideoUrl { .. } => None,
    }
}

pub fn convert_glm_message(message: &Message, reasoning_key: Option<&str>) -> GLMMessage {
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

    // Filter empty text parts
    let filtered: Vec<&ContentPart> = non_think
        .iter()
        .filter(|p| match p {
            ContentPart::Text { text } => !text.is_empty(),
            _ => true,
        })
        .cloned()
        .collect();

    // Convert remaining parts; ImageUrl/AudioUrl/VideoUrl are silently dropped
    let converted: Vec<OpenAIContentPart> = filtered
        .iter()
        .filter_map(|p| convert_glm_content_part(p))
        .collect();

    let content = if converted.is_empty() {
        None
    } else if converted.len() == 1 {
        if let Some(first) = converted.first() {
            if first.r#type == "text" {
                first.text.clone().map(Value::String)
            } else {
                Some(serde_json::to_value(first).unwrap())
            }
        } else {
            None
        }
    } else {
        let parts: Vec<Value> = converted
            .iter()
            .map(|c| serde_json::to_value(c).unwrap())
            .collect();
        Some(Value::Array(parts))
    };

    let tool_calls = if message.tool_calls.is_empty() {
        None
    } else {
        Some(
            message
                .tool_calls
                .iter()
                .map(|tc| GLMToolCallOut {
                    call_type: tc.call_type.clone(),
                    id: tc.id.clone(),
                    function: GLMToolFunctionOut {
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

    GLMMessage {
        role: format!("{:?}", message.role).to_lowercase(),
        content,
        name: message.name.clone(),
        tool_calls,
        tool_call_id: message.tool_call_id.clone(),
        extra,
    }
}

// ── Provider struct ──────────────────────────────────────────────────────

pub struct GLMChatProvider {
    model: String,
    stream: bool,
    api_key: Option<String>,
    base_url: String,
    default_headers: Option<HashMap<String, String>>,
    thinking_effort: Option<ThinkingEffort>,
    max_output_tokens_cap: Option<i64>,
    generation_kwargs: GLMGenerationKwargs,
    http_client: Arc<dyn HttpClient>,
}

impl GLMChatProvider {
    pub fn new(options: GLMOptions) -> Self {
        let api_key = options
            .api_key
            .or_else(|| std::env::var("GLM_API_KEY").ok())
            .filter(|k| !k.is_empty());
        let base_url = options
            .base_url
            .unwrap_or_else(|| "https://api.z.ai/api/paas/v4/".into());
        let http_client = options
            .http_client
            .unwrap_or_else(|| Arc::new(ReqwestClient::new(reqwest::Client::new())));

        let mut generation_kwargs = GLMGenerationKwargs::default();
        if let Some(max) = options.max_tokens {
            generation_kwargs.max_tokens = Some(max);
        }

        Self {
            model: options.model,
            stream: options.stream.unwrap_or(true),
            api_key,
            base_url,
            default_headers: options.default_headers,
            thinking_effort: None,
            max_output_tokens_cap: options.max_tokens,
            generation_kwargs,
            http_client,
        }
    }
}

impl Clone for GLMChatProvider {
    fn clone(&self) -> Self {
        Self {
            model: self.model.clone(),
            stream: self.stream,
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            default_headers: self.default_headers.clone(),
            thinking_effort: self.thinking_effort,
            max_output_tokens_cap: self.max_output_tokens_cap,
            generation_kwargs: self.generation_kwargs.clone(),
            http_client: Arc::clone(&self.http_client),
        }
    }
}

// ── ChatProvider impl ────────────────────────────────────────────────────

fn glm_tool_call_id_policy() -> ToolCallIdPolicy {
    ToolCallIdPolicy::new(|id| sanitize_tool_call_id(id, Some(64)), Some(64))
}

async fn read_body_bytes(
    stream: &mut crate::http_client::ByteStream,
) -> Result<Vec<u8>, ChatProviderError> {
    let mut buf = Vec::new();
    while let Some(chunk) = stream.next().await {
        buf.extend_from_slice(chunk?.as_ref());
    }
    Ok(buf)
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

#[async_trait]
impl ChatProvider for GLMChatProvider {
    fn name(&self) -> &str {
        "glm"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn thinking_effort(&self) -> Option<ThinkingEffort> {
        self.thinking_effort
    }

    fn get_capability(&self, _model: Option<&str>) -> ModelCapability {
        get_glm_model_capability(&self.model)
    }

    fn with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider> {
        let mut clone = self.clone();
        clone.thinking_effort = Some(effort);
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
        clone.generation_kwargs.max_tokens = Some(effective);
        Some(Box::new(clone))
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
            normalize_tool_call_ids_for_provider(history, &glm_tool_call_id_policy());

        let mut messages: Vec<GLMMessage> = Vec::new();
        if !system_prompt.is_empty() {
            messages.push(GLMMessage {
                role: "system".into(),
                content: Some(Value::String(system_prompt.into())),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                extra: HashMap::new(),
            });
        }
        for msg in &normalized_history {
            messages.push(convert_glm_message(msg, None));
        }

        let mut create_params = serde_json::Map::new();
        create_params.insert("model".into(), Value::String(self.model.clone()));
        create_params.insert("messages".into(), serde_json::to_value(&messages).unwrap());
        create_params.insert("stream".into(), Value::Bool(self.stream));

        // Add generation kwargs (max_tokens, temperature, top_p, stop)
        let kwargs = &self.generation_kwargs;
        if let Some(max_tokens) = kwargs.max_tokens {
            create_params.insert("max_tokens".into(), Value::Number(max_tokens.into()));
        }
        if let Some(temperature) = kwargs.temperature {
            create_params.insert(
                "temperature".into(),
                Value::Number(serde_json::Number::from_f64(temperature).unwrap()),
            );
        }
        if let Some(top_p) = kwargs.top_p {
            create_params.insert(
                "top_p".into(),
                Value::Number(serde_json::Number::from_f64(top_p).unwrap()),
            );
        }
        if let Some(ref stop) = kwargs.stop {
            create_params.insert("stop".into(), stop.clone());
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

        // GLM thinking control: when thinking_effort is Off, send thinking disabled
        if self.thinking_effort == Some(ThinkingEffort::Off) {
            create_params.insert("thinking".into(), serde_json::json!({"type": "disabled"}));
        }

        let url = format!("{}chat/completions", self.base_url.trim_end_matches('/'));
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
            parse_stream_response(body_bytes, None).await?
        } else {
            parse_non_stream_response(&body_bytes, None)?
        };

        Ok(StreamedMessage::from_parts(
            parts,
            id,
            usage,
            finish_reason,
            raw_finish_reason,
        ))
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_client::MockHttpClient;
    use crate::message::{ContentPart, Message, Role, StreamedMessagePart, ToolCall};

    fn text_sse_bytes() -> Vec<u8> {
        br#"data: {"id":"1","choices":[{"index":0,"delta":{"role":"assistant","content":"Hello"}}]}

data: {"id":"2","choices":[{"index":0,"delta":{"content":" world"}}]}

data: {"id":"3","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]

"#
        .to_vec()
    }

    fn non_stream_body() -> Vec<u8> {
        br#"{"id":"chat-1","choices":[{"message":{"content":"Hello world"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":3,"total_tokens":13}}"#.to_vec()
    }

    fn make_provider(
        api_key: Option<&str>,
        model: &str,
        stream: bool,
        status: u16,
        body: Vec<u8>,
    ) -> GLMChatProvider {
        GLMChatProvider::new(GLMOptions {
            api_key: api_key.map(String::from),
            base_url: Some("http://mock".into()),
            model: model.into(),
            stream: Some(stream),
            max_tokens: None,
            default_headers: None,
            http_client: Some(Arc::new(MockHttpClient::new(status, body))),
        })
    }

    // ── name_and_model / get_capability ───────────────────────────────

    #[test]
    fn name_and_model() {
        let p = make_provider(Some("sk"), "glm-4-flash", true, 200, vec![]);
        assert_eq!(p.name(), "glm");
        assert_eq!(p.model_name(), "glm-4-flash");
    }

    #[test]
    fn get_capability_is_unknown() {
        let p = make_provider(Some("sk"), "glm-4-flash", true, 200, vec![]);
        let cap = p.get_capability(None);
        assert!(cap.is_unknown());
    }

    // ── convert_message ──────────────────────────────────────────────

    #[test]
    fn convert_message_filters_empty_text() {
        let msg = Message {
            role: Role::User,
            name: None,
            content: vec![
                ContentPart::Text {
                    text: "hello".into(),
                },
                ContentPart::Text { text: "".into() },
                ContentPart::Text {
                    text: "world".into(),
                },
            ],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let glm_msg = convert_glm_message(&msg, None);
        // Empty text is filtered; two text parts → Array
        assert!(glm_msg.content.is_some());
        let content = glm_msg.content.unwrap();
        assert!(content.is_array());
        let arr = content.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["type"], "text");
        assert_eq!(arr[0]["text"], "hello");
        assert_eq!(arr[1]["type"], "text");
        assert_eq!(arr[1]["text"], "world");
    }

    #[test]
    fn convert_message_aggregates_think_to_reasoning_content() {
        let msg = Message {
            role: Role::Assistant,
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
                ContentPart::Text {
                    text: "answer".into(),
                },
            ],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let glm_msg = convert_glm_message(&msg, None);
        assert_eq!(glm_msg.content, Some(Value::String("answer".into())));
        assert_eq!(
            glm_msg.extra.get("reasoning_content"),
            Some(&Value::String("step1step2".into()))
        );
    }

    #[test]
    fn convert_message_rejects_image_url() {
        let msg = Message {
            role: Role::User,
            name: None,
            content: vec![
                ContentPart::Text {
                    text: "look at this".into(),
                },
                ContentPart::ImageUrl {
                    image_url: crate::message::UrlPayload {
                        url: "http://example.com/img.png".into(),
                        id: None,
                    },
                },
            ],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let glm_msg = convert_glm_message(&msg, None);
        // Image URL is silently dropped, only text remains
        assert_eq!(glm_msg.content, Some(Value::String("look at this".into())));
    }

    #[test]
    fn convert_message_rejects_image_url_only_is_none() {
        let msg = Message {
            role: Role::User,
            name: None,
            content: vec![ContentPart::ImageUrl {
                image_url: crate::message::UrlPayload {
                    url: "http://example.com/img.png".into(),
                    id: None,
                },
            }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let glm_msg = convert_glm_message(&msg, None);
        assert!(glm_msg.content.is_none());
    }

    #[test]
    fn convert_message_serializes_tool_calls() {
        let msg = Message {
            role: Role::Assistant,
            name: None,
            content: vec![],
            tool_calls: vec![ToolCall {
                call_type: "function".into(),
                id: "tc_1".into(),
                name: "read".into(),
                arguments: Some("{}".into()),
                extras: None,
                stream_index: None,
            }],
            tool_call_id: None,
            partial: None,
        };
        let glm_msg = convert_glm_message(&msg, None);
        let tcs = glm_msg.tool_calls.as_ref().unwrap();
        assert_eq!(tcs.len(), 1);
        assert_eq!(tcs[0].id, "tc_1");
        assert_eq!(tcs[0].function.name, "read");
    }

    // ── with_thinking / with_max_completion_tokens ──────────────────

    #[test]
    fn with_thinking_sets_disabled() {
        let p = make_provider(Some("sk"), "glm-4-flash", true, 200, vec![]);
        let q = p.with_thinking(ThinkingEffort::Off);
        assert_eq!(q.thinking_effort(), Some(ThinkingEffort::Off));
    }

    #[test]
    fn with_max_completion_tokens_clamps_to_cap() {
        let p = GLMChatProvider::new(GLMOptions {
            api_key: Some("sk".into()),
            base_url: None,
            model: "glm-4-flash".into(),
            stream: None,
            max_tokens: Some(100),
            default_headers: None,
            http_client: None,
        });
        // with_max_completion_tokens(200) should clamp to cap of 100
        let limited = p.with_max_completion_tokens(200);
        assert!(limited.is_some());
    }

    #[test]
    fn with_max_completion_tokens_no_cap() {
        let p = GLMChatProvider::new(GLMOptions {
            api_key: Some("sk".into()),
            base_url: None,
            model: "glm-4-flash".into(),
            stream: None,
            max_tokens: None,
            default_headers: None,
            http_client: None,
        });
        let limited = p.with_max_completion_tokens(200);
        assert!(limited.is_some());
    }

    // ── generate streaming ──────────────────────────────────────────

    #[tokio::test]
    async fn generate_streams_text() {
        let provider = make_provider(Some("sk"), "glm-4-flash", true, 200, text_sse_bytes());
        let stream = provider.generate("", &[], &[], None).await.unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(stream).await;
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], StreamedMessagePart::text("Hello"));
        assert_eq!(parts[1], StreamedMessagePart::text(" world"));
    }

    #[tokio::test]
    async fn generate_non_stream_text_and_usage() {
        let provider = make_provider(Some("sk"), "glm-4-flash", false, 200, non_stream_body());
        let stream = provider.generate("", &[], &[], None).await.unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(stream).await;
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0], StreamedMessagePart::text("Hello world"));
    }

    // ── error handling ──────────────────────────────────────────────

    #[tokio::test]
    async fn generate_rejects_missing_api_key() {
        let provider = make_provider(None, "glm-4-flash", true, 200, vec![]);
        let result = provider.generate("", &[], &[], None).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ChatProviderError::MissingApiKey(e) => {
                assert_eq!(e.provider, "glm");
            }
            other => panic!("expected MissingApiKey, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn generate_converts_http_error() {
        let error_body =
            br#"{"error":{"message":"Invalid model","code":"invalid_model"}}"#.to_vec();
        let provider = make_provider(Some("sk"), "bad-model", true, 400, error_body);
        let result = provider.generate("", &[], &[], None).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ChatProviderError::Status(e) => {
                assert_eq!(e.status_code, 400);
            }
            other => panic!("expected Status error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn generate_non_stream_produces_usage() {
        // Non-streaming response includes usage
        let provider = make_provider(Some("sk"), "glm-4-flash", false, 200, non_stream_body());
        let stream = provider.generate("", &[], &[], None).await.unwrap();
        assert!(stream.usage().is_some());
        let usage = stream.usage().unwrap();
        assert_eq!(usage.input_other, 10);
        assert_eq!(usage.output, 3);
    }
}
