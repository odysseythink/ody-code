# Part 2: Response Parsing

本 Part 在 `kosong-rs/src/openai_responses.rs` 中实现 OpenAI Responses API 的流式/非流式响应解析，包括 `response.id` 捕获、finish reason 映射、usage 提取、并行 function-call 路由、final arguments suffix 校验、reasoning summary 事件与 error / `response.failed` 映射。Part 1 的 `generate()` 桩将在本 Part 最后被替换为真实解析。

---

## Dependency Overview (Part 2)

```text
Task 5: 非流式响应解析（output items → text/function_call/reasoning/usage/finish reason）
  │
  ├──► Task 6: 流式事件路由（output_text.delta / response.created|in_progress / response.completed|incomplete / error / response.failed）
  │      │
  │      ├──► Task 7: 流式 function-call 解析（output_item.added / function_call_arguments.delta|done / output_item.done / 路由 + suffix 校验）
  │      │       │
  │      │       └──► Task 8: 流式 reasoning summary 解析 + generate() 替换
```

- Task 5 不依赖后续任务；建立 `OpenAIResponsesStreamedMessage` 适配器与公共 helper（字段读取、output item view、finish reason 映射）。
- Task 6 依赖 Task 5 的 helper 与 `from_non_stream_bytes`；新增 `from_stream_bytes` 处理基础事件。
- Task 7 依赖 Task 6 的 `from_stream_bytes` 框架；扩展 function-call 事件处理。
- Task 8 依赖 Task 7 的 function-call 路由；扩展 reasoning summary 事件，并替换 `generate()` 的桩返回。

---

### Task 5: 非流式响应解析

**Depends on:** Part 1 Task 4（`generate()` 桩与 `StreamedMessage::from_parts` 已存在）

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/openai_responses.rs`（新增 response parsing helper 与 `OpenAIResponsesStreamedMessage::from_non_stream_bytes`）
- Test: `rust-ody/crates/kosong-rs/src/openai_responses.rs` 内 `#[cfg(test)]`

步骤：

- [ ] **写失败测试**：在 `tests` 模块中加入：

```rust
    use crate::generate::StreamedMessage;
    use crate::usage::TokenUsage;

    fn collect_parts(msg: StreamedMessage) -> Vec<StreamedMessagePart> {
        futures_util::executor::block_on_stream(Box::pin(msg)).collect()
    }

    #[test]
    fn parses_non_stream_text_response() {
        let body = br#"{"id":"resp-1","status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"Hello world"}]}],"usage":{"input_tokens":10,"output_tokens":2}}"#;
        let msg = OpenAIResponsesStreamedMessage::from_non_stream_bytes(body.to_vec()).unwrap();
        assert_eq!(msg.id(), Some("resp-1".into()));
        assert_eq!(msg.finish_reason(), Some(FinishReason::Completed));
        assert_eq!(msg.raw_finish_reason(), Some("completed".into()));
        let usage = msg.usage().unwrap();
        assert_eq!(usage.input_other, 10);
        assert_eq!(usage.output, 2);
        let parts = collect_parts(msg);
        assert_eq!(parts, vec![StreamedMessagePart::text("Hello world")]);
    }

    #[test]
    fn parses_non_stream_function_call() {
        let body = br#"{"id":"resp-2","status":"completed","output":[{"type":"function_call","id":"fc-1","call_id":"call_1","name":"read","arguments":"{}"}]}"#;
        let msg = OpenAIResponsesStreamedMessage::from_non_stream_bytes(body.to_vec()).unwrap();
        let parts = collect_parts(msg);
        assert_eq!(parts.len(), 1);
        match &parts[0] {
            StreamedMessagePart::ToolCall(tc) => {
                assert_eq!(tc.id, "call_1");
                assert_eq!(tc.name, "read");
                assert_eq!(tc.arguments.as_deref(), Some("{}"));
            }
            _ => panic!("expected ToolCall"),
        }
    }

    #[test]
    fn parses_non_stream_reasoning() {
        let body = br#"{"id":"resp-3","status":"completed","output":[{"type":"reasoning","encrypted_content":"enc","summary":[{"type":"summary_text","text":"step1"},{"type":"summary_text","text":"step2"}]}]}"#;
        let msg = OpenAIResponsesStreamedMessage::from_non_stream_bytes(body.to_vec()).unwrap();
        let parts = collect_parts(msg);
        assert_eq!(parts.len(), 2);
        match &parts[0] {
            StreamedMessagePart::Content(ContentPart::Think { think, encrypted }) => {
                assert_eq!(think, "step1");
                assert_eq!(encrypted.as_deref(), Some("enc"));
            }
            _ => panic!("expected Think"),
        }
        match &parts[1] {
            StreamedMessagePart::Content(ContentPart::Think { think, encrypted }) => {
                assert_eq!(think, "step2");
                assert_eq!(encrypted.as_deref(), Some("enc"));
            }
            _ => panic!("expected Think"),
        }
    }

    #[test]
    fn parses_non_stream_incomplete_max_output_tokens() {
        let body = br#"{"id":"resp-4","status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},"output":[]}"#;
        let msg = OpenAIResponsesStreamedMessage::from_non_stream_bytes(body.to_vec()).unwrap();
        assert_eq!(msg.finish_reason(), Some(FinishReason::Truncated));
        assert_eq!(msg.raw_finish_reason(), Some("max_output_tokens".into()));
    }
```

