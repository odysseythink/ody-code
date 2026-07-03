# Part 1: Provider Model & Request Construction

本 Part 在 `kosong-rs/src/openai_responses.rs` 中落地 `OpenAIResponsesChatProvider` 的构造、ChatProvider trait 基础实现、消息/工具转换与请求体组装，并通过 `generate()` 把请求发出去。响应解析留到 Part 2。

---

## Dependency Overview (Part 1)

```text
Task 1: OpenAIResponsesOptions + 构造 + ChatProvider 基础实现
  │
  ├──► Task 2: Message → Response input item 转换
  │      │
  │      ├──► Task 3: Tool 转换 + request body 组装
  │      │       │
  │      │       └──► Task 4: generate() 网络调用骨架
```

- Task 1 不依赖后续任务；只暴露接口桩（消息转换函数先返回空或最小实现）。
- Task 2 依赖 Task 1 的 `OpenAIResponsesChatProvider` 与 `convert_message` 入口。
- Task 3 依赖 Task 2 的消息转换结果类型（`ResponseInputItem`）。
- Task 4 依赖 Task 3 的请求体构造逻辑，使用 `http_client::HttpClient` 发出请求。

---

### Task 1: OpenAIResponsesOptions 与 ChatProvider 基础实现

**Depends on:** none（复用 4.2.0/4.2.1/4.2.2 已落地模块）

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/openai_responses.rs:1-120`（初始骨架）
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs` 加入 `pub mod openai_responses;` 与 re-export
- Test: `rust-ody/crates/kosong-rs/src/openai_responses.rs` 内 `#[cfg(test)]`

步骤：

- [ ] **写失败测试**：在 `openai_responses.rs` 末尾加入：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_exposes_name_model_and_capability() {
        let provider = OpenAIResponsesChatProvider::new(OpenAIResponsesOptions {
            api_key: Some("sk-test".into()),
            base_url: None,
            model: "gpt-4o-mini".into(),
            max_output_tokens: None,
            default_headers: None,
            tool_message_conversion: None,
            http_client: None,
        });
        assert_eq!(provider.name(), "openai-responses");
        assert_eq!(provider.model_name(), "gpt-4o-mini");
        let cap = provider.get_capability(None);
        assert!(cap.tool_use);
        assert!(cap.image_in);
        assert!(!cap.thinking);
    }

    #[test]
    fn thinking_effort_round_trips_via_reasoning_effort() {
        let provider = OpenAIResponsesChatProvider::new(OpenAIResponsesOptions {
            api_key: Some("sk-test".into()),
            base_url: None,
            model: "o3-mini".into(),
            max_output_tokens: None,
            default_headers: None,
            tool_message_conversion: None,
            http_client: None,
        })
        .with_thinking(ThinkingEffort::High);
        assert_eq!(provider.thinking_effort(), Some(ThinkingEffort::High));
    }

    #[test]
    fn with_max_completion_tokens_sets_max_output_tokens() {
        let provider = OpenAIResponsesChatProvider::new(OpenAIResponsesOptions {
            api_key: Some("sk-test".into()),
            base_url: None,
            model: "gpt-4o-mini".into(),
            max_output_tokens: None,
            default_headers: None,
            tool_message_conversion: None,
            http_client: None,
        })
        .with_max_completion_tokens(123);
        // 通过 generate() 请求体断言更直接；这里先保证 clone 后 modelName 不变。
        assert_eq!(provider.model_name(), "gpt-4o-mini");
    }
}
```

- [ ] **运行并确认失败**：

```bash
cd rust-ody && cargo test -p kosong-rs openai_responses::tests
```

预期失败：`error[E0433]: failed to resolve: use of undeclared crate or module "openai_responses"` 或类似未找到符号错误。

- [ ] **写最小实现**：新建 `rust-ody/crates/kosong-rs/src/openai_responses.rs`：

```rust
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;

use crate::errors::{APIMissingApiKeyError, ChatProviderError};
use crate::http_client::{HttpClient, ReqwestClient};
use crate::message::Message;
use crate::openai_common::{
    reasoning_effort_to_thinking_effort, thinking_effort_to_reasoning_effort,
    ToolMessageConversion,
};
use crate::provider::{
    ChatProvider, GenerateOptions, ModelCapability, ThinkingEffort, Tool,
};
use crate::tool_call_id::{
    normalize_tool_call_ids_for_provider, sanitize_openai_responses_call_id, ToolCallIdPolicy,
};
use crate::{capability_registry, generate::StreamedMessage};

