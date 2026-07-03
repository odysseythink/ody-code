# Part 2: Kimi provider

本部分实现 `KimiChatProvider`（Chat Completions 协议 + Moonshot 专有 `reasoning_content` + Kimi 工具参数归一化）以及 `KimiFiles.uploadVideo`。Kimi 因需要在 assistant 含 tool_calls 时省略空 `content`、读取 `choices[0].usage`、以及内嵌 `thinking` extra_body，所以作为独立 provider 实现，而不是直接封装 `OpenAILegacyChatProvider`。

---

## File Structure

```
rust-ody/crates/kosong-rs/
  src/
    http_client.rs                 # 新增 post_multipart + MultipartPart
    providers/
      mod.rs                       # 导出 kimi 模块
      kimi.rs                      # KimiChatProvider + 测试
    kimi_files.rs                  # KimiFiles + 测试
    lib.rs                         # 导出 KimiChatProvider / KimiFiles
```

---

## Dependency Overview (Part 2 内部)

```
Task 2.1 KimiChatProvider 构造与 ChatProvider trait 壳
  │
  ├─ Task 2.2 消息/工具转换与请求体组装
  │     │
  │     └─ Task 2.3 generate() streaming + non-streaming 完整路径
  │
  └─ Task 2.4 KimiFiles.uploadVideo
        │
        └─ 依赖 Task 2.1 已存在的 provider 配置字段
```

- Task 2.1 为后续所有任务的前置。
- Task 2.2 与 Task 2.4 可独立开发（都依赖 2.1）。
- Task 2.3 依赖 Task 2.2（需要完整请求体）和 Part 1 Task 2（`parse_stream_response_with_usage_extractor`）。

---

## Risks

| 风险 | 缓解 |
|---|---|
| `HttpClient` trait 新增 `post_multipart` 是共享签名变更 | Task 2.4 同步更新 `ReqwestClient`、`MockHttpClient`、测试 mock，并以 `cargo check --workspace` 收尾 |
| Kimi `reasoning_content` 在流式/非流式响应中可能为空 | generate 使用 `reasoning_key = Some("reasoning_content")`；fixture 覆盖两者 |
| 工具调用时省略空 content 与 TS 行为不一致 | Task 2.2 用请求体断言覆盖 assistant + tool_calls + 空 content 场景 |
| 上传文件 MIME/扩展名校验遗漏 | Task 2.4 同时覆盖合法 video、非法扩展名、非法 mime type |

---

### Task 2.1: KimiChatProvider 构造与 ChatProvider trait 壳

**Depends on:** `shared.md` Task 1（`get_kimi_model_capability`）与 Task 3（`normalize_kimi_tool_schema`）

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/providers/kimi.rs:1-180`
- Modify: `rust-ody/crates/kosong-rs/src/providers/mod.rs:1-3`（新增 `pub mod kimi;`）
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs:1-40`（新增 re-export）

**实现步骤：**

- [ ] 先写测试，覆盖构造、name/model、capability、thinking_effort：

```rust
#[cfg(test)]
mod provider_shell_tests {
    use super::*;
    use crate::provider::{ModelCapability, ThinkingEffort};
    use std::collections::HashMap;

    fn provider() -> KimiChatProvider {
        KimiChatProvider::new(KimiOptions {
            api_key: Some("sk-test".into()),
            base_url: None,
            model: "kimi-k2-0711".into(),
            stream: None,
            default_headers: None,
            generation_kwargs: None,
            http_client: None,
        })
    }

    #[test]
    fn name_is_kimi() {
        assert_eq!(provider().name(), "kimi");
    }

    #[test]
    fn model_name_matches_constructor() {
        assert_eq!(provider().model_name(), "kimi-k2-0711");
    }

    #[test]
    fn k2_capability_thinks_and_uses_tools() {
        let cap = provider().get_capability(None);
        assert!(cap.thinking);
        assert!(cap.tool_use);
        assert!(!cap.image_in);
    }

    #[test]
    fn thinking_effort_from_kwargs() {
        let mut kwargs = HashMap::new();
        kwargs.insert("reasoning_effort".into(), serde_json::Value::String("medium".into()));
        let p = KimiChatProvider::new(KimiOptions {
            api_key: Some("sk-test".into()),
            base_url: None,
            model: "kimi-k2".into(),
            stream: None,
            default_headers: None,
            generation_kwargs: Some(kwargs),
            http_client: None,
        });
        assert_eq!(p.thinking_effort(), Some(ThinkingEffort::Medium));
    }

    #[test]
    fn with_thinking_returns_provider_with_effort() {
        let boxed = provider().with_thinking(ThinkingEffort::High);
        assert_eq!(boxed.thinking_effort(), Some(ThinkingEffort::High));
    }

    #[test]
    fn with_max_completion_tokens_returns_provider() {
        let boxed = provider().with_max_completion_tokens(1024);
        assert!(boxed.is_some());
    }
}
```

运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs providers::kimi::provider_shell_tests
```

预期失败：`KimiChatProvider` / `KimiOptions` 未定义。

- [ ] 实现 `KimiChatProvider` 壳与构造逻辑：

```rust
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use crate::capability_registry;
use crate::errors::{APIMissingApiKeyError, ChatProviderError};
use crate::http_client::{HttpClient, ReqwestClient};
use crate::kimi_schema::normalize_kimi_tool_schema;
use crate::message::{ContentPart, Message, Role, ToolCall};
use crate::openai_common::{
    convert_content_part, convert_openai_error, extract_usage, normalize_openai_finish_reason,
    reasoning_effort_to_thinking_effort, thinking_effort_to_reasoning_effort, tool_to_openai,
};
use crate::provider::{
    ChatProvider, GenerateOptions, ModelCapability, ProviderRequestAuth, ThinkingEffort, Tool,
};
use crate::request_auth::{merge_request_headers, require_provider_api_key};
use crate::tool_call_id::{normalize_tool_call_ids_for_provider, sanitize_tool_call_id, ToolCallIdPolicy};
use crate::usage::TokenUsage;
use crate::{chat_completions_stream::{parse_non_stream_response, parse_stream_response_with_usage_extractor}, generate::StreamedMessage};

