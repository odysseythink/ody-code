# Part 3: Response parsing & L1 parity

本部分完成 Anthropic provider 的**响应侧**：SSE 事件解析、流式/非流式响应到 `StreamedMessage` 的适配、Anthropic 错误映射、完整的 `generate()` 流式路径，以及 L1 golden fixtures + TS↔Rust parity 测试。

---

### Task 1: Anthropic SSE 事件类型与响应体解析器

**Depends on:** Part 2 (`request.md`) Task 4

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/anthropic.rs:1350-1550`

**实现步骤：**

- [ ] 先写测试，覆盖 SSE 分块解析、事件对象反序列化、空白/注释行跳过：

```rust
#[cfg(test)]
mod sse_tests {
    use super::*;

    #[test]
    fn parse_sse_text_event() {
        let body = "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n";
        let events = parse_sse_body(body);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "content_block_delta");
    }

    #[test]
    fn parse_sse_skips_comments_and_empty_lines() {
        let body = ":comment\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
        let events = parse_sse_body(body);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "message_stop");
    }

    #[test]
    fn parse_sse_multiple_events() {
        let body = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\"}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
        let events = parse_sse_body(body);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn deserialize_message_start_event() {
        let json = serde_json::json!({
            "type": "message_start",
            "message": {
                "id": "msg_1",
                "usage": { "input_tokens": 10, "output_tokens": 0 }
            }
        });
        let evt: AnthropicSseEvent = serde_json::from_value(json).unwrap();
        match evt {
            AnthropicSseEvent::MessageStart { message } => {
                assert_eq!(message.id, "msg_1");
                assert_eq!(message.usage.input_tokens, 10);
            }
            _ => panic!("expected message_start"),
        }
    }

    #[test]
    fn deserialize_content_block_start_tool_use() {
        let json = serde_json::json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": { "type": "tool_use", "id": "tc_1", "name": "read", "input": {} }
        });
        let evt: AnthropicSseEvent = serde_json::from_value(json).unwrap();
        match evt {
            AnthropicSseEvent::ContentBlockStart { index, content_block } => {
                assert_eq!(index, 0);
                assert_eq!(content_block.r#type, "tool_use");
            }
            _ => panic!("expected content_block_start"),
        }
    }

    #[test]
    fn deserialize_input_json_delta() {
        let json = serde_json::json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": { "type": "input_json_delta", "partial_json": "{\"a\":1" }
        });
        let evt: AnthropicSseEvent = serde_json::from_value(json).unwrap();
        match evt {
            AnthropicSseEvent::ContentBlockDelta { delta } => {
                match delta {
                    AnthropicSseDelta::InputJsonDelta { partial_json } => assert_eq!(partial_json, "{\"a\":1"),
                    _ => panic!("expected input_json_delta"),
                }
            }
            _ => panic!("expected content_block_delta"),
        }
    }
}
```

运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::sse_tests
```

预期失败：类型/函数未定义。

- [ ] 实现 SSE 解析类型与函数：

```rust
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum AnthropicSseEvent {
    MessageStart { message: AnthropicSseMessageStart },
    ContentBlockStart { index: usize, content_block: AnthropicSseContentBlock },
    ContentBlockDelta { index: usize, delta: AnthropicSseDelta },
    ContentBlockStop { index: usize },
    MessageDelta { delta: AnthropicSseMessageDelta, usage: Option<AnthropicUsage> },
    MessageStop,
    #[serde(rename = "error")]
    Error { error: serde_json::Value },
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AnthropicSseMessageStart {
    pub id: String,
    pub usage: AnthropicUsage,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct AnthropicSseContentBlock {
    pub r#type: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub thinking: Option<String>,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub input: Option<serde_json::Value>,
    #[serde(default)]
    pub signature: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum AnthropicSseDelta {
    TextDelta { text: String },
    ThinkingDelta { thinking: String },
    InputJsonDelta { partial_json: String },
    SignatureDelta { signature: String },
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct AnthropicSseMessageDelta {
    #[serde(default)]
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSseEvent {
    pub event_type: String,
    pub payload: serde_json::Value,
}

fn parse_sse_body(body: &str) -> Vec<ParsedSseEvent> {
    let mut events = Vec::new();
    let mut current_event: Option<String> = None;
    let mut current_data = String::new();

    for raw_line in body.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.is_empty() {
            if let Some(et) = current_event.take() {
                if !current_data.is_empty() {
                    if let Ok(payload) = serde_json::from_str(&current_data) {
                        events.push(ParsedSseEvent {
                            event_type: et,
                            payload,
                        });
                    }
                }
                current_data.clear();
            }
            continue;
        }
        if let Some(comment) = line.strip_prefix(':') {
            continue;
        }
        if let Some(et) = line.strip_prefix("event: ") {
            current_event = Some(et.to_string());
        } else if let Some(data) = line.strip_prefix("data: ") {
            if !current_data.is_empty() {
                current_data.push('\n');
            }
            current_data.push_str(data);
        }
    }

    if let Some(et) = current_event.take() {
        if !current_data.is_empty() {
            if let Ok(payload) = serde_json::from_str(&current_data) {
                events.push(ParsedSseEvent {
                    event_type: et,
                    payload,
                });
            }
        }
    }

    events
}
```