- [ ] **运行并确认失败**：

```bash
cd rust-ody && cargo test -p kosong-rs openai_responses::tests::parses_non_stream
```

预期失败：未找到 `OpenAIResponsesStreamedMessage` 与 `from_non_stream_bytes`。

- [ ] **写最小实现**：在 `openai_responses.rs` 的 `use` 区域追加：

```rust
use std::sync::atomic::{AtomicU64, Ordering};

use crate::generate::StreamedMessage;
use crate::provider::FinishReason;
use crate::usage::TokenUsage;
```

确保已有 `use crate::message::{ContentPart, Message, Role, StreamedMessagePart, ToolCall, UrlPayload};`（Task 2 已引入）。

在文件中加入 helper 与适配器（位置在 Task 1 的 `OpenAIResponsesOptions` 之前或之后均可，建议放在 `convert_message` 之后）：

```rust
static FUNCTION_CALL_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

fn function_call_id(call_id: Option<&str>) -> String {
    match call_id {
        Some(s) if !s.is_empty() => s.into(),
        _ => format!("call-{}", FUNCTION_CALL_ID_COUNTER.fetch_add(1, Ordering::Relaxed)),
    }
}

fn read_string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).map(String::from)
}

fn read_nullable_string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| {
        if v.is_null() {
            Some(String::new())
        } else {
            v.as_str().map(String::from)
        }
    })
}

fn read_number_field(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|v| v.as_i64())
}

fn read_object_field(value: &Value, key: &str) -> Option<Value> {
    value.get(key).cloned().filter(|v| v.is_object())
}

fn read_object_array_field(value: &Value, key: &str) -> Option<Vec<Value>> {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .cloned()
        .map(|arr| arr.into_iter().filter(|v| v.is_object()).collect())
}

fn require_string_field(value: &Value, key: &str, context: &str) -> Result<String, ChatProviderError> {
    read_string_field(value, key).ok_or_else(|| {
        ChatProviderError::Other(format!(
            "OpenAI Responses decode error: {}.{} must be a string.",
            context, key
        ))
    })
}

fn require_object_field(value: &Value, key: &str, context: &str) -> Result<Value, ChatProviderError> {
    read_object_field(value, key).ok_or_else(|| {
        ChatProviderError::Other(format!(
            "OpenAI Responses decode error: {}.{} must be an object.",
            context, key
        ))
    })
}

fn normalize_responses_finish_reason(
    status: Option<&str>,
    incomplete_reason: Option<&str>,
) -> (Option<FinishReason>, Option<String>) {
    match status {
        Some("completed") => (Some(FinishReason::Completed), Some("completed".into())),
        Some("incomplete") => match incomplete_reason {
            Some("max_output_tokens") => {
                (Some(FinishReason::Truncated), Some("max_output_tokens".into()))
            }
            Some("content_filter") => {
                (Some(FinishReason::Filtered), Some("content_filter".into()))
            }
            _ => (
                Some(FinishReason::Other),
                Some(incomplete_reason.unwrap_or("incomplete").into()),
            ),
        },
        Some("failed") => (Some(FinishReason::Other), Some("failed".into())),
        _ => (None, None),
    }
}

#[derive(Debug, Clone)]
enum ResponseOutputItemView {
    Message { content: Vec<Value> },
    FunctionCall {
        item_id: Option<String>,
        call_id: Option<String>,
        name: Option<String>,
        arguments: Option<String>,
    },
    Reasoning {
        encrypted_content: Option<String>,
        summary: Vec<Value>,
    },
    Other,
}

fn read_response_output_item(
    value: &Value,
    context: &str,
) -> Result<ResponseOutputItemView, ChatProviderError> {
    let item_type = require_string_field(value, "type", context)?;
    match item_type.as_str() {
        "message" => Ok(ResponseOutputItemView::Message {
            content: read_object_array_field(value, "content").unwrap_or_default(),
        }),
        "function_call" => Ok(ResponseOutputItemView::FunctionCall {
            item_id: read_string_field(value, "id"),
            call_id: read_string_field(value, "call_id"),
            name: read_string_field(value, "name"),
            arguments: read_nullable_string_field(value, "arguments"),
        }),
        "reasoning" => Ok(ResponseOutputItemView::Reasoning {
            encrypted_content: read_string_field(value, "encrypted_content"),
            summary: read_object_array_field(value, "summary").unwrap_or_default(),
        }),
        _ => Ok(ResponseOutputItemView::Other),
    }
}

pub fn parse_responses_non_stream(
    body: &[u8],
) -> Result<
    (
        Vec<StreamedMessagePart>,
        Option<String>,
        Option<TokenUsage>,
        Option<FinishReason>,
        Option<String>,
    ),
    ChatProviderError,
> {
    let response: Value = serde_json::from_slice(body)
        .map_err(|e| ChatProviderError::Other(format!("OpenAI Responses decode error: {}", e)))?;
    if !response.is_object() {
        return Err(ChatProviderError::Other(
            "OpenAI Responses decode error: response must be an object".into(),
        ));
    }

    let id = read_string_field(&response, "id");
    let usage = read_object_field(&response, "usage").and_then(|u| extract_usage(&u));
    let status = read_string_field(&response, "status").as_deref();
    let incomplete = read_object_field(&response, "incomplete_details");
    let incomplete_reason = incomplete
        .as_ref()
        .and_then(|v| read_string_field(v, "reason"))
        .as_deref();
    let (finish_reason, raw_finish_reason) =
        normalize_responses_finish_reason(status, incomplete_reason);

    let mut parts = Vec::new();
    let output = read_object_array_field(&response, "output").unwrap_or_default();
    for item in output {
        match read_response_output_item(&item, "response.output item")? {
            ResponseOutputItemView::Message { content } => {
                for content_item in content {
                    if read_string_field(&content_item, "type").as_deref() == Some("output_text") {
                        if let Some(text) = read_string_field(&content_item, "text") {
                            parts.push(StreamedMessagePart::text(text));
                        }
                    }
                }
            }
            ResponseOutputItemView::FunctionCall {
                call_id, name, arguments, ..
            } => {
                let name = name.ok_or_else(|| {
                    ChatProviderError::Other(
                        "OpenAI Responses function_call item is missing a name.".into(),
                    )
                })?;
                parts.push(StreamedMessagePart::ToolCall(ToolCall {
                    call_type: "function".into(),
                    id: function_call_id(call_id.as_deref()),
                    name,
                    arguments,
                    extras: None,
                    stream_index: None,
                }));
            }
            ResponseOutputItemView::Reasoning {
                encrypted_content,
                summary,
            } => {
                for summary_item in summary {
                    if read_string_field(&summary_item, "type").as_deref() == Some("summary_text") {
                        if let Some(text) = read_string_field(&summary_item, "text") {
                            parts.push(StreamedMessagePart::Content(ContentPart::Think {
                                think: text,
                                encrypted: encrypted_content.clone(),
                            }));
                        }
                    }
                }
            }
            ResponseOutputItemView::Other => {}
        }
    }

    Ok((parts, id, usage, finish_reason, raw_finish_reason))
}

pub struct OpenAIResponsesStreamedMessage;

impl OpenAIResponsesStreamedMessage {
    pub fn from_non_stream_bytes(body: Vec<u8>) -> Result<StreamedMessage, ChatProviderError> {
        let (parts, id, usage, finish_reason, raw_finish_reason) = parse_responses_non_stream(&body)?;
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

> 说明：`function_call_id` 在 `call_id` 缺失时回退为单调递增值；L1 fixture 中的 function_call 项应始终提供 `call_id` 以保证 TS↔Rust 输出可比。

- [ ] **运行并确认通过**：

```bash
cd rust-ody && cargo test -p kosong-rs openai_responses::tests::parses_non_stream
```

预期：4 个测试通过。

- [ ] **Commit**：`feat(kosong-rs): OpenAI Responses non-stream parsing`

---

### Task 6: 流式事件路由

**Depends on:** Task 5

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/openai_responses.rs`（新增 stream parser 基础事件处理与 `from_stream_bytes`）
- Test: `rust-ody/crates/kosong-rs/src/openai_responses.rs` 内 `#[cfg(test)]`

