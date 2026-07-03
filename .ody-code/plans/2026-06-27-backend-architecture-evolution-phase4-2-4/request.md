# Part 2: Request construction

本部分完成 Anthropic provider 的**请求侧**：内容块类型、消息/工具转换、并行 tool_result 合并、`cache_control` 注入、请求体与鉴权头组装，以及非流式 `generate()` 端到端路径。流式 SSE 解析留在 Part 3。

---

### Task 1: Anthropic 请求内容块类型与转换辅助函数

**Depends on:** Part 1 (`model.md`) Task 4

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/anthropic.rs:431-750`

**实现步骤：**

- [ ] 先写测试，覆盖 TS `convertMessage` / `toolResultToBlock` / `imageUrlPartToAnthropic` / `convertTool` 的核心行为：

```rust
#[cfg(test)]
mod request_tests {
    use super::*;
    use crate::message::{ContentPart, Message, Role, ToolCall, UrlPayload};
    use crate::provider::Tool;

    #[test]
    fn convert_tool_uses_input_schema() {
        let tool = Tool {
            name: "read".into(),
            description: "read a file".into(),
            parameters: serde_json::json!({"type": "object", "properties": {}}),
        };
        let at = convert_tool(&tool);
        assert_eq!(at.name, "read");
        assert_eq!(at.description, "read a file");
        assert_eq!(at.input_schema, tool.parameters);
        assert!(at.cache_control.is_none());
    }