- [ ] 重新运行测试：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::sse_tests
```

预期：全部通过。

- [ ] Commit: `feat(kosong-rs): anthropic SSE event parser`

---

### Task 2: 流式事件 → `StreamedMessagePart` 适配器

**Depends on:** Task 1

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/anthropic.rs:1550-1750`
- Modify: `rust-ody/crates/kosong-rs/src/generate.rs:25-65`（新增 `StreamedMessage::from_stream` 构造函数）

**实现步骤：**

- [ ] 先写测试，覆盖事件序列到 parts 的完整转换：

```rust
#[cfg(test)]
mod stream_adapter_tests {
    use super::*;
    use crate::message::{ContentPart, StreamedMessagePart};

    fn text_event(text: &str) -> ParsedSseEvent {
        ParsedSseEvent {
            event_type: "content_block_delta".into(),
            payload: serde_json::json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "text_delta", "text": text }
            }),
        }
    }

    #[test]
    fn text_events_yield_text_parts() {
        let events = vec![
            ParsedSseEvent {
                event_type: "message_start".into(),
                payload: serde_json::json!({"type":"message_start","message":{"id":"msg_1","usage":{"input_tokens":10,"output_tokens":0}}}),
            },
            text_event("Hello"),
            text_event(" world"),
            ParsedSseEvent {
                event_type: "message_delta".into(),
                payload: serde_json::json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":2}}),
            },
            ParsedSseEvent {
                event_type: "message_stop".into(),
                payload: serde_json::json!({"type":"message_stop"}),
            },
        ];
        let stream = anthropic_events_to_streamed_message(events).unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(stream).await;
        assert_eq!(parts, vec![StreamedMessagePart::text("Hello"), StreamedMessagePart::text(" world")]);
    }

    #[test]
    fn thinking_and_signature_events() {
        let events = vec![
            ParsedSseEvent {
                event_type: "content_block_start".into(),
                payload: serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"step1"}}),
            },
            ParsedSseEvent {
                event_type: "content_block_delta".into(),
                payload: serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":" step2"}}),
            },
            ParsedSseEvent {
                event_type: "content_block_delta".into(),
                payload: serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig"}}),
            },
        ];
        let stream = anthropic_events_to_streamed_message(events).unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(stream).await;
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], StreamedMessagePart::Content(ContentPart::Think { think: "step1".into(), encrypted: None }));
        assert_eq!(parts[1], StreamedMessagePart::Content(ContentPart::Think { think: "".into(), encrypted: Some("sig".into()) }));
    }

    #[test]
    fn tool_use_start_and_input_json_deltas() {
        let events = vec![
            ParsedSseEvent {
                event_type: "content_block_start".into(),
                payload: serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"tc_1","name":"read","input":{}}}),
            },
            ParsedSseEvent {
                event_type: "content_block_delta".into(),
                payload: serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\":\""}}),
            },
            ParsedSseEvent {
                event_type: "content_block_delta".into(),
                payload: serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"/etc/passwd\"}"}}),
            },
        ];
        let stream = anthropic_events_to_streamed_message(events).unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(stream).await;
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], StreamedMessagePart::tool_call("tc_1", "read", Some("")));
        match &parts[1] {
            StreamedMessagePart::ToolCallPart(p) => assert_eq!(p.arguments_part.as_deref(), Some("{\"path\":\"")),
            _ => panic!("expected tool_call_part"),
        }
    }

    #[test]
    fn redacted_thinking_event() {
        let events = vec![ParsedSseEvent {
            event_type: "content_block_start".into(),
            payload: serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"redacted_thinking","data":"secret"}}),
        }];
        let stream = anthropic_events_to_streamed_message(events).unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(stream).await;
        assert_eq!(parts[0], StreamedMessagePart::Content(ContentPart::Think { think: "".into(), encrypted: Some("secret".into()) }));
    }

    #[test]
    fn message_delta_updates_usage_and_stop_reason() {
        let events = vec![
            ParsedSseEvent {
                event_type: "message_start".into(),
                payload: serde_json::json!({"type":"message_start","message":{"id":"msg_1","usage":{"input_tokens":10,"output_tokens":0,"cache_read_input_tokens":2,"cache_creation_input_tokens":1}}}),
            },
            text_event("x"),
            ParsedSseEvent {
                event_type: "message_delta".into(),
                payload: serde_json::json!({"type":"message_delta","delta":{"stop_reason":"max_tokens"},"usage":{"output_tokens":5,"cache_read_input_tokens":3,"cache_creation_input_tokens":2,"input_tokens":11}}),
            },
        ];
        let stream = anthropic_events_to_streamed_message(events).unwrap();
        assert_eq!(stream.id(), Some("msg_1".into()));
        assert_eq!(stream.usage(), Some(TokenUsage {
            input_other: 11,
            output: 5,
            input_cache_read: 3,
            input_cache_creation: 2,
        }));
        assert_eq!(stream.finish_reason(), Some(FinishReason::Truncated));
        assert_eq!(stream.raw_finish_reason(), Some("max_tokens".into()));
    }
}
```

注意：`StreamExt::collect` 需要 `futures-util` 的 `StreamExt` trait。本 crate 已依赖 `futures-util`。

运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::stream_adapter_tests
```

- [ ] 新增 `StreamedMessage::from_stream` 构造函数（共享类型变更，需在同一 Task 内处理）：

```rust
// 在 rust-ody/crates/kosong-rs/src/generate.rs 中，为 StreamedMessage impl 新增：
impl StreamedMessage {
    pub fn from_parts(...) -> Self { /* 已有 */ }