步骤：

- [ ] **写失败测试**：在 `tests` 模块中加入：

```rust
    #[test]
    fn parses_stream_text_deltas() {
        let body = br#"{"type":"response.created","response":{"id":"resp-stream-1"}}
{"type":"response.output_text.delta","delta":"Hello"}
{"type":"response.output_text.delta","delta":" world"}
{"type":"response.completed","response":{"id":"resp-stream-1","usage":{"input_tokens":5,"output_tokens":2},"status":"completed"}}"#;
        let msg = OpenAIResponsesStreamedMessage::from_stream_bytes(body.to_vec()).unwrap();
        assert_eq!(msg.id(), Some("resp-stream-1".into()));
        assert_eq!(msg.finish_reason(), Some(FinishReason::Completed));
        let parts = collect_parts(msg);
        assert_eq!(parts, vec![StreamedMessagePart::text("Hello"), StreamedMessagePart::text(" world")]);
    }

    #[test]
    fn parses_stream_incomplete_max_output_tokens() {
        let body = br#"{"type":"response.incomplete","response":{"id":"resp-inc","status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},"usage":{"input_tokens":1,"output_tokens":1}}}"#;
        let msg = OpenAIResponsesStreamedMessage::from_stream_bytes(body.to_vec()).unwrap();
        assert_eq!(msg.finish_reason(), Some(FinishReason::Truncated));
        assert_eq!(msg.raw_finish_reason(), Some("max_output_tokens".into()));
    }

    #[test]
    fn parses_stream_error_event() {
        let body = br#"{"type":"error","code":"rate_limit_exceeded","message":"Rate limited","param":null}"#;
        let err = OpenAIResponsesStreamedMessage::from_stream_bytes(body.to_vec()).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("OpenAI Responses stream error"));
        assert!(msg.contains("rate_limit_exceeded"));
        assert!(msg.contains("Rate limited"));
    }

    #[test]
    fn parses_stream_response_failed() {
        let body = br#"{"type":"response.failed","response":{"error":{"code":"server_error","message":"Internal error"}}}"#;
        let err = OpenAIResponsesStreamedMessage::from_stream_bytes(body.to_vec()).unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("OpenAI Responses response.failed"));
        assert!(msg.contains("server_error"));
        assert!(msg.contains("Internal error"));
    }
```

- [ ] **运行并确认失败**：