#[derive(Clone)]
pub struct OpenAIResponsesOptions {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: String,
    pub max_output_tokens: Option<i64>,
    pub default_headers: Option<HashMap<String, String>>,
    pub tool_message_conversion: Option<ToolMessageConversion>,
    pub http_client: Option<Arc<dyn HttpClient>>,
}

impl std::fmt::Debug for OpenAIResponsesOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenAIResponsesOptions")
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

pub struct OpenAIResponsesChatProvider {
    model: String,
    api_key: Option<String>,
    base_url: String,
    default_headers: Option<HashMap<String, String>>,
    generation_kwargs: HashMap<String, Value>,
    tool_message_conversion: Option<ToolMessageConversion>,
    http_client: Arc<dyn HttpClient>,
}

impl OpenAIResponsesChatProvider {
    pub fn new(options: OpenAIResponsesOptions) -> Self {
        let api_key = options
            .api_key
            .or_else(|| std::env::var("OPENAI_API_KEY").ok())
            .filter(|k| !k.is_empty());
        let base_url = options
            .base_url
            .unwrap_or_else(|| "https://api.openai.com/v1".into());
        let mut generation_kwargs = HashMap::new();
        if let Some(max) = options.max_output_tokens {
            generation_kwargs.insert("max_output_tokens".into(), Value::Number(max.into()));
        }
        let http_client = options
            .http_client
            .unwrap_or_else(|| Arc::new(ReqwestClient::new(reqwest::Client::new())));
        Self {
            model: options.model,
            api_key,
            base_url,
            default_headers: options.default_headers,
            generation_kwargs,
            tool_message_conversion: options.tool_message_conversion,
            http_client,
        }
    }
}

impl Clone for OpenAIResponsesChatProvider {
    fn clone(&self) -> Self {
        Self {
            model: self.model.clone(),
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            default_headers: self.default_headers.clone(),
            generation_kwargs: self.generation_kwargs.clone(),
            tool_message_conversion: self.tool_message_conversion,
            http_client: Arc::clone(&self.http_client),
        }
    }
}

fn openai_responses_tool_call_id_policy() -> ToolCallIdPolicy {
    ToolCallIdPolicy::new(
        |id| sanitize_openai_responses_call_id(id, Some(64)),
        Some(64),
    )
}

#[async_trait]
impl ChatProvider for OpenAIResponsesChatProvider {
    fn name(&self) -> &str {
        "openai-responses"
    }

    fn model_name(&self) -> &str {
        &self.model
    }

    fn thinking_effort(&self) -> Option<ThinkingEffort> {
        self.generation_kwargs
            .get("reasoning_effort")
            .and_then(|v| v.as_str())
            .and_then(reasoning_effort_to_thinking_effort)
    }

    fn get_capability(&self, model: Option<&str>) -> ModelCapability {
        capability_registry::get_openai_responses_model_capability(model.unwrap_or(&self.model))
    }

    async fn generate(
        &self,
        _system_prompt: &str,
        _tools: &[Tool],
        _history: &[Message],
        _options: Option<GenerateOptions>,
    ) -> Result<StreamedMessage, ChatProviderError> {
        // Task 4 实现。
        todo!("generate() implemented in Task 4")
    }

    fn with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider> {
        let mut clone = self.clone();
        let reasoning_effort = thinking_effort_to_reasoning_effort(effort);
        if let Some(re) = reasoning_effort {
            clone.generation_kwargs.insert("reasoning_effort".into(), Value::String(re));
        } else {
            clone.generation_kwargs.remove("reasoning_effort");
        }
        Box::new(clone)
    }