    pub fn from_stream(
        stream: impl Stream<Item = StreamedMessagePart> + Send + 'static,
        id: Option<String>,
        usage: Option<TokenUsage>,
        finish_reason: Option<FinishReason>,
        raw_finish_reason: Option<String>,
    ) -> Self {
        Self {
            id,
            usage,
            finish_reason,
            raw_finish_reason,
            inner: Box::pin(stream),
        }
    }
}
```

该构造器为新增 API，不改变 `from_parts` 签名，也不影响现有调用方。搜索确认无其他 `StreamedMessage { ... }` 结构体字面量调用方：

```bash
cd rust-ody && grep -rn "StreamedMessage {" crates/kosong-rs/src/
```

预期：仅有 `from_parts` 内部一处构造，无其他调用方需要更新。

- [ ] 实现事件到 parts 的适配器：

```rust
#[derive(Debug, Default)]
struct AnthropicStreamState {
    id: Option<String>,
    usage: TokenUsage,
    finish_reason: Option<FinishReason>,
    raw_finish_reason: Option<String>,
}

pub fn anthropic_events_to_streamed_message(
    events: Vec<ParsedSseEvent>,
) -> Result<StreamedMessage, ChatProviderError> {
    let mut state = AnthropicStreamState::default();
    let mut parts = Vec::new();

    for event in events {
        let sse: AnthropicSseEvent = serde_json::from_value(event.payload).map_err(|e| {
            ChatProviderError::Status(APIStatusError {
                status_code: 500,
                message: format!("Invalid Anthropic SSE event: {}", e),
                request_id: None,
            })
        })?;
        match sse {
            AnthropicSseEvent::MessageStart { message } => {
                state.id = Some(message.id);
                state.usage.input_other = message.usage.input_tokens;
                state.usage.output = message.usage.output_tokens;
                state.usage.input_cache_read = message.usage.cache_read_input_tokens;
                state.usage.input_cache_creation = message.usage.cache_creation_input_tokens;
            }
            AnthropicSseEvent::ContentBlockStart { index, content_block } => {
                match content_block.r#type.as_str() {
                    "text" => {
                        if let Some(text) = content_block.text {
                            parts.push(StreamedMessagePart::text(text));
                        }
                    }
                    "thinking" => {
                        parts.push(StreamedMessagePart::Content(ContentPart::Think {
                            think: content_block.thinking.unwrap_or_default(),
                            encrypted: None,
                        }));
                    }
                    "redacted_thinking" => {
                        parts.push(StreamedMessagePart::Content(ContentPart::Think {
                            think: String::new(),
                            encrypted: content_block.data,
                        }));
                    }
                    "tool_use" => {
                        parts.push(StreamedMessagePart::tool_call(
                            content_block.id.unwrap_or_default(),
                            content_block.name.unwrap_or_default(),
                            Some(""),
                        ));
                    }
                    _ => {}
                }
            }
            AnthropicSseEvent::ContentBlockDelta { index, delta } => match delta {
                AnthropicSseDelta::TextDelta { text } => parts.push(StreamedMessagePart::text(text)),
                AnthropicSseDelta::ThinkingDelta { thinking } => {
                    parts.push(StreamedMessagePart::Content(ContentPart::Think {
                        think: thinking,
                        encrypted: None,
                    }));
                }
                AnthropicSseDelta::InputJsonDelta { partial_json } => {
                    parts.push(StreamedMessagePart::tool_call_part(Some(&partial_json)));
                }
                AnthropicSseDelta::SignatureDelta { signature } => {
                    parts.push(StreamedMessagePart::Content(ContentPart::Think {
                        think: String::new(),
                        encrypted: Some(signature),
                    }));
                }
            },
            AnthropicSseEvent::ContentBlockStop { .. } => {}
            AnthropicSseEvent::MessageDelta { delta, usage } => {
                if let Some(u) = usage {
                    state.usage.input_other = u.input_tokens;
                    state.usage.output = u.output_tokens;
                    state.usage.input_cache_read = u.cache_read_input_tokens;
                    state.usage.input_cache_creation = u.cache_creation_input_tokens;
                }
                if let Some(raw) = delta.stop_reason {
                    let (fr, raw_str) = normalize_stop_reason(Some(&raw));
                    state.finish_reason = fr;
                    state.raw_finish_reason = raw_str;
                }
            }
            AnthropicSseEvent::MessageStop => {}
            AnthropicSseEvent::Error { error } => {
                return Err(ChatProviderError::Status(APIStatusError {
                    status_code: 500,
                    message: format!("Anthropic SSE error event: {}", error),
                    request_id: None,
                }));
            }
        }
    }

    let usage = if state.usage.input_other == 0
        && state.usage.output == 0
        && state.usage.input_cache_read == 0
        && state.usage.input_cache_creation == 0
    {
        None
    } else {
        Some(state.usage)
    };

    Ok(StreamedMessage::from_stream(
        futures_util::stream::iter(parts),
        state.id,
        usage,
        state.finish_reason,
        state.raw_finish_reason,
    ))
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::stream_adapter_tests
```

预期：全部通过。

- [ ] 运行 workspace 级类型检查（共享签名变更收尾）：

```bash
cd rust-ody && cargo check --workspace
```

预期：无 error。

- [ ] Commit: `feat(kosong-rs): anthropic streaming event to StreamedMessage adapter`

---

### Task 3: Anthropic SDK 风格的错误映射

**Depends on:** Task 2

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/anthropic.rs:1750-1850`