```bash
cd rust-ody && cargo test -p kosong-rs openai_responses::tests::parses_stream
```

预期失败：未找到 `from_stream_bytes` 与 `parse_responses_stream`。

- [ ] **写最小实现**：在 `openai_responses.rs` 中加入 error 事件 helper：

```rust
use crate::errors::APIContextOverflowError;

fn format_responses_error_event(
    code: Option<&str>,
    message: &str,
    param: Option<&str>,
) -> String {
    let code = code.unwrap_or("unknown");
    let param_text = match param {
        Some(p) => format!(" (param: {})", p),
        None => String::new(),
    };
    format!("{}: {}{}", code, message, param_text)
}

fn error_from_openai_responses_event(
    prefix: &str,
    code: Option<&str>,
    message: &str,
    param: Option<&str>,
) -> ChatProviderError {
    let formatted = format_responses_error_event(code, message, param);
    let full_message = format!("{}: {}", prefix, formatted);
    if is_context_overflow_error_code(code) {
        ChatProviderError::ContextOverflow(APIContextOverflowError {
            status_code: 400,
            message: full_message,
            request_id: None,
        })
    } else {
        ChatProviderError::Other(full_message)
    }
}

struct FailedResponseError {
    code: String,
    message: String,
}

fn read_responses_failed_response_error(response: &Value) -> Option<FailedResponseError> {
    read_object_field(response, "error").map(|error| {
        let code = read_nullable_string_field(&error, "code").unwrap_or_else(|| "unknown".into());
        let message =
            read_string_field(&error, "message").unwrap_or_else(|| "no message".into());
        FailedResponseError { code, message }
    })
}

fn format_responses_failed_response(response: &Value) -> String {
    if let Some(error) = read_responses_failed_response_error(response) {
        return format_responses_error_event(Some(&error.code), &error.message, None);
    }
    if let Some(incomplete) = read_object_field(response, "incomplete_details") {
        if let Some(reason) = read_string_field(&incomplete, "reason") {
            return format!("incomplete: {}", reason);
        }
    }
    "Unknown error (no error details in response)".into()
}
```

再加入 stream parser：

```rust
#[derive(Default)]
struct StreamParserState {
    id: Option<String>,
    usage: Option<TokenUsage>,
    finish_reason: Option<FinishReason>,
    raw_finish_reason: Option<String>,
}

fn capture_finish_reason_from_response(state: &mut StreamParserState, response: &Value) {
    let status = read_string_field(response, "status").as_deref();
    let incomplete = read_object_field(response, "incomplete_details");
    let incomplete_reason = incomplete
        .as_ref()
        .and_then(|v| read_string_field(v, "reason"))
        .as_deref();
    let (fr, rfr) = normalize_responses_finish_reason(status, incomplete_reason);
    if state.finish_reason.is_none() {
        state.finish_reason = fr;
    }
    if state.raw_finish_reason.is_none() {
        state.raw_finish_reason = rfr;
    }
}

fn process_stream_event(
    event: &Value,
    parts: &mut Vec<StreamedMessagePart>,
    state: &mut StreamParserState,
) -> Result<(), ChatProviderError> {
    let event_type = require_string_field(event, "type", "stream event")?;
    match event_type.as_str() {
        "response.output_text.delta" => {
            parts.push(StreamedMessagePart::text(require_string_field(
                event,
                "delta",
                "response.output_text.delta",
            )?));
        }
        "response.created" | "response.in_progress" => {
            let response = require_object_field(event, "response", &event_type)?;
            if let Some(resp_id) = read_string_field(&response, "id") {
                state.id = Some(resp_id);
            }
        }
        "response.completed" | "response.incomplete" => {
            let response = require_object_field(event, "response", &event_type)?;
            if let Some(resp_id) = read_string_field(&response, "id") {
                state.id = Some(resp_id);
            }
            if let Some(usage) = read_object_field(&response, "usage") {
                state.usage = extract_usage(&usage);
            }
            capture_finish_reason_from_response(state, &response);
        }
        "error" => {
            let message = require_string_field(event, "message", "error")?;
            let code = read_nullable_string_field(event, "code");
            let param = read_nullable_string_field(event, "param");
            return Err(error_from_openai_responses_event(
                "OpenAI Responses stream error",
                code.as_deref(),
                &message,
                param.as_deref(),
            ));
        }
        "response.failed" => {
            let response = require_object_field(event, "response", "response.failed")?;
            if let Some(error) = read_responses_failed_response_error(&response) {
                return Err(error_from_openai_responses_event(
                    "OpenAI Responses response.failed",
                    Some(&error.code),
                    &error.message,
                    None,
                ));
            }
            return Err(ChatProviderError::Other(format!(
                "OpenAI Responses response.failed: {}",
                format_responses_failed_response(&response)
            )));
        }
        _ => {}
    }
    Ok(())
}

pub fn parse_responses_stream(
    body: &[u8],
) -> Result<
    (
        Vec<StreamedMessagePart>,
        Option<String>,
        Option<TokenUsage>,
        Option<FinishReason>,
        Option<String>,
    ),
    ChatProviderError,
> {
    let text = std::str::from_utf8(body).map_err(|e| {
        ChatProviderError::Other(format!(
            "OpenAI Responses stream is not valid UTF-8: {}",
            e
        ))
    })?;
    let mut parts = Vec::new();
    let mut state = StreamParserState::default();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let event: Value = serde_json::from_str(line)
            .map_err(|e| ChatProviderError::Other(format!("OpenAI Responses decode error: {}", e)))?;
        process_stream_event(&event, &mut parts, &mut state)?;
    }

    Ok((parts, state.id, state.usage, state.finish_reason, state.raw_finish_reason))
}
```

