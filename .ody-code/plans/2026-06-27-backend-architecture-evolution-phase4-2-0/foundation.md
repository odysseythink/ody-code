# Part 1: Foundation — crate 骨架 + 数据模型 + trait + 错误

本部分把 `kosong-rs` 的静态契约钉死：crate 边界、消息类型、provider trait、生成选项与错误类型。所有测试先用最简 fixture 或序列化断言钉住形状，不依赖 HTTP/SSE。

---

### Task 1: 新建 `kosong-rs` crate 并加入 workspace

**Depends on:** none

**Files:**
- Create: `rust-ody/crates/kosong-rs/Cargo.toml`
- Create: `rust-ody/crates/kosong-rs/src/lib.rs`
- Modify: `rust-ody/Cargo.toml:2`（workspace members）

**步骤：**

- [ ] 创建目录与初始 crate 文件。

`rust-ody/crates/kosong-rs/Cargo.toml`:
```toml
[package]
name = "kosong-rs"
version = "0.1.0"
edition = "2021"
description = "Rust implementation of the Kosong LLM abstraction layer"
license = "MIT"

[dependencies]
async-trait = "0.1"
futures-util = { version = "0.3", default-features = false, features = ["std"] }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = "1"
tokio = { workspace = true }

[dev-dependencies]
tokio-test = "0.4"
```

`rust-ody/crates/kosong-rs/src/lib.rs`:
```rust
pub mod errors;
pub mod generate;
pub mod message;
pub mod mock;
pub mod provider;
```

- [ ] 修改 `rust-ody/Cargo.toml` 的 workspace members：

```toml
members = ["crates/ody-rust", "crates/ody-crypto", "crates/ody-host", "crates/kaos-rs", "crates/kosong-rs"]
```

- [ ] 编译检查：

```bash
cd rust-ody && cargo check -p kosong-rs
```

预期输出：末尾显示 `Finished dev [unoptimized + debuginfo] target(s)`，无错误。

- [ ] Commit：`git add rust-ody/Cargo.toml rust-ody/crates/kosong-rs && git commit -m "feat(kosong-rs): scaffold crate"`

---

### Task 2: 实现 message 类型与 `merge_in_place`

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/message.rs`
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs`（已存在，无需改）

**目标：** 完全对齐 `packages/kosong/src/message.ts` 的 JSON 形状与合并语义。

**步骤：**

- [ ] 写失败测试。在 `rust-ody/crates/kosong-rs/src/message.rs` 末尾加入：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_consecutive_text_parts() {
        let mut a = StreamedMessagePart::text("hello");
        let b = StreamedMessagePart::text(" world");
        assert!(merge_in_place(&mut a, &b));
        assert_eq!(a, StreamedMessagePart::text("hello world"));
    }

    #[test]
    fn merges_consecutive_think_parts() {
        let mut a = StreamedMessagePart::think("step1");
        let b = StreamedMessagePart::think(" step2");
        assert!(merge_in_place(&mut a, &b));
        assert_eq!(a, StreamedMessagePart::think("step1 step2"));
    }

    #[test]
    fn appends_tool_call_part_to_tool_call() {
        let mut a = StreamedMessagePart::tool_call("tc_1", "read", None);
        let b = StreamedMessagePart::tool_call_part(Some("{\"a\":1}"));
        assert!(merge_in_place(&mut a, &b));
        match a {
            StreamedMessagePart::ToolCall(tc) => {
                assert_eq!(tc.arguments.as_deref(), Some("{\"a\":1}"));
            }
            _ => panic!("expected ToolCall"),
        }
    }

    #[test]
    fn refuses_incompatible_merge() {
        let mut a = StreamedMessagePart::text("hello");
        let b = StreamedMessagePart::think("reason");
        assert!(!merge_in_place(&mut a, &b));
        assert_eq!(a, StreamedMessagePart::text("hello"));
    }
}
```

- [ ] 运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs --lib message::tests
```

预期失败：找不到 `StreamedMessagePart`、`merge_in_place` 等符号。