**实现步骤：**

- [ ] 先写测试：

```rust
#[cfg(test)]
mod error_tests {
    use super::*;

    #[test]
    fn maps_reqwest_timeout_to_timeout_error() {
        let reqwest_err = reqwest::Error::from(std::io::Error::new(std::io::ErrorKind::TimedOut, "timeout"));
        let err = convert_anthropic_request_error(reqwest_err);
        assert!(matches!(err, ChatProviderError::Timeout(_)));
    }

    #[test]
    fn maps_reqwest_connect_to_connection_error() {
        let reqwest_err = reqwest::Error::from(std::io::Error::new(std::io::ErrorKind::NotFound, "dns failed"));
        let err = convert_anthropic_request_error(reqwest_err);
        assert!(matches!(err, ChatProviderError::Connection(_)));
    }

    #[test]
    fn maps_context_overflow_status() {
        let err = ChatProviderError::Status(APIStatusError {
            status_code: 413,
            message: "request exceeds model token limit".into(),
            request_id: None,
        });
        assert!(matches!(err, ChatProviderError::ContextOverflow(_)));
    }

    #[test]
    fn normalize_anthropic_api_error_extracts_request_id() {
        let body = serde_json::json!({
            "error": { "type": "rate_limit_error", "message": "rate limited" },
            "request_id": "req_123"
        });
        let err = convert_anthropic_api_error(429, &body.to_string(), None);
        match err {
            ChatProviderError::Status(APIStatusError { status_code, message, request_id }) => {
                assert_eq!(status_code, 429);
                assert!(message.contains("rate limited"));
                assert_eq!(request_id, Some("req_123".into()));
            }
            _ => panic!("expected status error"),
        }
    }
}
```

运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::error_tests
```

- [ ] 实现错误映射函数：

```rust
fn convert_anthropic_request_error(error: reqwest::Error) -> ChatProviderError {
    if error.is_timeout() {
        ChatProviderError::Timeout(APITimeoutError)
    } else {
        ChatProviderError::Connection(APIConnectionError)
    }
}

fn convert_anthropic_api_error(
    status: u16,
    body: &str,
    fallback_request_id: Option<String>,
) -> ChatProviderError {
    let request_id = fallback_request_id
        .or_else(|| serde_json::from_str::<serde_json::Value>(body).ok()
            .and_then(|v| v.get("request_id").and_then(|r| r.as_str()).map(|s| s.to_string())));
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.get("message").or(Some(e)))
                .and_then(|m| m.as_str().map(|s| s.to_string()))
        })
        .unwrap_or_else(|| body.into());
    normalize_api_status_error(status, message, request_id)
}
```

- [ ] 运行测试：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::error_tests
```

预期：全部通过。

- [ ] Commit: `feat(kosong-rs): anthropic error mapping`

---

### Task 4: 替换 `generate()` 的流式占位，接入 SSE 解析

**Depends on:** Task 3

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/providers/anthropic.rs:1100-1350`（替换 Task 4 的 501 占位）

**实现步骤：**

- [ ] 先写流式集成测试：

```rust
#[cfg(test)]
mod stream_integration_tests {
    use super::*;
    use crate::provider::ChatProvider;
    use httptest::{matchers::*, responders::*, Expectation, Server};

    fn sse_body(events: &[serde_json::Value]) -> String {
        events
            .iter()
            .map(|e| format!("event: {}\ndata: {}\n\n", e["type"].as_str().unwrap_or("message"), e.to_string()))
            .collect()
    }

    #[tokio::test]
    async fn generate_stream_hits_messages_endpoint() {
        let server = Server::run();
        let events = vec![
            serde_json::json!({"type":"message_start","message":{"id":"msg_1","usage":{"input_tokens":10,"output_tokens":0,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}),
            serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}),
            serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" world"}}),
            serde_json::json!({"type":"content_block_stop","index":0}),
            serde_json::json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":2}}),
            serde_json::json!({"type":"message_stop"}),
        ];
        server.expect(
            Expectation::matching(all_of![
                request::method_path("POST", "/v1/messages"),
                request::body(json_decoded(assertion(|v: &serde_json::Value| {
                    assert_eq!(v["stream"], true);
                })))
            ])
            .respond_with(status_code(200).body(sse_body(&events)).content_type("text/event-stream")),
        );

        let provider = AnthropicChatProvider::new(AnthropicOptions {
            model: "claude-opus-4-7".into(),
            api_key: Some("sk-test".into()),
            base_url: Some(server.url_str("/")),
            default_max_tokens: Some(1_024),
            beta_features: Some(vec![]),
            default_headers: None,
            metadata: None,
            stream: Some(true),
            adaptive_thinking: None,
        });

        let mut stream = provider.generate("sys", &[], &[Message::user_text("hi")], None).await.unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(&mut stream).await;
        assert_eq!(parts, vec![StreamedMessagePart::text("Hello"), StreamedMessagePart::text(" world")]);
        assert_eq!(stream.id(), Some("msg_1".into()));
        assert_eq!(stream.finish_reason(), Some(FinishReason::Completed));
    }

    #[tokio::test]
    async fn generate_stream_tool_call() {
        let server = Server::run();
        let events = vec![
            serde_json::json!({"type":"message_start","message":{"id":"msg_2","usage":{"input_tokens":20,"output_tokens":0}}}),
            serde_json::json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"tc_1","name":"read","input":{}}}),
            serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\":\""}}),
            serde_json::json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"/etc/passwd\"}"}}),
            serde_json::json!({"type":"content_block_stop","index":0}),
            serde_json::json!({"type":"message_delta","delta":{"stop_reason":"tool_use"}}),
            serde_json::json!({"type":"message_stop"}),
        ];
        server.expect(
            Expectation::matching(request::method_path("POST", "/v1/messages"))
                .respond_with(status_code(200).body(sse_body(&events)).content_type("text/event-stream")),
        );

        let provider = AnthropicChatProvider::new(AnthropicOptions {
            model: "claude-opus-4-7".into(),
            api_key: Some("sk-test".into()),
            base_url: Some(server.url_str("/")),
            default_max_tokens: Some(1_024),
            beta_features: Some(vec![]),
            default_headers: None,
            metadata: None,
            stream: Some(true),
            adaptive_thinking: None,
        });

        let mut stream = provider.generate("", &[], &[Message::user_text("read")], None).await.unwrap();
        let parts: Vec<_> = futures_util::StreamExt::collect(&mut stream).await;
        assert_eq!(parts[0], StreamedMessagePart::tool_call("tc_1", "read", Some("")));
        assert!(matches!(parts[1], StreamedMessagePart::ToolCallPart(_)));
        assert!(matches!(parts[2], StreamedMessagePart::ToolCallPart(_)));
    }
}
```

运行测试并确认失败（`generate()` 仍返回 501）：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::stream_integration_tests
```