并扩展 `OpenAIResponsesStreamedMessage`：

```rust
impl OpenAIResponsesStreamedMessage {
    pub fn from_non_stream_bytes(body: Vec<u8>) -> Result<StreamedMessage, ChatProviderError> {
        let (parts, id, usage, finish_reason, raw_finish_reason) = parse_responses_non_stream(&body)?;
        Ok(StreamedMessage::from_parts(
            parts,
            id,
            usage,
            finish_reason,
            raw_finish_reason,
        ))
    }

    pub fn from_stream_bytes(body: Vec<u8>) -> Result<StreamedMessage, ChatProviderError> {
        let (parts, id, usage, finish_reason, raw_finish_reason) = parse_responses_stream(&body)?;
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

- [ ] **运行并确认通过**：

```bash
cd rust-ody && cargo test -p kosong-rs openai_responses::tests::parses_stream
```

预期：4 个测试通过。

- [ ] **Commit**：`feat(kosong-rs): OpenAI Responses stream routing basics`

---

### Task 7: 流式 function-call 解析

**Depends on:** Task 6

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/openai_responses.rs`（扩展 `StreamParserState` 与 `process_stream_event` function-call 分支）
- Test: `rust-ody/crates/kosong-rs/src/openai_responses.rs` 内 `#[cfg(test)]`

步骤：

- [ ] **写失败测试**：在 `tests` 模块中加入：

```rust
    #[test]
    fn parses_stream_single_function_call() {
        let body = br#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"fc-1","call_id":"call_1","name":"read","arguments":""}}
{"type":"response.function_call_arguments.delta","item_id":"fc-1","output_index":0,"delta":"{\"a\":1"}
{"type":"response.function_call_arguments.delta","item_id":"fc-1","output_index":0,"delta":"}"}
{"type":"response.function_call_arguments.done","item_id":"fc-1","output_index":0,"arguments":"{\"a\":1}"}
{"type":"response.completed","response":{"status":"completed"}}"#;
        let msg = OpenAIResponsesStreamedMessage::from_stream_bytes(body.to_vec()).unwrap();
        let parts = collect_parts(msg);
        assert_eq!(parts.len(), 3);
        match &parts[0] {
            StreamedMessagePart::ToolCall(tc) => {
                assert_eq!(tc.id, "call_1");
                assert_eq!(tc.name, "read");
                assert_eq!(tc.arguments.as_deref(), Some(""));
                assert_eq!(tc.stream_index.as_ref().and_then(|v| v.as_u64()), Some(0));
            }
            _ => panic!("expected ToolCall"),
        }
        assert_eq!(parts[1], StreamedMessagePart::ToolCallPart(ToolCallPart {
            part_type: "tool_call_part".into(),
            arguments_part: Some("{\"a\":1".into()),
            index: Some(serde_json::json!(0)),
        }));
        assert_eq!(parts[2], StreamedMessagePart::ToolCallPart(ToolCallPart {
            part_type: "tool_call_part".into(),
            arguments_part: Some("}".into()),
            index: Some(serde_json::json!(0)),
        }));
    }

    #[test]
    fn parses_stream_parallel_function_calls_by_item_id() {
        let body = br#"{"type":"response.output_item.added","item":{"type":"function_call","id":"fc-a","call_id":"call_a","name":"read","arguments":""}}
{"type":"response.output_item.added","item":{"type":"function_call","id":"fc-b","call_id":"call_b","name":"write","arguments":""}}
{"type":"response.function_call_arguments.delta","item_id":"fc-b","delta":"{\"b\":2"}
{"type":"response.function_call_arguments.delta","item_id":"fc-a","delta":"{\"a\":1"}
{"type":"response.function_call_arguments.done","item_id":"fc-b","arguments":"{\"b\":2}"}
{"type":"response.function_call_arguments.done","item_id":"fc-a","arguments":"{\"a\":1}"}
{"type":"response.completed","response":{"status":"completed"}}"#;
        let msg = OpenAIResponsesStreamedMessage::from_stream_bytes(body.to_vec()).unwrap();
        let parts = collect_parts(msg);
        let deltas: Vec<_> = parts.iter().filter_map(|p| match p {
            StreamedMessagePart::ToolCallPart(tp) => tp.arguments_part.clone(),
            _ => None,
        }).collect();
        assert_eq!(deltas, vec!["{\"b\":2}", "{\"a\":1}"]);
    }

    #[test]
    fn rejects_stream_final_arguments_suffix_mismatch() {
        let body = br#"{"type":"response.output_item.added","item":{"type":"function_call","id":"fc-1","call_id":"call_1","name":"read","arguments":""}}
{"type":"response.function_call_arguments.delta","item_id":"fc-1","delta":"{\"a\":1"}
{"type":"response.function_call_arguments.done","item_id":"fc-1","arguments":"{\"b\":2}"}
{"type":"response.completed","response":{"status":"completed"}}"#;
        let err = OpenAIResponsesStreamedMessage::from_stream_bytes(body.to_vec()).unwrap_err();
        assert!(format!("{}", err).contains("do not match the streamed argument deltas"));
    }

    #[test]
    fn parses_stream_output_item_done_with_final_arguments_suffix() {
        let body = br#"{"type":"response.output_item.added","item":{"type":"function_call","id":"fc-1","call_id":"call_1","name":"read","arguments":"{\"a\":1"}}
{"type":"response.output_item.done","item":{"type":"function_call","id":"fc-1","call_id":"call_1","name":"read","arguments":"{\"a\":123}"},"output_index":7}
{"type":"response.completed","response":{"status":"completed"}}"#;
        let msg = OpenAIResponsesStreamedMessage::from_stream_bytes(body.to_vec()).unwrap();
        let parts = collect_parts(msg);
        assert_eq!(parts.len(), 2);
        match &parts[0] {
            StreamedMessagePart::ToolCall(tc) => {
                assert_eq!(tc.stream_index.as_ref().and_then(|v| v.as_u64()), Some(7));
            }
            _ => panic!("expected ToolCall"),
        }
        match &parts[1] {
            StreamedMessagePart::ToolCallPart(tp) => {
                assert_eq!(tp.arguments_part.as_deref(), Some("23"));
                assert_eq!(tp.index.as_ref().and_then(|v| v.as_u64()), Some(7));
            }
            _ => panic!("expected ToolCallPart"),
        }
    }
```

