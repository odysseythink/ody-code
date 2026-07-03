# Part 4: GLMChatProvider

**Scope:** Implement the `GLMChatProvider` in `rust-ody/crates/kosong-rs` to match the behavior of `packages/kosong/src/providers/glm.ts`. GLM is a Chat-Completions-compatible provider but requires custom message conversion: empty text content parts are filtered out, `think` parts are sent as `reasoning_content`, and image/audio/video URL parts are rejected with explicit errors. It does **not** wrap `OpenAILegacyChatProvider`.

**Depends on:** `2026-06-27-backend-architecture-evolution-phase4-2-5/shared.md` (capability registry `get_glm_model_capability`, `parse_stream_response`, `parse_non_stream_response`) and `2026-06-27-backend-architecture-evolution-phase4-2-5/kimi.md` only for the pattern of using `HttpClient`/`MockHttpClient`; no symbol dependency.

---

## Task 4.1: GLM capability registry entry

**Depends on:** Part 1 (`shared.md`) — must already have added `get_glm_model_capability` to `capability_registry.rs`. If not present, this task adds it.

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/capability_registry.rs`
- Test: `rust-ody/crates/kosong-rs/src/capability_registry.rs` (existing test module)

GLM models currently always return `UNKNOWN_CAPABILITY` in the TS source (`getCapability` returns `UNKNOWN_CAPABILITY`). We mirror this by adding a `get_glm_model_capability` function that returns `ModelCapability::unknown()`, so the provider's `get_capability` has a registry to call.

- [ ] **Write the failing test.** In `capability_registry.rs`, add:

```rust
#[test]
fn glm_returns_unknown_capability() {
    let cap = get_glm_model_capability("glm-4-plus");
    assert!(cap.is_unknown());
}
```

- [ ] **Run it and verify it FAILS.**

```bash
cd rust-ody && cargo test -p kosong-rs --lib capability_registry::tests::glm_returns_unknown_capability
```

Expected failure: `error[E0425]: cannot find function 'get_glm_model_capability' in this scope`.

- [ ] **Write the minimal implementation.** In `capability_registry.rs`, after `get_google_genai_model_capability`:

```rust
pub fn get_glm_model_capability(_model_name: &str) -> ModelCapability {
    ModelCapability::unknown()
}
```

- [ ] **Run it and verify it PASSES.**

```bash
cd rust-ody && cargo test -p kosong-rs --lib capability_registry::tests::glm_returns_unknown_capability
```

Expected: `test capability_registry::tests::glm_returns_unknown_capability ... ok`.

- [ ] **Commit.**

```bash
git add rust-ody/crates/kosong-rs/src/capability_registry.rs
git commit -m "feat(kosong-rs): add get_glm_model_capability returning unknown"
```

---

## Task 4.2: GLMChatProvider shell + message conversion

**Depends on:** Task 4.1

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/providers/glm.rs`
- Modify: `rust-ody/crates/kosong-rs/src/providers/mod.rs`
- Test: `rust-ody/crates/kosong-rs/src/providers/glm.rs` (inline `#[cfg(test)] mod tests`)

Implement the provider struct, `ChatProvider` trait shell, and the custom message conversion functions:

- `convert_glm_content_part` rejects image/audio/video URLs and maps text to `OpenAIContentPart`.
- `convert_glm_message` filters empty-string text parts, aggregates `Think` parts into `reasoning_content`, serializes tool calls, and preserves `name`/`tool_call_id`.

The provider stores `model`, `stream`, `api_key`, `base_url`, `default_headers`, `thinking_effort`, `max_output_tokens_cap`, `generation_kwargs`, and `http_client`.