- [ ] 替换 `generate()` 中流式分支的实现：

```rust
async fn generate(...) -> Result<StreamedMessage, ChatProviderError> {
    // ... 前置 abort/callback 钩子保持不变 ...

    let auth = options.as_ref().and_then(|o| o.auth.clone());
    let stream = self.stream;
    let create_params = self.build_create_params(system_prompt, tools, history, stream)?;
    let headers = self.build_extra_headers(auth.as_ref())?;

    let url = format!(
        "{}v1/messages",
        self.base_url.as_deref().unwrap_or("https://api.anthropic.com/")
    );
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|_| ChatProviderError::Connection(APIConnectionError))?;

    let mut req = client.post(&url).json(&create_params);
    for (k, v) in headers {
        req = req.header(k, v);
    }

    // 流式/非流式请求发送前均检查 abort
    if let Some(ref opts) = options {
        if let Some(ref signal) = opts.signal {
            if signal.is_aborted() {
                return Err(ChatProviderError::Aborted(AbortError));
            }
        }
    }

    let response = req.send().await.map_err(convert_anthropic_request_error)?;

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
        return Err(convert_anthropic_api_error(status.as_u16(), &body, None));
    }

    let result = if stream {
        let body = response.text().await.map_err(convert_anthropic_request_error)?;
        let events = parse_sse_body(&body);
        anthropic_events_to_streamed_message(events)
    } else {
        let message_response: AnthropicMessageResponse = response.json().await.map_err(|_| {
            ChatProviderError::Status(APIStatusError {
                status_code: 500,
                message: "Failed to parse Anthropic response JSON".into(),
                request_id: None,
            })
        })?;
        parse_non_stream_response(message_response)
    };

    if let Some(ref opts) = options {
        if let Some(ref hook) = opts.on_stream_end {
            hook();
        }
    }

    result
}
```

- [ ] 运行流式集成测试：

```bash
cd rust-ody && cargo test -p kosong-rs providers::anthropic::stream_integration_tests
```

预期：全部通过。

- [ ] 运行整个 crate 测试：

```bash
cd rust-ody && cargo test -p kosong-rs
```

- [ ] Commit: `feat(kosong-rs): wire streaming SSE path into generate()`

---

### Task 5: L1 golden fixtures 与 Rust golden 二进制

**Depends on:** Task 4

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/bin/anthropic_golden.rs:1-180`
- Modify: `rust-ody/crates/kosong-rs/Cargo.toml:21-28`（新增 `[[bin]]`）
- Create: `packages/integration-tests/src/parity/fixtures/kosong-anthropic/l1-stream-text.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-anthropic/l1-stream-thinking.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-anthropic/l1-stream-tool-call.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-anthropic/l1-stream-parallel-tool-calls.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-anthropic/l1-nonstream-text-tool.json`

**实现步骤：**

- [ ] 在 `Cargo.toml` 新增 binary：

```toml
[[bin]]
name = "kosong-anthropic-golden"
path = "src/bin/anthropic_golden.rs"
```

- [ ] 实现 `anthropic_golden.rs`：

```rust
use std::env;
use std::fs;

