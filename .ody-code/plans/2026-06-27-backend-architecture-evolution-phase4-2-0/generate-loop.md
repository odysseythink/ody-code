# Part 2: Generate Loop + Mock Provider

本部分实现 `kosong-rs` 的核心算法 `generate()`：把 provider 产出的 `StreamedMessagePart` 序列合并成最终的 `Message`，处理 abort、并行 tool-call 路由、空响应与 think-only 拒绝。同时提供一个确定性 mock provider 用于 golden 对照。

---

### Task 5: 定义 `StreamedMessage` 与确定性 `MockProvider`

**Depends on:** Task 3 (provider trait), Task 4 (errors)

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/generate.rs`（初始定义 `StreamedMessage`）
- Create: `rust-ody/crates/kosong-rs/src/mock.rs`
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs`（加入 `pub mod mock;`）

**目标：** 让 `ChatProvider.generate()` 返回可迭代、可携带最终 metadata 的 `StreamedMessage`；mock provider 按固定 parts 列表产出。

**步骤：**

- [ ] 写失败测试。在 `rust-ody/crates/kosong-rs/src/mock.rs` 末尾加入：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Message, StreamedMessagePart};
    use crate::provider::{GenerateOptions, Tool};

    #[tokio::test]
    async fn mock_provider_yields_configured_parts() {
        let provider = MockProvider::new("mock", "m1")
            .with_parts(vec![
                StreamedMessagePart::text("hello"),
                StreamedMessagePart::text(" world"),
            ])
            .with_finish_reason(crate::provider::FinishReason::Completed);

        let stream = provider.generate("sys", &[], &[], None).await.unwrap();
        let parts: Vec<_> = stream.collect().await;
        assert_eq!(parts.len(), 2);
        assert_eq!(stream.id(), Some("resp_1".to_string()));
        assert_eq!(stream.finish_reason(), Some(crate::provider::FinishReason::Completed));
    }
}
```

- [ ] 运行测试并确认失败：

```bash
cd rust-ody && cargo test -p kosong-rs --lib mock::tests
```

预期失败：`MockProvider`、`StreamedMessage` 未定义。

- [ ] 实现 `rust-ody/crates/kosong-rs/src/generate.rs` 的 `StreamedMessage`：

```rust
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::{Stream, StreamExt};

use crate::message::StreamedMessagePart;
use crate::provider::{FinishReason, TokenUsage};

pub struct StreamedMessage {
    id: Option<String>,
    usage: Option<TokenUsage>,
    finish_reason: Option<FinishReason>,
    raw_finish_reason: Option<String>,
    inner: Pin<Box<dyn Stream<Item = StreamedMessagePart> + Send>>,
}

impl StreamedMessage {
    pub fn from_parts(
        parts: Vec<StreamedMessagePart>,
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
            inner: Box::pin(futures_util::stream::iter(parts)),
        }
    }

    pub fn id(&self) -> Option<String> {
        self.id.clone()
    }

    pub fn usage(&self) -> Option<TokenUsage> {
        self.usage
    }

    pub fn finish_reason(&self) -> Option<FinishReason> {
        self.finish_reason
    }

    pub fn raw_finish_reason(&self) -> Option<String> {
        self.raw_finish_reason.clone()
    }
}

impl Stream for StreamedMessage {
    type Item = StreamedMessagePart;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}
```

- [ ] 实现 `rust-ody/crates/kosong-rs/src/mock.rs`：

```rust
use crate::errors::ChatProviderError;
use crate::generate::StreamedMessage;
use crate::message::{Message, StreamedMessagePart};
use crate::provider::{ChatProvider, FinishReason, GenerateOptions, ModelCapability, ThinkingEffort, Tool};

pub struct MockProvider {
    name: String,
    model_name: String,
    parts: Vec<StreamedMessagePart>,
    id: Option<String>,
    usage: Option<crate::usage::TokenUsage>,
    finish_reason: Option<FinishReason>,
    raw_finish_reason: Option<String>,
    thinking_effort: Option<ThinkingEffort>,
}