#[derive(Clone)]
pub struct KimiOptions {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: String,
    pub stream: Option<bool>,
    pub default_headers: Option<HashMap<String, String>>,
    pub generation_kwargs: Option<HashMap<String, Value>>,
    pub http_client: Option<Arc<dyn HttpClient>>,
}

pub struct KimiChatProvider {
    model: String,
    api_key: Option<String>,
    base_url: String,
    default_headers: Option<HashMap<String, String>>,
    generation_kwargs: HashMap<String, Value>,
    reasoning_effort: Option<String>,
    extra_body: serde_json::Map<String, Value>,
    stream: bool,
    http_client: Arc<dyn HttpClient>,
}

impl std::fmt::Debug for KimiChatProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KimiChatProvider")
            .field("model", &self.model)
            .field("stream", &self.stream)
            .finish_non_exhaustive()
    }
}

impl Clone for KimiChatProvider {
    fn clone(&self) -> Self {
        Self {
            model: self.model.clone(),
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            default_headers: self.default_headers.clone(),
            generation_kwargs: self.generation_kwargs.clone(),
            reasoning_effort: self.reasoning_effort.clone(),
            extra_body: self.extra_body.clone(),
            stream: self.stream,
            http_client: Arc::clone(&self.http_client),
        }
    }
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

        let mut raw_kwargs = options.generation_kwargs.unwrap_or_default();
        raw_kwargs.retain(|_, v| !v.is_null());

        let reasoning_effort = raw_kwargs
            .remove("reasoning_effort")
            .and_then(|v| v.as_str().map(String::from));
        let extra_body = raw_kwargs
            .remove("extra_body")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();

        Self {
            model: options.model,
            api_key,
            base_url,
            default_headers: options.default_headers,
            generation_kwargs: raw_kwargs,
            reasoning_effort,
            extra_body,
            stream: options.stream.unwrap_or(true),
            http_client,
        }
    }

}

fn kimi_tool_call_id_policy() -> ToolCallIdPolicy {
    ToolCallIdPolicy::new(|id| sanitize_tool_call_id(id, Some(64)), Some(64))
}

#[async_trait]
impl ChatProvider for KimiChatProvider {
    fn name(&self) -> &str { "kimi" }
    fn model_name(&self) -> &str { &self.model }
    fn thinking_effort(&self) -> Option<ThinkingEffort> {
        reasoning_effort_to_thinking_effort(self.reasoning_effort.as_deref())
    }
    fn get_capability(&self, model: Option<&str>) -> ModelCapability {
        capability_registry::get_kimi_model_capability(model.unwrap_or(&self.model))
    }
    fn with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider> {
        let reasoning_effort = match effort {
            ThinkingEffort::Off => None,
            ThinkingEffort::Low => Some("low".into()),
            ThinkingEffort::Medium => Some("medium".into()),
            ThinkingEffort::High | ThinkingEffort::Xhigh | ThinkingEffort::Max => Some("high".into()),
        };
        let thinking = serde_json::json!({"type": if effort == ThinkingEffort::Off { "disabled" } else { "enabled" }});
        let mut clone = self.clone();
        clone.reasoning_effort = reasoning_effort;
        clone.extra_body.insert("thinking".into(), thinking);
        Box::new(clone)
    }
    fn with_max_completion_tokens(&self, max_completion_tokens: i64) -> Option<Box<dyn ChatProvider>> {
        let mut clone = self.clone();
        clone.generation_kwargs.insert(
            "max_completion_tokens".into(),
            Value::Number(max_completion_tokens.into()),
        );
        Some(Box::new(clone))
    }

    async fn generate(
        &self,
        _system_prompt: &str,
        _tools: &[Tool],
        _history: &[Message],
        _options: Option<GenerateOptions>,
    ) -> Result<StreamedMessage, ChatProviderError> {
        // Task 2.3 填充实现
        unimplemented!()
    }
}
```

- [ ] 修改 `providers/mod.rs`：

```rust
pub mod anthropic;
pub mod google_genai;
pub mod kimi;
```

- [ ] 修改 `lib.rs`，新增 re-export：

```rust
pub use providers::kimi::{KimiChatProvider, KimiOptions};
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p kosong-rs providers::kimi::provider_shell_tests
```

预期：除 `generate` unimplemented panic 相关的编译警告外，测试全部通过（`generate` 尚未被调用，所以不会 panic）。

- [ ] Commit: `feat(kosong-rs): scaffold KimiChatProvider shell and options`

---

### Task 2.2: 消息/工具转换与请求体组装

**Depends on:** Task 2.1

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/kimi.rs:180-450`（追加转换函数与 `build_create_params`）
- Test: `rust-ody/crates/kosong-rs/src/providers/kimi.rs` 内 `#[cfg(test)]` 模块

**实现步骤：**

- [ ] 先写测试，断言请求体 JSON 的关键形状。需要在测试模块里实现一个捕获型 mock：