    fn with_max_completion_tokens(&self, max_completion_tokens: i64) -> Option<Box<dyn ChatProvider>> {
        let mut clone = self.clone();
        clone
            .generation_kwargs
            .insert("max_output_tokens".into(), Value::Number(max_completion_tokens.into()));
        Some(Box::new(clone))
    }
}
```

并在 `rust-ody/crates/kosong-rs/src/lib.rs` 增加：

```rust
pub mod openai_responses;
```

以及 re-export：

```rust
pub use openai_responses::{OpenAIResponsesChatProvider, OpenAIResponsesOptions};
```

- [ ] **运行并确认通过**：

```bash
cd rust-ody && cargo test -p kosong-rs openai_responses::tests
```

预期：3 个测试通过。

- [ ] **Commit**：`feat(kosong-rs): scaffold OpenAIResponsesChatProvider`

---

### Task 2: Message → Response input item 转换

**Depends on:** Task 1

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/openai_responses.rs`（加入转换函数与类型）
- Test: `rust-ody/crates/kosong-rs/src/openai_responses.rs` 内 `#[cfg(test)]`

步骤：

- [ ] **写失败测试**：在 Task 1 的 `tests` 模块中加入：

```rust
    use crate::message::{ContentPart, Role, UrlPayload};

    #[test]
    fn convert_user_text_message() {
        let msg = Message::user_text("Hello");
        let items = convert_message(&msg, "gpt-4o-mini", None);
        assert_eq!(items.len(), 1);
        let v = serde_json::to_value(&items[0]).unwrap();
        assert_eq!(v["type"], "message");
        assert_eq!(v["role"], "user");
        assert_eq!(v["content"][0]["type"], "input_text");
        assert_eq!(v["content"][0]["text"], "Hello");
    }

    #[test]
    fn convert_system_to_developer_for_reasoning_model() {
        let msg = Message {
            role: Role::System,
            name: None,
            content: vec![ContentPart::Text { text: "sys".into() }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let items = convert_message(&msg, "o3-mini", None);
        assert_eq!(items.len(), 1);
        let v = serde_json::to_value(&items[0]).unwrap();
        assert_eq!(v["role"], "developer");
    }

    #[test]
    fn convert_assistant_with_tool_call() {
        let msg = Message {
            role: Role::Assistant,
            name: None,
            content: vec![],
            tool_calls: vec![crate::message::ToolCall {
                call_type: "function".into(),
                id: "call_1".into(),
                name: "read".into(),
                arguments: Some("{}".into()),
                extras: None,
                stream_index: None,
            }],
            tool_call_id: None,
            partial: None,
        };
        let items = convert_message(&msg, "gpt-4o-mini", None);
        assert_eq!(items.len(), 1);
        let v = serde_json::to_value(&items[0]).unwrap();
        assert_eq!(v["type"], "function_call");
        assert_eq!(v["call_id"], "call_1");
        assert_eq!(v["name"], "read");
        assert_eq!(v["arguments"], "{}");
    }

    #[test]
    fn convert_tool_result_with_extract_text() {
        let msg = Message {
            role: Role::Tool,
            name: None,
            content: vec![
                ContentPart::Text { text: "a".into() },
                ContentPart::Text { text: "b".into() },
            ],
            tool_calls: vec![],
            tool_call_id: Some("call_1".into()),
            partial: None,
        };
        let items = convert_message(&msg, "gpt-4o-mini", Some(ToolMessageConversion::ExtractText));
        assert_eq!(items.len(), 1);
        let v = serde_json::to_value(&items[0]).unwrap();
        assert_eq!(v["type"], "function_call_output");
        assert_eq!(v["output"], "ab");
    }

    #[test]
    fn convert_user_image_url() {
        let msg = Message {
            role: Role::User,
            name: None,
            content: vec![ContentPart::ImageUrl {
                image_url: UrlPayload {
                    url: "https://example.com/img.png".into(),
                    id: None,
                },
            }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let items = convert_message(&msg, "gpt-4o-mini", None);
        let v = serde_json::to_value(&items[0]).unwrap();
        assert_eq!(v["content"][0]["type"], "input_image");
        assert_eq!(v["content"][0]["image_url"], "https://example.com/img.png");
    }

    #[test]
    fn convert_think_parts_to_reasoning_item() {
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
        let items = convert_message(&msg, "gpt-4o-mini", None);
        assert_eq!(items.len(), 1);
        let v = serde_json::to_value(&items[0]).unwrap();
        assert_eq!(v["type"], "reasoning");
        assert_eq!(v["summary"][0]["text"], "step1");
        assert_eq!(v["summary"][1]["text"], "step2");
    }
```