- [ ] 实现 `rust-ody/crates/kosong-rs/src/message.rs`：

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UrlPayload {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type")]
pub enum ContentPart {
    Text { text: String },
    Think {
        think: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        encrypted: Option<String>,
    },
    #[serde(rename = "image_url")]
    ImageUrl { #[serde(rename = "imageUrl")] image_url: UrlPayload },
    #[serde(rename = "audio_url")]
    AudioUrl { #[serde(rename = "audioUrl")] audio_url: UrlPayload },
    #[serde(rename = "video_url")]
    VideoUrl { #[serde(rename = "videoUrl")] video_url: UrlPayload },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    #[serde(rename = "type")]
    pub call_type: String,
    pub id: String,
    pub name: String,
    pub arguments: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extras: Option<serde_json::Value>,
    #[serde(rename = "_streamIndex", skip_serializing_if = "Option::is_none")]
    pub stream_index: Option<StreamIndex>,
}

pub type StreamIndex = serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallPart {
    #[serde(rename = "type")]
    pub part_type: String,
    #[serde(rename = "argumentsPart")]
    pub arguments_part: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<StreamIndex>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StreamedMessagePart {
    Content(ContentPart),
    ToolCall(ToolCall),
    ToolCallPart(ToolCallPart),
}

impl StreamedMessagePart {
    pub fn text(s: impl Into<String>) -> Self {
        Self::Content(ContentPart::Text { text: s.into() })
    }

    pub fn think(s: impl Into<String>) -> Self {
        Self::Content(ContentPart::Think { think: s.into(), encrypted: None })
    }

    pub fn tool_call(id: impl Into<String>, name: impl Into<String>, arguments: Option<&str>) -> Self {
        Self::ToolCall(ToolCall {
            call_type: "function".to_string(),
            id: id.into(),
            name: name.into(),
            arguments: arguments.map(Into::into),
            extras: None,
            stream_index: None,
        })
    }

    pub fn tool_call_part(arguments_part: Option<&str>) -> Self {
        Self::ToolCallPart(ToolCallPart {
            part_type: "tool_call_part".to_string(),
            arguments_part: arguments_part.map(Into::into),
            index: None,
        })
    }
}

pub fn is_content_part(part: &StreamedMessagePart) -> bool {
    matches!(part, StreamedMessagePart::Content(_))
}

pub fn is_tool_call(part: &StreamedMessagePart) -> bool {
    matches!(part, StreamedMessagePart::ToolCall(_))
}

pub fn is_tool_call_part(part: &StreamedMessagePart) -> bool {
    matches!(part, StreamedMessagePart::ToolCallPart(_))
}

pub fn merge_in_place(target: &mut StreamedMessagePart, source: &StreamedMessagePart) -> bool {
    use StreamedMessagePart::*;
    match (target, source) {
        (Content(ContentPart::Text { text: a }), Content(ContentPart::Text { text: b })) => {
            a.push_str(b);
            true
        }
        (Content(ContentPart::Think { think: a, encrypted: ea }), Content(ContentPart::Think { think: b, encrypted: eb })) => {
            if ea.is_some() {
                return false;
            }
            a.push_str(b);
            if let Some(sig) = eb {
                *ea = Some(sig.clone());
            }
            true
        }
        (ToolCall(tc), ToolCallPart(delta)) => {
            if let Some(delta_args) = &delta.arguments_part {
                tc.arguments = Some(match &tc.arguments {
                    Some(existing) => format!("{}{}", existing, delta_args),
                    None => delta_args.clone(),
                });
            }
            true
        }
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub role: Role,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub content: Vec<ContentPart>,
    pub tool_calls: Vec<ToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial: Option<bool>,
}

impl Message {
    pub fn assistant(content: Vec<ContentPart>, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            name: None,
            content,
            tool_calls,
            tool_call_id: None,
            partial: None,
        }
    }

    pub fn user_text(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            name: None,
            content: vec![ContentPart::Text { text: text.into() }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        }
    }
}
```

- [ ] 运行测试并确认通过：

```bash
cd rust-ody && cargo test -p kosong-rs --lib message::tests
```

预期输出：`test result: ok. 4 passed; 0 failed`

- [ ] 额外序列化断言测试（防止字段名漂移）：

```rust
#[test]
fn message_serializes_to_ts_shape() {
    let msg = Message::user_text("hi");
    let v = serde_json::to_value(&msg).unwrap();
    assert_eq!(v["role"], "user");
    assert_eq!(v["content"][0]["type"], "text");
    assert_eq!(v["content"][0]["text"], "hi");
    assert!(v["toolCalls"].is_array());
}
```

- [ ] Commit：`git add rust-ody/crates/kosong-rs/src/message.rs && git commit -m "feat(kosong-rs): message types and merge_in_place"`

---

### Task 3: 实现 provider trait、GenerateOptions、GenerateResult

**Depends on:** Task 2

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/provider.rs`
- Create: `rust-ody/crates/kosong-rs/src/usage.rs`（可选，也可内联在 provider.rs；推荐独立模块）

**目标：** 对齐 `packages/kosong/src/provider.ts` 的公共面与 `packages/kosong/src/usage.ts`。

**步骤：**

- [ ] 写失败测试。在 `rust-ody/crates/kosong-rs/src/provider.rs` 末尾加入：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_reason_serializes_to_snake_case() {
        assert_eq!(
            serde_json::to_string(&FinishReason::ToolCalls).unwrap(),
            "\"tool_calls\""
        );
    }

    #[test]
    fn token_usage_serializes_to_camel_case() {
        let u = TokenUsage {
            input_other: 10,
            output: 5,
            input_cache_read: 2,
            input_cache_creation: 1,
        };
        let v = serde_json::to_value(&u).unwrap();
        assert_eq!(v["inputOther"], 10);
        assert_eq!(v["inputCacheRead"], 2);
        assert_eq!(v["inputCacheCreation"], 1);
    }

    #[test]
    fn unknown_capability_is_all_false() {
        let cap = ModelCapability::unknown();
        assert!(!cap.image_in);
        assert!(!cap.thinking);
        assert_eq!(cap.max_context_tokens, 0);
    }
}
```

- [ ] 运行测试并确认失败：类型未定义。

- [ ] 实现 `rust-ody/crates/kosong-rs/src/usage.rs`：

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input_other: i64,
    pub output: i64,
    pub input_cache_read: i64,
    pub input_cache_creation: i64,
}

impl TokenUsage {
    pub fn input_total(&self) -> i64 {
        self.input_other + self.input_cache_read + self.input_cache_creation
    }

    pub fn grand_total(&self) -> i64 {
        self.input_total() + self.output
    }
}
```

- [ ] 实现 `rust-ody/crates/kosong-rs/src/provider.rs`：

```rust
use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::errors::ChatProviderError;
use crate::generate::StreamedMessage;
use crate::message::{Message, StreamedMessagePart, ToolCall};
use crate::usage::TokenUsage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingEffort {
    Off,
    Low,
    Medium,
    High,
    #[serde(rename = "xhigh")]
    Xhigh,
    Max,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Completed,
    ToolCalls,
    Truncated,
    Filtered,
    Paused,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapability {
    pub image_in: bool,
    pub video_in: bool,
    pub audio_in: bool,
    pub thinking: bool,
    pub tool_use: bool,
    pub max_context_tokens: i64,
    pub max_output_tokens: i64,
}

impl ModelCapability {
    pub fn unknown() -> Self {
        Self {
            image_in: false,
            video_in: false,
            audio_in: false,
            thinking: false,
            tool_use: false,
            max_context_tokens: 0,
            max_output_tokens: 0,
        }
    }

    pub fn is_unknown(&self) -> bool {
        !self.image_in
            && !self.video_in
            && !self.audio_in
            && !self.thinking
            && !self.tool_use
            && self.max_context_tokens == 0
            && self.max_output_tokens == 0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRequestAuth {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Default)]
pub struct AbortSignal {
    aborted: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl AbortSignal {
    pub fn new() -> Self {
        Self {
            aborted: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn abort(&self) {
        self.aborted.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_aborted(&self) -> bool {
        self.aborted.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, Default)]
pub struct GenerateOptions {
    pub auth: Option<ProviderRequestAuth>,
    pub signal: Option<AbortSignal>,
    pub on_request_start: Option<std::sync::Arc<dyn Fn() + Send + Sync + 'static>>,
    pub on_stream_end: Option<std::sync::Arc<dyn Fn() + Send + Sync + 'static>>,
}

#[derive(Default)]
pub struct GenerateCallbacks {
    pub on_message_part: Option<Box<dyn Fn(StreamedMessagePart) + Send + Sync + 'static>>,
    pub on_tool_call: Option<Box<dyn Fn(ToolCall) + Send + Sync + 'static>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateResult {
    pub id: Option<String>,
    pub message: Message,
    pub usage: Option<TokenUsage>,
    pub finish_reason: Option<FinishReason>,
    pub raw_finish_reason: Option<String>,
}

#[async_trait]
pub trait ChatProvider: Send + Sync {
    fn name(&self) -> &str;
    fn model_name(&self) -> &str;
    fn thinking_effort(&self) -> Option<ThinkingEffort>;

    async fn generate(
        &self,
        system_prompt: &str,
        tools: &[Tool],
        history: &[Message],
        options: Option<GenerateOptions>,
    ) -> Result<StreamedMessage, ChatProviderError>;

    fn with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider>;

    fn with_max_completion_tokens(&self, _max_tokens: i64) -> Option<Box<dyn ChatProvider>> {
        None
    }

    fn get_capability(&self, _model: Option<&str>) -> ModelCapability {
        ModelCapability::unknown()
    }
}
```

- [ ] 更新 `rust-ody/crates/kosong-rs/src/lib.rs` 加入 `pub mod usage;`

- [ ] 运行测试并确认通过：

```bash
cd rust-ody && cargo test -p kosong-rs --lib provider::tests
```

- [ ] Commit：`git add rust-ody/crates/kosong-rs/src/provider.rs rust-ody/crates/kosong-rs/src/usage.rs rust-ody/crates/kosong-rs/src/lib.rs && git commit -m "feat(kosong-rs): provider trait and options"`

---

### Task 4: 实现错误分类

**Depends on:** Task 1

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/errors.rs`

**目标：** 对齐 `packages/kosong/src/errors.ts` 的继承关系、字段与判定函数。

**步骤：**

- [ ] 写失败测试。在 `rust-ody/crates/kosong-rs/src/errors.rs` 末尾加入：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_errors() {
        assert!(is_retryable_generate_error(&ChatProviderError::Connection(APIConnectionError)));
        assert!(is_retryable_generate_error(&ChatProviderError::Timeout(APITimeoutError)));
        assert!(is_retryable_generate_error(&ChatProviderError::Empty(APIEmptyResponseError)));
        assert!(is_retryable_generate_error(&ChatProviderError::Status(APIStatusError {
            status_code: 429,
            request_id: None,
        })));
        assert!(!is_retryable_generate_error(&ChatProviderError::Status(APIStatusError {
            status_code: 400,
            request_id: None,
        })));
    }

    #[test]
    fn context_overflow_detection() {
        assert!(is_context_overflow_status_error(400, "This model's maximum context length is 8192 tokens"));
        assert!(!is_context_overflow_status_error(400, "bad request"));
        assert!(!is_context_overflow_status_error(500, "maximum context length"));
    }
}
```

- [ ] 运行测试并确认失败：类型未定义。

- [ ] 实现 `rust-ody/crates/kosong-rs/src/errors.rs`：

```rust
use thiserror::Error;

#[derive(Debug, Error)]
#[error("{0}")]
pub struct ChatProviderErrorBase {
    pub message: String,
}

#[derive(Debug, Clone, Copy, Error)]
#[error("Chat provider error")]
pub struct ChatProviderErrorKind;

#[derive(Debug, Clone, Copy, Error)]
#[error("API connection error")]
pub struct APIConnectionError;

#[derive(Debug, Clone, Copy, Error)]
#[error("API timeout error")]
pub struct APITimeoutError;

#[derive(Debug, Clone, Error)]
#[error("API status error {status_code}: {message}")]
pub struct APIStatusError {
    pub status_code: u16,
    pub message: String,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Error)]
#[error("API context overflow error {status_code}: {message}")]
pub struct APIContextOverflowError {
    pub status_code: u16,
    pub message: String,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Error)]
#[error("The API returned an empty response (no content, no tool calls). Provider: {provider}, model: {model}")]
pub struct APIEmptyResponseError {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Error)]
pub enum ChatProviderError {
    #[error("API connection error")]
    Connection(APIConnectionError),
    #[error("API timeout error")]
    Timeout(APITimeoutError),
    #[error(transparent)]
    Status(APIStatusError),
    #[error(transparent)]
    ContextOverflow(APIContextOverflowError),
    #[error("The API returned an empty response (no content, no tool calls). Provider: {0.provider}, model: {0.model}")]
    Empty(APIEmptyResponseError),
}

pub fn is_retryable_generate_error(error: &ChatProviderError) -> bool {
    matches!(
        error,
        ChatProviderError::Connection(_)
            | ChatProviderError::Timeout(_)
            | ChatProviderError::Empty(_)
            | ChatProviderError::Status(APIStatusError { status_code, .. })
            if matches!(status_code, 429 | 500 | 502 | 503 | 504)
    )
}

const CONTEXT_OVERFLOW_PATTERNS: &[&str] = &[
    r"context[ _-]?length",
    r"context[ _-]?window.*exceed|exceed.*context[ _-]?window",
    r"maximum context",
    r"exceed(?:ed|s|ing)?\s+(?:the\s+)?max(?:imum)?\s+tokens?",
    r"too many tokens.*(?:prompt|input|context)|(?:prompt|input|context).*too many tokens",
    r"prompt is too long.*maximum",
    r"input token count.*exceeds?.*maximum number of tokens",
    r"request.*exceed(?:ed|s|ing)?.*model token limit",
];

pub fn is_context_overflow_error_code(code: Option<&str>) -> bool {
    code == Some("context_length_exceeded")
}

pub fn is_context_overflow_status_error(status_code: u16, message: &str) -> bool {
    if !matches!(status_code, 400 | 413 | 422) {
        return false;
    }
    let lower = message.to_lowercase();
    CONTEXT_OVERFLOW_PATTERNS.iter().any(|pat| {
        regex::Regex::new(pat)
            .ok()
            .map(|re| re.is_match(&lower))
            .unwrap_or(false)
    })
}

pub fn normalize_api_status_error(
    status_code: u16,
    message: impl Into<String>,
    request_id: Option<String>,
) -> ChatProviderError {
    let msg = message.into();
    if is_context_overflow_status_error(status_code, &msg) {
        ChatProviderError::ContextOverflow(APIContextOverflowError {
            status_code,
            message: msg,
            request_id,
        })
    } else {
        ChatProviderError::Status(APIStatusError {
            status_code,
            message: msg,
            request_id,
        })
    }
}
```

- [ ] 添加 `regex` 依赖到 `rust-ody/crates/kosong-rs/Cargo.toml`：

```toml
regex = "1"
```

- [ ] 运行测试并确认通过：

```bash
cd rust-ody && cargo test -p kosong-rs --lib errors::tests
```

- [ ] Commit：`git add rust-ody/crates/kosong-rs/src/errors.rs rust-ody/crates/kosong-rs/Cargo.toml && git commit -m "feat(kosong-rs): error classification"`

---

## Local Self-Review

- [ ] 1. Spec-coverage table: 本部分覆盖 4.2.0.1、4.2.0.2、4.2.0.4、4.2.0.5（crate、类型、trait、错误）。4.2.0.3 与 4.2.0.6 在 Part 2/3 覆盖。
- [ ] 2. Placeholder scan: 无 TODO/TBD；所有代码片段完整可粘贴。
- [ ] 3. No phantom tasks: 每个任务产生可编译/可测试的变更。
- [ ] 4. Dependency soundness: Task 2 依赖 Task 1；Task 3 依赖 Task 2；Task 4 依赖 Task 1（仅需要 crate 骨架，不依赖具体类型）。
- [ ] 5. Caller & build soundness: 本部分未改动 TS/Rust 共享签名；Task 1 修改 `rust-ody/Cargo.toml` 后需运行 `cargo check -p kosong-rs`。
- [ ] 6. Test-the-risk: merge 规则、序列化字段名、错误分类函数均有行为断言；无 filter/word-list 风险。
- [ ] 7. Type consistency: `StreamedMessagePart`、`ToolCall` 的字段名与 TS JSON shape 对齐；后续 Part 2 直接使用本部分定义的类型。