```rust
#[cfg(test)]
mod request_body_tests {
    use super::*;
    use crate::http_client::{HttpClient, HttpResponse};
    use crate::message::{Message, Role, ToolCall};
    use crate::provider::Tool;
    use futures_util::StreamExt;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    struct CaptureJsonClient {
        status: u16,
        body: Vec<u8>,
        last: Mutex<Option<(String, HashMap<String, String>, Value)>>,
    }

    impl CaptureJsonClient {
        fn new(status: u16, body: Vec<u8>) -> Self {
            Self { status, body, last: Mutex::new(None) }
        }
        fn last_request(&self) -> Option<(String, HashMap<String, String>, Value)> {
            self.last.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl HttpClient for CaptureJsonClient {
        async fn post_json(
            &self,
            url: &str,
            headers: HashMap<String, String>,
            body: Value,
        ) -> Result<HttpResponse, ChatProviderError> {
            *self.last.lock().unwrap() = Some((url.into(), headers, body));
            let chunks = vec![self.body.clone()];
            let stream = futures_util::stream::iter(
                chunks.into_iter().map(|c| Ok(bytes::Bytes::from(c))),
            )
            .boxed();
            Ok(HttpResponse::new(self.status, stream))
        }
    }

    fn provider_with_client(client: Arc<CaptureJsonClient>) -> KimiChatProvider {
        KimiChatProvider::new(KimiOptions {
            api_key: Some("sk".into()),
            base_url: Some("http://mock".into()),
            model: "kimi-k2".into(),
            stream: Some(true),
            default_headers: None,
            generation_kwargs: None,
            http_client: Some(client),
        })
    }

    #[tokio::test]
    async fn omits_empty_assistant_content_when_tool_calls_present() {
        let client = Arc::new(CaptureJsonClient::new(200, b"data: [DONE]\n\n".to_vec()));
        let provider = provider_with_client(Arc::clone(&client));
        let tool = Tool {
            name: "read".into(),
            description: "read".into(),
            parameters: serde_json::json!({"type":"object"}),
        };
        let history = vec![Message::assistant(
            vec![],
            vec![ToolCall {
                call_type: "function".into(),
                id: "tc1".into(),
                name: "read".into(),
                arguments: None,
                extras: None,
                stream_index: None,
            }],
        )];
        let _ = provider.generate("", &[tool], &history, None).await;
        let (_, _, body) = client.last_request().unwrap();
        let assistant = &body["messages"].as_array().unwrap()[0];
        assert_eq!(assistant["role"], "assistant");
        assert!(assistant["content"].is_null());
        assert!(assistant["tool_calls"].is_array());
    }

    #[tokio::test]
    async fn normalizes_max_tokens_to_max_completion_tokens() {
        let client = Arc::new(CaptureJsonClient::new(200, b"data: [DONE]\n\n".to_vec()));
        let mut kwargs = HashMap::new();
        kwargs.insert("max_tokens".into(), serde_json::json!(256));
        let provider = KimiChatProvider::new(KimiOptions {
            api_key: Some("sk".into()),
            base_url: Some("http://mock".into()),
            model: "kimi-k2".into(),
            stream: Some(true),
            default_headers: None,
            generation_kwargs: Some(kwargs),
            http_client: Some(Arc::clone(&client)),
        });
        let _ = provider.generate("", &[], &[], None).await;
        let (_, _, body) = client.last_request().unwrap();
        assert!(body["max_tokens"].is_null());
        assert_eq!(body["max_completion_tokens"], 256);
    }

    #[tokio::test]
    async fn flattens_extra_body_thinking() {
        let client = Arc::new(CaptureJsonClient::new(200, b"data: [DONE]\n\n".to_vec()));
        let provider = KimiChatProvider::new(KimiOptions {
            api_key: Some("sk".into()),
            base_url: Some("http://mock".into()),
            model: "kimi-k2".into(),
            stream: Some(true),
            default_headers: None,
            generation_kwargs: Some({
                let mut m = HashMap::new();
                m.insert(
                    "extra_body".into(),
                    serde_json::json!({"thinking": {"type": "enabled"}}),
                );
                m
            }),
            http_client: Some(Arc::clone(&client)),
        });
        let _ = provider.generate("", &[], &[], None).await;
        let (_, _, body) = client.last_request().unwrap();
        assert_eq!(body["thinking"]["type"], "enabled");
    }

    #[tokio::test]
    async fn normalizes_tool_schema_and_keeps_builtin_prefix() {
        let client = Arc::new(CaptureJsonClient::new(200, b"data: [DONE]\n\n".to_vec()));
        let provider = provider_with_client(Arc::clone(&client));
        let tool = Tool {
            name: "$web_search".into(),
            description: "search".into(),
            parameters: serde_json::Value::Null,
        };
        let user_tool = Tool {
            name: "read".into(),
            description: "read".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "description": "file path" }
                }
            }),
        };
        let _ = provider.generate("", &[tool, user_tool], &[], None).await;
        let (_, _, body) = client.last_request().unwrap();
        let tools = body["tools"].as_array().unwrap();
        assert_eq!(tools[0]["type"], "builtin_function");
        assert_eq!(tools[0]["function"]["name"], "$web_search");
        assert_eq!(tools[1]["function"]["parameters"]["properties"]["path"]["type"], "string");
    }
}
```

运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs providers::kimi::request_body_tests
```

预期失败：`CaptureJsonClient` 未实现 `post_multipart`（Task 2.4 才加），但当前 trait 只有 `post_json`，所以实际失败点是 `build_create_params`、`convert_message`、`convert_tool` 等未定义。待 Task 2.4 共享签名变更后，这个测试 mock 需要补 `post_multipart`。

- [ ] 实现消息/工具转换与请求体组装（追加到 `kimi.rs`）：

```rust
#[derive(Debug, Clone, Serialize)]
struct OpenAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "tool_calls")]
    tool_calls: Option<Vec<KimiToolCallOut>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "tool_call_id")]
    tool_call_id: Option<String>,
    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