- [ ] **运行并确认失败**：

```bash
cd rust-ody && cargo test -p kosong-rs openai_responses::tests::convert
```

预期失败：未找到 `convert_message` 等函数。

- [ ] **写最小实现**：在 `openai_responses.rs` 中 `OpenAIResponsesChatProvider` 之前加入：

```rust
use crate::message::{ContentPart, Message, Role, UrlPayload};
use crate::openai_common::convert_tool_message_content;

#[derive(Debug, Clone, Serialize)]
struct ResponseInputItem {
    #[serde(rename = "type")]
    item_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "call_id")]
    call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "encrypted_content")]
    encrypted_content: Option<String>,
}

fn content_parts_to_input_items(parts: &[ContentPart]) -> Vec<Value> {
    let mut items = Vec::new();
    for part in parts {
        match part {
            ContentPart::Text { text } if !text.is_empty() => {
                items.push(serde_json::json!({"type": "input_text", "text": text}));
            }
            ContentPart::ImageUrl { image_url } => {
                items.push(serde_json::json!({"type": "input_image", "detail": "auto", "image_url": image_url.url}));
            }
            ContentPart::AudioUrl { audio_url } => {
                if let Some(mapped) = map_audio_url_to_input_item(&audio_url.url) {
                    items.push(mapped);
                }
            }
            ContentPart::Think { .. } | ContentPart::VideoUrl { .. } | ContentPart::Text { .. } => {}
        }
    }
    items
}

fn content_parts_to_output_items(parts: &[ContentPart]) -> Vec<Value> {
    let mut items = Vec::new();
    for part in parts {
        if let ContentPart::Text { text } = part {
            if !text.is_empty() {
                items.push(serde_json::json!({"type": "output_text", "text": text, "annotations": []}));
            }
        }
    }
    items
}

fn message_content_to_function_output_items(content: &[ContentPart]) -> Vec<Value> {
    let mut items = Vec::new();
    for part in content {
        match part {
            ContentPart::Text { text } if !text.is_empty() => {
                items.push(serde_json::json!({"type": "input_text", "text": text}));
            }
            ContentPart::ImageUrl { image_url } => {
                items.push(serde_json::json!({"type": "input_image", "image_url": image_url.url}));
            }
            ContentPart::AudioUrl { audio_url } => {
                if let Some(mapped) = map_audio_url_to_input_item(&audio_url.url) {
                    items.push(mapped);
                }
            }
            ContentPart::Think { .. } | ContentPart::VideoUrl { .. } => {}
        }
    }
    items
}

fn map_audio_url_to_input_item(url: &str) -> Option<Value> {
    if url.starts_with("data:audio/") {
        let parts: Vec<&str> = url.splitn(2, ',').collect();
        if parts.len() != 2 {
            return None;
        }
        let header = parts[0];
        let b64 = parts[1];
        let subtype_part = header.split('/').nth(1)?;
        let subtype_head = subtype_part.split(';').next()?;
        let subtype = subtype_head.to_lowercase();
        let ext = if subtype == "mp3" || subtype == "mpeg" {
            "mp3"
        } else if subtype == "wav" {
            "wav"
        } else {
            return None;
        };
        Some(serde_json::json!({"type": "input_file", "file_data": b64, "filename": format!("inline.{}", ext)}))
    } else if url.starts_with("http://") || url.starts_with("https://") {
        Some(serde_json::json!({"type": "input_file", "file_url": url}))
    } else {
        None
    }
}

fn convert_message(
    message: &Message,
    model_name: &str,
    tool_message_conversion: Option<ToolMessageConversion>,
) -> Vec<ResponseInputItem> {
    let mut role = format!("{:?}", message.role).to_lowercase();
    if role == "system" && capability_registry::uses_openai_responses_developer_role(model_name) {
        role = "developer".into();
    }

    if role == "tool" {
        let call_id = message.tool_call_id.clone().unwrap_or_default();
        let output = if tool_message_conversion == Some(ToolMessageConversion::ExtractText) {
            Some(Value::String(crate::message::extract_text(message, "")))
        } else {
            let items = message_content_to_function_output_items(&message.content);
            if items.is_empty() {
                None
            } else {
                Some(Value::Array(items))
            }
        };
        return vec![ResponseInputItem {
            item_type: "function_call_output".into(),
            role: None,
            content: None,
            call_id: Some(call_id),
            output,
            name: None,
            arguments: None,
            summary: None,
            encrypted_content: None,
        }];
    }

    let mut result = Vec::new();
    if !message.content.is_empty() {
        let mut pending_parts: Vec<ContentPart> = Vec::new();
        let mut flush = |parts: &mut Vec<ContentPart>| {
            if parts.is_empty() {
                return;
            }
            let content = if role == "assistant" {
                content_parts_to_output_items(parts)
            } else {
                content_parts_to_input_items(parts)
            };
            if !content.is_empty() {
                result.push(ResponseInputItem {
                    item_type: "message".into(),
                    role: Some(role.clone()),
                    content: Some(Value::Array(content)),
                    call_id: None,
                    output: None,
                    name: None,
                    arguments: None,
                    summary: None,
                    encrypted_content: None,
                });
            }
            parts.clear();
        };

        let mut i = 0;
        while i < message.content.len() {
            let part = &message.content[i];
            if let ContentPart::Think { think, encrypted } = part {
                flush(&mut pending_parts);
                let encrypted_value = encrypted.clone();
                let mut summaries = vec![serde_json::json!({"type": "summary_text", "text": think.as_str()})];
                i += 1;
                while i < message.content.len() {
                    if let ContentPart::Think { think: t, encrypted: e } = &message.content[i] {
                        if e == &encrypted_value {
                            summaries.push(serde_json::json!({"type": "summary_text", "text": t.as_str()}));
                            i += 1;
                            continue;
                        }
                    }
                    break;
                }
                result.push(ResponseInputItem {
                    item_type: "reasoning".into(),
                    role: None,
                    content: None,
                    call_id: None,
                    output: None,
                    name: None,
                    arguments: None,
                    summary: Some(Value::Array(summaries)),
                    encrypted_content: encrypted_value,
                });
            } else {
                pending_parts.push(part.clone());
                i += 1;
            }
        }
        flush(&mut pending_parts);
    }

    for tc in &message.tool_calls {
        result.push(ResponseInputItem {
            item_type: "function_call".into(),
            role: None,
            content: None,
            call_id: Some(tc.id.clone()),
            output: None,
            name: Some(tc.name.clone()),
            arguments: Some(tc.arguments.clone().unwrap_or_else(|| "{}".into())),
            summary: None,
            encrypted_content: None,
        });
    }

    result
}
```