impl MockProvider {
    pub fn new(name: impl Into<String>, model_name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            model_name: model_name.into(),
            parts: vec![],
            id: None,
            usage: None,
            finish_reason: None,
            raw_finish_reason: None,
            thinking_effort: None,
        }
    }

    pub fn with_parts(mut self, parts: Vec<StreamedMessagePart>) -> Self {
        self.parts = parts;
        self
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn with_usage(mut self, usage: crate::usage::TokenUsage) -> Self {
        self.usage = Some(usage);
        self
    }

    pub fn with_finish_reason(mut self, reason: FinishReason) -> Self {
        self.finish_reason = Some(reason);
        self
    }

    pub fn with_raw_finish_reason(mut self, reason: impl Into<String>) -> Self {
        self.raw_finish_reason = Some(reason.into());
        self
    }
}

#[async_trait::async_trait]
impl ChatProvider for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn thinking_effort(&self) -> Option<ThinkingEffort> {
        self.thinking_effort
    }

    async fn generate(
        &self,
        _system_prompt: &str,
        _tools: &[Tool],
        _history: &[Message],
        _options: Option<GenerateOptions>,
    ) -> Result<StreamedMessage, ChatProviderError> {
        Ok(StreamedMessage::from_parts(
            self.parts.clone(),
            self.id.clone(),
            self.usage,
            self.finish_reason,
            self.raw_finish_reason.clone(),
        ))
    }

    fn with_thinking(&self, effort: ThinkingEffort) -> Box<dyn ChatProvider> {
        let mut clone = self.clone();
        clone.thinking_effort = Some(effort);
        Box::new(clone)
    }
}

impl Clone for MockProvider {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            model_name: self.model_name.clone(),
            parts: self.parts.clone(),
            id: self.id.clone(),
            usage: self.usage,
            finish_reason: self.finish_reason,
            raw_finish_reason: self.raw_finish_reason.clone(),
            thinking_effort: self.thinking_effort,
        }
    }
}
```

- [ ] 更新 `rust-ody/crates/kosong-rs/src/lib.rs`：

```rust
pub mod errors;
pub mod generate;
pub mod message;
pub mod mock;
pub mod provider;
pub mod usage;
```

- [ ] 运行测试并确认通过：

```bash
cd rust-ody && cargo test -p kosong-rs --lib mock::tests
```

- [ ] Commit：`git add rust-ody/crates/kosong-rs/src/generate.rs rust-ody/crates/kosong-rs/src/mock.rs rust-ody/crates/kosong-rs/src/lib.rs && git commit -m "feat(kosong-rs): StreamedMessage and MockProvider"`

---

### Task 6: 实现 `generate()` 主循环（合并与最终 message 组装）

**Depends on:** Task 5

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/generate.rs`

**目标：** 实现 `generate()` 的核心合并循环，复刻 TS 的 `onMessagePart` 回调与最终 `GenerateResult` 构造。

**步骤：**

- [ ] 写失败测试。在 `rust-ody/crates/kosong-rs/src/generate.rs` 末尾加入：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ContentPart, StreamedMessagePart};
    use crate::mock::MockProvider;
    use crate::provider::{FinishReason, GenerateCallbacks, GenerateResult};

    #[tokio::test]
    async fn generate_merges_text_parts() {
        let provider = MockProvider::new("mock", "m1").with_parts(vec![
            StreamedMessagePart::text("hello"),
            StreamedMessagePart::text(" world"),
        ]);
        let result = generate(&provider, "", &[], &[], None, None).await.unwrap();
        assert_eq!(result.message.content, vec![ContentPart::Text { text: "hello world".into() }]);
        assert!(result.message.tool_calls.is_empty());
    }

    #[tokio::test]
    async fn generate_calls_on_message_part() {
        let provider = MockProvider::new("mock", "m1").with_parts(vec![
            StreamedMessagePart::text("a"),
            StreamedMessagePart::text("b"),
        ]);
        let parts = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let parts2 = parts.clone();
        let callbacks = GenerateCallbacks {
            on_message_part: Some(Box::new(move |p| parts2.lock().unwrap().push(p))),
            ..Default::default()
        };
        let _ = generate(&provider, "", &[], &[], Some(&callbacks), None).await.unwrap();
        assert_eq!(parts.lock().unwrap().len(), 2);
    }
}
```

- [ ] 运行测试并确认失败：`generate` 函数未实现。

- [ ] 在 `rust-ody/crates/kosong-rs/src/generate.rs` 中追加 `generate()` 实现：

```rust
use std::collections::HashMap;

use crate::errors::{APIEmptyResponseError, ChatProviderError};
use crate::message::{merge_in_place, ContentPart, Message, StreamedMessagePart, ToolCall};
use crate::provider::{ChatProvider, GenerateCallbacks, GenerateOptions, GenerateResult, Tool};