- [ ] **运行并确认失败**：

```bash
cd rust-ody && cargo test -p kosong-rs openai_responses::tests::parses_stream.*function
```

预期失败：未找到 function-call 相关处理逻辑，测试无法通过。

- [ ] **写最小实现**：替换 `StreamParserState` 为带 function-call 状态版本：

```rust
#[derive(Default)]
struct StreamParserState {
    id: Option<String>,
    usage: Option<TokenUsage>,
    finish_reason: Option<FinishReason>,
    raw_finish_reason: Option<String>,
    function_call_args: HashMap<Value, String>,
    unindexed_args: Option<String>,
}

impl StreamParserState {
    fn has_args(&self, index: Option<&Value>) -> bool {
        match index {
            None => self.unindexed_args.is_some(),
            Some(v) => self.function_call_args.contains_key(v),
        }
    }

    fn get_args(&self, index: Option<&Value>) -> &str {
        match index {
            None => self.unindexed_args.as_deref().unwrap_or(""),
            Some(v) => self.function_call_args.get(v).map(String::as_str).unwrap_or(""),
        }
    }

    fn set_args(&mut self, index: Option<Value>, value: String) {
        match index {
            None => self.unindexed_args = Some(value),
            Some(v) => {
                self.function_call_args.insert(v, value);
            }
        }
    }

    fn append_args(
        &mut self,
        index: Option<&Value>,
        part: &str,
        context: &str,
    ) -> Result<(), ChatProviderError> {
        if !self.has_args(index) {
            return Err(ChatProviderError::Other(format!(
                "OpenAI Responses decode error: {} received function-call arguments for unknown stream index {}.",
                context,
                format_response_stream_index(index)
            )));
        }
        let new_value = format!("{}{}", self.get_args(index), part);
        self.set_args(index.cloned(), new_value);
        Ok(())
    }

    fn yield_final_args_suffix(
        &mut self,
        index: Option<&Value>,
        final_args: &str,
        context: &str,
    ) -> Result<Option<String>, ChatProviderError> {
        if !self.has_args(index) {
            return Err(ChatProviderError::Other(format!(
                "OpenAI Responses decode error: {} received final function-call arguments for unknown stream index {}.",
                context,
                format_response_stream_index(index)
            )));
        }
        let accumulated = self.get_args(index).to_string();
        if final_args == accumulated {
            return Ok(None);
        }
        if !final_args.starts_with(&accumulated) {
            return Err(ChatProviderError::Other(format!(
                "OpenAI Responses final function-call arguments for stream index {} do not match the streamed argument deltas.",
                format_response_stream_index(index)
            )));
        }
        let suffix = &final_args[accumulated.len()..];
        self.set_args(index.cloned(), final_args.into());
        if suffix.is_empty() {
            Ok(None)
        } else {
            Ok(Some(suffix.into()))
        }
    }
}

fn format_response_stream_index(index: Option<&Value>) -> String {
    match index {
        None => "<unindexed>".into(),
        Some(v) => v.to_string(),
    }
}

fn response_stream_index(item_id: Option<&str>, output_index: Option<i64>) -> Option<Value> {
    item_id
        .map(|s| Value::String(s.into()))
        .or_else(|| output_index.map(|n| Value::Number(n.into())))
}
```

在 `process_stream_event` 的 `match` 中追加以下 arms（放在 `"response.completed"` 之前或 `_` 之前）：