注意：此实现中 `convert_tool_message_content` 在 `tool_message_conversion == None` 且内容全为文本时会返回文本字符串；若含多模态则返回数组。为了保持与 TS 对齐，当 `tool_message_conversion != ExtractText` 时应优先调用 `convert_tool_message_content` 并用其结果；上述实现直接构造了数组/字符串。更精确的最小实现应为：

```rust
let output = if tool_message_conversion == Some(ToolMessageConversion::ExtractText) {
    Some(Value::String(crate::message::extract_text(message, "")))
} else {
    convert_tool_message_content(message, tool_message_conversion.unwrap_or(ToolMessageConversion::Standard))
};
```

其中 `convert_tool_message_content` 已处理"多模态 → extract_text"的回落逻辑。请按此替换 Task 2 中 tool role 分支的 `output` 赋值。

- [ ] **运行并确认通过**：

```bash
cd rust-ody && cargo test -p kosong-rs openai_responses::tests::convert
```

预期：6 个测试通过。

- [ ] **Commit**：`feat(kosong-rs): OpenAI Responses message conversion`

---

### Task 3: Tool 转换与 Request Body 组装

**Depends on:** Task 2

**Files：**
- Modify: `rust-ody/crates/kosong-rs/src/openai_responses.rs`（加入 `convert_tool`、`build_request_body`）
- Test: `rust-ody/crates/kosong-rs/src/openai_responses.rs` 内 `#[cfg(test)]`

步骤：

- [ ] **写失败测试**：在 `tests` 模块中加入：