pub async fn generate(
    provider: &dyn ChatProvider,
    system_prompt: &str,
    tools: &[Tool],
    history: &[Message],
    callbacks: Option<&GenerateCallbacks>,
    _options: Option<&GenerateOptions>,
) -> Result<GenerateResult, ChatProviderError> {
    let mut message = Message::assistant(vec![], vec![]);
    let mut pending_part: Option<StreamedMessagePart> = None;
    let mut tool_call_index_map: HashMap<String, usize> = HashMap::new();

    let mut stream = provider.generate(system_prompt, tools, history, _options.cloned()).await?;

    while let Some(part) = stream.next().await {
        if let Some(cb) = callbacks.and_then(|c| c.on_message_part.as_ref()) {
            cb(part.clone());
        }

        if pending_part.is_none() {
            pending_part = Some(part);
        } else if !merge_in_place(pending_part.as_mut().unwrap(), &part) {
            flush_part(&mut message, pending_part.take().unwrap(), &mut tool_call_index_map);
            pending_part = Some(part);
        }
    }

    if let Some(p) = pending_part {
        flush_part(&mut message, p, &mut tool_call_index_map);
    }

    if message.content.is_empty() && message.tool_calls.is_empty() {
        return Err(ChatProviderError::Empty(APIEmptyResponseError {
            provider: provider.name().to_string(),
            model: provider.model_name().to_string(),
        }));
    }

    Ok(GenerateResult {
        id: stream.id(),
        message,
        usage: stream.usage(),
        finish_reason: stream.finish_reason(),
        raw_finish_reason: stream.raw_finish_reason(),
    })
}

fn flush_part(
    message: &mut Message,
    part: StreamedMessagePart,
    tool_call_index_map: &mut HashMap<String, usize>,
) {
    match part {
        StreamedMessagePart::Content(c) => message.content.push(c),
        StreamedMessagePart::ToolCall(tc) => {
            let stream_index = tc.stream_index.as_ref().map(|v| v.to_string());
            let stored = ToolCall {
                stream_index: None,
                ..tc
            };
            let ordinal = message.tool_calls.len();
            message.tool_calls.push(stored);
            if let Some(idx) = stream_index {
                tool_call_index_map.insert(idx, ordinal);
            }
        }
        StreamedMessagePart::ToolCallPart(_) => {}
    }
}
```

- [ ] 运行测试并确认通过：

```bash
cd rust-ody && cargo test -p kosong-rs --lib generate::tests
```

- [ ] Commit：`git add rust-ody/crates/kosong-rs/src/generate.rs && git commit -m "feat(kosong-rs): generate() merge loop"`

---

### Task 7: 实现 `generate()` abort 检查

**Depends on:** Task 6

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/provider.rs`（`GenerateOptions` 增加 `signal`）
- Modify: `rust-ody/crates/kosong-rs/src/generate.rs`

**目标：** 复刻 TS 的 abort 时机：`provider.generate()` 前后、每个 part 后、每次 callback 后。

**步骤：**

- [ ] 写失败测试。在 `rust-ody/crates/kosong-rs/src/generate.rs` 测试模块加入：

```rust
#[tokio::test]
async fn generate_aborts_before_request() {
    let provider = MockProvider::new("mock", "m1").with_parts(vec![]);
    let signal = AbortSignal::new();
    signal.abort();
    let options = GenerateOptions { signal: Some(signal), auth: None, ..Default::default() };
    let err = generate(&provider, "", &[], &[], None, Some(&options)).await.unwrap_err();
    assert!(format!("{}", err).contains("aborted"));
}

#[tokio::test]
async fn generate_aborts_mid_stream() {
    let signal = AbortSignal::new();
    let provider = MockProvider::new("mock", "m1").with_parts(vec![
        StreamedMessagePart::text("a"),
        StreamedMessagePart::text("b"),
    ]);
    let callbacks = GenerateCallbacks {
        on_message_part: Some(Box::new(move |_| {
            signal.abort();
        })),
        ..Default::default()
    };
    let options = GenerateOptions { signal: Some(signal), auth: None, ..Default::default() };
    let err = generate(&provider, "", &[], &[], Some(&callbacks), Some(&options)).await.unwrap_err();
    assert!(format!("{}", err).contains("aborted"));
}
```

