# core.md — Rust `kosong-rs` OpenAI Chat Completions 共享层

## Task 1: HttpClient trait + reqwest/mock 实现

**Depends on:** none（4.2.0/4.2.1 已落地）  
**Files:**
- Create: `rust-ody/crates/kosong-rs/src/http_client.rs`
- Modify: `rust-ody/crates/kosong-rs/Cargo.toml:8-16`
- Modify: `rust-ody/crates/kosong-rs/src/errors.rs:44-60`
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs:1-33`
- Test: `rust-ody/crates/kosong-rs/src/http_client.rs`（模块内 `#[cfg(test)]`）

### 步骤

- [ ] 在 `Cargo.toml` `[dependencies]` 增加 `reqwest = { workspace = true }`。
- [ ] 给 `ChatProviderError` 增加 `Other(String)` 变体，用于承载无法归类的 `Error: ...` 消息；更新 `is_retryable_generate_error` 返回 `false`。
- [ ] 编写 `http_client.rs`。
- [ ] 写失败单测：构造 `MockHttpClient` 返回 401 与错误 body，断言 provider 后续能拿到 status 与 body。
- [ ] 运行 `cargo test -p kosong-rs http_client::` 确认失败（模块尚不存在）。
- [ ] 补全实现后运行 `cargo test -p kosong-rs http_client::` 通过；再运行 `cargo check -p kosong-rs --all-targets`。
- [ ] Commit: `feat(kosong-rs): add HttpClient abstraction for testable providers`

### 实现代码

`Cargo.toml` 变更（在 `[dependencies]` 内追加一行）：

```toml
reqwest = { workspace = true }
```

`src/errors.rs` 变更：

```rust
#[derive(Debug, Error)]
pub enum ChatProviderError {
    // ... existing variants ...
    #[error("{0}")]
    Other(String),
}
```

并在 `is_retryable_generate_error` 中追加：

```rust
ChatProviderError::Other(_) => false,
```

`src/http_client.rs`：

```rust
use std::collections::HashMap;
use std::pin::Pin;

use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use serde_json::Value;

use crate::errors::{APIConnectionError, APITimeoutError, ChatProviderError};

pub type ByteStream = Pin<Box<dyn Stream<Item = Result<reqwest::Bytes, ChatProviderError>> + Send>>;

pub struct HttpResponse {
    status: u16,
    body: ByteStream,
}

impl HttpResponse {
    pub fn new(status: u16, body: ByteStream) -> Self {
        Self { status, body }
    }

    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn bytes_stream(self) -> ByteStream {
        self.body
    }
}

#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn post_json(
        &self,
        url: &str,
        headers: HashMap<String, String>,
        body: Value,
    ) -> Result<HttpResponse, ChatProviderError>;
}

pub struct ReqwestClient {
    client: reqwest::Client,
}

impl ReqwestClient {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl HttpClient for ReqwestClient {
    async fn post_json(
        &self,
        url: &str,
        headers: HashMap<String, String>,
        body: Value,
    ) -> Result<HttpResponse, ChatProviderError> {
        let mut req = self.client.post(url).json(&body);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        let resp = req.send().await.map_err(classify_reqwest_error)?;
        let status = resp.status().as_u16();
        let body = resp
            .bytes_stream()
            .map(|r| r.map_err(classify_reqwest_error))
            .boxed();
        Ok(HttpResponse::new(status, body))
    }
}

pub struct MockHttpClient {
    status: u16,
    chunks: Vec<Vec<u8>>,
}

impl MockHttpClient {
    pub fn new(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            chunks: vec![body.into()],
        }
    }

    pub fn with_chunks(status: u16, chunks: Vec<Vec<u8>>) -> Self {
        Self { status, chunks }
    }
}

#[async_trait]
impl HttpClient for MockHttpClient {
    async fn post_json(
        &self,
        _url: &str,
        _headers: HashMap<String, String>,
        _body: Value,
    ) -> Result<HttpResponse, ChatProviderError> {
        let chunks = self.chunks.clone();
        let stream = futures_util::stream::iter(
            chunks.into_iter().map(|c| Ok(reqwest::Bytes::from(c))),
        )
        .boxed();
        Ok(HttpResponse::new(self.status, stream))
    }
}

fn classify_reqwest_error(err: reqwest::Error) -> ChatProviderError {
    if err.is_timeout() {
        return ChatProviderError::Timeout(APITimeoutError);
    }
    let msg = err.to_string();
    let re_network = regex::Regex::new(r"(?i)network|connection|connect|disconnect|terminated").unwrap();
    let re_timeout = regex::Regex::new(r"(?i)timed?\s*out|timeout|deadline").unwrap();
    if re_timeout.is_match(&msg) {
        ChatProviderError::Timeout(APITimeoutError)
    } else if re_network.is_match(&msg) || err.is_connect() {
        ChatProviderError::Connection(APIConnectionError)
    } else if let Some(status) = err.status() {
        crate::errors::normalize_api_status_error(status.as_u16(), msg, None)
    } else {
        ChatProviderError::Other(format!("Error: {msg}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;

    #[tokio::test]
    async fn mock_returns_configured_status_and_body() {
        let client = MockHttpClient::new(200, b"hello");
        let resp = client
            .post_json("http://x", HashMap::new(), Value::Null)
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let bytes = resp.bytes_stream().next().await.unwrap().unwrap();
        assert_eq!(bytes.as_ref(), b"hello");
    }

    #[tokio::test]
    async fn chunked_mock_streams_all_chunks() {
        let client = MockHttpClient::with_chunks(200, vec![b"a".to_vec(), b"b".to_vec()]);
        let resp = client
            .post_json("http://x", HashMap::new(), Value::Null)
            .await
            .unwrap();
        let chunks: Vec<_> = resp.bytes_stream().collect().await;
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].as_ref().unwrap().as_ref(), b"a");
        assert_eq!(chunks[1].as_ref().unwrap().as_ref(), b"b");
    }
}
```