```rust
    #[test]
    fn convert_tool_to_response_function_param() {
        let tool = Tool {
            name: "add".into(),
            description: "Add two integers.".into(),
            parameters: serde_json::json!({"type": "object", "properties": {"a": {"type": "integer"}}}),
        };
        let v = serde_json::to_value(convert_tool(&tool)).unwrap();
        assert_eq!(v["type"], "function");
        assert_eq!(v["name"], "add");
        assert_eq!(v["description"], "Add two integers.");
        assert_eq!(v["strict"], false);
    }

    #[test]
    fn build_request_body_includes_model_input_tools_and_reasoning() {
        let provider = OpenAIResponsesChatProvider::new(OpenAIResponsesOptions {
            api_key: Some("sk-test".into()),
            base_url: None,
            model: "o3-mini".into(),
            max_output_tokens: Some(100),
            default_headers: None,
            tool_message_conversion: None,
            http_client: None,
        })
        .with_thinking(ThinkingEffort::High);

        let tool = Tool {
            name: "add".into(),
            description: "Add.".into(),
            parameters: serde_json::json!({"type": "object"}),
        };
        let history = vec![Message::user_text("1+1")];
        let body = provider.build_request_body("sys", &[tool], &history).unwrap();

        assert_eq!(body["model"], "o3-mini");
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert!(body["input"].as_array().unwrap().len() >= 2); // system + user
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["reasoning"]["effort"], "high");
        assert_eq!(body["reasoning"]["summary"], "auto");
        assert!(body["include"].as_array().unwrap().contains(&serde_json::json!("reasoning.encrypted_content")));
        assert_eq!(body["max_output_tokens"], 100);
    }

    #[test]
    fn build_request_body_uses_developer_role_for_reasoning_model() {
        let provider = OpenAIResponsesChatProvider::new(OpenAIResponsesOptions {
            api_key: Some("sk-test".into()),
            base_url: None,
            model: "o3-mini".into(),
            max_output_tokens: None,
            default_headers: None,
            tool_message_conversion: None,
            http_client: None,
        });
        let body = provider.build_request_body("sys", &[], &[]).unwrap();
        let input = body["input"].as_array().unwrap();
        assert_eq!(input[0]["role"], "developer");
    }
```

- [ ] **运行并确认失败**：

```bash
cd rust-ody && cargo test -p kosong-rs openai_responses::tests::build_request_body
```

预期失败：未找到 `build_request_body` 方法。

- [ ] **写最小实现**：在 `openai_responses.rs` 中 `OpenAIResponsesChatProvider` impl 内加入：

```rust
#[derive(Debug, Clone, Serialize)]
struct ResponseToolParam {
    #[serde(rename = "type")]
    tool_type: String,
    name: String,
    description: String,
    parameters: Value,
    strict: bool,
}

fn convert_tool(tool: &Tool) -> ResponseToolParam {
    ResponseToolParam {
        tool_type: "function".into(),
        name: tool.name.clone(),
        description: tool.description.clone(),
        parameters: tool.parameters.clone(),
        strict: false,
    }
}

impl OpenAIResponsesChatProvider {
    fn build_request_body(
        &self,
        system_prompt: &str,
        tools: &[Tool],
        history: &[Message],
    ) -> Result<Value, ChatProviderError> {
        let mut input: Vec<Value> = Vec::new();
        if !system_prompt.is_empty() {
            let role = if capability_registry::uses_openai_responses_developer_role(&self.model) {
                "developer"
            } else {
                "system"
            };
            input.push(serde_json::json!({"role": role, "content": system_prompt}));
        }

        let normalized_history =
            normalize_tool_call_ids_for_provider(history, &openai_responses_tool_call_id_policy());
        for msg in &normalized_history {
            for item in convert_message(msg, &self.model, self.tool_message_conversion) {
                input.push(serde_json::to_value(item).unwrap());
            }
        }

        let mut kwargs = self.generation_kwargs.clone();
        let reasoning_effort = kwargs.remove("reasoning_effort").and_then(|v| v.as_str().map(String::from));
        if let Some(effort) = reasoning_effort {
            kwargs.insert(
                "reasoning".into(),
                serde_json::json!({"effort": effort, "summary": "auto"}),
            );
            kwargs.insert(
                "include".into(),
                Value::Array(vec![Value::String("reasoning.encrypted_content".into())]),
            );
        }

        let mut body = serde_json::Map::new();
        body.insert("model".into(), Value::String(self.model.clone()));
        body.insert("input".into(), Value::Array(input));
        body.insert("store".into(), Value::Bool(false));
        body.insert("stream".into(), Value::Bool(true));
        for (k, v) in kwargs {
            if !v.is_null() {
                body.insert(k, v);
            }
        }
        if !tools.is_empty() {
            body.insert(
                "tools".into(),
                Value::Array(tools.iter().map(|t| serde_json::to_value(convert_tool(t)).unwrap()).collect()),
            );
        }
        Ok(Value::Object(body))
    }
}
```