- [ ] 运行测试并确认失败：`AbortSignal` 未实现 abort 逻辑。

- [ ] 在 `rust-ody/crates/kosong-rs/src/errors.rs` 中新增 `AbortError` 并扩展 `ChatProviderError`：

```rust
#[derive(Debug, Clone, thiserror::Error)]
#[error("The operation was aborted.")]
pub struct AbortError;

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
    #[error("The operation was aborted.")]
    Aborted(AbortError),
}
```

- [ ] 在 `rust-ody/crates/kosong-rs/src/generate.rs` 中增加 abort 检查：

```rust
use crate::errors::{AbortError, APIEmptyResponseError, ChatProviderError};
use crate::provider::AbortSignal;

fn throw_if_aborted(signal: Option<&AbortSignal>) -> Result<(), ChatProviderError> {
    if signal.map(|s| s.is_aborted()).unwrap_or(false) {
        Err(ChatProviderError::Aborted(AbortError))
    } else {
        Ok(())
    }
}
```

- [ ] 修改 `generate()`：

```rust
pub async fn generate(
    provider: &dyn ChatProvider,
    system_prompt: &str,
    tools: &[Tool],
    history: &[Message],
    callbacks: Option<&GenerateCallbacks>,
    options: Option<&GenerateOptions>,
) -> Result<GenerateResult, ChatProviderError> {
    let signal = options.and_then(|o| o.signal.as_ref());
    throw_if_aborted(signal)?;

    if let Some(options) = options {
        if let Some(hook) = &options.on_request_start {
            hook();
        }
    }

    let mut message = Message::assistant(vec![], vec![]);
    let mut pending_part: Option<StreamedMessagePart> = None;
    let mut tool_call_index_map: HashMap<String, usize> = HashMap::new();

    let mut stream = provider.generate(system_prompt, tools, history, options.cloned()).await?;
    throw_if_aborted(signal)?;

    while let Some(part) = stream.next().await {
        throw_if_aborted(signal)?;

        if let Some(cb) = callbacks.and_then(|c| c.on_message_part.as_ref()) {
            cb(part.clone());
            throw_if_aborted(signal)?;
        }

        if pending_part.is_none() {
            pending_part = Some(part);
        } else if !merge_in_place(pending_part.as_mut().unwrap(), &part) {
            flush_part(&mut message, pending_part.take().unwrap(), &mut tool_call_index_map);
            pending_part = Some(part);
        }
    }

    throw_if_aborted(signal)?;

    if let Some(p) = pending_part {
        flush_part(&mut message, p, &mut tool_call_index_map);
    }

    if message.content.is_empty() && message.tool_calls.is_empty() {
        return Err(ChatProviderError::Empty(APIEmptyResponseError {
            provider: provider.name().to_string(),
            model: provider.model_name().to_string(),
        }));
    }

    if let Some(options) = options {
        if let Some(hook) = &options.on_stream_end {
            hook();
        }
    }

    Ok(GenerateResult {
        id: stream.id(),
        message,
        usage: stream.usage(),
        finish_reason: stream.finish_reason(),
        raw_finish_reason: stream.raw_finish_reason(),
    })
}
```

- [ ] 运行测试并确认通过：

```bash
cd rust-ody && cargo test -p kosong-rs --lib generate::tests
```

- [ ] Commit：`git add rust-ody/crates/kosong-rs/src/generate.rs rust-ody/crates/kosong-rs/src/errors.rs && git commit -m "feat(kosong-rs): abort handling in generate()"`

---

### Task 8: 并行 tool-call 路由、flush 与空响应/think-only 拒绝

**Depends on:** Task 7