### 验证命令

```bash
cd rust-ody
cargo test -p kosong-rs http_client::
# expected: 2 passed
cargo check -p kosong-rs --all-targets
# expected: clean
```

---

## Task 2: openai_common 通用转换/错误/usage/finish_reason

**Depends on:** Task 1（使用 `ChatProviderError::Other`）  
**Files:**
- Create: `rust-ody/crates/kosong-rs/src/openai_common.rs`
- Modify: `rust-ody/crates/kosong-rs/src/message.rs`（新增 `extract_text`）
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs`
- Test: `rust-ody/crates/kosong-rs/src/openai_common.rs`（模块内 `#[cfg(test)]`）

### 步骤

- [ ] 在 `message.rs` 末尾追加 `extract_text(message: &Message)` 纯函数。
- [ ] 创建 `openai_common.rs`，实现所有转换函数。
- [ ] 写失败单测：断言 `extract_usage` 对 `cached_tokens` 与 `prompt_tokens_details.cached_tokens` 都返回正确 `inputCacheRead`。
- [ ] 运行 `cargo test -p kosong-rs openai_common::` 确认失败；补全实现后再次运行通过。
- [ ] Commit: `feat(kosong-rs): openai-common conversion helpers`

### 实现代码

`src/message.rs` 追加：

```rust
pub fn extract_text(message: &Message, sep: &str) -> String {
    message
        .content
        .iter()
        .filter_map(|p| match p {
            ContentPart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(sep)
}
```

`src/openai_common.rs`：

```rust
use std::collections::HashMap;

use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use crate::errors::{
    APIConnectionError, APIContextOverflowError, APIStatusError, APITimeoutError,
    ChatProviderError, is_context_overflow_error_code, normalize_api_status_error,
};
use crate::message::{ContentPart, Message, StreamedMessagePart};
use crate::provider::{FinishReason, ThinkingEffort, Tool, TokenUsage};

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
    let prompt_tokens = usage.get("prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
    let completion_tokens = usage.get("completion_tokens").and_then(|v| v.as_i64()).unwrap_or(0);

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
            (Some(FinishReason::ToolCalls), Some(raw.into()))
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
        || non_think.iter().any(|p| !matches!(p, ContentPart::Text { .. }))
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
        let usage = serde_json::json!({
            "prompt_tokens": 100,
            "completion_tokens": 20,
            "cached_tokens": 30,
        });
        let u = extract_usage(&usage).unwrap();
        assert_eq!(u.input_other, 70);
        assert_eq!(u.output, 20);
        assert_eq!(u.input_cache_read, 30);
    }

    #[test]
    fn extract_usage_falls_back_to_prompt_tokens_details() {
        let usage = serde_json::json!({
            "prompt_tokens": 100,
            "completion_tokens": 20,
            "prompt_tokens_details": { "cached_tokens": 25 },
        });
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
        assert_eq!(
            normalize_openai_finish_reason(Some("unknown_reason")),
            (Some(FinishReason::Other), Some("unknown_reason".into()))
        );
        assert_eq!(normalize_openai_finish_reason(None), (None, None));
    }

    #[test]
    fn reasoning_effort_round_trip() {
        assert_eq!(
            reasoning_effort_to_thinking_effort(Some("xhigh")),
            Some(ThinkingEffort::Xhigh)
        );
        assert_eq!(thinking_effort_to_reasoning_effort(ThinkingEffort::Max), Some("xhigh".into()));
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
```