    #[test]
    fn convert_system_message_wraps_in_user_system_tag() {
        let msg = Message {
            role: Role::System,
            name: None,
            content: vec![ContentPart::Text { text: "be helpful".into() }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let out = convert_message(&msg, "claude-opus-4-7");
        assert_eq!(out.role, "user");
        assert_eq!(out.content.len(), 1);
        match &out.content[0] {
            AnthropicContentBlock::Text { text, .. } => assert_eq!(text, "<system>be helpful</system>"),
            _ => panic!("expected text block"),
        }
    }

    #[test]
    fn convert_tool_message_to_tool_result() {
        let msg = Message {
            role: Role::Tool,
            name: None,
            content: vec![ContentPart::Text { text: "42".into() }],
            tool_calls: vec![],
            tool_call_id: Some("tc_1".into()),
            partial: None,
        };
        let out = convert_message(&msg, "claude-opus-4-7");
        assert_eq!(out.role, "user");
        match &out.content[0] {
            AnthropicContentBlock::ToolResult { tool_use_id, content, .. } => {
                assert_eq!(tool_use_id, "tc_1");
                assert_eq!(content.len(), 1);
                match &content[0] {
                    AnthropicToolResultContent::Text { text } => assert_eq!(text, "42"),
                    _ => panic!("expected text"),
                }
            }
            _ => panic!("expected tool_result"),
        }
    }

    #[test]
    fn convert_tool_message_requires_tool_call_id() {
        let msg = Message {
            role: Role::Tool,
            name: None,
            content: vec![ContentPart::Text { text: "x".into() }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let err = convert_message(&msg, "claude-opus-4-7").unwrap_err();
        assert!(err.to_string().contains("toolCallId"));
    }

    #[test]
    fn convert_image_url_data_to_base64_source() {
        let url = "data:image/png;base64,abcd";
        let block = image_url_part_to_anthropic(url).unwrap();
        match block {
            AnthropicContentBlock::Image { source: ImageSource::Base64 { data, media_type }, .. } => {
                assert_eq!(data, "abcd");
                assert_eq!(media_type, "image/png");
            }
            _ => panic!("expected base64 image"),
        }
    }

    #[test]
    fn convert_image_url_remote_to_url_source() {
        let block = image_url_part_to_anthropic("https://example.com/x.png").unwrap();
        match block {
            AnthropicContentBlock::Image { source: ImageSource::Url { url }, .. } => {
                assert_eq!(url, "https://example.com/x.png");
            }
            _ => panic!("expected url image"),
        }
    }

    #[test]
    fn convert_image_rejects_unsupported_media_type() {
        let err = image_url_part_to_anthropic("data:image/bmp;base64,abcd").unwrap_err();
        assert!(err.to_string().contains("Unsupported media type"));
    }

    #[test]
    fn convert_signed_thinking_preserved() {
        let msg = Message {
            role: Role::Assistant,
            name: None,
            content: vec![ContentPart::Think { think: "reason".into(), encrypted: Some("sig".into()) }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let out = convert_message(&msg, "claude-opus-4-7");
        match &out.content[0] {
            AnthropicContentBlock::Thinking { thinking, signature } => {
                assert_eq!(thinking, "reason");
                assert_eq!(signature.as_deref(), Some("sig"));
            }
            _ => panic!("expected thinking block"),
        }
    }

    #[test]
    fn convert_unsigned_thinking_dropped_for_claude() {
        let msg = Message {
            role: Role::Assistant,
            name: None,
            content: vec![ContentPart::Think { think: "reason".into(), encrypted: None }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let out = convert_message(&msg, "claude-opus-4-7");
        assert!(out.content.is_empty());
    }

    #[test]
    fn convert_unsigned_thinking_preserved_for_non_claude_alias() {
        let msg = Message {
            role: Role::Assistant,
            name: None,
            content: vec![ContentPart::Think { think: "reason".into(), encrypted: None }],
            tool_calls: vec![],
            tool_call_id: None,
            partial: None,
        };
        let out = convert_message(&msg, "custom-compatible-model");
        match &out.content[0] {
            AnthropicContentBlock::Thinking { thinking, signature } => {
                assert_eq!(thinking, "reason");
                assert!(signature.is_none());
            }
            _ => panic!("expected thinking block"),
        }
    }

    #[test]
    fn convert_tool_call_to_tool_use() {
        let tc = ToolCall {
            call_type: "function".into(),
            id: "tc_1".into(),
            name: "read".into(),
            arguments: Some("{\"path\":\"/etc/passwd\"}".into()),
            extras: None,
            stream_index: None,
        };
        let msg = Message {
            role: Role::Assistant,
            name: None,
            content: vec![],
            tool_calls: vec![tc],
            tool_call_id: None,
            partial: None,
        };
        let out = convert_message(&msg, "claude-opus-4-7");
        match &out.content[0] {
            AnthropicContentBlock::ToolUse { id, name, input, .. } => {
                assert_eq!(id, "tc_1");
                assert_eq!(name, "read");
                assert_eq!(input, &serde_json::json!({"path":"/etc/passwd"}));
            }
            _ => panic!("expected tool_use"),
        }
    }

    #[test]
    fn convert_tool_call_rejects_non_object_arguments() {
        let tc = ToolCall {
            call_type: "function".into(),
            id: "tc_1".into(),
            name: "read".into(),
            arguments: Some("\"not-an-object\"".into()),
            extras: None,
            stream_index: None,
        };
        let msg = Message::assistant(vec![], vec![tc]);
        let err = convert_message(&msg, "claude-opus-4-7").unwrap_err();
        assert!(err.to_string().contains("JSON object"));
    }
}
```

运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::request_tests
```

预期失败：类型/函数未定义。

- [ ] 实现 Anthropic 请求/响应类型与转换函数：

```rust
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CacheControl {
    pub r#type: String,
}

fn cache_control_ephemeral() -> CacheControl {
    CacheControl { r#type: "ephemeral".into() }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ImageSource {
    Base64 { data: String, media_type: String },
    Url { url: String },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum AnthropicToolResultContent {
    Text { text: String },
    Image { source: ImageSource },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum AnthropicContentBlock {
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    Image {
        source: ImageSource,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    Thinking {
        thinking: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    ToolResult {
        tool_use_id: String,
        content: Vec<AnthropicToolResultContent>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnthropicToolParam {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AnthropicMessageParam {
    pub role: String,
    pub content: Vec<AnthropicContentBlock>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnthropicCreateParams {
    pub model: String,
    pub messages: Vec<AnthropicMessageParam>,
    pub max_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<Vec<AnthropicContentBlock>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<AnthropicToolParam>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<AnthropicThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<AnthropicOutputConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    pub stream: bool,
}

const SUPPORTED_B64_MEDIA_TYPES: &[&str] = &["image/png", "image/jpeg", "image/gif", "image/webp"];

pub fn convert_tool(tool: &Tool) -> AnthropicToolParam {
    AnthropicToolParam {
        name: tool.name.clone(),
        description: tool.description.clone(),
        input_schema: tool.parameters.clone(),
        cache_control: None,
    }
}

pub fn image_url_part_to_anthropic(url: &str) -> Result<AnthropicContentBlock, ChatProviderError> {
    if let Some(rest) = url.strip_prefix("data:") {
        let parts: Vec<&str> = rest.split(";base64,").collect();
        if parts.len() != 2 {
            return Err(ChatProviderError::Status(APIStatusError {
                status_code: 400,
                message: format!("Invalid data URL for image: {}", url),
                request_id: None,
            }));
        }
        let media_type = parts[0];
        let data = parts[1];
        if !SUPPORTED_B64_MEDIA_TYPES.contains(&media_type) {
            return Err(ChatProviderError::Status(APIStatusError {
                status_code: 400,
                message: format!("Unsupported media type for base64 image: {}", media_type),
                request_id: None,
            }));
        }
        Ok(AnthropicContentBlock::Image {
            source: ImageSource::Base64 {
                data: data.into(),
                media_type: media_type.into(),
            },
            cache_control: None,
        })
    } else {
        Ok(AnthropicContentBlock::Image {
            source: ImageSource::Url { url: url.into() },
            cache_control: None,
        })
    }
}

fn tool_result_to_block(tool_call_id: &str, content: &[ContentPart]) -> Result<AnthropicContentBlock, ChatProviderError> {
    let mut blocks = Vec::new();
    for part in content {
        match part {
            ContentPart::Text { text } if !text.is_empty() => {
                blocks.push(AnthropicToolResultContent::Text { text: text.clone() });
            }
            ContentPart::ImageUrl { image_url } => {
                let block = image_url_part_to_anthropic(&image_url.url)?;
                if let AnthropicContentBlock::Image { source, .. } = block {
                    blocks.push(AnthropicToolResultContent::Image { source });
                }
            }
            _ => {}
        }
    }
    Ok(AnthropicContentBlock::ToolResult {
        tool_use_id: tool_call_id.into(),
        content: blocks,
        cache_control: None,
    })
}

fn should_preserve_unsigned_thinking(model: &str) -> bool {
    parse_claude_version(model).is_none()
}

pub fn convert_message(message: &Message, model: &str) -> Result<AnthropicMessageParam, ChatProviderError> {
    match message.role {
        Role::System => {
            let text = message
                .content
                .iter()
                .filter_map(|p| match p {
                    ContentPart::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            Ok(AnthropicMessageParam {
                role: "user".into(),
                content: vec![AnthropicContentBlock::Text {
                    text: format!("<system>{}</system>", text),
                    cache_control: None,
                }],
            })
        }
        Role::Tool => {
            let id = message
                .tool_call_id
                .as_deref()
                .ok_or_else(|| ChatProviderError::Status(APIStatusError {
                    status_code: 400,
                    message: "Tool message missing `toolCallId`.".into(),
                    request_id: None,
                }))?;
            Ok(AnthropicMessageParam {
                role: "user".into(),
                content: vec![tool_result_to_block(id, &message.content)?],
            })
        }
        Role::User | Role::Assistant => {
            let mut blocks = Vec::new();
            for part in &message.content {
                match part {
                    ContentPart::Text { text } => blocks.push(AnthropicContentBlock::Text {
                        text: text.clone(),
                        cache_control: None,
                    }),
                    ContentPart::ImageUrl { image_url } => {
                        blocks.push(image_url_part_to_anthropic(&image_url.url)?);
                    }
                    ContentPart::Think { think, encrypted } => {
                        if encrypted.is_some() {
                            blocks.push(AnthropicContentBlock::Thinking {
                                thinking: think.clone(),
                                signature: encrypted.clone(),
                            });
                        } else if !think.is_empty() && should_preserve_unsigned_thinking(model) {
                            blocks.push(AnthropicContentBlock::Thinking {
                                thinking: think.clone(),
                                signature: None,
                            });
                        }
                    }
                    _ => {}
                }
            }
            for tc in &message.tool_calls {
                let input = match tc.arguments.as_deref() {
                    Some(args) => {
                        let parsed: serde_json::Value = serde_json::from_str(args)
                            .map_err(|_| ChatProviderError::Status(APIStatusError {
                                status_code: 400,
                                message: "Tool call arguments must be valid JSON.".into(),
                                request_id: None,
                            }))?;
                        if !parsed.is_object() {
                            return Err(ChatProviderError::Status(APIStatusError {
                                status_code: 400,
                                message: "Tool call arguments must be a JSON object.".into(),
                                request_id: None,
                            }));
                        }
                        parsed
                    }
                    None => serde_json::Value::Object(serde_json::Map::new()),
                };
                blocks.push(AnthropicContentBlock::ToolUse {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    input,
                    cache_control: None,
                });
            }
            Ok(AnthropicMessageParam {
                role: match message.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    _ => unreachable!(),
                }
                .into(),
                content: blocks,
            })
        }
    }
}
```

- [ ] 重新运行测试：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::request_tests
```

预期：全部通过。

- [ ] Commit: `feat(kosong-rs): anthropic content block conversion helpers`

---

### Task 2: 并行 tool_result 合并、cache_control 注入与 system prompt 组装

**Depends on:** Task 1

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/anthropic.rs:750-900`

**实现步骤：**

- [ ] 先写测试：

```rust
#[cfg(test)]
mod message_build_tests {
    use super::*;
    use crate::message::{ContentPart, Message, Role, ToolCall};

    fn tool_result_msg(id: &str, text: &str) -> Message {
        Message {
            role: Role::Tool,
            name: None,
            content: vec![ContentPart::Text { text: text.into() }],
            tool_calls: vec![],
            tool_call_id: Some(id.into()),
            partial: None,
        }
    }

    #[test]
    fn merge_consecutive_tool_result_only_messages() {
        let msgs = vec![tool_result_msg("a", "1"), tool_result_msg("b", "2")];
        let out = merge_parallel_tool_results(&msgs, "claude-opus-4-7").unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].role, "user");
        assert_eq!(out[0].content.len(), 2);
        match (&out[0].content[0], &out[0].content[1]) {
            (
                AnthropicContentBlock::ToolResult { tool_use_id: a, .. },
                AnthropicContentBlock::ToolResult { tool_use_id: b, .. },
            ) => {
                assert_eq!(a, "a");
                assert_eq!(b, "b");
            }
            _ => panic!("expected two tool_result blocks"),
        }
    }

    #[test]
    fn do_not_merge_tool_result_with_interleaved_user_message() {
        let msgs = vec![
            tool_result_msg("a", "1"),
            Message::user_text("ok?"),
            tool_result_msg("b", "2"),
        ];
        let out = merge_parallel_tool_results(&msgs, "claude-opus-4-7").unwrap();
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn inject_cache_control_on_last_text_block() {
        let msgs = vec![Message::user_text("hello")];
        let mut converted = merge_parallel_tool_results(&msgs, "claude-opus-4-7").unwrap();
        inject_cache_control_on_last_block(&mut converted);
        match &converted[0].content[0] {
            AnthropicContentBlock::Text { cache_control, .. } => {
                assert_eq!(cache_control.as_ref().unwrap().r#type, "ephemeral");
            }
            _ => panic!("expected text with cache_control"),
        }
    }

    #[test]
    fn inject_cache_control_on_last_tool_result_after_merge() {
        let msgs = vec![tool_result_msg("a", "1"), tool_result_msg("b", "2")];
        let mut converted = merge_parallel_tool_results(&msgs, "claude-opus-4-7").unwrap();
        inject_cache_control_on_last_block(&mut converted);
        match converted[0].content.last().unwrap() {
            AnthropicContentBlock::ToolResult { cache_control, .. } => {
                assert_eq!(cache_control.as_ref().unwrap().r#type, "ephemeral");
            }
            _ => panic!("expected cache_control on last tool_result"),
        }
    }

    #[test]
    fn system_prompt_becomes_system_param_with_cache_control() {
        let system = build_system_param("be helpful");
        assert_eq!(system.len(), 1);
        match &system[0] {
            AnthropicContentBlock::Text { text, cache_control } => {
                assert_eq!(text, "be helpful");
                assert_eq!(cache_control.as_ref().unwrap().r#type, "ephemeral");
            }
            _ => panic!("expected system text block"),
        }
    }

    #[test]
    fn empty_system_prompt_returns_none() {
        assert!(build_system_param("").is_empty());
        assert!(build_system_param("   ").is_empty());
    }
}
```

运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::message_build_tests
```

- [ ] 实现合并、cache 注入与 system prompt 函数：

```rust
const CACHEABLE_BLOCK_TYPES: &[&str] = &["text", "image", "document", "search_result", "tool_use", "tool_result", "server_tool_use", "web_search_tool_result"];

fn is_tool_result_only(message: &AnthropicMessageParam) -> bool {
    if message.role != "user" {
        return false;
    }
    !message.content.is_empty() && message.content.iter().all(|b| matches!(b, AnthropicContentBlock::ToolResult { .. }))
}

pub fn inject_cache_control_on_last_block(messages: &mut [AnthropicMessageParam]) {
    let last = match messages.last_mut() {
        Some(l) => l,
        None => return,
    };
    let block = match last.content.last_mut() {
        Some(b) => b,
        None => return,
    };
    let type_name = match block {
        AnthropicContentBlock::Text { .. } => "text",
        AnthropicContentBlock::Image { .. } => "image",
        AnthropicContentBlock::ToolUse { .. } => "tool_use",
        AnthropicContentBlock::ToolResult { .. } => "tool_result",
        _ => return,
    };
    if CACHEABLE_BLOCK_TYPES.contains(&type_name) {
        let cc = cache_control_ephemeral();
        match block {
            AnthropicContentBlock::Text { cache_control, .. }
            | AnthropicContentBlock::Image { cache_control, .. }
            | AnthropicContentBlock::ToolUse { cache_control, .. }
            | AnthropicContentBlock::ToolResult { cache_control, .. } => {
                *cache_control = Some(cc);
            }
            _ => {}
        }
    }
}

pub fn merge_parallel_tool_results(messages: &[Message], model: &str) -> Result<Vec<AnthropicMessageParam>, ChatProviderError> {
    let mut out: Vec<AnthropicMessageParam> = Vec::new();
    for msg in messages {
        let converted = convert_message(msg, model)?;
        if let Some(last) = out.last_mut() {
            if is_tool_result_only(last) && is_tool_result_only(&converted) {
                last.content.extend(converted.content);
                continue;
            }
        }
        out.push(converted);
    }
    Ok(out)
}

pub fn build_system_param(system_prompt: &str) -> Vec<AnthropicContentBlock> {
    let trimmed = system_prompt.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    vec![AnthropicContentBlock::Text {
        text: trimmed.into(),
        cache_control: Some(cache_control_ephemeral()),
    }]
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::message_build_tests
```

预期：全部通过。

- [ ] Commit: `feat(kosong-rs): anthropic message merge and cache control injection`

---

### Task 3: 请求体、鉴权头与 tool cache_control 组装

**Depends on:** Task 2

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/anthropic.rs:900-1100`

**实现步骤：**

- [ ] 先写测试：

```rust
#[cfg(test)]
mod params_tests {
    use super::*;
    use crate::provider::{ProviderRequestAuth, ThinkingEffort, Tool};

    fn provider(model: &str) -> AnthropicChatProvider {
        AnthropicChatProvider::new(AnthropicOptions {
            model: model.into(),
            api_key: Some("sk-test".into()),
            base_url: None,
            default_max_tokens: None,
            beta_features: None,
            default_headers: None,
            metadata: None,
            stream: Some(false),
            adaptive_thinking: None,
        })
    }

    #[test]
    fn build_create_params_serializes_stream_false() {
        let p = provider("claude-opus-4-7");
        let params = p.build_create_params("be helpful", &[], &[Message::user_text("hi")], false).unwrap();
        let v = serde_json::to_value(&params).unwrap();
        assert_eq!(v["model"], "claude-opus-4-7");
        assert_eq!(v["stream"], false);
        assert_eq!(v["max_tokens"], 128_000);
        assert!(v["system"].is_array());
        assert_eq!(v["messages"][0]["role"], "user");
    }

    #[test]
    fn build_create_params_includes_thinking_and_output_config() {
        let p = provider("claude-opus-4-7").with_thinking(ThinkingEffort::High);
        let params = p.build_create_params("", &[], &[Message::user_text("hi")], false).unwrap();
        let v = serde_json::to_value(&params).unwrap();
        assert_eq!(v["thinking"]["type"], "adaptive");
        assert_eq!(v["thinking"]["display"], "summarized");
        assert_eq!(v["output_config"]["effort"], "high");
    }

    #[test]
    fn build_create_params_injects_tool_cache_control() {
        let tool = Tool {
            name: "read".into(),
            description: "read".into(),
            parameters: serde_json::json!({}),
        };
        let p = provider("claude-opus-4-7");
        let params = p.build_create_params("", &[tool], &[Message::user_text("hi")], false).unwrap();
        let v = serde_json::to_value(&params).unwrap();
        assert_eq!(v["tools"][0]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn build_extra_headers_contains_beta_and_version() {
        let p = provider("claude-opus-4-7");
        let headers = p.build_extra_headers(Some(&ProviderRequestAuth {
            api_key: None,
            headers: None,
        })).unwrap();
        assert!(headers.contains_key("x-api-key"));
        assert!(headers.contains_key("anthropic-version"));
        assert!(headers.contains_key("anthropic-beta"));
        assert!(headers.contains_key("content-type"));
    }

    #[test]
    fn request_headers_merge_auth_headers() {
        let p = provider("claude-opus-4-7");
        let mut req_headers = HashMap::new();
        req_headers.insert("x-custom".into(), "v".into());
        let headers = p.build_extra_headers(Some(&ProviderRequestAuth {
            api_key: None,
            headers: Some(req_headers),
        })).unwrap();
        assert_eq!(headers.get("x-custom").unwrap(), "v");
    }

    #[test]
    fn resolve_api_key_prefers_request_auth() {
        let p = AnthropicChatProvider::new(AnthropicOptions {
            model: "claude-opus-4-7".into(),
            api_key: Some("default".into()),
            base_url: None,
            default_max_tokens: None,
            beta_features: None,
            default_headers: None,
            metadata: None,
            stream: Some(false),
            adaptive_thinking: None,
        });
        let auth = ProviderRequestAuth {
            api_key: Some("request".into()),
            headers: None,
        };
        assert_eq!(p.resolve_api_key(Some(&auth)).unwrap(), "request");
    }
}
```

运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::params_tests
```

- [ ] 实现请求体与鉴权头组装函数（作为 `AnthropicChatProvider` 的 `pub(crate)` 方法）：

```rust
impl AnthropicChatProvider {
    pub(crate) fn resolve_api_key(&self, auth: Option<&ProviderRequestAuth>) -> Result<String, ChatProviderError> {
        require_provider_api_key("anthropic", auth, self.api_key.as_deref())
    }

    pub(crate) fn build_extra_headers(
        &self,
        auth: Option<&ProviderRequestAuth>,
    ) -> Result<HashMap<String, String>, ChatProviderError> {
        let mut headers = HashMap::new();
        headers.insert("content-type".into(), "application/json".into());
        headers.insert("anthropic-version".into(), "2023-06-01".into());
        headers.insert("x-api-key".into(), self.resolve_api_key(auth)?);

        let betas = &self.generation_kwargs.beta_features;
        if !betas.is_empty() {
            headers.insert("anthropic-beta".into(), betas.join(","));
        }

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

    pub(crate) fn build_create_params(
        &self,
        system_prompt: &str,
        tools: &[Tool],
        history: &[Message],
        stream: bool,
    ) -> Result<AnthropicCreateParams, ChatProviderError> {
        let normalized = normalize_tool_call_ids_for_provider(
            history,
            &ToolCallIdPolicy::new(
                |id| sanitize_tool_call_id(id, Some(64)),
                Some(64),
            ),
        );
        let mut messages = merge_parallel_tool_results(&normalized, &self.model)?;
        inject_cache_control_on_last_block(&mut messages);

        let mut anthropic_tools: Vec<AnthropicToolParam> = tools.iter().map(convert_tool).collect();
        if let Some(last) = anthropic_tools.last_mut() {
            last.cache_control = Some(cache_control_ephemeral());
        }

        let system = {
            let blocks = build_system_param(system_prompt);
            if blocks.is_empty() { None } else { Some(blocks) }
        };

        Ok(AnthropicCreateParams {
            model: self.model.clone(),
            messages,
            max_tokens: self.generation_kwargs.max_tokens,
            system,
            tools: if anthropic_tools.is_empty() { None } else { Some(anthropic_tools) },
            temperature: self.generation_kwargs.temperature,
            top_k: self.generation_kwargs.top_k,
            top_p: self.generation_kwargs.top_p,
            thinking: self.generation_kwargs.thinking.clone(),
            output_config: self.generation_kwargs.output_config.clone(),
            metadata: self.metadata.clone(),
            stream,
        })
    }
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::params_tests
```

预期：全部通过。

- [ ] Commit: `feat(kosong-rs): anthropic create params and request headers`

---

### Task 4: 非流式 `generate()` 端到端路径

**Depends on:** Task 3

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/anthropic.rs:1100-1350`
- Modify: `rust-ody/crates/kosong-rs/src/providers/anthropic.rs:745-760`（替换 stub `generate`）

**实现步骤：**

- [ ] 先定义非流式响应类型与解析函数，并写测试：

```rust
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AnthropicUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    #[serde(default)]
    pub cache_read_input_tokens: i64,
    #[serde(default)]
    pub cache_creation_input_tokens: i64,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum AnthropicResponseContentBlock {
    Text { text: String },
    Thinking { thinking: String, signature: Option<String> },
    RedactedThinking { data: String },
    ToolUse { id: String, name: String, input: serde_json::Value },
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AnthropicMessageResponse {
    pub id: String,
    pub stop_reason: Option<String>,
    pub usage: AnthropicUsage,
    pub content: Vec<AnthropicResponseContentBlock>,
}

fn normalize_stop_reason(raw: Option<&str>) -> (Option<FinishReason>, Option<String>) {
    let raw = match raw {
        Some(r) => r,
        None => return (None, None),
    };
    let finish = match raw {
        "end_turn" | "stop_sequence" => FinishReason::Completed,
        "max_tokens" => FinishReason::Truncated,
        "tool_use" => FinishReason::ToolCalls,
        "pause_turn" => FinishReason::Paused,
        "refusal" => FinishReason::Filtered,
        _ => FinishReason::Other,
    };
    (Some(finish), Some(raw.into()))
}

fn parse_non_stream_response(response: AnthropicMessageResponse) -> Result<StreamedMessage, ChatProviderError> {
    let mut parts = Vec::new();
    for block in response.content {
        match block {
            AnthropicResponseContentBlock::Text { text } => {
                parts.push(StreamedMessagePart::text(text));
            }
            AnthropicResponseContentBlock::Thinking { thinking, signature } => {
                parts.push(StreamedMessagePart::Content(ContentPart::Think {
                    think: thinking,
                    encrypted: signature,
                }));
            }
            AnthropicResponseContentBlock::RedactedThinking { data } => {
                parts.push(StreamedMessagePart::Content(ContentPart::Think {
                    think: String::new(),
                    encrypted: Some(data),
                }));
            }
            AnthropicResponseContentBlock::ToolUse { id, name, input } => {
                let arguments = if input.is_object() && !input.as_object().unwrap().is_empty() {
                    Some(input.to_string())
                } else {
                    None
                };
                parts.push(StreamedMessagePart::tool_call(id, name, arguments.as_deref()));
            }
        }
    }
    let (finish_reason, raw_finish_reason) = normalize_stop_reason(response.stop_reason.as_deref());
    let usage = TokenUsage {
        input_other: response.usage.input_tokens,
        output: response.usage.output_tokens,
        input_cache_read: response.usage.cache_read_input_tokens,
        input_cache_creation: response.usage.cache_creation_input_tokens,
    };
    Ok(StreamedMessage::from_parts(parts, Some(response.id), Some(usage), finish_reason, raw_finish_reason))
}
```

测试：

```rust
#[cfg(test)]
mod response_parse_tests {
    use super::*;

    #[test]
    fn parse_text_response() {
        let json = serde_json::json!({
            "id": "msg_1",
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5,
                "cache_read_input_tokens": 2,
                "cache_creation_input_tokens": 1
            },
            "content": [{"type":"text","text":"hello"}]
        });
        let resp: AnthropicMessageResponse = serde_json::from_value(json).unwrap();
        let stream = parse_non_stream_response(resp).unwrap();
        assert_eq!(stream.id(), Some("msg_1".into()));
        assert_eq!(stream.usage(), Some(TokenUsage {
            input_other: 10,
            output: 5,
            input_cache_read: 2,
            input_cache_creation: 1,
        }));
        assert_eq!(stream.finish_reason(), Some(FinishReason::Completed));
    }

    #[test]
    fn parse_tool_use_response() {
        let json = serde_json::json!({
            "id": "msg_2",
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 20, "output_tokens": 10},
            "content": [{"type":"tool_use","id":"tc_1","name":"read","input":{"path":"/etc/passwd"}}]
        });
        let resp: AnthropicMessageResponse = serde_json::from_value(json).unwrap();
        let stream = parse_non_stream_response(resp).unwrap();
        assert_eq!(stream.finish_reason(), Some(FinishReason::ToolCalls));
    }

    #[test]
    fn normalize_unknown_stop_reason() {
        let (fr, raw) = normalize_stop_reason(Some("weird"));
        assert_eq!(fr, Some(FinishReason::Other));
        assert_eq!(raw, Some("weird".into()));
    }
}
```

运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::response_parse_tests
```

- [ ] 替换 `ChatProvider::generate` stub 为非流式实现：

```rust
#[async_trait::async_trait]
impl ChatProvider for AnthropicChatProvider {
    // ... keep name, model_name, thinking_effort, with_thinking, with_max_completion_tokens, get_capability ...

    async fn generate(
        &self,
        system_prompt: &str,
        tools: &[Tool],
        history: &[Message],
        options: Option<GenerateOptions>,
    ) -> Result<StreamedMessage, ChatProviderError> {
        if let Some(ref opts) = options {
            if let Some(ref signal) = opts.signal {
                if signal.is_aborted() {
                    return Err(ChatProviderError::Aborted(AbortError));
                }
            }
            if let Some(ref hook) = opts.on_request_start {
                hook();
            }
        }

        // Only non-streaming is implemented in this part; streaming SSE parser comes in Part 3.
        if self.stream {
            return Err(ChatProviderError::Status(APIStatusError {
                status_code: 501,
                message: "Anthropic streaming SSE parse is not yet implemented (Part 3).".into(),
                request_id: None,
            }));
        }

        let auth = options.as_ref().and_then(|o| o.auth.clone());
        let create_params = self.build_create_params(system_prompt, tools, history, false)?;
        let headers = self.build_extra_headers(auth.as_ref())?;

        let url = format!("{}v1/messages", self.base_url.as_deref().unwrap_or("https://api.anthropic.com/"));
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .map_err(|e| ChatProviderError::Connection(APIConnectionError))?;

        let mut req = client.post(&url).json(&create_params);
        for (k, v) in headers {
            req = req.header(k, v);
        }

        if let Some(ref opts) = options {
            if let Some(ref signal) = opts.signal {
                if signal.is_aborted() {
                    return Err(ChatProviderError::Aborted(AbortError));
                }
            }
        }

        let response = req.send().await.map_err(|e| {
            if e.is_timeout() {
                ChatProviderError::Timeout(APITimeoutError)
            } else {
                ChatProviderError::Connection(APIConnectionError)
            }
        })?;

        if let Some(ref opts) = options {
            if let Some(ref signal) = opts.signal {
                if signal.is_aborted() {
                    return Err(ChatProviderError::Aborted(AbortError));
                }
            }
        }

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(normalize_api_status_error(status.as_u16(), body, None));
        }

        let message_response: AnthropicMessageResponse = response.json().await.map_err(|_| {
            ChatProviderError::Status(APIStatusError {
                status_code: 500,
                message: "Failed to parse Anthropic response JSON".into(),
                request_id: None,
            })
        })?;

        if let Some(ref opts) = options {
            if let Some(ref hook) = opts.on_stream_end {
                hook();
            }
        }

        parse_non_stream_response(message_response)
    }
}
```

- [ ] 添加 httptest 集成测试：

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::provider::ChatProvider;
    use httptest::{matchers::*, responders::*, Expectation, Server};

    #[tokio::test]
    async fn generate_non_stream_hits_messages_endpoint() {
        let server = Server::run();
        server.expect(
            Expectation::matching(all_of![
                request::method_path("POST", "/v1/messages"),
                request::header("x-api-key", "sk-test"),
                request::header("anthropic-version", "2023-06-01"),
                request::body(json_decoded(assertion(|v: &serde_json::Value| {
                    assert_eq!(v["model"], "claude-opus-4-7");
                    assert_eq!(v["stream"], false);
                    assert!(v["system"].is_array());
                    assert_eq!(v["messages"][0]["role"], "user");
                    assert_eq!(v["messages"][0]["content"][0]["text"], "hi");
                })))
            ])
            .respond_with(json_body(serde_json::json!({
                "id": "msg_1",
                "type": "message",
                "role": "assistant",
                "stop_reason": "end_turn",
                "usage": {"input_tokens": 10, "output_tokens": 5, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0},
                "content": [{"type":"text","text":"Hello"}]
            }))),
        );

        let provider = AnthropicChatProvider::new(AnthropicOptions {
            model: "claude-opus-4-7".into(),
            api_key: Some("sk-test".into()),
            base_url: Some(server.url_str("/")),
            default_max_tokens: Some(1_024),
            beta_features: Some(vec![]),
            default_headers: None,
            metadata: None,
            stream: Some(false),
            adaptive_thinking: None,
        });

        let mut stream = provider.generate("sys", &[], &[Message::user_text("hi")], None).await.unwrap();
        let parts: Vec<_> = futures_util::StreamExt::next(&mut stream).await.into_iter().collect();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0], StreamedMessagePart::text("Hello"));
    }

    #[tokio::test]
    async fn generate_maps_4xx_to_status_error() {
        let server = Server::run();
        server.expect(
            Expectation::matching(request::method_path("POST", "/v1/messages"))
                .respond_with(status_code(429).body("rate limited")),
        );

        let provider = AnthropicChatProvider::new(AnthropicOptions {
            model: "claude-opus-4-7".into(),
            api_key: Some("sk-test".into()),
            base_url: Some(server.url_str("/")),
            default_max_tokens: Some(1_024),
            beta_features: Some(vec![]),
            default_headers: None,
            metadata: None,
            stream: Some(false),
            adaptive_thinking: None,
        });

        let err = provider.generate("", &[], &[Message::user_text("hi")], None).await.unwrap_err();
        assert!(matches!(err, ChatProviderError::Status(APIStatusError { status_code: 429, .. })));
    }
}
```

运行测试：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::integration_tests
```

预期：全部通过。

- [ ] 运行整个 crate 测试确保无回归：

```bash
cd rust-ody && cargo test -p kosong-rs
```

- [ ] Commit: `feat(kosong-rs): anthropic non-streaming generate end-to-end`

---

## Part 2 Self-Review

- [ ] 1. Spec-coverage table: 本部分覆盖 4.2.4.2（system/messages/tools 请求体构造）、4.2.4.3（cache_control / 并行 tool_result 合并）、4.2.4.5（非流式请求路径）。流式 SSE 解析与错误映射为 GAP，由 Part 3 覆盖。
- [ ] 2. Placeholder scan: 无 TODO/TBD；`generate()` 中对 `self.stream == true` 返回 501 是显式边界声明，Part 3 替换为 SSE 解析器，非死代码占位。
- [ ] 3. No phantom tasks: 每个 Task 均提供完整代码、测试命令、预期结果、commit。
- [ ] 4. Dependency soundness: Task 1 → 2 → 3 → 4；只使用 Part 1 已定义的 `AnthropicChatProvider` / `AnthropicOptions` / `AnthropicThinkingConfig` / `AnthropicOutputConfig` / `AnthropicGenerationKwargs`。
- [ ] 5. Caller & build soundness: 新增方法均为 `pub(crate)` 或局部函数，未修改共享 trait 签名；`ChatProvider::generate` 在本 Task 内一次性替换 stub；结束运行 `cargo test -p kosong-rs`。
- [ ] 6. Test-the-risk: 测试覆盖 data URL media type 校验、tool_call_id 缺失错误、非 object tool 参数拒绝、cache_control 落点、合并边界、鉴权头优先级、HTTP 4xx 映射。
- [ ] 7. Type consistency: 复用 `ChatProviderError`、`APIStatusError`、`APIConnectionError`、`APITimeoutError`、`AbortError`、`TokenUsage`、`FinishReason`、`ToolCallIdPolicy`、`normalize_tool_call_ids_for_provider`、`sanitize_tool_call_id`、`require_provider_api_key`、`merge_request_headers`；新增类型名与 TS 对应事件/参数一致。