struct KimiToolCallOut {
    #[serde(rename = "type")]
    call_type: String,
    id: String,
    function: KimiToolFunctionOut,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    extras: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
struct KimiToolFunctionOut {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<String>,
}

fn is_effectively_empty_content(parts: &[&ContentPart]) -> bool {
    parts.iter().all(|p| match p {
        ContentPart::Text { text } => text.trim().is_empty(),
        _ => false,
    })
}

fn convert_message(message: &Message) -> OpenAIMessage {
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

    let has_tool_calls = !message.tool_calls.is_empty();
    let should_omit_content = message.role == Role::Assistant
        && has_tool_calls
        && is_effectively_empty_content(&non_think);

    let content = if should_omit_content || non_think.is_empty() {
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
    };

    let tool_calls = if has_tool_calls {
        Some(
            message
                .tool_calls
                .iter()
                .map(|tc| KimiToolCallOut {
                    call_type: tc.call_type.clone(),
                    id: tc.id.clone(),
                    function: KimiToolFunctionOut {
                        name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                    },
                    extras: tc.extras.clone(),
                })
                .collect(),
        )
    } else {
        None
    };

    let mut extra = HashMap::new();
    if !reasoning_content.is_empty() {
        extra.insert("reasoning_content".into(), Value::String(reasoning_content));
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

fn convert_tool(tool: &Tool) -> Value {
    if tool.name.starts_with('$') {
        return serde_json::json!({
            "type": "builtin_function",
            "function": { "name": tool.name }
        });
    }
    let mut value = serde_json::to_value(tool_to_openai(tool)).unwrap();
    if let Some(function) = value.get_mut("function").and_then(|f| f.as_object_mut()) {
        if let Some(parameters) = function.get_mut("parameters") {
            if let Some(obj) = parameters.as_object_mut() {
                *parameters = Value::Object(normalize_kimi_tool_schema(obj.clone()));
            }
        }
    }
    value
}

impl KimiChatProvider {
    fn build_create_params(
        &self,
        tools: &[Tool],
        messages: &[OpenAIMessage],
    ) -> serde_json::Map<String, Value> {
        let mut kwargs = self.generation_kwargs.clone();
        kwargs.retain(|_, v| !v.is_null());

        if !kwargs.contains_key("max_completion_tokens") {
            if let Some(v) = kwargs.remove("max_tokens") {
                kwargs.insert("max_completion_tokens".into(), v);
            }
        }

        let mut create_params = serde_json::Map::new();
        create_params.insert("model".into(), Value::String(self.model.clone()));
        create_params.insert("messages".into(), serde_json::to_value(messages).unwrap());
        create_params.insert("stream".into(), Value::Bool(self.stream));

        for (k, v) in kwargs {
            create_params.insert(k, v);
        }
        for (k, v) in &self.extra_body {
            create_params.insert(k.clone(), v.clone());
        }

        if !tools.is_empty() {
            create_params.insert(
                "tools".into(),
                Value::Array(tools.iter().map(|t| convert_tool(t)).collect()),
            );
        }

        if self.stream {
            create_params.insert(
                "stream_options".into(),
                serde_json::json!({"include_usage": true}),
            );
        }

        if let Some(re) = &self.reasoning_effort {
            create_params.insert("reasoning_effort".into(), Value::String(re.clone()));
        }

        create_params
    }
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p kosong-rs providers::kimi::request_body_tests
```

预期：全部通过。

- [ ] Commit: `feat(kosong-rs): add Kimi message/tool conversion and request assembly`

---

### Task 2.3: generate() streaming + non-streaming 完整路径

**Depends on:** Task 2.2、`shared.md` Task 2（`parse_stream_response_with_usage_extractor`）

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/kimi.rs:450-650`（替换 `generate` 的 `unimplemented!()`，并补充辅助函数）
- Test: `rust-ody/crates/kosong-rs/src/providers/kimi.rs` 内 `#[cfg(test)]` 模块

**实现步骤：**

- [ ] 先写测试，覆盖流式文本/推理/工具调用、`choices[0].usage`、非流式、缺失 API key：

```rust
#[cfg(test)]
mod generate_tests {
    use super::*;
    use crate::http_client::MockHttpClient;
    use crate::message::{StreamedMessagePart, ToolCall};
    use crate::provider::ProviderRequestAuth;
    use futures_util::StreamExt;
    use std::sync::Arc;

    fn streaming_provider(body: Vec<u8>) -> KimiChatProvider {
        KimiChatProvider::new(KimiOptions {
            api_key: Some("sk".into()),
            base_url: Some("http://mock".into()),
            model: "kimi-k2".into(),
            stream: Some(true),
            default_headers: None,
            generation_kwargs: None,
            http_client: Some(Arc::new(MockHttpClient::new(200, body))),
        })
    }

    #[tokio::test]
    async fn generate_streams_text_reasoning_and_tool_call() {
        let sse = br#"data: {"id":"1","choices":[{"index":0,"delta":{"reasoning_content":"step1"}}]}
data: {"id":"1","choices":[{"index":0,"delta":{"content":" answer"}}]}
data: {"id":"1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"name":"read"}}]}}]}
data: {"id":"1","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{}"}}]}}]}
data: {"id":"1","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]}
data: [DONE]
"#
        .to_vec();
        let provider = streaming_provider(sse);
        let stream = provider.generate("", &[], &[], None).await.unwrap();
        let parts: Vec<_> = stream.collect().await;
        assert_eq!(parts[0], StreamedMessagePart::think("step1"));
        assert_eq!(parts[1], StreamedMessagePart::text(" answer"));
        assert!(
            matches!(&parts[2], StreamedMessagePart::ToolCall(ToolCall { name, .. }) if name == "read")
        );
    }

    #[tokio::test]
    async fn generate_reads_choice_usage() {
        let sse = br#"data: {"id":"1","choices":[{"index":0,"delta":{"content":"hi"},"usage":{"prompt_tokens":20,"completion_tokens":4,"cached_tokens":5}}]}
data: [DONE]
"#
        .to_vec();
        let provider = streaming_provider(sse);
        let stream = provider.generate("", &[], &[], None).await.unwrap();
        let parts: Vec<_> = stream.collect().await;
        assert_eq!(parts[0], StreamedMessagePart::text("hi"));
        assert_eq!(stream.usage().map(|u| u.input_other), Some(15));
        assert_eq!(stream.usage().map(|u| u.input_cache_read), Some(5));
        assert_eq!(stream.usage().map(|u| u.output), Some(4));
    }

    #[tokio::test]
    async fn generate_non_stream_with_tool_call() {
        let body = br#"{"id":"chat-1","choices":[{"message":{"content":"ok","tool_calls":[{"id":"tc1","type":"function","function":{"name":"read","arguments":"{}"}}]},"finish_reason":"tool_calls"}]}"#.to_vec();
        let provider = KimiChatProvider::new(KimiOptions {
            api_key: Some("sk".into()),
            base_url: Some("http://mock".into()),
            model: "kimi-k2".into(),
            stream: Some(false),
            default_headers: None,
            generation_kwargs: None,
            http_client: Some(Arc::new(MockHttpClient::new(200, body))),
        });
        let stream = provider.generate("", &[], &[], None).await.unwrap();
        let parts: Vec<_> = stream.collect().await;
        assert_eq!(parts[0], StreamedMessagePart::text("ok"));
        assert!(
            matches!(&parts[1], StreamedMessagePart::ToolCall(ToolCall { name, .. }) if name == "read")
        );
    }

    #[tokio::test]
    async fn generate_rejects_empty_request_api_key() {
        let provider = KimiChatProvider::new(KimiOptions {
            api_key: None,
            base_url: Some("http://mock".into()),
            model: "kimi-k2".into(),
            stream: Some(true),
            default_headers: None,
            generation_kwargs: None,
            http_client: Some(Arc::new(MockHttpClient::new(200, vec![]))),
        });
        let auth = ProviderRequestAuth {
            api_key: Some("".into()),
            headers: None,
        };
        let options = GenerateOptions {
            auth: Some(auth),
            ..Default::default()
        };
        let err = provider.generate("", &[], &[], Some(options)).await.unwrap_err();
        assert!(matches!(err, ChatProviderError::MissingApiKey(_)));
    }
}
```

运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs providers::kimi::generate_tests
```

预期失败：`generate` 方法当前为 `unimplemented!()`，测试会 panic 或编译时提示未实现的方法不存在。

- [ ] 替换 `generate` 实现并补充辅助函数：

```rust
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

fn kimi_usage_extractor(value: &Value) -> Option<Value> {
    value
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("usage"))
        .cloned()
}

#[async_trait]
impl ChatProvider for KimiChatProvider {
    fn name(&self) -> &str { "kimi" }
    fn model_name(&self) -> &str { &self.model }
    fn thinking_effort(&self) -> Option<ThinkingEffort> {
        reasoning_effort_to_thinking_effort(self.reasoning_effort.as_deref())
    }
    fn get_capability(&self, model: Option<&str>) -> ModelCapability {
        capability_registry::get_kimi_model_capability(model.unwrap_or(&self.model))
    }
    fn with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider> {
        let reasoning_effort = match effort {
            ThinkingEffort::Off => None,
            ThinkingEffort::Low => Some("low".into()),
            ThinkingEffort::Medium => Some("medium".into()),
            ThinkingEffort::High | ThinkingEffort::Xhigh | ThinkingEffort::Max => Some("high".into()),
        };
        let thinking = serde_json::json!({
            "type": if effort == ThinkingEffort::Off { "disabled" } else { "enabled" }
        });
        let mut clone = self.clone();
        clone.reasoning_effort = reasoning_effort;
        clone.extra_body.insert("thinking".into(), thinking);
        Box::new(clone)
    }
    fn with_max_completion_tokens(&self, max_completion_tokens: i64) -> Option<Box<dyn ChatProvider>> {
        let mut clone = self.clone();
        clone.generation_kwargs.insert(
            "max_completion_tokens".into(),
            Value::Number(max_completion_tokens.into()),
        );
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
            normalize_tool_call_ids_for_provider(history, &kimi_tool_call_id_policy());

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
            messages.push(convert_message(msg));
        }

        let create_params = self.build_create_params(tools, &messages);
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
            parse_stream_response_with_usage_extractor(
                body_bytes,
                Some("reasoning_content"),
                kimi_usage_extractor,
            )
            .await?
        } else {
            parse_non_stream_response(&body_bytes, Some("reasoning_content"))?
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
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p kosong-rs providers::kimi::generate_tests
```

预期：全部通过。

- [ ] 运行整个 crate 测试确认没有破坏既有解析器：

```bash
cd rust-ody && cargo test -p kosong-rs
```

预期：通过（新增测试 + 既有测试）。

- [ ] Commit: `feat(kosong-rs): implement Kimi generate() streaming and non-streaming`

---

### Task 2.4: KimiFiles.uploadVideo

**Depends on:** Task 2.1

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/kimi_files.rs:1-300`
- Modify: `rust-ody/crates/kosong-rs/src/http_client.rs:1-30`（共享签名：新增 `MultipartPart` + `post_multipart`）
- Modify: `rust-ody/crates/kosong-rs/src/providers/kimi.rs:80-120`（追加 `files()` 与 `upload_video()` 方法）
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs:1-40`（新增 `kimi_files` 模块与 re-export）
- Test: `rust-ody/crates/kosong-rs/src/kimi_files.rs` 内 `#[cfg(test)]` 模块

**实现步骤：**

- [ ] 先修改共享 `HttpClient` trait（这是本 Task 的共享签名变更，必须同时更新所有实现者）：

在 `rust-ody/crates/kosong-rs/src/http_client.rs` 中，将 `HttpClient` trait、`ReqwestClient`、`MockHttpClient` 替换为以下等价扩展：

```rust
#[derive(Debug, Clone)]
pub struct MultipartPart {
    pub name: String,
    pub file_name: Option<String>,
    pub mime_type: Option<String>,
    pub data: Vec<u8>,
}

#[async_trait]
pub trait HttpClient: Send + Sync {
    async fn post_json(
        &self,
        url: &str,
        headers: HashMap<String, String>,
        body: Value,
    ) -> Result<HttpResponse, ChatProviderError>;

    async fn post_multipart(
        &self,
        url: &str,
        headers: HashMap<String, String>,
        parts: Vec<MultipartPart>,
        fields: HashMap<String, String>,
    ) -> Result<HttpResponse, ChatProviderError>;
}

// ReqwestClient 新增 post_multipart 实现
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
        let body = resp.bytes_stream().map(|r| r.map_err(classify_reqwest_error)).boxed();
        Ok(HttpResponse::new(status, body))
    }

    async fn post_multipart(
        &self,
        url: &str,
        headers: HashMap<String, String>,
        parts: Vec<MultipartPart>,
        fields: HashMap<String, String>,
    ) -> Result<HttpResponse, ChatProviderError> {
        let mut form = reqwest::multipart::Form::new();
        for part in parts {
            let mut p = reqwest::multipart::Part::bytes(part.data);
            if let Some(name) = part.file_name {
                p = p.file_name(name);
            }
            if let Some(mt) = part.mime_type {
                p = p.mime_str(&mt).map_err(|e| ChatProviderError::Other(e.to_string()))?;
            }
            form = form.part(part.name, p);
        }
        for (k, v) in fields {
            form = form.text(k, v);
        }
        let mut req = self.client.post(url).multipart(form);
        for (k, v) in headers {
            req = req.header(k, v);
        }
        let resp = req.send().await.map_err(classify_reqwest_error)?;
        let status = resp.status().as_u16();
        let body = resp.bytes_stream().map(|r| r.map_err(classify_reqwest_error)).boxed();
        Ok(HttpResponse::new(status, body))
    }
}

// MockHttpClient 新增 post_multipart 实现（返回相同配置的响应）
#[async_trait]
impl HttpClient for MockHttpClient {
    async fn post_json(
        &self,
        _url: &str,
        _headers: HashMap<String, String>,
        _body: Value,
    ) -> Result<HttpResponse, ChatProviderError> {
        let chunks = self.chunks.clone();
        let stream = futures_util::stream::iter(chunks.into_iter().map(|c| Ok(bytes::Bytes::from(c)))).boxed();
        Ok(HttpResponse::new(self.status, stream))
    }

    async fn post_multipart(
        &self,
        _url: &str,
        _headers: HashMap<String, String>,
        _parts: Vec<MultipartPart>,
        _fields: HashMap<String, String>,
    ) -> Result<HttpResponse, ChatProviderError> {
        let chunks = self.chunks.clone();
        let stream = futures_util::stream::iter(chunks.into_iter().map(|c| Ok(bytes::Bytes::from(c)))).boxed();
        Ok(HttpResponse::new(self.status, stream))
    }
}
```

同时更新 `rust-ody/crates/kosong-rs/src/providers/kimi.rs` 里的测试 mock `CaptureJsonClient`，使其也实现 `post_multipart`（直接返回错误即可，这些测试不会调用上传）：

```rust
#[async_trait]
impl HttpClient for CaptureJsonClient {
    async fn post_json(
        &self,
        url: &str,
        headers: HashMap<String, String>,
        body: Value,
    ) -> Result<HttpResponse, ChatProviderError> {
        *self.last.lock().unwrap() = Some((url.into(), headers, body));
        let chunks = vec![self.body.clone()];
        let stream = futures_util::stream::iter(chunks.into_iter().map(|c| Ok(bytes::Bytes::from(c)))).boxed();
        Ok(HttpResponse::new(self.status, stream))
    }

    async fn post_multipart(
        &self,
        _url: &str,
        _headers: HashMap<String, String>,
        _parts: Vec<MultipartPart>,
        _fields: HashMap<String, String>,
    ) -> Result<HttpResponse, ChatProviderError> {
        Err(ChatProviderError::Other("unexpected multipart in generate test".into()))
    }
}
```

- [ ] 搜索所有 `HttpClient` 实现者与 `post_json` 调用者，确认没有遗漏：

```bash
cd rust-ody && grep -rn "impl HttpClient for" crates/kosong-rs/src
cd rust-ody && grep -rn "post_json(" crates/kosong-rs/src
```

预期只看到 `ReqwestClient`、`MockHttpClient`、`CaptureJsonClient` 三个实现；`post_json` 调用方在 `openai_legacy.rs`、`kimi.rs` 与 golden binary 中，无需改动。

- [ ] 先写 `kimi_files.rs` 的测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_client::{HttpClient, HttpResponse, MultipartPart};
    use crate::message::ContentPart;
    use futures_util::StreamExt;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    struct CaptureMultipartClient {
        status: u16,
        body: Vec<u8>,
        last: Mutex<Option<(Vec<MultipartPart>, HashMap<String, String>)>>,
    }

    impl CaptureMultipartClient {
        fn new(status: u16, body: Vec<u8>) -> Self {
            Self { status, body, last: Mutex::new(None) }
        }
        fn last_request(&self) -> Option<(Vec<MultipartPart>, HashMap<String, String>)> {
            self.last.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl HttpClient for CaptureMultipartClient {
        async fn post_json(
            &self,
            _url: &str,
            _headers: HashMap<String, String>,
            _body: serde_json::Value,
        ) -> Result<HttpResponse, ChatProviderError> {
            Err(ChatProviderError::Other("unexpected json".into()))
        }

        async fn post_multipart(
            &self,
            _url: &str,
            _headers: HashMap<String, String>,
            parts: Vec<MultipartPart>,
            fields: HashMap<String, String>,
        ) -> Result<HttpResponse, ChatProviderError> {
            *self.last.lock().unwrap() = Some((parts, fields));
            let chunks = vec![self.body.clone()];
            let stream = futures_util::stream::iter(chunks.into_iter().map(|c| Ok(bytes::Bytes::from(c)))).boxed();
            Ok(HttpResponse::new(self.status, stream))
        }
    }

    fn files_with_client(client: Arc<CaptureMultipartClient>) -> KimiFiles {
        KimiFiles::new(KimiFilesOptions {
            api_key: Some("sk".into()),
            base_url: "http://mock".into(),
            default_headers: None,
            http_client: Some(client),
        })
    }

    #[tokio::test]
    async fn upload_video_from_bytes_returns_ms_url() {
        let client = Arc::new(CaptureMultipartClient::new(200, br#"{"id":"file_123"}"#.to_vec()));
        let files = files_with_client(Arc::clone(&client));
        let input = VideoUploadInput {
            data: b"fake video".to_vec(),
            mime_type: "video/mp4".into(),
            filename: Some("clip.mp4".into()),
        };
        let part = files.upload_video(KimiVideoUpload::Bytes(input), None).await.unwrap();
        match part {
            ContentPart::VideoUrl { video_url } => {
                assert_eq!(video_url.url, "ms://file_123");
                assert_eq!(video_url.id, Some("file_123".into()));
            }
            _ => panic!("expected VideoUrl content part"),
        }
        let (parts, texts) = client.last_request().unwrap();
        assert_eq!(texts["purpose"], "video");
        assert_eq!(parts[0].name, "file");
        assert_eq!(parts[0].file_name.as_deref(), Some("clip.mp4"));
        assert_eq!(parts[0].mime_type.as_deref(), Some("video/mp4"));
    }

    #[tokio::test]
    async fn upload_video_rejects_non_video_mime() {
        let client = Arc::new(CaptureMultipartClient::new(200, br#"{"id":"x"}"#.to_vec()));
        let files = files_with_client(client);
        let input = VideoUploadInput {
            data: b"x".to_vec(),
            mime_type: "image/png".into(),
            filename: None,
        };
        let err = files.upload_video(KimiVideoUpload::Bytes(input), None).await.unwrap_err();
        assert!(err.to_string().contains("video"));
    }

    #[tokio::test]
    async fn upload_video_from_path_rejects_bad_extension() {
        let client = Arc::new(CaptureMultipartClient::new(200, br#"{"id":"x"}"#.to_vec()));
        let files = files_with_client(client);
        let path = std::env::temp_dir().join("kimi_upload_test.txt");
        std::fs::write(&path, b"x").unwrap();
        let err = files
            .upload_video(KimiVideoUpload::Path(path.clone()), None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("video"));
        std::fs::remove_file(&path).unwrap();
    }
}
```

运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs kimi_files::tests
```

预期失败：`MultipartPart`、`post_multipart`、`KimiFiles` 等未定义。

- [ ] 创建 `rust-ody/crates/kosong-rs/src/kimi_files.rs`：

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde_json::Value;

use crate::errors::{APIMissingApiKeyError, ChatProviderError};
use crate::http_client::{ByteStream, HttpClient, MultipartPart, ReqwestClient};
use crate::message::{ContentPart, UrlPayload};
use crate::openai_common::convert_openai_error;
use crate::provider::ProviderRequestAuth;
use crate::request_auth::{merge_request_headers, require_provider_api_key};

#[derive(Debug, Clone)]
pub struct VideoUploadInput {
    pub data: Vec<u8>,
    pub mime_type: String,
    pub filename: Option<String>,
}

#[derive(Debug, Clone)]
pub enum KimiVideoUpload {
    Path(PathBuf),
    Bytes(VideoUploadInput),
}

#[derive(Clone)]
pub struct KimiFilesOptions {
    pub api_key: Option<String>,
    pub base_url: String,
    pub default_headers: Option<HashMap<String, String>>,
    pub http_client: Option<Arc<dyn HttpClient>>,
}

#[derive(Clone, Default)]
pub struct KimiUploadOptions {
    pub auth: Option<ProviderRequestAuth>,
}

pub struct KimiFiles {
    api_key: Option<String>,
    base_url: String,
    default_headers: Option<HashMap<String, String>>,
    http_client: Arc<dyn HttpClient>,
}

impl KimiFiles {
    pub fn new(options: KimiFilesOptions) -> Self {
        Self {
            api_key: options.api_key,
            base_url: options.base_url,
            default_headers: options.default_headers,
            http_client: options
                .http_client
                .unwrap_or_else(|| Arc::new(ReqwestClient::new(reqwest::Client::new()))),
        }
    }

    pub async fn upload_video(
        &self,
        input: KimiVideoUpload,
        options: Option<KimiUploadOptions>,
    ) -> Result<ContentPart, ChatProviderError> {
        let (data, mime_type, filename) = match input {
            KimiVideoUpload::Path(path) => {
                if !path.exists() {
                    return Err(ChatProviderError::Other(format!(
                        "Video file not found: {}",
                        path.display()
                    )));
                }
                let filename = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("upload.bin")
                    .to_string();
                let mime_type = guess_mime_type_from_ext(&filename).ok_or_else(|| {
                    ChatProviderError::Other(format!(
                        "KimiFiles.uploadVideo: file extension does not indicate a video type: {}",
                        filename
                    ))
                })?;
                if !mime_type.starts_with("video/") {
                    return Err(ChatProviderError::Other(format!(
                        "KimiFiles.uploadVideo: file extension does not indicate a video type: {}",
                        filename
                    )));
                }
                let data = tokio::fs::read(&path).await.map_err(|e| {
                    ChatProviderError::Other(format!("Failed to read video file: {e}"))
                })?;
                (data, mime_type, filename)
            }
            KimiVideoUpload::Bytes(input) => {
                if !input.mime_type.starts_with("video/") {
                    return Err(ChatProviderError::Other(format!(
                        "Expected a video mime type, got {}",
                        input.mime_type
                    )));
                }
                let filename = input
                    .filename
                    .unwrap_or_else(|| guess_filename(&input.mime_type));
                (input.data, input.mime_type, filename)
            }
        };

        let auth = options.as_ref().and_then(|o| o.auth.clone());
        let api_key = auth
            .as_ref()
            .and_then(|a| a.api_key.clone())
            .or_else(|| self.api_key.clone())
            .filter(|k| !k.is_empty())
            .ok_or_else(|| {
                ChatProviderError::MissingApiKey(APIMissingApiKeyError {
                    provider: "KimiFiles.uploadVideo".to_string(),
                })
            })?;

        let merged_headers = merge_request_headers(
            self.default_headers.as_ref(),
            auth.as_ref().and_then(|a| a.headers.as_ref()),
        );
        let mut headers = HashMap::new();
        headers.insert("Authorization".into(), format!("Bearer {api_key}"));
        if let Some(m) = merged_headers {
            headers.extend(m);
        }

        let parts = vec![MultipartPart {
            name: "file".into(),
            file_name: Some(filename),
            mime_type: Some(mime_type.clone()),
            data,
        }];
        let mut fields = HashMap::new();
        fields.insert("purpose".into(), "video".into());

        let url = format!("{}/files", self.base_url.trim_end_matches('/'));
        let response = self
            .http_client
            .post_multipart(&url, headers, parts, fields)
            .await?;
        let status = response.status();
        let mut stream = response.bytes_stream();
        let body_bytes = read_body_bytes(&mut stream).await.unwrap_or_default();

        if status < 200 || status >= 300 {
            let body = String::from_utf8_lossy(&body_bytes);
            let (msg, code) = parse_upload_error_body(&body);
            return Err(convert_openai_error(&msg, Some(status), code.as_deref()));
        }

        let body: Value = serde_json::from_slice(&body_bytes)
            .map_err(|e| ChatProviderError::Other(format!("Invalid upload response: {e}")))?;
        let id = body
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ChatProviderError::Other("Upload response missing file id".into()))?
            .to_string();

        Ok(ContentPart::VideoUrl {
            video_url: UrlPayload {
                url: format!("ms://{}", id),
                id: Some(id),
            },
        })
    }
}

async fn read_body_bytes(stream: &mut ByteStream) -> Result<Vec<u8>, ChatProviderError> {
    use futures_util::StreamExt;
    let mut buf = Vec::new();
    while let Some(chunk) = stream.next().await {
        buf.extend_from_slice(chunk?.as_ref());
    }
    Ok(buf)
}

fn parse_upload_error_body(body: &str) -> (String, Option<String>) {
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

fn guess_filename(mime_type: &str) -> String {
    let ext = MIME_TO_EXT.get(mime_type.to_lowercase().as_str()).copied().unwrap_or("bin");
    format!("upload.{}", ext)
}

fn guess_mime_type_from_ext(filename: &str) -> Option<String> {
    let dot = filename.rfind('.')?;
    let ext = filename[dot + 1..].to_lowercase();
    EXT_TO_MIME.get(ext.as_str()).cloned()
}

const MIME_TO_EXT: &[(&str, &str)] = &[
    ("video/mp4", "mp4"),
    ("video/mpeg", "mpeg"),
    ("video/quicktime", "mov"),
    ("video/webm", "webm"),
    ("video/x-matroska", "mkv"),
    ("video/x-msvideo", "avi"),
    ("video/x-flv", "flv"),
    ("video/3gpp", "3gp"),
];

lazy_static::lazy_static! {
    static ref EXT_TO_MIME: std::collections::HashMap<String, String> = {
        MIME_TO_EXT
            .iter()
            .map(|(mime, ext)| (ext.to_string(), mime.to_string()))
            .collect()
    };
}
```

> 说明：`lazy_static` 在 `kosong-rs` 当前依赖中不存在，需要在本 Task 中把它加入 `rust-ody/crates/kosong-rs/Cargo.toml` 的 `[dependencies]`：

```toml
lazy_static = "1"
```

- [ ] 修改 `rust-ody/crates/kosong-rs/src/providers/kimi.rs`，为 `KimiChatProvider` 增加文件入口：

```rust
use crate::kimi_files::{KimiFiles, KimiFilesOptions, KimiUploadOptions, KimiVideoUpload};

impl KimiChatProvider {
    // ... 已有 new / generate trait 方法 ...

    pub fn files(&self) -> KimiFiles {
        KimiFiles::new(KimiFilesOptions {
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            default_headers: self.default_headers.clone(),
            http_client: Some(Arc::clone(&self.http_client)),
        })
    }

    pub async fn upload_video(
        &self,
        input: KimiVideoUpload,
        options: Option<KimiUploadOptions>,
    ) -> Result<ContentPart, ChatProviderError> {
        self.files().upload_video(input, options).await
    }
}
```

- [ ] 修改 `lib.rs`：

```rust
pub mod kimi_files;
pub use providers::kimi::{KimiChatProvider, KimiOptions};
pub use kimi_files::{KimiFiles, KimiFilesOptions, KimiUploadOptions, KimiVideoUpload, VideoUploadInput};
```

- [ ] 运行 `kimi_files` 测试：

```bash
cd rust-ody && cargo test -p kosong-rs kimi_files::tests
```

预期：全部通过。

- [ ] 运行整个 Rust workspace typecheck（共享签名变更后的必须步骤）：

```bash
cd rust-ody && cargo check --workspace
```

预期：无编译错误。

- [ ] Commit: `feat(kosong-rs): add KimiFiles.uploadVideo with multipart support`

---

## Part 2 Self-Review

- [ ] 1. Spec-coverage table:
  | 路线图/TS 行为 | Task | 状态 |
  |---|---|---|
  | KimiChatProvider 构造（api_key / base_url / default_headers / stream 默认值） | 2.1 | covered |
  | name/model/get_capability | 2.1 | covered |
  | `with_thinking` 映射到 `reasoning_effort` + `extra_body.thinking` | 2.1 + 2.2 | covered |
  | `with_max_completion_tokens` | 2.1 + 2.2 | covered |
  | `convert_message`：assistant tool_calls 存在且 content 为空时省略 `content` | 2.2 | covered |
  | `convert_message`：将 `ContentPart::Think` 提取为 `reasoning_content` | 2.2 | covered |
  | `convert_tool`：`$` 前缀 -> `builtin_function` | 2.2 | covered |
  | `convert_tool`：普通工具参数走 `normalize_kimi_tool_schema` | 2.2 | covered |
  | 请求体：`max_tokens` 归一化为 `max_completion_tokens` | 2.2 | covered |
  | 请求体：`extra_body` 平铺、`stream_options.include_usage` | 2.2 | covered |
  | 流式 `generate`：解析 `reasoning_content`、text、tool_call | 2.3 | covered |
  | 流式 `generate`：读取 `choices[0].usage` | 2.3 | covered |
  | 非流式 `generate` | 2.3 | covered |
  | 缺失 API key 报错 | 2.3 | covered |
  | `KimiFiles.uploadVideo`：bytes / path、MIME 与扩展名校验、multipart、`ms://<id>` | 2.4 | covered |
  | 上传 `signal` 取消 | — | no-op（当前 TS 实现也依赖 OpenAI SDK 的 signal；Rust 端先保持等价的最小可用，不阻塞核心迁移） |
- [ ] 2. Placeholder scan: 无 `TODO`/`TBD`；Task 2.1 中 `generate` 的 `unimplemented!()` 是阶段占位，Task 2.3 已给出完整替换实现，不是未完成的 placeholder。
- [ ] 3. No phantom tasks: 每个 Task 都包含完整代码、测试命令、预期、commit message；没有 `--allow-empty`。
- [ ] 4. Dependency soundness: Task 2.1 不引用 Task 2.4 的 `KimiFiles`；Task 2.2 依赖 2.1；Task 2.3 依赖 2.2 与 Part 1 Task 2；Task 2.4 依赖 2.1。所有 `Depends on:` 均指向前序 Task。
- [ ] 5. Caller & build soundness: Task 2.4 修改 `HttpClient` trait 时同步更新了 `ReqwestClient`、`MockHttpClient`、以及 `kimi.rs` 测试里的 `CaptureJsonClient`；通过 `grep -rn "impl HttpClient for"` / `grep -rn "post_json("` 确认无遗漏；Task 2.4 以 `cargo check --workspace` 结尾。
- [ ] 6. Test-the-risk: 请求体形状、usage 提取、multipart 字段、MIME/扩展名过滤均有行为断言；测试常量与实现常量（`video/`、`ms://`、`purpose=video`、`max_tokens` -> `max_completion_tokens`）一一对应。
- [ ] 7. Type consistency: `KimiChatProvider` 使用 Part 1 的 `normalize_kimi_tool_schema` 与 `get_kimi_model_capability`，返回类型与 `ChatProvider` trait 一致；`KimiFiles` 返回 `ContentPart::VideoUrl`，结构与 `message.rs` 一致。