**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/generate.rs`

**目标：** 复刻 TS 的 `toolCallIndexMap` 路由逻辑；处理空响应与 think-only 拒绝；触发 `onToolCall` 回调。

**步骤：**

- [ ] 写失败测试。在 `rust-ody/crates/kosong-rs/src/generate.rs` 测试模块加入：

```rust
#[tokio::test]
async fn generate_routes_parallel_tool_call_deltas() {
    let mut tc1 = StreamedMessagePart::tool_call("id1", "read", None);
    if let StreamedMessagePart::ToolCall(ref mut tc) = tc1 {
        tc.stream_index = Some(serde_json::json!(0));
    }
    let mut tc2 = StreamedMessagePart::tool_call("id2", "write", None);
    if let StreamedMessagePart::ToolCall(ref mut tc) = tc2 {
        tc.stream_index = Some(serde_json::json!(1));
    }
    let mut part0 = StreamedMessagePart::tool_call_part(Some("{\"a\":1"));
    if let StreamedMessagePart::ToolCallPart(ref mut p) = part0 {
        p.index = Some(serde_json::json!(0));
    }
    let mut part1 = StreamedMessagePart::tool_call_part(Some("{\"b\":2"));
    if let StreamedMessagePart::ToolCallPart(ref mut p) = part1 {
        p.index = Some(serde_json::json!(1));
    }
    let mut part0_end = StreamedMessagePart::tool_call_part(Some("}"));
    if let StreamedMessagePart::ToolCallPart(ref mut p) = part0_end {
        p.index = Some(serde_json::json!(0));
    }
    let mut part1_end = StreamedMessagePart::tool_call_part(Some("}"));
    if let StreamedMessagePart::ToolCallPart(ref mut p) = part1_end {
        p.index = Some(serde_json::json!(1));
    }

    let provider = MockProvider::new("mock", "m1").with_parts(vec![
        tc1.clone(),
        tc2.clone(),
        part0,
        part1,
        part0_end,
        part1_end,
    ]);

    let result = generate(&provider, "", &[], &[], None, None).await.unwrap();
    assert_eq!(result.message.tool_calls.len(), 2);
    assert_eq!(result.message.tool_calls[0].arguments.as_deref(), Some("{\"a\":1}"));
    assert_eq!(result.message.tool_calls[1].arguments.as_deref(), Some("{\"b\":2}"));
}

#[tokio::test]
async fn generate_rejects_empty_response() {
    let provider = MockProvider::new("mock", "m1").with_parts(vec![]);
    let err = generate(&provider, "", &[], &[], None, None).await.unwrap_err();
    assert!(format!("{}", err).contains("empty response"));
}

#[tokio::test]
async fn generate_rejects_think_only_response() {
    let provider = MockProvider::new("mock", "m1").with_parts(vec![
        StreamedMessagePart::think("reasoning..."),
    ]);
    let err = generate(&provider, "", &[], &[], None, None).await.unwrap_err();
    assert!(format!("{}", err).contains("thinking content"));
}