### 验证命令

```bash
cargo test -p kosong-rs openai_common::
# expected: 8 passed
cargo check -p kosong-rs --all-targets
# expected: clean
```

---

## Task 3: chat_completions_stream SSE 解析与 tool-call 路由

**Depends on:** Task 2（`convert_content_part`、`extract_usage`、`normalize_openai_finish_reason`、`convert_openai_error`）  
**Files:**
- Create: `rust-ody/crates/kosong-rs/src/chat_completions_stream.rs`
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs`
- Test: `rust-ody/crates/kosong-rs/src/chat_completions_stream.rs`（模块内 `#[cfg(test)]`）

### 步骤

- [ ] 创建 `chat_completions_stream.rs`，实现 SSE 解析与 `convert_chat_completion_stream_tool_call`。
- [ ] 写失败单测：并行 tool-call delta 交错输入，断言最终两个 tool-call 的 `arguments` 分别正确，无交叉污染。
- [ ] 运行 `cargo test -p kosong-rs chat_completions_stream::` 确认失败；补全实现后通过。
- [ ] Commit: `feat(kosong-rs): chat-completions SSE stream parser with tool-call buffering`

### 实现代码

`src/chat_completions_stream.rs`：

```rust
use std::collections::HashMap;

use serde::Deserialize;
use serde_json::Value;

use crate::errors::ChatProviderError;
use crate::message::{StreamedMessagePart, ToolCall};
use crate::openai_common::{convert_content_part, extract_usage, normalize_openai_finish_reason};
use crate::provider::FinishReason;
use crate::usage::TokenUsage;

#[derive(Debug, Clone, Default)]
pub struct BufferedChatCompletionToolCall {
    pub id: Option<String>,
    pub arguments: String,
    pub emitted: bool,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(default)]
    choices: Vec<ChatCompletionChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoice {
    #[serde(default)]
    index: i64,
    delta: ChatCompletionDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ChatCompletionDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ChatCompletionToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
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

pub fn convert_chat_completion_stream_tool_call(
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
            return vec![StreamedMessagePart::ToolCallPart(crate::message::ToolCallPart {
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
        buffered.arguments.clear();
        buffered_by_index.insert(idx.clone(), buffered.clone());

        let tool_call_id = buffered
            .id
            .clone()
            .or_else(|| tool_call.id.clone())
            .unwrap_or_else(|| "tc".into());
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
        return vec![StreamedMessagePart::ToolCallPart(crate::message::ToolCallPart {
            part_type: "tool_call_part".into(),
            arguments_part: Some(args.into()),
            index: tool_call.index.clone(),
        })];
    }

    vec![]
}

pub async fn parse_stream_response(
    body: Vec<u8>,
    reasoning_key: Option<&str>,
) -> Result<(Vec<StreamedMessagePart>, Option<String>, Option<TokenUsage>, Option<FinishReason>, Option<String>), ChatProviderError> {
    let text = String::from_utf8_lossy(&body);
    let mut parts = Vec::new();
    let mut id: Option<String> = None;
    let mut usage: Option<TokenUsage> = None;
    let mut finish_reason: Option<FinishReason> = None;
    let mut raw_finish_reason: Option<String> = None;
    let mut buffered_tool_calls: HashMap<String, BufferedChatCompletionToolCall> = HashMap::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !line.starts_with("data: ") {
            continue;
        }
        let data = &line[6..];
        if data == "[DONE]" {
            break;
        }

        let chunk: ChatCompletionChunk = match serde_json::from_str(data) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("SSE parse warning: {e} for line: {data}");
                continue;
            }
        };

        if let Some(chunk_id) = chunk.id {
            id = Some(chunk_id);
        }
        if let Some(chunk_usage) = chunk.usage {
            usage = extract_usage(&chunk_usage);
        }

        for choice in chunk.choices {
            if let Some(raw) = &choice.finish_reason {
                let (fr, rfr) = normalize_openai_finish_reason(Some(raw.as_str()));
                finish_reason = fr.or(finish_reason);
                raw_finish_reason = rfr.or(raw_finish_reason);
            }

            if let Some(reasoning) = extract_reasoning_content(
                &serde_json::to_value(&choice.delta).unwrap(),
                reasoning_key,
            ) {
                parts.push(StreamedMessagePart::Think {
                    think: reasoning,
                    encrypted: None,
                });
            }

            if let Some(content) = choice.delta.content {
                parts.push(StreamedMessagePart::Text(content));
            }

            if let Some(tool_calls) = choice.delta.tool_calls {
                for tc in tool_calls {
                    parts.extend(convert_chat_completion_stream_tool_call(&tc, &mut buffered_tool_calls));
                }
            }
        }
    }

    Ok((parts, id, usage, finish_reason, raw_finish_reason))
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

#[derive(Debug, Deserialize)]
struct ChatCompletionMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
    #[serde(default, rename = "tool_calls")]
    tool_calls: Vec<ChatCompletionNonStreamToolCall>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionNonStreamToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: ChatCompletionNonStreamFunction,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionNonStreamFunction {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<String>,
}

pub fn parse_non_stream_response(
    body: &[u8],
    reasoning_key: Option<&str>,
) -> Result<(Vec<StreamedMessagePart>, Option<String>, Option<TokenUsage>, Option<FinishReason>, Option<String>), ChatProviderError> {
    let completion: ChatCompletion = serde_json::from_slice(body)?;
    let mut parts = Vec::new();

    let choice = completion.choices.into_iter().next();
    let (finish_reason, raw_finish_reason) = choice
        .as_ref()
        .and_then(|c| c.finish_reason.as_deref())
        .map(|raw| normalize_openai_finish_reason(Some(raw)))
        .unwrap_or((None, None));

    if let Some(msg) = choice.map(|c| c.message) {
        let msg_value = serde_json::to_value(&msg).unwrap();
        if let Some(reasoning) = extract_reasoning_content(&msg_value, reasoning_key) {
            parts.push(StreamedMessagePart::Think {
                think: reasoning,
                encrypted: None,
            });
        }
        if let Some(content) = msg.content {
            if !content.is_empty() {
                parts.push(StreamedMessagePart::Text(content));
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
    Ok((parts, Some(completion.id), usage, finish_reason, raw_finish_reason))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parses_text_stream() {
        let sse = "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hello\"}}]}\n\n\
                       data: {\"id\":\"2\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"}}]}\n\n\
                       data: {\"id\":\"3\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
                       data: [DONE]\n\n";
        let (parts, id, usage, finish, raw) = parse_stream_response(sse.into(), None).await.unwrap();
        assert_eq!(id, Some("3".into()));
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], StreamedMessagePart::Text("Hello".into()));
        assert_eq!(parts[1], StreamedMessagePart::Text(" world".into()));
        assert_eq!(finish, Some(FinishReason::Completed));
        assert_eq!(raw, Some("stop".into()));
        assert!(usage.is_none());
    }

    #[tokio::test]
    async fn routes_parallel_tool_call_deltas() {
        let sse = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"name\":\"read\"}}]}}]}\n\n\
                       data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":1,\"function\":{\"name\":\"write\"}}]}}]}\n\n\
                       data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"a\\\":1\"}}]}}]}\n\n\
                       data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":1,\"function\":{\"arguments\":\"{\\\"b\\\":2\"}}]}}]}\n\n\
                       data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"}\"}}]}}]}\n\n\
                       data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":1,\"function\":{\"arguments\":\"}\"}}]}}]}\n\n";
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
        assert_eq!(calls[0].arguments.as_deref(), Some("{\"a\":1}"));
        assert_eq!(calls[1].name, "write");
        assert_eq!(calls[1].arguments.as_deref(), Some("{\"b\":2}"));
    }

    #[test]
    fn parses_non_stream_with_tool_call() {
        let body = br#"{"id":"chat-1","choices":[{"message":{"content":"ok","tool_calls":[{"id":"tc1","type":"function","function":{"name":"read","arguments":"{}"}}]},"finish_reason":"tool_calls"}]}"#;
        let (parts, id, _, finish, raw) = parse_non_stream_response(body, None).unwrap();
        assert_eq!(id, Some("chat-1".into()));
        assert_eq!(finish, Some(FinishReason::ToolCalls));
        assert_eq!(raw, Some("tool_calls".into()));
        assert!(parts.iter().any(|p| matches!(p, StreamedMessagePart::Text(_))));
        assert!(parts.iter().any(|p| matches!(p, StreamedMessagePart::ToolCall(_))));
    }
}
```