- [ ] **运行并确认通过**：

```bash
cd rust-ody && cargo test -p kosong-rs openai_responses::tests::build_request_body
```

预期：3 个测试通过。

- [ ] **Commit**：`feat(kosong-rs): OpenAI Responses request body assembly`

---

### Task 4: generate() 网络调用骨架

**Depends on:** Task 3

**Files：**
- Modify: `rust-ody/crates/kosong-rs/src/openai_responses.rs`（替换 `generate()` 的 `todo!()`）
- Test: `rust-ody/crates/kosong-rs/src/openai_responses.rs` 内 `#[cfg(test)]`

步骤：

- [ ] **写失败测试**：在 `tests` 模块中加入一个记录请求的内部 mock 与测试：

```rust
    use std::sync::{Arc, Mutex};
    use crate::http_client::{HttpClient, HttpResponse};
    use crate::provider::ProviderRequestAuth;
    use serde_json::Value;

    struct RecordingMockHttpClient {
        status: u16,
        body: Vec<u8>,
        requests: Arc<Mutex<Vec<(String, HashMap<String, String>, Value)>>>,
    }

    #[async_trait]
    impl HttpClient for RecordingMockHttpClient {
        async fn post_json(
            &self,
            url: &str,
            headers: HashMap<String, String>,
            body: Value,
        ) -> Result<HttpResponse, ChatProviderError> {
            self.requests.lock().unwrap().push((url.to_string(), headers, body));
            let chunks = vec![self.body.clone()];
            let stream = futures_util::stream::iter(
                chunks.into_iter().map(|c| Ok(bytes::Bytes::from(c))),
            )
            .boxed();
            Ok(HttpResponse::new(self.status, stream))
        }
    }

    #[tokio::test]
    async fn generate_posts_to_responses_endpoint() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let client = Arc::new(RecordingMockHttpClient {
            status: 200,
            body: br#"{"id":"resp-1","status":"completed","output":[],"usage":{"input_tokens":1,"output_tokens":1}}"#.to_vec(),
            requests: Arc::clone(&requests),
        });
        let provider = OpenAIResponsesChatProvider::new(OpenAIResponsesOptions {
            api_key: Some("sk-test".into()),
            base_url: Some("http://mock".into()),
            model: "gpt-4o-mini".into(),
            max_output_tokens: None,
            default_headers: None,
            tool_message_conversion: None,
            http_client: Some(client),
        });
        let _ = provider.generate("sys", &[], &[Message::user_text("hi")], None).await.unwrap();

        let reqs = requests.lock().unwrap();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].0, "http://mock/responses");
        assert_eq!(reqs[0].1["Authorization"], "Bearer sk-test");
        assert_eq!(reqs[0].2["model"], "gpt-4o-mini");
        assert_eq!(reqs[0].2["stream"], true);
    }

    #[tokio::test]
    async fn generate_rejects_missing_api_key() {
        let provider = OpenAIResponsesChatProvider::new(OpenAIResponsesOptions {
            api_key: None,
            base_url: Some("http://mock".into()),
            model: "gpt-4o-mini".into(),
            max_output_tokens: None,
            default_headers: None,
            tool_message_conversion: None,
            http_client: Some(Arc::new(crate::http_client::MockHttpClient::new(200, vec![]))),
        });
        let err = provider.generate("", &[], &[], None).await.unwrap_err();
        assert!(matches!(err, ChatProviderError::MissingApiKey(_)));
    }

    #[tokio::test]
    async fn generate_prefers_request_auth_over_default() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let client = Arc::new(RecordingMockHttpClient {
            status: 200,
            body: br#"{"id":"resp-1","status":"completed","output":[],"usage":{"input_tokens":1,"output_tokens":1}}"#.to_vec(),
            requests: Arc::clone(&requests),
        });
        let provider = OpenAIResponsesChatProvider::new(OpenAIResponsesOptions {
            api_key: Some("sk-default".into()),
            base_url: Some("http://mock".into()),
            model: "gpt-4o-mini".into(),
            max_output_tokens: None,
            default_headers: None,
            tool_message_conversion: None,
            http_client: Some(client),
        });
        let auth = ProviderRequestAuth {
            api_key: Some("sk-req".into()),
            headers: None,
        };
        let options = GenerateOptions {
            auth: Some(auth),
            ..Default::default()
        };
        let _ = provider.generate("", &[], &[Message::user_text("hi")], Some(options)).await.unwrap();
        let reqs = requests.lock().unwrap();
        assert_eq!(reqs[0].1["Authorization"], "Bearer sk-req");
    }
```