```rust
        "response.output_item.added" => {
            let item = read_response_output_item(
                event.get("item").unwrap_or(&Value::Null),
                &format!("{}.item", event_type),
            )?;
            let output_index = read_number_field(event, "output_index");
            if let ResponseOutputItemView::FunctionCall {
                item_id,
                call_id,
                name,
                arguments,
            } = item
            {
                let stream_index = response_stream_index(item_id.as_deref(), output_index);
                state.set_args(stream_index.clone(), arguments.clone().unwrap_or_default());
                let name = name.ok_or_else(|| {
                    ChatProviderError::Other(
                        "OpenAI Responses function_call item is missing a name.".into(),
                    )
                })?;
                parts.push(StreamedMessagePart::ToolCall(ToolCall {
                    call_type: "function".into(),
                    id: function_call_id(call_id.as_deref()),
                    name,
                    arguments,
                    extras: None,
                    stream_index: stream_index.clone(),
                }));
            }
        }
        "response.output_item.done" => {
            let item = read_response_output_item(
                event.get("item").unwrap_or(&Value::Null),
                &format!("{}.item", event_type),
            )?;
            let output_index = read_number_field(event, "output_index");
            match item {
                ResponseOutputItemView::Reasoning { encrypted_content, .. } => {
                    parts.push(StreamedMessagePart::Content(ContentPart::Think {
                        think: "".into(),
                        encrypted: encrypted_content,
                    }));
                }
                ResponseOutputItemView::FunctionCall {
                    item_id,
                    arguments: Some(args),
                    ..
                } => {
                    let stream_index = response_stream_index(item_id.as_deref(), output_index);
                    if let Some(suffix) =
                        state.yield_final_args_suffix(stream_index.as_ref(), &args, &event_type)?
                    {
                        parts.push(StreamedMessagePart::ToolCallPart(ToolCallPart {
                            part_type: "tool_call_part".into(),
                            arguments_part: Some(suffix),
                            index: stream_index,
                        }));
                    }
                }
                _ => {}
            }
        }
        "response.function_call_arguments.delta" => {
            let stream_index = response_stream_index(
                read_string_field(event, "item_id").as_deref(),
                read_number_field(event, "output_index"),
            );
            let delta = require_string_field(event, "delta", "response.function_call_arguments.delta")?;
            state.append_args(stream_index.as_ref(), &delta, "response.function_call_arguments.delta")?;
            parts.push(StreamedMessagePart::ToolCallPart(ToolCallPart {
                part_type: "tool_call_part".into(),
                arguments_part: Some(delta),
                index: stream_index,
            }));
        }
        "response.function_call_arguments.done" => {
            let stream_index = response_stream_index(
                read_string_field(event, "item_id").as_deref(),
                read_number_field(event, "output_index"),
            );
            let args = require_string_field(event, "arguments", "response.function_call_arguments.done")?;
            if let Some(suffix) =
                state.yield_final_args_suffix(stream_index.as_ref(), &args, "response.function_call_arguments.done")?
            {
                parts.push(StreamedMessagePart::ToolCallPart(ToolCallPart {
                    part_type: "tool_call_part".into(),
                    arguments_part: Some(suffix),
                    index: stream_index,
                }));
            }
        }
```

> 说明：`output_item.done` 中的 reasoning 分支已提前放入，Task 8 不再扩展该分支；Task 8 只增加 `reasoning_summary_*` 事件。

- [ ] **运行并确认通过**：

```bash
cd rust-ody && cargo test -p kosong-rs openai_responses::tests::parses_stream
```

预期：Task 6 的 4 个测试 + Task 7 的 4 个测试全部通过。

- [ ] **Commit**：`feat(kosong-rs): OpenAI Responses stream function-call routing`

---

### Task 8: 流式 reasoning summary 解析与 generate() 替换

**Depends on:** Task 7

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/openai_responses.rs`（增加 reasoning summary 事件、增加 `from_bytes` 调度、替换 `generate()` 桩）
- Test: `rust-ody/crates/kosong-rs/src/openai_responses.rs` 内 `#[cfg(test)]`

步骤：

- [ ] **写失败测试**：在 `tests` 模块中加入：

```rust
    #[test]
    fn parses_stream_reasoning_summary() {
        let body = br#"{"type":"response.reasoning_summary_part.added"}
{"type":"response.reasoning_summary_text.delta","delta":"step1"}
{"type":"response.reasoning_summary_text.delta","delta":" step2"}
{"type":"response.output_item.done","item":{"type":"reasoning","encrypted_content":"enc","summary":[]}}
{"type":"response.completed","response":{"status":"completed"}}"#;
        let msg = OpenAIResponsesStreamedMessage::from_stream_bytes(body.to_vec()).unwrap();
        let parts = collect_parts(msg);
        assert_eq!(parts.len(), 3);
        match &parts[0] {
            StreamedMessagePart::Content(ContentPart::Think { think, encrypted }) => {
                assert_eq!(think, "");
                assert!(encrypted.is_none());
            }
            _ => panic!("expected Think"),
        }
        match &parts[1] {
            StreamedMessagePart::Content(ContentPart::Think { think, .. }) => assert_eq!(think, "step1"),
            _ => panic!("expected Think"),
        }
        match &parts[2] {
            StreamedMessagePart::Content(ContentPart::Think { think, encrypted }) => {
                assert_eq!(think, "");
                assert_eq!(encrypted.as_deref(), Some("enc"));
            }
            _ => panic!("expected Think"),
        }
    }

    #[tokio::test]
    async fn generate_parses_stream_response() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let body = br#"{"type":"response.created","response":{"id":"resp-real"}}
{"type":"response.output_text.delta","delta":"Hi"}
{"type":"response.completed","response":{"id":"resp-real","status":"completed","usage":{"input_tokens":3,"output_tokens":1}}}"#;
        let client = Arc::new(RecordingMockHttpClient {
            status: 200,
            body: body.to_vec(),
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
        let msg = provider.generate("sys", &[], &[Message::user_text("hi")], None).await.unwrap();
        assert_eq!(msg.id(), Some("resp-real".into()));
        let parts: Vec<_> = msg.collect().await;
        assert_eq!(parts, vec![StreamedMessagePart::text("Hi")]);
    }
```