### 验证命令

```bash
cargo test -p kosong-rs chat_completions_stream::
# expected: 3 passed
cargo check -p kosong-rs --all-targets
# expected: clean
```

---

## Task 4: OpenAILegacyChatProvider

**Depends on:** Task 1–Task 3  
**Files:**
- Create: `rust-ody/crates/kosong-rs/src/openai_legacy.rs`
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs`
- Test: `rust-ody/crates/kosong-rs/src/openai_legacy.rs`（模块内 `#[cfg(test)]`）

### 步骤

- [ ] 创建 `openai_legacy.rs`，实现 `OpenAILegacyChatProvider`。
- [ ] 写失败单测：构造 `MockHttpClient` 返回 SSE 纯文本流，断言 `generate()` 产出的 `StreamedMessagePart` 序列与 id/usage/finish_reason 正确。
- [ ] 运行 `cargo test -p kosong-rs openai_legacy::` 确认失败；补全实现后通过。
- [ ] 运行 `cargo test -p kosong-rs` 与 `cargo check -p kosong-rs --all-targets` 全量通过。
- [ ] Commit: `feat(kosong-rs): OpenAILegacyChatProvider with stream+non-stream parity`

### 实现代码

`src/openai_legacy.rs`：

```rust
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::TryStreamExt;
use serde::Serialize;
use serde_json::Value;

use crate::chat_completions_stream::{parse_non_stream_response, parse_stream_response};
use crate::errors::{APIMissingApiKeyError, ChatProviderError};
use crate::http_client::{HttpClient, HttpResponse, MockHttpClient, ReqwestClient};
use crate::message::{ContentPart, Message, StreamedMessagePart, ToolCall};
use crate::openai_common::{
    convert_content_part, convert_openai_error, convert_tool_message_content, extract_usage,
    normalize_openai_finish_reason, reasoning_effort_to_thinking_effort,
    thinking_effort_to_reasoning_effort, tool_to_openai, ToolMessageConversion,
};
use crate::provider::{
    AbortSignal, ChatProvider, FinishReason, GenerateOptions, ModelCapability, ProviderRequestAuth,
    ThinkingEffort, Tool,
};
use crate::request_auth::{merge_request_headers, require_provider_api_key};
use crate::tool_call_id::{normalize_tool_call_ids_for_provider, sanitize_tool_call_id, ToolCallIdPolicy};
use crate::usage::TokenUsage;
use crate::{capability_registry, generate::StreamedMessage};

const OPENAI_CHAT_TOOL_CALL_ID_POLICY: ToolCallIdPolicy = ToolCallIdPolicy::new(
    |id| sanitize_tool_call_id(id, Some(64)),
    Some(64),
);

#[derive(Debug, Clone)]
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

    let content = if message.role == crate::message::Role::Tool {
        let has_non_text = non_think.iter().any(|p| !matches!(p, ContentPart::Text { .. }));
        let effective = if has_non_text {
            Some(ToolMessageConversion::ExtractText)
        } else {
            tool_message_conversion
        };
        convert_tool_message_content(&Message {
            role: message.role,
            name: message.name.clone(),
            content: non_think.into_iter().cloned().collect(),
            tool_calls: message.tool_calls.clone(),
            tool_call_id: message.tool_call_id.clone(),
            partial: message.partial,
        }, effective.unwrap_or(ToolMessageConversion::Standard))
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
            if parts.is_empty() { None } else { Some(Value::Array(parts)) }
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

        let normalized_history = normalize_tool_call_ids_for_provider(history, &OPENAI_CHAT_TOOL_CALL_ID_POLICY);
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
            messages.push(convert_message(msg, self.reasoning_key.as_deref(), self.tool_message_conversion));
        }

        let mut create_params = serde_json::Map::new();
        create_params.insert("model".into(), Value::String(self.model.clone()));
        create_params.insert("messages".into(), serde_json::to_value(&messages).unwrap());
        create_params.insert("stream".into(), Value::Bool(self.stream));

        let mut kwargs = self.generation_kwargs.clone();
        let mut reasoning_effort = self.reasoning_effort.clone();
        if reasoning_effort.is_none() && !kwargs.contains_key("reasoning_effort") {
            let has_think = history.iter().any(|m| {
                m.content.iter().any(|p| matches!(p, ContentPart::Think { .. }))
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
            let tools_json: Vec<Value> = tools.iter().map(|t| serde_json::to_value(tool_to_openai(t)).unwrap()).collect();
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

        if response.status() < 200 || response.status() >= 300 {
            let body_bytes: Vec<u8> = response.bytes_stream().try_concat().await.unwrap_or_default();
            let body = String::from_utf8_lossy(&body_bytes);
            let (msg, code) = parse_error_body(&body);
            return Err(convert_openai_error(
                &msg,
                Some(response.status()),
                code.as_deref(),
            ));
        }

        let body_bytes: Vec<u8> = response.bytes_stream().try_concat().await?;
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

    fn with_max_completion_tokens(&self, max_completion_tokens: i64) -> Option<Box<dyn ChatProvider>> {
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
    use crate::message::{ContentPart, Role, ToolCall};

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
        assert_eq!(parts[0], StreamedMessagePart::Text("Hello".into()));
        assert_eq!(parts[1], StreamedMessagePart::Text(" world".into()));
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
        let err = provider.generate("", &[], &[], None).await.unwrap_err();
        assert!(matches!(err, ChatProviderError::MissingApiKey(_)));
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
```