- [ ] **运行并确认失败**：

```bash
cd rust-ody && cargo test -p kosong-rs openai_responses::tests::generate
```

预期失败：`generate()` 仍为 `todo!()`，测试 panic 或编译失败（因 `generate()` 未正确处理请求体）。

- [ ] **写最小实现**：替换 `ChatProvider::generate` 实现为：

```rust
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
            .ok_or_else(|| ChatProviderError::MissingApiKey(APIMissingApiKeyError {
                provider: self.name().to_string(),
            }))?;

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

        let body = self.build_request_body(system_prompt, tools, history)?;
        let url = format!("{}/responses", self.base_url.trim_end_matches('/'));
        let response = self.http_client.post_json(&url, headers, body).await?;
        let status = response.status();
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            bytes.extend_from_slice(chunk?.as_ref());
        }

        if status < 200 || status >= 300 {
            let body_str = String::from_utf8_lossy(&bytes);
            let (msg, code) = parse_error_body(&body_str);
            return Err(convert_openai_error(&msg, Some(status), code.as_deref()));
        }

        // Task 4 仅验证请求能发出去；响应解析由 Part 2 完整实现。
        // 这里返回空 StreamedMessage，使编译与测试通过。
        Ok(StreamedMessage::from_parts(
            vec![],
            Some("resp-stub".into()),
            None,
            None,
            None,
        ))
    }
```

并在文件底部加入辅助函数：

```rust
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

> 注：Part 2 会用真正的 `OpenAIResponsesStreamedMessage` 替换 `from_parts(vec![], ...)` 的桩，届时本 Task 的 `generate_posts_to_responses_endpoint` 等测试仍可通过（只验证请求），响应解析由新增 fixture 测试覆盖。

- [ ] **运行并确认通过**：

```bash
cd rust-ody && cargo test -p kosong-rs openai_responses::tests::generate
```

预期：3 个测试通过。

- [ ] **运行 crate 级测试保证无回归**：

```bash
cd rust-ody && cargo test -p kosong-rs
```

预期：所有已有测试通过。

- [ ] **Commit**：`feat(kosong-rs): OpenAI Responses generate() request dispatch`

---

## Local Self-Review

- [x] 1. Spec-coverage table（索引中）：Part 1 覆盖 4.2.3.1、4.2.3.4 与 4.2.3.5 的请求侧。
- [x] 2. Placeholder scan：Task 4 的 `from_parts(vec![], ...)` 是显式桩，Part 2 Task 5 会替换；其余无 TODO/TBD。
- [x] 3. No phantom tasks：Task 1–4 均有 Files + 测试 + commit。
- [x] 4. Dependency soundness：Task 1 → Task 2 → Task 3 → Task 4，无向后依赖。
- [x] 5. Caller & build soundness：本 Part 新增 `openai_responses` 模块并在 `lib.rs` 导出，不修改既有共享签名；Task 1 的 re-export 在同一任务完成；Task 4 以 crate 级 `cargo test -p kosong-rs` 结束。
- [x] 6. Test-the-risk：请求构造（model/role/tools/reasoning/auth）均有行为断言；auth precedence 用 recording mock 直接断言 header。
- [x] 7. Type consistency：复用 4.2.0/4.2.1/4.2.2 已定义类型（`Message`、`Tool`、`ToolMessageConversion`、`HttpClient`、`GenerateOptions` 等），新增内部类型字段与 TS 对齐。