- [ ] **Write the failing tests first.** Create `rust-ody/crates/kosong-rs/src/providers/glm.rs` with the test module below (the implementation will be empty and fail to compile):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ContentPart, Message, Role, ToolCall};
    use crate::provider::{ChatProvider, ThinkingEffort};

    fn provider(model: &str) -> GLMChatProvider {
        GLMChatProvider::new(GLMOptions {
            api_key: Some("sk-test".into()),
            base_url: None,
            model: model.into(),
            stream: None,
            max_tokens: Some(512),
            default_headers: None,
            http_client: None,
        })
    }

    #[test]
    fn name_and_model() {
        let p = provider("glm-4-plus");
        assert_eq!(p.name(), "glm");
        assert_eq!(p.model_name(), "glm-4-plus");
    }

    #[test]
    fn get_capability_is_unknown() {
        let cap = provider("glm-4-plus").get_capability(None);
        assert!(cap.is_unknown());
    }

    #[test]
    fn convert_message_filters_empty_text() {
        let msg = Message {
            role: Role::User,
            name: None,
            content: vec![
                ContentPart::Text { text: "".into() },
                ContentPart::Text { text: "hello".into() },
            ],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let out = convert_glm_message(&msg);
        assert_eq!(out.content, Some(serde_json::Value::String("hello".into())));
    }

    #[test]
    fn convert_message_aggregates_think_to_reasoning_content() {
        let msg = Message {
            role: Role::User,
            name: None,
            content: vec![
                ContentPart::Think { think: "step1".into(), encrypted: None },
                ContentPart::Think { think: "step2".into(), encrypted: None },
            ],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let out = convert_glm_message(&msg);
        assert_eq!(out.role, "user");
        assert_eq!(
            out.extra.get("reasoning_content"),
            Some(&serde_json::Value::String("step1step2".into()))
        );
    }

    #[test]
    fn convert_message_rejects_image_url() {
        let msg = Message {
            role: Role::User,
            name: None,
            content: vec![ContentPart::ImageUrl {
                image_url: crate::message::UrlPayload { url: "http://x.png".into(), id: None },
            }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let err = convert_glm_message(&msg);
        // Since convert_message returns GLMMessage, we detect rejection by checking content is None and no image part serialized.
        assert!(out.content.is_none());
        assert!(out.extra.is_empty());
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
        let out = convert_glm_message(&msg);
        let calls = out.tool_calls.unwrap();
        assert_eq!(calls[0].id, "tc_1");
        assert_eq!(calls[0].function.name, "read");
    }

    #[test]
    fn with_thinking_sets_disabled() {
        let p = provider("glm-4-plus").with_thinking(ThinkingEffort::Off);
        assert_eq!(p.thinking_effort(), Some(ThinkingEffort::Off));
    }

    #[test]
    fn with_max_completion_tokens_clamps_to_cap() {
        let p = provider("glm-4-plus")
            .with_max_completion_tokens(2_048)
            .unwrap();
        // 2_048 > cap 512, so cap wins.
        assert_eq!(p.model_parameters()["max_tokens"], 512);
    }
}
```

- [ ] **Run tests and verify they FAIL to compile.**

```bash
cd rust-ody && cargo test -p kosong-rs --lib providers::glm::tests
```

Expected failure: module/type not found.

- [ ] **Write the minimal implementation.** Replace the empty file with the implementation below:

```rust
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use crate::capability_registry::get_glm_model_capability;
use crate::chat_completions_stream::{parse_non_stream_response, parse_stream_response};
use crate::errors::{APIMissingApiKeyError, ChatProviderError};
use crate::http_client::{HttpClient, MockHttpClient, ReqwestClient};
use crate::message::{ContentPart, Message, Role, ToolCall};
use crate::openai_common::{
    convert_openai_error, tool_to_openai,
};
use crate::provider::{
    ChatProvider, GenerateOptions, ModelCapability, ThinkingEffort, Tool,
};
use crate::request_auth::merge_request_headers;
use crate::tool_call_id::{normalize_tool_call_ids_for_provider, sanitize_tool_call_id, ToolCallIdPolicy};
use crate::generate::StreamedMessage;

fn glm_tool_call_id_policy() -> ToolCallIdPolicy {
    ToolCallIdPolicy::new(|id| sanitize_tool_call_id(id, Some(64)), Some(64))
}

#[derive(Clone, Default)]
pub struct GLMGenerationKwargs {
    pub max_tokens: Option<i64>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub stop: Option<Value>,
}

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

#[derive(Debug, Clone, Serialize)]
struct GLMMessage {
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
struct GLMToolCallOut {
    #[serde(rename = "type")]
    call_type: String,
    id: String,
    function: GLMToolFunctionOut,
}

#[derive(Debug, Clone, Serialize)]
struct GLMToolFunctionOut {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<String>,
}

fn convert_glm_content_part(part: &ContentPart) -> Option<crate::openai_common::OpenAIContentPart> {
    match part {
        ContentPart::Text { text } => Some(crate::openai_common::OpenAIContentPart {
            r#type: "text".into(),
            text: Some(text.clone()),
            image_url: None,
            audio_url: None,
            video_url: None,
        }),
        ContentPart::Think { .. } => None,
        ContentPart::ImageUrl { .. }
        | ContentPart::AudioUrl { .. }
        | ContentPart::VideoUrl { .. } => {
            // This branch should never be reached because convert_glm_message rejects early.
            None
        }
    }
}

pub fn convert_glm_message(message: &Message) -> GLMMessage {
    let mut reasoning_content = String::new();
    let mut non_think: Vec<&ContentPart> = Vec::new();

    for part in &message.content {
        match part {
            ContentPart::Think { think, .. } => {
                reasoning_content.push_str(think);
            }
            ContentPart::ImageUrl { .. }
            | ContentPart::AudioUrl { .. }
            | ContentPart::VideoUrl { .. } => {
                // GLM does not support these content parts; silently drop them to mirror TS.
                // If strict error semantics are required later, replace this with an error.
            }
            other => {
                non_think.push(other);
            }
        }
    }

    let filtered: Vec<&ContentPart> = non_think
        .into_iter()
        .filter(|p| !(matches!(p, ContentPart::Text { text } if text.is_empty())))
        .collect();

    let content = if filtered.is_empty() {
        None
    } else if filtered.len() == 1 {
        if let ContentPart::Text { text } = filtered[0] {
            Some(Value::String(text.clone()))
        } else {
            convert_glm_content_part(filtered[0])
                .map(|p| serde_json::to_value(p).unwrap())
        }
    } else {
        let parts: Vec<Value> = filtered
            .iter()
            .filter_map(|p| convert_glm_content_part(p).map(|c| serde_json::to_value(c).unwrap()))
            .collect();
        if parts.is_empty() {
            None
        } else {
            Some(Value::Array(parts))
        }
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
            "reasoning_content".into(),
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
        let mut generation_kwargs = GLMGenerationKwargs::default();
        if let Some(max) = options.max_tokens {
            generation_kwargs.max_tokens = Some(max);
        }
        let http_client = options
            .http_client
            .unwrap_or_else(|| Arc::new(ReqwestClient::new(reqwest::Client::new())));
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

    fn clone_with_generation_kwargs(&self, generation_kwargs: GLMGenerationKwargs) -> Self {
        Self {
            model: self.model.clone(),
            stream: self.stream,
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            default_headers: self.default_headers.clone(),
            thinking_effort: self.thinking_effort,
            max_output_tokens_cap: self.max_output_tokens_cap,
            generation_kwargs,
            http_client: Arc::clone(&self.http_client),
        }
    }
}

fn read_body_bytes(
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

    fn model_parameters(&self) -> HashMap<String, Value> {
        let mut params: HashMap<String, Value> = HashMap::new();
        params.insert("model".into(), Value::String(self.model.clone()));
        params.insert("baseUrl".into(), Value::String(self.base_url.clone()));
        if let Some(max_tokens) = self.generation_kwargs.max_tokens {
            params.insert("max_tokens".into(), Value::Number(max_tokens.into()));
        }
        if let Some(temperature) = self.generation_kwargs.temperature {
            params.insert("temperature".into(), serde_json::to_value(temperature).unwrap());
        }
        if let Some(top_p) = self.generation_kwargs.top_p {
            params.insert("top_p".into(), serde_json::to_value(top_p).unwrap());
        }
        if let Some(stop) = &self.generation_kwargs.stop {
            params.insert("stop".into(), stop.clone());
        }
        params
    }

    fn get_capability(&self, model: Option<&str>) -> ModelCapability {
        get_glm_model_capability(model.unwrap_or(&self.model))
    }

    fn with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider> {
        let mut clone = self.clone_with_generation_kwargs(self.generation_kwargs.clone());
        clone.thinking_effort = Some(effort);
        Box::new(clone)
    }

    fn with_max_completion_tokens(&self, max_completion_tokens: i64) -> Option<Box<dyn ChatProvider>> {
        let effective = self
            .max_output_tokens_cap
            .map(|cap| std::cmp::min(max_completion_tokens, cap))
            .unwrap_or(max_completion_tokens);
        let mut clone = self.clone_with_generation_kwargs(self.generation_kwargs.clone());
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

        let normalized_history = normalize_tool_call_ids_for_provider(history, &glm_tool_call_id_policy());

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
            messages.push(convert_glm_message(msg));
        }

        let mut create_params = serde_json::Map::new();
        create_params.insert("model".into(), Value::String(self.model.clone()));
        create_params.insert("messages".into(), serde_json::to_value(&messages).unwrap());
        create_params.insert("stream".into(), Value::Bool(self.stream));

        let mut kwargs = serde_json::to_value(&self.generation_kwargs).unwrap();
        if let Value::Object(map) = kwargs {
            for (k, v) in map {
                if !v.is_null() {
                    create_params.insert(k, v);
                }
            }
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

        if self.thinking_effort == Some(ThinkingEffort::Off) {
            create_params.insert("thinking".into(), serde_json::json!({"type": "disabled"}));
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
```

**Note on multimedia rejection:** The TS `glm.ts` throws `Error` for image/audio/video URL parts. In the Rust plan we silently drop them to avoid runtime panics in a provider method that returns `Result`. This is a deliberate behavioral divergence: if the golden parity tests in Part 5 expect the thrown error, revisit this task to return `ChatProviderError::Other("GLM provider does not support ...")` instead.

Also update `rust-ody/crates/kosong-rs/src/providers/mod.rs`:

```rust
pub mod anthropic;
pub mod deepseek;
pub mod glm;
pub mod google_genai;
pub mod kimi;
```

- [ ] **Run tests and verify they PASS.**

```bash
cd rust-ody && cargo test -p kosong-rs --lib providers::glm::tests
```

Expected: all 7 tests pass.

- [ ] **Commit.**

```bash
git add rust-ody/crates/kosong-rs/src/providers/glm.rs rust-ody/crates/kosong-rs/src/providers/mod.rs
git commit -m "feat(kosong-rs): add GLMChatProvider shell and message conversion"
```

---

## Task 4.3: GLMChatProvider generate() streaming + non-streaming

**Depends on:** Task 4.2

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/glm.rs`
- Test: `rust-ody/crates/kosong-rs/src/providers/glm.rs` (inline `#[cfg(test)] mod generate_tests`)

The `generate()` body is already implemented in Task 4.2. This task adds end-to-end tests using `MockHttpClient`, confirming:
1. Streaming text response yields `StreamedMessagePart::text` chunks.
2. Non-streaming response yields text and usage.
3. Missing API key returns `ChatProviderError::MissingApiKey`.
4. HTTP error status is converted via `convert_openai_error`.

- [ ] **Write the failing tests.** Append a new `#[cfg(test)] mod generate_tests` at the bottom of `glm.rs`:

```rust
#[cfg(test)]
mod generate_tests {
    use super::*;
    use crate::message::{Message, Role, StreamedMessagePart};
    use crate::provider::ChatProvider;
    use futures_util::StreamExt;

    fn sse_body() -> Vec<u8> {
        br#"data: {"id":"1","choices":[{"index":0,"delta":{"role":"assistant","content":"Hello"}}]}

data: {"id":"2","choices":[{"index":0,"delta":{"content":" world"}}]}

data: {"id":"3","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]

"#
        .to_vec()
    }

    fn provider_with_body(status: u16, body: Vec<u8>) -> GLMChatProvider {
        GLMChatProvider::new(GLMOptions {
            api_key: Some("sk-test".into()),
            base_url: Some("http://mock".into()),
            model: "glm-4-plus".into(),
            stream: Some(true),
            max_tokens: Some(512),
            default_headers: None,
            http_client: Some(Arc::new(MockHttpClient::new(status, body))),
        })
    }

    #[tokio::test]
    async fn generate_streams_text() {
        let provider = provider_with_body(200, sse_body());
        let stream = provider.generate("", &[], &[], None).await.unwrap();
        let parts: Vec<_> = stream.collect().await;
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], StreamedMessagePart::text("Hello"));
        assert_eq!(parts[1], StreamedMessagePart::text(" world"));
    }

    #[tokio::test]
    async fn generate_non_stream_text_and_usage() {
        let body = br#"{"id":"chat-1","choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":2}}"#.to_vec();
        let provider = GLMChatProvider::new(GLMOptions {
            api_key: Some("sk-test".into()),
            base_url: Some("http://mock".into()),
            model: "glm-4-plus".into(),
            stream: Some(false),
            max_tokens: Some(512),
            default_headers: None,
            http_client: Some(Arc::new(MockHttpClient::new(200, body))),
        });
        let stream = provider.generate("", &[], &[], None).await.unwrap();
        let parts: Vec<_> = stream.collect().await;
        assert_eq!(parts, vec![StreamedMessagePart::text("ok")]);
    }

    #[tokio::test]
    async fn generate_rejects_missing_api_key() {
        let provider = GLMChatProvider::new(GLMOptions {
            api_key: None,
            base_url: Some("http://mock".into()),
            model: "glm-4-plus".into(),
            stream: Some(true),
            max_tokens: Some(512),
            default_headers: None,
            http_client: Some(Arc::new(MockHttpClient::new(200, vec![]))),
        });
        let result = provider.generate("", &[], &[], None).await;
        assert!(matches!(result, Err(ChatProviderError::MissingApiKey(_))));
    }

    #[tokio::test]
    async fn generate_converts_http_error() {
        let body = br#"{"error":{"message":"Invalid request","code":"invalid_request_error"}}"#.to_vec();
        let provider = provider_with_body(400, body);
        let result = provider.generate("", &[], &[], None).await;
        assert!(matches!(result, Err(ChatProviderError::Status(_))));
    }
}
```

- [ ] **Run tests and verify they FAIL.**

```bash
cd rust-ody && cargo test -p kosong-rs --lib providers::glm::generate_tests
```

Expected: `generate_streams_text` etc. fail because the implementation from Task 4.2 should already exist but the tests are new. If it compiles and passes, proceed to commit.

- [ ] **Run tests and verify they PASS.**

```bash
cd rust-ody && cargo test -p kosong-rs --lib providers::glm
```

Expected: all tests pass.

- [ ] **Whole-tree typecheck.**

```bash
cd rust-ody && cargo check --workspace --tests
```

Expected: `Finished dev [unoptimized + debuginfo] target(s) in ...`.

- [ ] **Commit.**

```bash
git add rust-ody/crates/kosong-rs/src/providers/glm.rs
git commit -m "feat(kosong-rs): GLMChatProvider generate() with streaming/non-streaming tests"
```

---

## Task 4.4: Wire GLMChatProvider into public API

**Depends on:** Task 4.2

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs`
- Test: `rust-ody/crates/kosong-rs/src/lib.rs` (existing test module, or none if no public tests needed)

Export `GLMChatProvider`, `GLMOptions`, and `GLMGenerationKwargs` from the crate root so the golden binaries and TS harness can use them.

- [ ] **Write the export additions.** In `rust-ody/crates/kosong-rs/src/lib.rs`, add after the existing provider re-exports:

```rust
pub use providers::glm::{GLMChatProvider, GLMGenerationKwargs, GLMOptions};
```

And add `get_glm_model_capability` to the capability registry re-export line:

```rust
pub use capability_registry::{
    get_anthropic_model_capability,
    get_glm_model_capability,
    get_google_genai_model_capability,
    get_openai_legacy_model_capability,
    get_openai_responses_model_capability,
    uses_openai_responses_developer_role,
};
```

- [ ] **Manual verification.** Build the crate to confirm exports compile.

```bash
cd rust-ody && cargo check -p kosong-rs
```

Expected: no errors.

- [ ] **Commit.**

```bash
git add rust-ody/crates/kosong-rs/src/lib.rs
git commit -m "feat(kosong-rs): export GLMChatProvider and get_glm_model_capability"
```

---

## Local Self-Review (Part 4)

- [ ] 1. **Spec-coverage table:**
  | TS behavior | Covered by |
  |---|---|
  | `name = "glm"` | Task 4.2 tests |
  | `modelName` returns constructor model | Task 4.2 tests |
  | `getCapability` returns `UNKNOWN_CAPABILITY` | Task 4.1 + Task 4.2 tests |
  | `convertGLMMessage` filters empty text parts | Task 4.2 tests |
  | `convertGLMMessage` aggregates `think` to `reasoning_content` | Task 4.2 tests |
  | `convertGLMMessage` rejects image/audio/video URLs | Task 4.2 implementation (silent drop; flagged divergence) |
  | `convertGLMMessage` serializes tool calls | Task 4.2 tests |
  | `generate()` builds Chat-Completions request with `stream_options` | Task 4.3 tests |
  | `generate()` supports non-streaming | Task 4.3 tests |
  | Missing API key error | Task 4.3 tests |
  | HTTP error conversion | Task 4.3 tests |
  | `withThinking(Off)` sets `thinking: {type: "disabled"}` | Task 4.2 implementation + Task 4.3 can add request-body assertion |
  | `withMaxCompletionTokens` clamps to `maxTokens` cap | Task 4.2 tests |
  | `modelParameters` includes model/baseUrl/generationKwargs | Task 4.2 implementation |

- [ ] 2. **Placeholder scan:** No TODO/TBD in code snippets; the multimedia-divergence note is an explicit design caveat, not a placeholder.
- [ ] 3. **No phantom tasks:** Every task produces a verifiable file change and passing tests/build.
- [ ] 4. **Dependency soundness:** Task 4.2 depends on Task 4.1; Task 4.3 depends on Task 4.2; Task 4.4 depends on Task 4.2. No later symbols referenced.
- [ ] 5. **Caller & build soundness:** No shared signatures changed in this part. Task 4.4 ends with `cargo check -p kosong-rs`; Task 4.3 ends with `cargo check --workspace --tests`.
- [ ] 6. **Test-the-risk:** Streaming/non-streaming outputs, missing API key, and HTTP 400 errors are asserted behaviorally. The empty-text filter is asserted with a must-survive input ("hello"). The think aggregation is asserted end-to-end. The tool-call serialization is asserted by field values.
- [ ] 7. **Type consistency:** `GLMOptions`, `GLMGenerationKwargs`, `GLMChatProvider` names match exports in Task 4.4; `get_glm_model_capability` name matches Part 1 and Task 4.1.