use kosong_rs::{
    generate, AnthropicChatProvider, AnthropicOptions, GenerateOptions, Message, ProviderRequestAuth,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    system_prompt: Option<String>,
    tools: Option<Value>,
    history: Vec<Message>,
    options: Option<FixtureOptions>,
    provider_step: ProviderStep,
    #[allow(dead_code)]
    expect_error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FixtureOptions {
    auth: Option<ProviderRequestAuth>,
    stream: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderStep {
    events: Option<Vec<Value>>,
    response: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoldenResult {
    generate_result: Option<GenerateResultOut>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerateResultOut {
    id: Option<String>,
    message: Value,
    usage: Option<kosong_rs::TokenUsage>,
    finish_reason: Option<String>,
    raw_finish_reason: Option<String>,
}

fn events_to_sse_body(events: &[Value]) -> String {
    events
        .iter()
        .map(|e| {
            let event_type = e.get("type").and_then(|v| v.as_str()).unwrap_or("message");
            format!("event: {}\ndata: {}\n\n", event_type, e.to_string())
        })
        .collect()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = env::args().nth(1).expect("fixture path required");
    let input = fs::read_to_string(&path)?;
    let fixture: Fixture = serde_json::from_str(&input)?;

    let stream = fixture.options.as_ref().and_then(|o| o.stream).unwrap_or(true);
    let base_url = {
        let server = httptest::Server::run();
        if stream {
            let events = fixture.provider_step.events.as_ref().expect("stream fixture requires events");
            server.expect(
                httptest::Expectation::matching(httptest::matchers::request::method_path("POST", "/v1/messages"))
                    .respond_with(httptest::responders::status_code(200)
                        .body(events_to_sse_body(events))
                        .content_type("text/event-stream")),
            );
        } else {
            let response = fixture.provider_step.response.as_ref().expect("non-stream fixture requires response");
            server.expect(
                httptest::Expectation::matching(httptest::matchers::request::method_path("POST", "/v1/messages"))
                    .respond_with(httptest::responders::status_code(200)
                        .body(response.to_string())
                        .content_type("application/json")),
            );
        }
        server.url_str("/")
    };

    let provider = AnthropicChatProvider::new(AnthropicOptions {
        model: "claude-opus-4-7".into(),
        api_key: Some("sk-golden".into()),
        base_url: Some(base_url),
        default_max_tokens: Some(1_024),
        beta_features: Some(vec![]),
        default_headers: None,
        metadata: None,
        stream: Some(stream),
        adaptive_thinking: None,
    });

    let tools: Vec<kosong_rs::provider::Tool> = fixture
        .tools
        .and_then(|t| serde_json::from_value(t).ok())
        .unwrap_or_default();
    let options = fixture.options.map(|o| GenerateOptions {
        auth: o.auth,
        ..Default::default()
    });

    let output = match generate(
        &provider,
        &fixture.system_prompt.unwrap_or_default(),
        &tools,
        &fixture.history,
        None,
        options.as_ref(),
    )
    .await
    {
        Ok(r) => GoldenResult {
            generate_result: Some(GenerateResultOut {
                id: r.id,
                message: serde_json::to_value(&r.message)?,
                usage: r.usage,
                finish_reason: r.finish_reason.map(|fr| serde_json::to_string(&fr).unwrap().trim_matches('"').to_string()),
                raw_finish_reason: r.raw_finish_reason,
            }),
            error: None,
        },
        Err(e) => GoldenResult {
            generate_result: None,
            error: Some(format!("{}", e)),
        },
    };

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
```

注意：`httptest` 已经是 dev-dependency，但 binary 默认也参与构建。需要把 `httptest` 从 `[dev-dependencies]` 移到 `[dependencies]`，或者单独为 binary 启用。由于 golden binary 只在测试时由 `cargo build --bin kosong-anthropic-golden` 构建，可保持 `httptest` 为 dev-dependency 并通过 `cargo build --bin kosong-anthropic-golden` 构建（binary 使用 dev deps 仅在 test profile？实际上 `cargo build --bin` 不使用 dev-dependencies）。因此需要把 `httptest` 加入 `[dependencies]`。

- [ ] 将 `httptest` 移至 `[dependencies]`：

```toml
[dependencies]
anyhow = "1"
async-trait = "0.1"
futures-util = { version = "0.3", default-features = false, features = ["std"] }
httptest = "0.16"
regex = "1"
reqwest = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = "1"
tokio = { workspace = true }

[dev-dependencies]
tokio-test = "0.4"
```

- [ ] 创建 fixture `l1-stream-text.json`：

```json
{
  "systemPrompt": "be helpful",
  "history": [{ "role": "user", "content": [{"type":"text","text":"say hi"}], "toolCalls": [] }],
  "options": { "stream": true, "auth": { "apiKey": "sk-golden" } },
  "providerStep": {
    "events": [
      {"type":"message_start","message":{"id":"msg_stream_text","usage":{"input_tokens":10,"output_tokens":0,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}},
      {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}},
      {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}},
      {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" world"}},
      {"type":"content_block_stop","index":0},
      {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":2,"cache_read_input_tokens":0,"cache_creation_input_tokens":0,"input_tokens":10}},
      {"type":"message_stop"}
    ]
  }
}
```

- [ ] 创建 fixture `l1-stream-thinking.json`：

```json
{
  "systemPrompt": "",
  "history": [{ "role": "user", "content": [{"type":"text","text":"think"}], "toolCalls": [] }],
  "options": { "stream": true },
  "providerStep": {
    "events": [
      {"type":"message_start","message":{"id":"msg_stream_think","usage":{"input_tokens":5,"output_tokens":0}}},
      {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"reason"}},
      {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"ing"}},
      {"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig"}},
      {"type":"content_block_stop","index":0},
      {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":2}},
      {"type":"message_stop"}
    ]
  }
}
```

- [ ] 创建 fixture `l1-stream-tool-call.json`：

```json
{
  "systemPrompt": "",
  "tools": [{"name":"read","description":"read file","parameters":{"type":"object","properties":{"path":{"type":"string"}}}}],
  "history": [{ "role": "user", "content": [{"type":"text","text":"read"}], "toolCalls": [] }],
  "options": { "stream": true },
  "providerStep": {
    "events": [
      {"type":"message_start","message":{"id":"msg_stream_tool","usage":{"input_tokens":20,"output_tokens":0}}},
      {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"tc_1","name":"read","input":{}}},
      {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\":\""}},
      {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"/etc/passwd\"}"}},
      {"type":"content_block_stop","index":0},
      {"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":10}},
      {"type":"message_stop"}
    ]
  }
}
```

- [ ] 创建 fixture `l1-stream-parallel-tool-calls.json`：

```json
{
  "systemPrompt": "",
  "tools": [
    {"name":"read","description":"read file","parameters":{"type":"object","properties":{"path":{"type":"string"}}}},
    {"name":"write","description":"write file","parameters":{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}}}}
  ],
  "history": [{ "role": "user", "content": [{"type":"text","text":"parallel"}], "toolCalls": [] }],
  "options": { "stream": true },
  "providerStep": {
    "events": [
      {"type":"message_start","message":{"id":"msg_stream_parallel","usage":{"input_tokens":25,"output_tokens":0}}},
      {"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"tc_read","name":"read","input":{}}},
      {"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tc_write","name":"write","input":{}}},
      {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\":\""}},
      {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\":\""}},
      {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"/etc/passwd\"}"}},
      {"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"/tmp/out\",\"content\":\"x\"}"}},
      {"type":"content_block_stop","index":0},
      {"type":"content_block_stop","index":1},
      {"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":20}},
      {"type":"message_stop"}
    ]
  }
}
```

- [ ] 创建 fixture `l1-nonstream-text-tool.json`：

```json
{
  "systemPrompt": "",
  "tools": [{"name":"read","description":"read file","parameters":{"type":"object","properties":{"path":{"type":"string"}}}}],
  "history": [{ "role": "user", "content": [{"type":"text","text":"read"}], "toolCalls": [] }],
  "options": { "stream": false },
  "providerStep": {
    "response": {
      "id": "msg_nonstream",
      "type": "message",
      "role": "assistant",
      "stop_reason": "tool_use",
      "usage": {"input_tokens": 15, "output_tokens": 10, "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0},
      "content": [
        {"type":"text","text":"I will read it."},
        {"type":"tool_use","id":"tc_1","name":"read","input":{"path":"/etc/passwd"}}
      ]
    }
  }
}
```

- [ ] 构建 golden binary 并验证 fixture：

```bash
cd rust-ody && cargo build -p kosong-rs --bin kosong-anthropic-golden
./target/debug/kosong-anthropic-golden ../packages/integration-tests/src/parity/fixtures/kosong-anthropic/l1-stream-text.json
```

预期输出包含 `generateResult.message.content[0].text == "Hello world"`（由 `generate()` 的合并逻辑产生）以及 `id == "msg_stream_text"`、`finishReason == "completed"`。

- [ ] Commit: `feat(kosong-rs): anthropic L1 golden binary and fixtures`

---

### Task 6: TS golden runner 与 TS↔Rust parity 测试

**Depends on:** Task 5

**Files:**
- Create: `packages/integration-tests/src/parity/kosong-anthropic-golden.ts`
- Create: `packages/integration-tests/test/parity/kosong/l1-anthropic.test.ts`

**实现步骤：**

- [ ] 实现 TS golden runner：

```ts
import Anthropic from '@anthropic-ai/sdk';
import type { MessageStreamEvent, MessageParam, Tool } from '@anthropic-ai/sdk/resources/messages/messages.js';
import { AnthropicChatProvider } from '@odysseythink/kosong/dist/providers/anthropic.js';
import { generate } from '@odysseythink/kosong';
import type { Message } from '@odysseythink/kosong';

export interface Fixture {
  systemPrompt?: string;
  tools?: Tool[];
  history: Message[];
  options?: {
    stream?: boolean;
    auth?: { apiKey?: string; headers?: Record<string, string> };
  };
  providerStep: {
    events?: unknown[];
    response?: {
      id: string;
      stop_reason?: string | null;
      usage: {
        input_tokens: number;
        output_tokens: number;
        cache_read_input_tokens?: number;
        cache_creation_input_tokens?: number;
      };
      content: unknown[];
    };
  };
  expectError?: string | null;
}

function createMockClient(fixture: Fixture): Anthropic {
  const stream = fixture.options?.stream ?? true;
  return {
    messages: {
      create: async (_params: unknown, _options: unknown) => {
        if (stream) {
          const events = fixture.providerStep.events ?? [];
          return (async function* () {
            for (const event of events) {
              yield event as MessageStreamEvent;
            }
          })();
        }
        return fixture.providerStep.response as Anthropic.Messages.Message;
      },
    },
  } as unknown as Anthropic;
}

export async function runTsAnthropicGolden(fixture: Fixture): Promise<{
  generateResult: {
    id: string | null;
    message: unknown;
    usage: unknown;
    finishReason: string | null;
    rawFinishReason: string | null;
  } | null;
  error: string | null;
}> {
  const provider = new AnthropicChatProvider({
    model: 'claude-opus-4-7',
    apiKey: 'sk-golden',
    baseUrl: 'http://localhost:0',
    stream: fixture.options?.stream ?? true,
    defaultMaxTokens: 1024,
    betaFeatures: [],
    clientFactory: () => createMockClient(fixture),
  });

  try {
    const result = await generate(
      provider,
      fixture.systemPrompt ?? '',
      fixture.tools ?? [],
      fixture.history,
      undefined,
      { auth: fixture.options?.auth },
    );
    return {
      generateResult: {
        id: result.id,
        message: result.message,
        usage: result.usage,
        finishReason: result.finishReason,
        rawFinishReason: result.rawFinishReason,
      },
      error: null,
    };
  } catch (e) {
    return { generateResult: null, error: String(e) };
  }
}
```

- [ ] 实现 parity 测试：

```ts
import { existsSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { beforeAll, describe, expect, it } from 'vitest';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { runTsAnthropicGolden, type Fixture } from '../../../src/parity/kosong-anthropic-golden';

function findProjectRoot(): string {
  let current = dirname(fileURLToPath(import.meta.url));
  while (current !== dirname(current)) {
    if (existsSync(join(current, '.git'))) return current;
    current = dirname(current);
  }
  return process.cwd();
}

const rootDir = findProjectRoot();
const fixturesDir = join(
  rootDir,
  'packages',
  'integration-tests',
  'src',
  'parity',
  'fixtures',
  'kosong-anthropic',
);

const fixtures: string[] = [
  'l1-stream-text.json',
  'l1-stream-thinking.json',
  'l1-stream-tool-call.json',
  'l1-stream-parallel-tool-calls.json',
  'l1-nonstream-text-tool.json',
];

function loadFixture(name: string): Fixture {
  const raw = readFileSync(join(fixturesDir, name), 'utf8');
  return JSON.parse(raw);
}

function sortKeys(obj: unknown): unknown {
  if (Array.isArray(obj)) return obj.map(sortKeys);
  if (obj !== null && typeof obj === 'object') {
    const sorted: Record<string, unknown> = {};
    for (const key of Object.keys(obj as Record<string, unknown>).sort()) {
      const val = (obj as Record<string, unknown>)[key];
      if (val === undefined) continue;
      sorted[key] = sortKeys(val);
    }
    return sorted;
  }
  return obj;
}

describe('kosong Anthropic L1 golden parity', () => {
  beforeAll(() => {
    spawnSync('cargo', ['build', '-p', 'kosong-rs', '--bin', 'kosong-anthropic-golden'], {
      cwd: join(rootDir, 'rust-ody'),
      stdio: 'inherit',
    });
  });

  const binaryPath = join(rootDir, 'rust-ody', 'target', 'debug', 'kosong-anthropic-golden');

  it.each(fixtures)('$name TS matches Rust', async (name) => {
    const fixture = loadFixture(name);
    const ts = await runTsAnthropicGolden(fixture);
    const result = spawnSync(binaryPath, [join(fixturesDir, name)], { encoding: 'utf8' });
    if (result.status !== 0) {
      throw new Error(`kosong-anthropic-golden exited ${result.status}: ${result.stderr}`);
    }
    const rust = JSON.parse(result.stdout);

    if (fixture.expectError) {
      expect(ts.error).toBeTruthy();
      expect(rust.error).toBeTruthy();
      return;
    }

    expect(sortKeys(rust)).toStrictEqual(sortKeys(ts));
  });
});
```

- [ ] 安装/确认 `@anthropic-ai/sdk` 在 `packages/integration-tests` 可用。若 `package.json` 未声明，添加 devDependency：

```bash
cd packages/integration-tests && pnpm add -D @anthropic-ai/sdk
```

- [ ] 运行 parity 测试：

```bash
cd packages/integration-tests && pnpm vitest run test/parity/kosong/l1-anthropic.test.ts
```

预期：5 个 fixture 全部通过。

- [ ] Commit: `test(integration-tests): anthropic L1 TS↔Rust parity runner`

---

## Part 3 Self-Review

- [ ] 1. Spec-coverage table: 本部分覆盖 4.2.4.2（stream 事件解析，Task 1-2）、4.2.4.3（non-stream 解析已在 Part 2，Task 4 将其与流式统一）、4.2.4.5（L1 fixtures + parity，Task 5-6）。错误映射补充 §4.2 共享约束。
- [ ] 2. Placeholder scan: 无 TODO/TBD；Part 2 的 501 占位在本 Task 4 被替换为完整 SSE 路径。
- [ ] 3. No phantom tasks: 每个 Task 均产出文件/代码/测试/commit。
- [ ] 4. Dependency soundness: Task 1 → 2 → 3 → 4 → 5 → 6；Task 2 新增的 `StreamedMessage::from_stream` 为纯新增构造函数，无向后依赖。
- [ ] 5. Caller & build soundness: `StreamedMessage::from_stream` 共享类型变更在同一 Task 2 完成；已搜索 `StreamedMessage {` 结构体字面量并确认无其他调用方；以 `cargo check --workspace` 收尾。Task 6 修改 TS 测试文件，未改动共享运行时签名。
- [ ] 6. Test-the-risk: 测试覆盖 SSE 注释/空行跳过、事件反序列化边界、thinking/signature/redacted_thinking/tool_use/input_json_delta/message_delta 全事件类型、HTTP 流式集成、非流式与流式 golden 对照。
- [ ] 7. Type consistency: 复用 `AnthropicSseEvent` 内部类型与 TS 事件类型字段名一致；`GenerateResultOut` 字段与 TS `generate()` 返回值一致；fixture 同时喂给两边，字段名统一使用 camelCase。
