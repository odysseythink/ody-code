use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::{Stream, StreamExt};

use crate::errors::{APIEmptyResponseError, AbortError, ChatProviderError};
use crate::message::{
    is_tool_call_part, merge_in_place, ContentPart, Message, StreamedMessagePart, ToolCall,
};
use crate::provider::{
    AbortSignal, ChatProvider, FinishReason, GenerateCallbacks, GenerateOptions, GenerateResult,
    Tool,
};
use crate::usage::TokenUsage;

pub struct StreamedMessage {
    id: Option<String>,
    usage: Option<TokenUsage>,
    finish_reason: Option<FinishReason>,
    raw_finish_reason: Option<String>,
    inner: Pin<Box<dyn Stream<Item = StreamedMessagePart> + Send>>,
}

impl std::fmt::Debug for StreamedMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamedMessage")
            .field("id", &self.id)
            .field("usage", &self.usage)
            .field("finish_reason", &self.finish_reason)
            .field("raw_finish_reason", &self.raw_finish_reason)
            .finish()
    }
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

fn throw_if_aborted(signal: Option<&AbortSignal>) -> Result<(), ChatProviderError> {
    if signal.map(|s| s.is_aborted()).unwrap_or(false) {
        Err(ChatProviderError::Aborted(AbortError))
    } else {
        Ok(())
    }
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

    let mut stream = provider
        .generate(system_prompt, tools, history, options.cloned())
        .await?;
    throw_if_aborted(signal)?;

    while let Some(part) = stream.next().await {
        throw_if_aborted(signal)?;

        if let Some(cb) = callbacks.and_then(|c| c.on_message_part.as_ref()) {
            // Deep copy to avoid aliasing mutations (like TS structuredClone)
            let deep = serde_json::from_value(serde_json::to_value(&part).unwrap()).unwrap();
            cb(deep);
            throw_if_aborted(signal)?;
        }

        // Index-based routing for parallel tool call argument deltas
        if is_tool_call_part(&part) {
            if let Some(idx) = part.index().and_then(|i| {
                i.as_str()
                    .map(|s| s.to_string())
                    .or_else(|| i.as_u64().map(|n| n.to_string()))
            }) {
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
            flush_part(
                &mut message,
                pending_part.take().unwrap(),
                &mut tool_call_index_map,
            );
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

    let has_think = message
        .content
        .iter()
        .any(|p| matches!(p, ContentPart::Think { .. }));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ToolCall, ToolCallPart};
    use crate::mock::MockProvider;
    use std::sync::{Arc, Mutex};

    #[tokio::test]
    async fn generate_merges_text_parts() {
        let provider = MockProvider::new("mock", "m1").with_parts(vec![
            StreamedMessagePart::text("hello"),
            StreamedMessagePart::text(" world"),
        ]);
        let result = generate(&provider, "", &[], &[], None, None).await.unwrap();
        assert_eq!(
            result.message.content,
            vec![ContentPart::Text {
                text: "hello world".into()
            }]
        );
        assert!(result.message.tool_calls.is_empty());
    }

    #[tokio::test]
    async fn generate_calls_on_message_part() {
        let provider = MockProvider::new("mock", "m1").with_parts(vec![
            StreamedMessagePart::text("a"),
            StreamedMessagePart::text("b"),
        ]);
        let parts = Arc::new(Mutex::new(Vec::new()));
        let parts2 = parts.clone();
        let callbacks = GenerateCallbacks {
            on_message_part: Some(Box::new(move |p| parts2.lock().unwrap().push(p))),
            ..Default::default()
        };
        let _ = generate(&provider, "", &[], &[], Some(&callbacks), None)
            .await
            .unwrap();
        assert_eq!(parts.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn generate_aborts_before_request() {
        let provider = MockProvider::new("mock", "m1").with_parts(vec![]);
        let signal = AbortSignal::new();
        signal.abort();
        let options = GenerateOptions {
            signal: Some(signal),
            ..Default::default()
        };
        let err = generate(&provider, "", &[], &[], None, Some(&options))
            .await
            .unwrap_err();
        assert!(format!("{}", err).contains("aborted"));
    }

    #[tokio::test]
    async fn generate_aborts_mid_stream() {
        let signal = AbortSignal::new();
        let provider = MockProvider::new("mock", "m1").with_parts(vec![
            StreamedMessagePart::text("a"),
            StreamedMessagePart::text("b"),
        ]);
        let signal_clone = signal.clone();
        let callbacks = GenerateCallbacks {
            on_message_part: Some(Box::new(move |_| {
                signal_clone.abort();
            })),
            ..Default::default()
        };
        let options = GenerateOptions {
            signal: Some(signal),
            ..Default::default()
        };
        let err = generate(&provider, "", &[], &[], Some(&callbacks), Some(&options))
            .await
            .unwrap_err();
        assert!(format!("{}", err).contains("aborted"));
    }

    #[tokio::test]
    async fn generate_routes_parallel_tool_call_deltas() {
        // Build ToolCalls with stream_index
        let tc1 = {
            let tc = ToolCall {
                call_type: "function".into(),
                id: "id1".into(),
                name: "read".into(),
                arguments: None,
                extras: None,
                stream_index: Some(serde_json::json!(0)),
            };
            StreamedMessagePart::ToolCall(tc)
        };
        let tc2 = {
            let tc = ToolCall {
                call_type: "function".into(),
                id: "id2".into(),
                name: "write".into(),
                arguments: None,
                extras: None,
                stream_index: Some(serde_json::json!(1)),
            };
            StreamedMessagePart::ToolCall(tc)
        };
        let part0 = {
            let p = ToolCallPart {
                part_type: "tool_call_part".into(),
                arguments_part: Some("{\"a\":1".into()),
                index: Some(serde_json::json!(0)),
            };
            StreamedMessagePart::ToolCallPart(p)
        };
        let part1 = {
            let p = ToolCallPart {
                part_type: "tool_call_part".into(),
                arguments_part: Some("{\"b\":2".into()),
                index: Some(serde_json::json!(1)),
            };
            StreamedMessagePart::ToolCallPart(p)
        };
        let part0_end = {
            let p = ToolCallPart {
                part_type: "tool_call_part".into(),
                arguments_part: Some("}".into()),
                index: Some(serde_json::json!(0)),
            };
            StreamedMessagePart::ToolCallPart(p)
        };
        let part1_end = {
            let p = ToolCallPart {
                part_type: "tool_call_part".into(),
                arguments_part: Some("}".into()),
                index: Some(serde_json::json!(1)),
            };
            StreamedMessagePart::ToolCallPart(p)
        };

        let provider = MockProvider::new("mock", "m1")
            .with_parts(vec![tc1, tc2, part0, part1, part0_end, part1_end]);

        let result = generate(&provider, "", &[], &[], None, None).await.unwrap();
        assert_eq!(result.message.tool_calls.len(), 2);
        assert_eq!(
            result.message.tool_calls[0].arguments.as_deref(),
            Some("{\"a\":1}")
        );
        assert_eq!(
            result.message.tool_calls[1].arguments.as_deref(),
            Some("{\"b\":2}")
        );
    }

    #[tokio::test]
    async fn generate_rejects_empty_response() {
        let provider = MockProvider::new("mock", "m1").with_parts(vec![]);
        let err = generate(&provider, "", &[], &[], None, None)
            .await
            .unwrap_err();
        assert!(format!("{}", err).contains("empty response"));
    }

    #[tokio::test]
    async fn generate_rejects_think_only_response() {
        let provider = MockProvider::new("mock", "m1")
            .with_parts(vec![StreamedMessagePart::think("reasoning...")]);
        let err = generate(&provider, "", &[], &[], None, None)
            .await
            .unwrap_err();
        assert!(format!("{}", err).contains("empty response"));
    }

    #[tokio::test]
    async fn generate_calls_on_tool_call() {
        let provider =
            MockProvider::new("mock", "m1").with_parts(vec![StreamedMessagePart::tool_call(
                "id1",
                "read",
                Some("{}"),
            )]);
        let called = Arc::new(Mutex::new(Vec::new()));
        let called2 = called.clone();
        let callbacks = GenerateCallbacks {
            on_tool_call: Some(Box::new(move |tc| {
                called2.lock().unwrap().push(tc.name.clone())
            })),
            ..Default::default()
        };
        let _ = generate(&provider, "", &[], &[], Some(&callbacks), None)
            .await
            .unwrap();
        assert_eq!(*called.lock().unwrap(), vec!["read"]);
    }
}