- [ ] **运行并确认失败**：

```bash
cd rust-ody && cargo test -p kosong-rs openai_responses::tests::parses_stream_reasoning
```

预期失败：未找到 reasoning summary 事件处理；`generate_parses_stream_response` 仍返回空桩。

- [ ] **写最小实现**：

1. 在 `process_stream_event` 的 `match` 中追加：

```rust
        "response.reasoning_summary_part.added" => {
            parts.push(StreamedMessagePart::think(""));
        }
        "response.reasoning_summary_text.delta" => {
            parts.push(StreamedMessagePart::think(require_string_field(
                event,
                "delta",
                "response.reasoning_summary_text.delta",
            )?));
        }
```

2. 扩展 `OpenAIResponsesStreamedMessage` 增加 `from_bytes`：

```rust
impl OpenAIResponsesStreamedMessage {
    pub fn from_non_stream_bytes(body: Vec<u8>) -> Result<StreamedMessage, ChatProviderError> {
        let (parts, id, usage, finish_reason, raw_finish_reason) = parse_responses_non_stream(&body)?;
        Ok(StreamedMessage::from_parts(
            parts,
            id,
            usage,
            finish_reason,
            raw_finish_reason,
        ))
    }

    pub fn from_stream_bytes(body: Vec<u8>) -> Result<StreamedMessage, ChatProviderError> {
        let (parts, id, usage, finish_reason, raw_finish_reason) = parse_responses_stream(&body)?;
        Ok(StreamedMessage::from_parts(
            parts,
            id,
            usage,
            finish_reason,
            raw_finish_reason,
        ))
    }

    pub fn from_bytes(body: Vec<u8>, is_stream: bool) -> Result<StreamedMessage, ChatProviderError> {
        if is_stream {
            Self::from_stream_bytes(body)
        } else {
            Self::from_non_stream_bytes(body)
        }
    }
}
```

3. 替换 `ChatProvider::generate` 末尾的桩：

将：

```rust
        // Task 4 仅验证请求能发出去；响应解析由 Part 2 完整实现。
        // 这里返回空 StreamedMessage，使编译与测试通过。
        Ok(StreamedMessage::from_parts(
            vec![],
            Some("resp-stub".into()),
            None,
            None,
            None,
        ))
```

替换为：

```rust
        OpenAIResponsesStreamedMessage::from_bytes(bytes, true)
```

即 `generate()` 最后：

```rust
        if status < 200 || status >= 300 {
            let body_str = String::from_utf8_lossy(&bytes);
            let (msg, code) = parse_error_body(&body_str);
            return Err(convert_openai_error(&msg, Some(status), code.as_deref()));
        }

        OpenAIResponsesStreamedMessage::from_bytes(bytes, true)
```

- [ ] **运行并确认通过**：

```bash
cd rust-ody && cargo test -p kosong-rs openai_responses::tests::parses_stream_reasoning
cd rust-ody && cargo test -p kosong-rs openai_responses::tests::generate_parses_stream_response
```

预期：2 个测试通过。

- [ ] **运行 crate 级测试保证无回归**：

```bash
cd rust-ody && cargo test -p kosong-rs
```

预期：所有已有测试通过。

- [ ] **Commit**：`feat(kosong-rs): OpenAI Responses reasoning summary and end-to-end generate()`

---

## Local Self-Review

- [x] 1. Spec-coverage table（索引中）：Part 2 覆盖 4.2.3.2（stream 事件解析）、4.2.3.3（non-stream 解析）、4.2.3.5 的响应侧 golden 输入。
- [x] 2. Placeholder scan：Task 4 的 `from_parts(vec![], ...)` 桩在 Task 8 被替换；其余无 TODO/TBD。流式 function-call/reasoning 事件按 Task 6→7→8 顺序实现，不是占位符。
- [x] 3. No phantom tasks：Task 5–8 均有 Files + 测试 + commit，每个任务产生可验证的代码变更。
- [x] 4. Dependency soundness：Task 5 → Task 6 → Task 7 → Task 8，无向后依赖；Task 8 仅使用 Task 5–7 已定义的 helper 与适配器。
- [x] 5. Caller & build soundness：本 Part 不修改既有共享签名；`generate()` 返回类型不变（仍为 `StreamedMessage`），仅替换实现；Task 8 以 crate 级 `cargo test -p kosong-rs` 结束。
- [x] 6. Test-the-risk：response.id 捕获、finish reason 映射、usage 提取、并行 function-call 路由与交错 delta、final arguments suffix 校验、reasoning summary 拼接、error / response.failed 映射均有行为断言。
- [x] 7. Type consistency：复用 Part 1 与 4.2.0/4.2.1/4.2.2 已定义类型（`StreamedMessagePart`、`ToolCall`、`ToolCallPart`、`ContentPart::Think`、`FinishReason`、`TokenUsage`、`ChatProviderError`、`extract_usage`）；新增 helper 字段名与 TS 实现逐值对齐。