#[tokio::test]
async fn generate_calls_on_tool_call() {
    let provider = MockProvider::new("mock", "m1").with_parts(vec![
        StreamedMessagePart::tool_call("id1", "read", Some("{}")),
    ]);
    let called = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let called2 = called.clone();
    let callbacks = GenerateCallbacks {
        on_tool_call: Some(Box::new(move |tc| called2.lock().unwrap().push(tc.name.clone()))),
        ..Default::default()
    };
    let _ = generate(&provider, "", &[], &[], Some(&callbacks), None).await.unwrap();
    assert_eq!(*called.lock().unwrap(), vec!["read"]);
}
```

- [ ] 运行测试并确认失败：并行路由与 think-only 拒绝未实现。

- [ ] 修改 `rust-ody/crates/kosong-rs/src/generate.rs` 的 `generate()` 函数：

```rust
pub async fn generate(
    provider: &dyn ChatProvider,
    system_prompt: &str,
    tools: &[Tool],
    history: &[Message],
    callbacks: Option<&GenerateCallbacks>,
    options: Option<&GenerateOptions>,
) -> Result<GenerateResult, ChatProviderError> {
    use crate::message::{is_tool_call_part, is_tool_call};

    let signal = options.and_then(|o| o.signal.as_ref());
    throw_if_aborted(signal)?;

    if let Some(options) = options {
        if let Some(hook) = &options.on_request_start {
            hook();
        }
    }

    let mut message = Message::assistant(vec![], vec![]);
    let mut pending_part: Option<StreamedMessagePart> = None;
    let mut tool_call_index_map: HashMap<String, usize> = HashMap::new();

    let mut stream = provider.generate(system_prompt, tools, history, options.cloned()).await?;
    throw_if_aborted(signal)?;

    while let Some(part) = stream.next().await {
        throw_if_aborted(signal)?;

        if let Some(cb) = callbacks.and_then(|c| c.on_message_part.as_ref()) {
            let deep = serde_json::from_value(serde_json::to_value(&part).unwrap()).unwrap();
            cb(deep);
            throw_if_aborted(signal)?;
        }

        if is_tool_call_part(&part) {
            if let Some(idx) = part.index().and_then(|i| i.as_str().map(|s| s.to_string()).or_else(|| i.as_u64().map(|n| n.to_string()))) {
                if let Some(&array_idx) = tool_call_index_map.get(&idx) {
                    if let Some(target) = message.tool_calls.get_mut(array_idx) {
                        if let StreamedMessagePart::ToolCallPart(delta) = &part {
                            if let Some(delta_args) = &delta.arguments_part {
                                target.arguments = Some(match &target.arguments {
                                    Some(existing) => format!("{}{}", existing, delta_args),
                                    None => delta_args.clone(),
                                });
                            }
                        }
                        continue;
                    }
                }
            }
        }

        if pending_part.is_none() {
            pending_part = Some(part);
        } else if !merge_in_place(pending_part.as_mut().unwrap(), &part) {
            flush_part(&mut message, pending_part.take().unwrap(), &mut tool_call_index_map);
            pending_part = Some(part);
        }
    }

    throw_if_aborted(signal)?;

    if let Some(p) = pending_part {
        flush_part(&mut message, p, &mut tool_call_index_map);
    }

    if message.content.is_empty() && message.tool_calls.is_empty() {
        return Err(ChatProviderError::Empty(APIEmptyResponseError {
            provider: provider.name().to_string(),
            model: provider.model_name().to_string(),
        }));
    }

    let has_think = message.content.iter().any(|p| matches!(p, ContentPart::Think { .. }));
    let has_text = message.content.iter().any(|p| {
        if let ContentPart::Text { text } = p {
            !text.trim().is_empty()
        } else {
            false
        }
    });
    let has_tool_calls = !message.tool_calls.is_empty();

    if has_think && !has_text && !has_tool_calls {
        return Err(ChatProviderError::Empty(APIEmptyResponseError {
            provider: provider.name().to_string(),
            model: provider.model_name().to_string(),
        }));
    }

    if let Some(cb) = callbacks.and_then(|c| c.on_tool_call.as_ref()) {
        for tc in &message.tool_calls {
            throw_if_aborted(signal)?;
            cb(tc.clone());
        }
    }

    if let Some(options) = options {
        if let Some(hook) = &options.on_stream_end {
            hook();
        }
    }

    Ok(GenerateResult {
        id: stream.id(),
        message,
        usage: stream.usage(),
        finish_reason: stream.finish_reason(),
        raw_finish_reason: stream.raw_finish_reason(),
    })
}
```

- [ ] 为 `StreamedMessagePart` 增加 `index()` 辅助方法。在 `message.rs` 中：

```rust
impl StreamedMessagePart {
    pub fn index(&self) -> Option<&serde_json::Value> {
        match self {
            StreamedMessagePart::ToolCallPart(p) => p.index.as_ref(),
            StreamedMessagePart::ToolCall(tc) => tc.stream_index.as_ref(),
            _ => None,
        }
    }
}
```

- [ ] 运行测试并确认通过：

```bash
cd rust-ody && cargo test -p kosong-rs --lib generate::tests
```

- [ ] Commit：`git add rust-ody/crates/kosong-rs/src/generate.rs rust-ody/crates/kosong-rs/src/message.rs && git commit -m "feat(kosong-rs): tool-call routing and empty/think-only rejection"`

---

## Local Self-Review

- [ ] 1. Spec-coverage table: 本部分覆盖 4.2.0.3（`generate()` 循环）与 4.2.0.4（`ChatProvider` trait 在 mock 中落地）。
- [ ] 2. Placeholder scan: 无 TODO；所有代码片段完整。
- [ ] 3. No phantom tasks: 每个任务产生可编译/可测试变更。
- [ ] 4. Dependency soundness: Task 5 依赖 Part 1 的 trait/错误；Task 6 依赖 Task 5；Task 7 依赖 Task 6；Task 8 依赖 Task 7。
- [ ] 5. Caller & build soundness: Task 7 扩展 `ChatProviderError` 增加 `Aborted`，需同步更新所有 `match`（本 crate 内）。每次任务结束运行 `cargo test -p kosong-rs`。
- [ ] 6. Test-the-risk: 合并逻辑、abort 时机、并行 tool-call 参数路由、空响应/think-only 拒绝均有行为断言。
- [ ] 7. Type consistency: `StreamedMessage` 在 Task 5 定义，`AbortSignal` 与 `GenerateOptions` 在 Part 1 Task 3 定义，后续 Part 3 的 golden runner 直接使用。