### 验证命令

```bash
cargo test -p kosong-rs openai_legacy::
# expected: 4 passed
cargo test -p kosong-rs
# expected: all passed
cargo check -p kosong-rs --all-targets
# expected: clean
```

---

## Local Self-Review

- [ ] 1. Spec-coverage table（对应 4.2.2）：
  | 4.2.2 条目 | 覆盖任务 |
  |---|---|
  | 4.2.2.1 `openai-common` | Task 2 |
  | 4.2.2.2 `chat-completions-stream` | Task 3 |
  | 4.2.2.3 `OpenAILegacyChatProvider` | Task 4 |
- [ ] 2. Placeholder scan：`core.md` 无 `TODO`/`TBD`，每 Task 给出完整文件内容。
- [ ] 3. No phantom tasks：Task 1–4 每个都产生可编译、可测试的文件改动。
- [ ] 4. Dependency soundness：Task 2 依赖 Task 1 的 `Other` 错误变体；Task 3 依赖 Task 2 的转换函数；Task 4 依赖 Task 1/2/3。
- [ ] 5. Caller & build soundness：Task 1 修改 `ChatProviderError` 后，本 crate 内 `is_retryable_generate_error` 已同步更新；Task 4 未改变 `ChatProvider` trait 签名；每个 Task 都以 `cargo check -p kosong-rs --all-targets` 收尾。
- [ ] 6. Test-the-risk：
  - `extract_usage` 覆盖两种 cached_tokens 来源；
  - `normalize_openai_finish_reason` 覆盖全部映射；
  - 并行 tool-call 路由用交错 delta 断言无交叉污染；
  - `OpenAILegacyChatProvider` 用 mock HTTP 端到端验证 generate 行为；
  - API key 缺失路径显式断言。
- [ ] 7. Type consistency：所有类型（`FinishReason`、`ThinkingEffort`、`TokenUsage`、`ToolCall`、`Message`、`ContentPart`）复用 4.2.0 已定义字段名与 serde 设置；新增 `OpenAIMessage` / `OpenAIToolCallOut` 仅用于内部序列化。
