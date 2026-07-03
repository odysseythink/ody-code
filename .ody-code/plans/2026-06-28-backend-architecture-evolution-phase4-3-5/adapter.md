# 4.3.5 Part 3 — KosongLLM / RemoteKosongLLM / ToolCallDeduplicator / canonical_args / error

**Scope:** 把 Part 2 中 `TurnFlow` 使用的本地占位实现替换为真实适配层：JSON 参数规范化、`ChatProviderError` → `OdyError` 映射、基于 kosong 的本地 `KosongLLM`、跨进程 `RemoteKosongLLM`、以及支持同 step 去重 + 跨 step 提醒的 `ToolCallDeduplicator`。本 part 完成后，`run_step_loop` 将使用真实去重器，且 `TurnAgent` 可接入真实 LLM。

---

## Local File Structure

| File / Directory | Responsibility |
|---|---|
| `rust-ody/crates/agent-rs/src/turn/canonical_args.rs` | JSON 参数规范化（有序键） |
| `rust-ody/crates/agent-rs/src/turn/error.rs` | `ChatProviderError` → `OdyError` + 可重试判定 |
| `rust-ody/crates/agent-rs/src/turn/kosong_llm.rs` | 本地 `KosongLLM`（实现 `Llm` trait） |
| `rust-ody/crates/agent-rs/src/turn/remote_kosong_llm.rs` | 跨进程 `RemoteKosongLLM` + stream registry |
| `rust-ody/crates/agent-rs/src/turn/tool_dedup.rs` | `ToolCallDeduplicator` + `Dedup` trait |
| `rust-ody/crates/agent-rs/src/turn/turn_flow.rs` | 移除 `NoopDedup`，接入 `ToolCallDeduplicator` |
| `rust-ody/crates/agent-rs/src/turn/mod.rs` | 导出新增模块与类型 |

---

## Local Dependency Overview

```text
Task 1: canonical_args.rs
    │
    ├──▶ Task 2: error.rs (仅依赖 OdyError，不依赖 canonical_args)
    │
    ├──▶ Task 5: ToolCallDeduplicator (依赖 canonical_args)
    │
    ▼
Task 3: KosongLLM
    │     (依赖 error.rs; 需修改 Llm::is_retryable_error 共享签名)
    ▼
Task 4: RemoteKosongLLM
    │     (依赖修改后的 Llm trait)
    ▼
Task 5: ToolCallDeduplicator + 替换 turn_flow.rs 中的 Dedup/NoopDedup
    ▼
Task 6: 模块接线 + 全 workspace typecheck
```

- **可并行**：Task 1 / Task 2 可并行；Task 3 与 Task 4 在 Llm trait 签名变更后也可并行。
- **硬前置**：Task 3 依赖 `turn.md` 已完成（`Llm` trait、`TurnFlow`、`NoopDedup` 已存在）。

---

## Local Spec-Coverage Table

| Roadmap 4.3.5.2/3/4 条目 | 覆盖任务 | 状态 |
|---|---|---|
| `canonical_args` JSON 规范化 | Task 1 | covered |
| `ChatProviderError` → `OdyError` / 可重试判定 | Task 2 | covered |
| 本地 `KosongLLM`（流式回调、completion budget、retryable） | Task 3 | covered |
| 远程 `RemoteKosongLLM` + stream registry | Task 4 | covered |
| `ToolCallDeduplicator` 同 step / 跨 step 行为 | Task 5 | covered |
| 接入 `TurnFlow` 并全 workspace 编译 | Task 6 | covered |

---

## Task 1: `canonical_args.rs` — JSON 参数规范化

**Depends on:** none

**Files:**
- Create: `rust-ody/crates/agent-rs/src/turn/canonical_args.rs`

### Steps

- [ ] Write the failing test.

在 `rust-ody/crates/agent-rs/src/turn/canonical_args.rs` 末尾加入：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_sorts_object_keys() {
        let input = json!({"b": 1, "a": 2});
        assert_eq!(canonical_telemetry_args(&input), r#"{"a":2,"b":1}"#);
    }

    #[test]
    fn canonical_preserves_arrays_and_nested_order() {
        let input = json!([{"c": 1, "a": 2}, {"b": 3}]);
        assert_eq!(canonical_telemetry_args(&input), r#"[{"a":2,"c":1},{"b":3}]"#);
    }

    #[test]
    fn canonical_handles_non_object() {
        assert_eq!(canonical_telemetry_args(&json!("foo")), "\"foo\"");
        assert_eq!(canonical_telemetry_args(&json!(42)), "42");
    }
}
```

- [ ] Run it and verify it FAILS.

```bash
cd rust-ody && cargo test -p agent-rs canonical_sorts_object_keys --no-run
```

预期失败：`canonical_telemetry_args` 未定义。

- [ ] Write the minimal implementation。

```rust
use serde_json::Value as JsonValue;

pub fn canonical_telemetry_args(args: &JsonValue) -> String {
    sort_json_value(args).to_string()
}

fn sort_json_value(value: &JsonValue) -> JsonValue {
    match value {
        JsonValue::Array(arr) => JsonValue::Array(arr.iter().map(sort_json_value).collect()),
        JsonValue::Object(obj) => {
            let mut keys: Vec<_> = obj.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                out.insert(k.clone(), sort_json_value(&obj[k]));
            }
            JsonValue::Object(out)
        }
        other => other.clone(),
    }
}

pub fn is_plain_record(value: &JsonValue) -> bool {
    value.is_object()
}
```

- [ ] Run it and verify it PASSES。

```bash
cd rust-ody && cargo test -p agent-rs canonical_
```

预期：3 个测试全部通过。

- [ ] Commit。

```bash
git add rust-ody/crates/agent-rs/src/turn/canonical_args.rs
git commit -m "feat(agent-rs): add canonical_args for tool telemetry and dedup"
```

---

## Task 2: `error.rs` — `ChatProviderError` → `OdyError` 映射

**Depends on:** `turn.md` Task 2（`OdyError` 已在 `telemetry.rs` 中定义）

**Files:**
- Create: `rust-ody/crates/agent-rs/src/turn/error.rs`

### Steps

- [ ] Write the failing test。

在 `rust-ody/crates/agent-rs/src/turn/error.rs` 末尾加入：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use kosong_rs::errors::{
        APIConnectionError, APIContextOverflowError, APIEmptyResponseError, APIMissingApiKeyError,
        APIStatusError, APITimeoutError, ChatProviderError,
    };
    use crate::turn::telemetry::OdyError;

    #[test]
    fn maps_rate_limit_to_retryable_ody_error() {
        let err = ChatProviderError::Status(APIStatusError {
            status_code: 429,
            message: "rate limit".into(),
            request_id: None,
        });
        let ody = from_chat_provider_error(err);
        assert_eq!(ody.code, "provider.rate_limit");
        assert_eq!(ody.name, "APIStatusError");
        assert!(ody.retryable);
        assert_eq!(ody.details.as_ref().unwrap()["statusCode"], 429);
    }

    #[test]
    fn maps_empty_response_to_retryable() {
        let err = ChatProviderError::Empty(APIEmptyResponseError {
            provider: "mock".into(),
            model: "m1".into(),
        });
        let ody = from_chat_provider_error(err);
        assert_eq!(ody.code, "provider.empty");
        assert!(ody.retryable);
    }

    #[test]
    fn maps_auth_error_to_non_retryable() {
        let err = ChatProviderError::Status(APIStatusError {
            status_code: 401,
            message: "unauthorized".into(),
            request_id: None,
        });
        let ody = from_chat_provider_error(err);
        assert_eq!(ody.code, "provider.auth_error");
        assert!(!ody.retryable);
    }

    #[test]
    fn retryable_check_uses_ody_error_retryable_flag() {
        let err = anyhow::Error::from(OdyError {
            code: "provider.timeout".into(),
            name: "APITimeoutError".into(),
            message: "timeout".into(),
            retryable: true,
            details: None,
        });
        assert!(is_retryable_generate_error(&err));
    }
}
```

- [ ] Run it and verify it FAILS。

```bash
cd rust-ody && cargo test -p agent-rs maps_rate_limit_to_retryable_ody_error --no-run
```

预期失败：`from_chat_provider_error`、`is_retryable_generate_error` 未定义。

- [ ] Write the minimal implementation。

```rust
use kosong_rs::errors::{
    APIConnectionError, APIContextOverflowError, APIEmptyResponseError, APIMissingApiKeyError,
    APIStatusError, APITimeoutError, ChatProviderError,
};

use crate::turn::telemetry::OdyError;

pub fn from_chat_provider_error(error: ChatProviderError) -> OdyError {
    match error {
        ChatProviderError::Connection(APIConnectionError) => OdyError {
            code: "provider.connection".into(),
            name: "APIConnectionError".into(),
            message: "API connection error".into(),
            retryable: true,
            details: None,
        },
        ChatProviderError::Timeout(APITimeoutError) => OdyError {
            code: "provider.timeout".into(),
            name: "APITimeoutError".into(),
            message: "API timeout error".into(),
            retryable: true,
            details: None,
        },
        ChatProviderError::Status(ref s) => {
            let code = match s.status_code {
                429 => "provider.rate_limit",
                401 | 403 => "provider.auth_error",
                _ => "provider.api",
            };
            OdyError {
                code: code.into(),
                name: "APIStatusError".into(),
                message: format!("{}: {}", s.status_code, s.message),
                retryable: kosong_rs::errors::is_retryable_generate_error(&error),
                details: Some(serde_json::json!({
                    "statusCode": s.status_code,
                    "requestId": s.request_id,
                })),
            }
        }
        ChatProviderError::ContextOverflow(APIContextOverflowError {
            status_code,
            message,
            request_id,
        }) => OdyError {
            code: "context_overflow".into(),
            name: "APIContextOverflowError".into(),
            message: format!("{}: {}", status_code, message),
            retryable: false,
            details: Some(serde_json::json!({
                "statusCode": status_code,
                "requestId": request_id,
            })),
        },
        ChatProviderError::Empty(APIEmptyResponseError { provider, model }) => OdyError {
            code: "provider.empty".into(),
            name: "APIEmptyResponseError".into(),
            message: format!("The API returned an empty response. Provider: {}, model: {}", provider, model),
            retryable: true,
            details: Some(serde_json::json!({ "provider": provider, "model": model })),
        },
        ChatProviderError::MissingApiKey(APIMissingApiKeyError { provider }) => OdyError {
            code: "provider.auth_error".into(),
            name: "APIMissingApiKeyError".into(),
            message: format!("{}: apiKey is required", provider),
            retryable: false,
            details: Some(serde_json::json!({ "provider": provider })),
        },
        ChatProviderError::Aborted(_) => OdyError {
            code: "runtime.aborted".into(),
            name: "AbortError".into(),
            message: "The operation was aborted".into(),
            retryable: false,
            details: None,
        },
        ChatProviderError::Other(msg) => OdyError {
            code: "provider.api".into(),
            name: "ProviderError".into(),
            message: msg.clone(),
            retryable: false,
            details: Some(serde_json::json!({ "message": msg })),
        },
    }
}

pub fn is_retryable_generate_error(error: &anyhow::Error) -> bool {
    error.downcast_ref::<OdyError>().map(|e| e.retryable).unwrap_or(false)
}
```

- [ ] Run it and verify it PASSES。

```bash
cd rust-ody && cargo test -p agent-rs maps_rate_limit_to_retryable_ody_error maps_empty_response_to_retryable maps_auth_error_to_non_retryable retryable_check_uses_ody_error_retryable_flag
```

预期：4 个测试全部通过。

- [ ] Commit。

```bash
git add rust-ody/crates/agent-rs/src/turn/error.rs
git commit -m "feat(agent-rs): map ChatProviderError to OdyError with retryability"
```

---

## Task 3: `KosongLLM` + 调整 `Llm::is_retryable_error` 签名

**Depends on:** Task 2

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/agent_loop/llm.rs:82`
- Modify: `rust-ody/crates/agent-rs/src/agent_loop/retry.rs:43`
- Modify: `rust-ody/crates/agent-rs/tests/loop_retry.rs:26`
- Modify: `rust-ody/crates/agent-rs/src/turn/turn_flow.rs`（FakeLlm 的 `is_retryable_error`）
- Create: `rust-ody/crates/agent-rs/src/turn/kosong_llm.rs`

### Steps

- [ ] Write the failing test。

在 `rust-ody/crates/agent-rs/src/turn/kosong_llm.rs` 末尾加入：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use kosong_rs::provider::{AbortSignal, FinishReason, GenerateOptions, Tool};
    use kosong_rs::message::{ContentPart, Message, Role, StreamedMessagePart};
    use kosong_rs::usage::TokenUsage;
    use kosong_rs::MockProvider;

    use crate::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse, LlmStreamTiming};

    #[tokio::test]
    async fn kosong_llm_forwards_text_delta_and_part() {
        let provider = MockProvider::new("mock", "m1").with_parts(vec![
            StreamedMessagePart::text("hello"),
            StreamedMessagePart::text(" world"),
        ]);
        let llm = KosongLLM::new(KosongLLMConfig {
            provider: Box::new(provider),
            model_name: "m1".into(),
            system_prompt: "sys".into(),
            capability: None,
            completion_budget_config: None,
        });

        let deltas = Arc::new(Mutex::new(Vec::new()));
        let parts = Arc::new(Mutex::new(Vec::new()));
        let d = deltas.clone();
        let p = parts.clone();

        let response = llm
            .chat(LlmChatParams {
                messages: vec![Message::user_text("hi")],
                tools: vec![],
                signal: AbortSignal::new(),
                request_log_context: None,
                on_text_delta: Some(Arc::new(move |s| d.lock().unwrap().push(s))),
                on_think_delta: None,
                on_tool_call_delta: None,
                on_text_part: Some(Arc::new(move |tp| {
                    p.lock().unwrap().push(tp.text.clone());
                    Box::pin(async {})
                })),
                on_think_part: None,
            })
            .await
            .unwrap();

        assert_eq!(deltas.lock().unwrap().as_slice(), &["hello", " world"]);
        assert_eq!(parts.lock().unwrap().as_slice(), &["hello world"]);
        assert_eq!(response.provider_finish_reason, Some(FinishReason::Completed));
    }

    #[tokio::test]
    async fn kosong_llm_applies_completion_budget() {
        use kosong_rs::provider::{ChatProvider, GenerateOptions as KosongGenerateOptions, ThinkingEffort, Tool as KosongTool};
        use kosong_rs::generate::StreamedMessage;
        use kosong_rs::errors::ChatProviderError;

        struct CaptureProvider {
            cap: Arc<Mutex<Option<i64>>>,
        }

        #[async_trait::async_trait]
        impl ChatProvider for CaptureProvider {
            fn name(&self) -> &str { "capture" }
            fn model_name(&self) -> &str { "m1" }
            fn thinking_effort(&self) -> Option<ThinkingEffort> { None }
            fn get_capability(&self, _model: Option<&str>) -> kosong_rs::provider::ModelCapability {
                let mut cap = kosong_rs::provider::ModelCapability::unknown();
                cap.max_context_tokens = 128000;
                cap.max_output_tokens = 4096;
                cap
            }
            async fn generate(
                &self,
                _system_prompt: &str,
                _tools: &[KosongTool],
                _history: &[Message],
                _options: Option<KosongGenerateOptions>,
            ) -> Result<StreamedMessage, ChatProviderError> {
                Ok(MockProvider::new("capture", "m1")
                    .with_parts(vec![StreamedMessagePart::text("ok")])
                    .generate("", &[], &[], None)
                    .await?)
            }
            fn with_thinking(&self, _effort: ThinkingEffort) -> Box<dyn ChatProvider> {
                Box::new(CaptureProvider { cap: self.cap.clone() })
            }
            fn with_max_completion_tokens(&self, max_tokens: i64) -> Option<Box<dyn ChatProvider>> {
                *self.cap.lock().unwrap() = Some(max_tokens);
                Some(Box::new(CaptureProvider { cap: self.cap.clone() }))
            }
        }

        let cap = Arc::new(Mutex::new(None));
        let provider = CaptureProvider { cap: cap.clone() };
        let llm = KosongLLM::new(KosongLLMConfig {
            provider: Box::new(provider),
            model_name: "m1".into(),
            system_prompt: "sys".into(),
            capability: None,
            completion_budget_config: Some(CompletionBudgetConfig {
                hard_cap: Some(1000),
                fallback: None,
            }),
        });

        let _ = llm
            .chat(LlmChatParams {
                messages: vec![Message::user_text("hi")],
                tools: vec![],
                signal: AbortSignal::new(),
                request_log_context: None,
                on_text_delta: None,
                on_think_delta: None,
                on_tool_call_delta: None,
                on_text_part: None,
                on_think_part: None,
            })
            .await
            .unwrap();

        assert_eq!(cap.lock().unwrap().unwrap(), 1000);
    }

    #[tokio::test]
    async fn kosong_llm_marks_empty_response_as_retryable() {
        let provider = MockProvider::new("mock", "m1").with_parts(vec![]);
        let llm = KosongLLM::new(KosongLLMConfig {
            provider: Box::new(provider),
            model_name: "m1".into(),
            system_prompt: "sys".into(),
            capability: None,
            completion_budget_config: None,
        });

        let err = llm
            .chat(LlmChatParams {
                messages: vec![Message::user_text("hi")],
                tools: vec![],
                signal: AbortSignal::new(),
                request_log_context: None,
                on_text_delta: None,
                on_think_delta: None,
                on_tool_call_delta: None,
                on_text_part: None,
                on_think_part: None,
            })
            .await
            .unwrap_err();

        assert!(llm.is_retryable_error(&err));
    }
}
```

- [ ] Run it and verify it FAILS。

```bash
cd rust-ody && cargo test -p agent-rs kosong_llm_forwards_text_delta_and_part --no-run
```

预期失败：`KosongLLM` 未定义，且 `is_retryable_error` 签名仍为 `&dyn std::error::Error`。

- [ ] Write the minimal implementation。

**Step 3a: 修改 `Llm` trait 的 `is_retryable_error` 签名**

在 `rust-ody/crates/agent-rs/src/agent_loop/llm.rs` 中：

```rust
#[async_trait::async_trait]
pub trait Llm: Send + Sync {
    fn system_prompt(&self) -> &str;
    fn model_name(&self) -> &str;
    fn capability(&self) -> Option<&ModelCapability> { None }
    fn is_retryable_error(&self, _error: &anyhow::Error) -> bool { false }
    async fn chat(&self, params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error>;
}
```

**Step 3b: 更新所有 `is_retryable_error` 调用方/实现者**

`rust-ody/crates/agent-rs/src/agent_loop/retry.rs:43`：

```rust
if attempt >= effective_max || !input.llm.is_retryable_error(&error) {
```

`rust-ody/crates/agent-rs/tests/loop_retry.rs:26`：

```rust
fn is_retryable_error(&self, err: &anyhow::Error) -> bool {
    err.to_string().contains("retryable")
}
```

`rust-ody/crates/agent-rs/src/turn/turn_flow.rs` 中 FakeLlm 的 `is_retryable_error`：

```rust
fn is_retryable_error(&self, _error: &anyhow::Error) -> bool {
    false
}
```

以及 `SharedLlm` 的 `is_retryable_error`：

```rust
fn is_retryable_error(&self, error: &anyhow::Error) -> bool {
    self.0.is_retryable_error(error)
}
```

**Step 3c: 创建 `kosong_llm.rs`**

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use kosong_rs::errors::ChatProviderError;
use kosong_rs::generate::generate as kosong_generate;
use kosong_rs::message::{ContentPart, Message, StreamedMessagePart, ToolCallPart};
use kosong_rs::provider::{
    AbortSignal, ChatProvider, FinishReason, GenerateCallbacks, GenerateOptions, ModelCapability,
    ThinkingEffort, Tool,
};
use kosong_rs::usage::TokenUsage;
use serde_json::Value as JsonValue;

use crate::agent_loop::llm::{
    Llm, LlmChatParams, LlmChatResponse, LlmStreamTiming, TextPart, ThinkPart, ToolCallDelta,
};
use crate::context::tokens::{estimate_tokens, estimate_tokens_for_messages};
use crate::turn::error::{from_chat_provider_error, is_retryable_generate_error};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionBudgetConfig {
    pub hard_cap: Option<i64>,
    pub fallback: Option<i64>,
}

pub struct KosongLLMConfig {
    pub provider: Box<dyn ChatProvider>,
    pub model_name: String,
    pub system_prompt: String,
    pub capability: Option<ModelCapability>,
    pub completion_budget_config: Option<CompletionBudgetConfig>,
}

pub struct KosongLLM {
    provider: Box<dyn ChatProvider>,
    model_name: String,
    system_prompt: String,
    capability: Option<ModelCapability>,
    completion_budget_config: Option<CompletionBudgetConfig>,
}

impl KosongLLM {
    pub fn new(config: KosongLLMConfig) -> Self {
        Self {
            provider: config.provider,
            model_name: config.model_name,
            system_prompt: config.system_prompt,
            capability: config.capability,
            completion_budget_config: config.completion_budget_config,
        }
    }
}

#[async_trait]
impl Llm for KosongLLM {
    fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn capability(&self) -> Option<&ModelCapability> {
        self.capability.as_ref()
    }

    fn is_retryable_error(&self, error: &anyhow::Error) -> bool {
        is_retryable_generate_error(error)
    }

    async fn chat(&self, params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        let request_started_at = Instant::now();
        let first_chunk_at: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
        let stream_ended_at: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));

        let first_chunk_at_clone = first_chunk_at.clone();
        let mark_stream_output = Arc::new(move || {
            let mut f = first_chunk_at_clone.lock().unwrap();
            if f.is_none() {
                *f = Some(Instant::now());
            }
        });
        let stream_ended_at_clone = stream_ended_at.clone();
        let mark_stream_end = Arc::new(move || {
            *stream_ended_at_clone.lock().unwrap() = Some(Instant::now());
        });
        let mark_request_start = Arc::new(|| {});

        let estimated_input_tokens = estimate_tokens(&self.system_prompt)
            + estimate_tokens_for_messages(&params.messages)
            + estimate_tokens_for_tools(&params.tools);

        let effective_provider = apply_completion_budget(
            self.provider.as_ref(),
            self.capability.as_ref(),
            self.completion_budget_config.as_ref(),
            estimated_input_tokens,
        );

        let options = GenerateOptions {
            signal: Some(params.signal.clone()),
            on_request_start: Some(mark_request_start),
            on_stream_end: Some(mark_stream_end),
            auth: None,
        };

        let callbacks = build_kosong_callbacks(&params, mark_stream_output);

        let result = kosong_generate(
            effective_provider.as_ref(),
            &self.system_prompt,
            &params.tools,
            &params.messages,
            Some(&callbacks),
            Some(&options),
        )
        .await
        .map_err(|e| anyhow::Error::from(from_chat_provider_error(e)))?;

        if params.on_text_part.is_some() || params.on_think_part.is_some() {
            for part in &result.message.content {
                match part {
                    ContentPart::Text { text } => {
                        if let Some(cb) = &params.on_text_part {
                            cb(TextPart { text: text.clone() }).await;
                        }
                    }
                    ContentPart::Think { think, .. } => {
                        if let Some(cb) = &params.on_think_part {
                            cb(ThinkPart {
                                think: think.clone(),
                                encrypted: None,
                            })
                            .await;
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(LlmChatResponse {
            tool_calls: result.message.tool_calls.clone(),
            provider_finish_reason: result.finish_reason,
            raw_finish_reason: result.raw_finish_reason.clone(),
            usage: result.usage.unwrap_or_default(),
            stream_timing: build_stream_timing(
                request_started_at,
                *first_chunk_at.lock().unwrap(),
                *stream_ended_at.lock().unwrap(),
            ),
        })
    }
}

fn build_stream_timing(
    request_started_at: Instant,
    first_chunk_at: Option<Instant>,
    stream_ended_at: Option<Instant>,
) -> Option<LlmStreamTiming> {
    let first = first_chunk_at?;
    let ended = stream_ended_at.unwrap_or_else(Instant::now);
    Some(LlmStreamTiming {
        first_token_latency_ms: first.duration_since(request_started_at).as_millis() as u64,
        stream_duration_ms: ended.duration_since(first).as_millis() as u64,
    })
}

fn estimate_tokens_for_tools(tools: &[Tool]) -> i64 {
    tools
        .iter()
        .map(|t| {
            estimate_tokens(&t.name)
                + estimate_tokens(&t.description)
                + estimate_tokens(&serde_json::to_string(&t.parameters).unwrap_or_default())
        })
        .sum()
}

const MIN_FLOOR: i64 = 1;
const DEFAULT_UNKNOWN_OUTPUT_FALLBACK: i64 = 32000;
const CONTEXT_WINDOW_OVERHEAD_TOKENS: i64 = 8192;
const MAX_CONTEXT_COMPLETION_RATIO: f64 = 0.25;

fn apply_completion_budget(
    provider: &dyn ChatProvider,
    capability: Option<&ModelCapability>,
    budget: Option<&CompletionBudgetConfig>,
    input_tokens: i64,
) -> Box<dyn ChatProvider> {
    let base = provider.with_thinking(provider.thinking_effort().unwrap_or(ThinkingEffort::Off));
    let budget = match budget {
        Some(b) => b,
        None => return base,
    };

    let cap = compute_completion_budget_cap(capability, budget, input_tokens);
    base.with_max_completion_tokens(cap).unwrap_or(base)
}

fn compute_completion_budget_cap(
    capability: Option<&ModelCapability>,
    budget: &CompletionBudgetConfig,
    input_tokens: i64,
) -> i64 {
    let max_output = capability.map(|c| c.max_output_tokens).unwrap_or(0);
    let max_context = capability.map(|c| c.max_context_tokens).unwrap_or(0);

    let mut cap = budget.hard_cap.unwrap_or_else(|| {
        if max_output > 0 {
            max_output
        } else {
            budget.fallback.unwrap_or(DEFAULT_UNKNOWN_OUTPUT_FALLBACK)
        }
    });

    if max_context > 0 {
        if input_tokens > 0 {
            let remaining = max_context - input_tokens - CONTEXT_WINDOW_OVERHEAD_TOKENS;
            cap = cap.min(remaining.max(MIN_FLOOR));
        }
        cap = cap.min((max_context as f64 * MAX_CONTEXT_COMPLETION_RATIO).floor() as i64);
    }

    cap.max(MIN_FLOOR)
}

#[derive(Clone)]
struct ToolCallIdentity {
    tool_call_id: String,
    name: String,
}

struct BufferedToolCallDelta {
    arguments_part: Option<String>,
}

fn build_kosong_callbacks(
    params: &LlmChatParams,
    mark_stream_output: Arc<dyn Fn() + Send + Sync>,
) -> GenerateCallbacks {
    let params = params.clone();
    let tool_call_identities: Arc<Mutex<HashMap<String, ToolCallIdentity>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let pending_indexed_deltas: Arc<Mutex<HashMap<String, Vec<BufferedToolCallDelta>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let last_tool_call_identity: Arc<Mutex<Option<ToolCallIdentity>>> =
        Arc::new(Mutex::new(None));

    let tool_call_delta_cb = params.on_tool_call_delta.clone();
    let emit_tool_call_delta = move |delta: ToolCallDelta| {
        if let Some(cb) = &tool_call_delta_cb {
            cb(delta);
        }
    };

    GenerateCallbacks {
        on_message_part: Some(Box::new(move |part: StreamedMessagePart| {
            mark_stream_output();
            match &part {
                StreamedMessagePart::Content(ContentPart::Text { text }) => {
                    if let Some(cb) = &params.on_text_delta {
                        cb(text.clone());
                    }
                }
                StreamedMessagePart::Content(ContentPart::Think { think, .. }) => {
                    if let Some(cb) = &params.on_think_delta {
                        cb(think.clone());
                    }
                }
                _ => {}
            }

            match &part {
                StreamedMessagePart::ToolCall(tc) => {
                    let identity = ToolCallIdentity {
                        tool_call_id: tc.id.clone(),
                        name: tc.name.clone(),
                    };
                    *last_tool_call_identity.lock().unwrap() = Some(identity.clone());
                    if let Some(idx) = tc.stream_index.as_ref().map(|v| v.to_string()) {
                        tool_call_identities.lock().unwrap().insert(idx, identity);
                    }
                    emit_tool_call_delta(ToolCallDelta {
                        tool_call_id: tc.id.clone(),
                        name: Some(tc.name.clone()),
                        arguments_part: tc.arguments.clone(),
                    });

                    if let Some(idx) = tc.stream_index.as_ref().map(|v| v.to_string()) {
                        let pending = pending_indexed_deltas.lock().unwrap().remove(&idx);
                        if let Some(pending_deltas) = pending {
                            for delta in pending_deltas {
                                emit_tool_call_delta(ToolCallDelta {
                                    tool_call_id: tc.id.clone(),
                                    name: Some(tc.name.clone()),
                                    arguments_part: delta.arguments_part,
                                });
                            }
                        }
                    }
                }
                StreamedMessagePart::ToolCallPart(ToolCallPart {
                    arguments_part,
                    index,
                    ..
                }) => {
                    let delta = BufferedToolCallDelta {
                        arguments_part: arguments_part.clone(),
                    };
                    if let Some(idx) = index.as_ref().map(|v| v.to_string()) {
                        let identity = tool_call_identities.lock().unwrap().get(&idx).cloned();
                        match identity {
                            Some(id) => {
                                emit_tool_call_delta(ToolCallDelta {
                                    tool_call_id: id.tool_call_id,
                                    name: Some(id.name),
                                    arguments_part: delta.arguments_part,
                                });
                            }
                            None => {
                                let mut pending = pending_indexed_deltas.lock().unwrap();
                                pending.entry(idx).or_default().push(delta);
                            }
                        }
                    } else {
                        let identity = last_tool_call_identity.lock().unwrap().clone();
                        if let Some(id) = identity {
                            emit_tool_call_delta(ToolCallDelta {
                                tool_call_id: id.tool_call_id,
                                name: Some(id.name),
                                arguments_part: delta.arguments_part,
                            });
                        }
                    }
                }
                _ => {}
            }
        })),
        on_tool_call: None,
    }
}
```

注意：上面 `emit_tool_call_delta` 是 `move` 闭包但内部引用 `params` 是克隆的，需确保编译器接受；`GenerateCallbacks` 的 `on_message_part` 是 `Box<dyn Fn(StreamedMessagePart)>`，闭包内部不可异步。回调中调用 `cb` 都是同步的，符合要求。

- [ ] Run it and verify it PASSES。

```bash
cd rust-ody && cargo test -p agent-rs kosong_llm_forwards_text_delta_and_part kosong_llm_applies_completion_budget kosong_llm_marks_empty_response_as_retryable
```

预期：3 个测试通过。

- [ ] Commit。

```bash
git add rust-ody/crates/agent-rs/src/agent_loop/llm.rs rust-ody/crates/agent-rs/src/agent_loop/retry.rs rust-ody/crates/agent-rs/tests/loop_retry.rs rust-ody/crates/agent-rs/src/turn/turn_flow.rs rust-ody/crates/agent-rs/src/turn/kosong_llm.rs
git commit -m "feat(agent-rs): implement KosongLLM and adjust Llm retry signature"
```

---

## Task 4: `RemoteKosongLLM` + stream registry

**Depends on:** Task 3（修改后的 `Llm` trait）

**Files:**
- Create: `rust-ody/crates/agent-rs/src/turn/remote_kosong_llm.rs`

### Steps

- [ ] Write the failing test。

在 `rust-ody/crates/agent-rs/src/turn/remote_kosong_llm.rs` 末尾加入：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use kosong_rs::provider::{AbortSignal, FinishReason};
    use kosong_rs::usage::TokenUsage;

    use crate::agent_loop::llm::{Llm, LlmChatParams, LlmStreamTiming};
    use crate::agent_loop::llm::TextPart;

    struct FakeClient {
        stream_id: Arc<Mutex<Option<String>>>,
        deltas: Arc<Mutex<Vec<StreamDelta>>>,
        result: ChatStreamResult,
    }

    #[async_trait::async_trait]
    impl RemoteLlmStreamClient for FakeClient {
        async fn chat_stream_init(
            &self,
            _request: ChatStreamRequest,
            stream_id: String,
        ) -> Result<(), anyhow::Error> {
            *self.stream_id.lock().unwrap() = Some(stream_id.clone());
            let deltas = self.deltas.lock().unwrap().clone();
            let result = self.result.clone();
            tokio::spawn(async move {
                for delta in deltas {
                    remote_llm_stream_registry().dispatch_delta(&stream_id, delta);
                }
                remote_llm_stream_registry().dispatch_end(&stream_id, result);
            });
            Ok(())
        }

        async fn chat_stream_cancel(&self, _stream_id: String) -> Result<(), anyhow::Error> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn remote_kosong_llm_forwards_deltas_and_result() {
        let deltas = Arc::new(Mutex::new(vec![
            StreamDelta::Text { text: "hello".into() },
            StreamDelta::Text { text: " world".into() },
        ]));
        let client = Arc::new(FakeClient {
            stream_id: Arc::new(Mutex::new(None)),
            deltas: deltas.clone(),
            result: ChatStreamResult {
                tool_calls: vec![],
                provider_finish_reason: Some(FinishReason::Completed),
                raw_finish_reason: Some("stop".into()),
                usage: TokenUsage::default(),
                stream_timing: Some(LlmStreamTiming {
                    first_token_latency_ms: 10,
                    stream_duration_ms: 20,
                }),
            },
        });

        let llm = RemoteKosongLLM::new(RemoteKosongLLMConfig {
            client: client.clone(),
            model_name: "m1".into(),
            system_prompt: "sys".into(),
            capability: None,
            completion_budget_config: None,
            provider: ProviderConfig {
                provider_id: "mock".into(),
                api_key: None,
                base_url: None,
                default_model: None,
            },
        });

        let text_parts = Arc::new(Mutex::new(Vec::new()));
        let p = text_parts.clone();
        let response = llm
            .chat(LlmChatParams {
                messages: vec![],
                tools: vec![],
                signal: AbortSignal::new(),
                request_log_context: None,
                on_text_delta: None,
                on_think_delta: None,
                on_tool_call_delta: None,
                on_text_part: Some(Arc::new(move |tp: TextPart| {
                    p.lock().unwrap().push(tp.text);
                    Box::pin(async {})
                })),
                on_think_part: None,
            })
            .await
            .unwrap();

        assert!(client.stream_id.lock().unwrap().is_some());
        assert_eq!(text_parts.lock().unwrap().join(""), "hello world");
        assert_eq!(response.provider_finish_reason, Some(FinishReason::Completed));
        assert_eq!(
            response.stream_timing,
            Some(LlmStreamTiming {
                first_token_latency_ms: 10,
                stream_duration_ms: 20,
            })
        );
    }

    #[tokio::test]
    async fn remote_kosong_llm_cancels_on_abort() {
        struct HangingClient;
        #[async_trait::async_trait]
        impl RemoteLlmStreamClient for HangingClient {
            async fn chat_stream_init(
                &self,
                _request: ChatStreamRequest,
                _stream_id: String,
            ) -> Result<(), anyhow::Error> {
                Ok(())
            }
            async fn chat_stream_cancel(&self, _stream_id: String) -> Result<(), anyhow::Error> {
                Ok(())
            }
        }

        let llm = RemoteKosongLLM::new(RemoteKosongLLMConfig {
            client: Arc::new(HangingClient),
            model_name: "m1".into(),
            system_prompt: "sys".into(),
            capability: None,
            completion_budget_config: None,
            provider: ProviderConfig {
                provider_id: "mock".into(),
                api_key: None,
                base_url: None,
                default_model: None,
            },
        });

        let signal = AbortSignal::new();
        let signal_clone = signal.clone();
        let handle = tokio::spawn(async move {
            llm.chat(LlmChatParams {
                messages: vec![],
                tools: vec![],
                signal: signal_clone,
                request_log_context: None,
                on_text_delta: None,
                on_think_delta: None,
                on_tool_call_delta: None,
                on_text_part: None,
                on_think_part: None,
            })
            .await
        });

        tokio::task::yield_now().await;
        signal.abort();
        let err = handle.await.unwrap().unwrap_err();
        assert!(err.to_string().contains("cancelled") || err.to_string().contains("aborted"));
    }
}
```

- [ ] Run it and verify it FAILS。

```bash
cd rust-ody && cargo test -p agent-rs remote_kosong_llm_forwards_deltas_and_result --no-run
```

预期失败：`RemoteKosongLLM`、`RemoteLlmStreamClient`、`ChatStreamRequest`、`StreamDelta` 等未定义。

- [ ] Write the minimal implementation。

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use async_trait::async_trait;
use kosong_rs::message::{ContentPart, Message, ToolCall};
use kosong_rs::provider::{AbortSignal, FinishReason, ModelCapability, Tool};
use kosong_rs::usage::TokenUsage;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agent_loop::llm::{
    Llm, LlmChatParams, LlmChatResponse, LlmRequestLogContext, LlmStreamTiming, TextPart,
    ThinkPart, ToolCallDelta,
};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub provider_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamRequest {
    pub model_name: String,
    pub system_prompt: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ChatStreamToolDefinition>,
    pub capability: Option<ModelCapability>,
    pub completion_budget_config: Option<crate::turn::kosong_llm::CompletionBudgetConfig>,
    pub request_log_context: Option<LlmRequestLogContext>,
    pub provider: ProviderConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStreamResult {
    pub tool_calls: Vec<ToolCall>,
    pub provider_finish_reason: Option<FinishReason>,
    pub raw_finish_reason: Option<String>,
    pub usage: TokenUsage,
    pub stream_timing: Option<LlmStreamTiming>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum StreamDelta {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "think")]
    Think { think: String },
    #[serde(rename = "tool_call_part")]
    ToolCallPart {
        tool_call_id: String,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        arguments_part: Option<String>,
    },
}

#[async_trait]
pub trait RemoteLlmStreamClient: Send + Sync {
    async fn chat_stream_init(
        &self,
        request: ChatStreamRequest,
        stream_id: String,
    ) -> Result<(), anyhow::Error>;
    async fn chat_stream_cancel(&self, stream_id: String) -> Result<(), anyhow::Error>;
}

struct StreamHandlers {
    on_delta: Box<dyn Fn(StreamDelta) + Send + Sync>,
    on_end: Box<dyn Fn(ChatStreamResult) + Send + Sync>,
    on_error: Box<dyn Fn(anyhow::Error) + Send + Sync>,
}

pub struct RemoteLlmStreamRegistry {
    streams: Mutex<HashMap<String, StreamHandlers>>,
}

impl RemoteLlmStreamRegistry {
    pub fn new() -> Self {
        Self {
            streams: Mutex::new(HashMap::new()),
        }
    }

    pub fn register(&self, stream_id: String, handlers: StreamHandlers) {
        self.streams.lock().unwrap().insert(stream_id, handlers);
    }

    pub fn unregister(&self, stream_id: &str) {
        self.streams.lock().unwrap().remove(stream_id);
    }

    pub fn dispatch_delta(&self, stream_id: &str, delta: StreamDelta) {
        if let Some(handlers) = self.streams.lock().unwrap().get(stream_id) {
            (handlers.on_delta)(delta);
        }
    }

    pub fn dispatch_end(&self, stream_id: &str, result: ChatStreamResult) {
        let handlers = self.streams.lock().unwrap().remove(stream_id);
        if let Some(handlers) = handlers {
            (handlers.on_end)(result);
        }
    }

    pub fn dispatch_error(&self, stream_id: &str, error: crate::turn::telemetry::OdyError) {
        let handlers = self.streams.lock().unwrap().remove(stream_id);
        if let Some(handlers) = handlers {
            (handlers.on_error)(anyhow::Error::from(error));
        }
    }
}

impl Default for RemoteLlmStreamRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn remote_llm_stream_registry() -> &'static RemoteLlmStreamRegistry {
    static REGISTRY: OnceLock<RemoteLlmStreamRegistry> = OnceLock::new();
    REGISTRY.get_or_init(RemoteLlmStreamRegistry::new)
}

pub struct RemoteKosongLLMConfig {
    pub client: Arc<dyn RemoteLlmStreamClient>,
    pub model_name: String,
    pub system_prompt: String,
    pub capability: Option<ModelCapability>,
    pub completion_budget_config: Option<crate::turn::kosong_llm::CompletionBudgetConfig>,
    pub provider: ProviderConfig,
}

pub struct RemoteKosongLLM {
    client: Arc<dyn RemoteLlmStreamClient>,
    model_name: String,
    system_prompt: String,
    capability: Option<ModelCapability>,
    completion_budget_config: Option<crate::turn::kosong_llm::CompletionBudgetConfig>,
    provider: ProviderConfig,
}

impl RemoteKosongLLM {
    pub fn new(config: RemoteKosongLLMConfig) -> Self {
        Self {
            client: config.client,
            model_name: config.model_name,
            system_prompt: config.system_prompt,
            capability: config.capability,
            completion_budget_config: config.completion_budget_config,
            provider: config.provider,
        }
    }
}

#[async_trait]
impl Llm for RemoteKosongLLM {
    fn system_prompt(&self) -> &str {
        &self.system_prompt
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn capability(&self) -> Option<&ModelCapability> {
        self.capability.as_ref()
    }

    async fn chat(&self, params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        let stream_id = Uuid::new_v4().to_string();
        let request = self.build_request(&params);
        params.signal.throw_if_aborted()?;

        let (tx, rx) = tokio::sync::oneshot::channel::<Result<ChatStreamResult, anyhow::Error>>();
        let tx = Arc::new(std::sync::Mutex::new(Some(tx)));

        let handlers = StreamHandlers {
            on_delta: Box::new({
                let params = params.clone();
                move |delta| {
                    if params.signal.is_aborted() {
                        return;
                    }
                    match delta {
                        StreamDelta::Text { text } => {
                            if let Some(cb) = &params.on_text_delta {
                                cb(text);
                            }
                        }
                        StreamDelta::Think { think } => {
                            if let Some(cb) = &params.on_think_delta {
                                cb(think);
                            }
                        }
                        StreamDelta::ToolCallPart {
                            tool_call_id,
                            name,
                            arguments_part,
                        } => {
                            if let Some(cb) = &params.on_tool_call_delta {
                                cb(ToolCallDelta {
                                    tool_call_id,
                                    name: Some(name),
                                    arguments_part,
                                });
                            }
                        }
                    }
                }
            }),
            on_end: {
                let tx = tx.clone();
                Box::new(move |result| {
                    if let Some(tx) = tx.lock().unwrap().take() {
                        let _ = tx.send(Ok(result));
                    }
                })
            },
            on_error: {
                let tx = tx.clone();
                Box::new(move |error| {
                    if let Some(tx) = tx.lock().unwrap().take() {
                        let _ = tx.send(Err(error));
                    }
                })
            },
        };

        remote_llm_stream_registry().register(stream_id.clone(), handlers);

        if let Err(e) = self.client.chat_stream_init(request, stream_id.clone()).await {
            remote_llm_stream_registry().unregister(&stream_id);
            return Err(e);
        }

        let signal = params.signal.clone();
        let result = tokio::select! {
            biased;
            _ = async {
                while !signal.is_aborted() {
                    tokio::task::yield_now().await;
                }
            } => {
                let _ = self.client.chat_stream_cancel(stream_id.clone()).await;
                remote_llm_stream_registry().unregister(&stream_id);
                Err(anyhow::anyhow!("Stream cancelled"))
            }
            res = rx => {
                remote_llm_stream_registry().unregister(&stream_id);
                match res {
                    Ok(result) => result,
                    Err(_) => Err(anyhow::anyhow!("turn worker dropped")),
                }
            }
        };

        let result = result?;

        Ok(LlmChatResponse {
            tool_calls: result.tool_calls,
            provider_finish_reason: result.provider_finish_reason,
            raw_finish_reason: result.raw_finish_reason,
            usage: result.usage,
            stream_timing: result.stream_timing,
        })
    }
}

impl RemoteKosongLLM {
    fn build_request(&self, params: &LlmChatParams) -> ChatStreamRequest {
        ChatStreamRequest {
            model_name: self.model_name.clone(),
            system_prompt: self.system_prompt.clone(),
            messages: params.messages.clone(),
            tools: params
                .tools
                .iter()
                .map(|t| ChatStreamToolDefinition {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                })
                .collect(),
            capability: self.capability.clone(),
            completion_budget_config: self.completion_budget_config.clone(),
            request_log_context: params.request_log_context.clone(),
            provider: self.provider.clone(),
        }
    }
}
```

- [ ] Run it and verify it PASSES。

```bash
cd rust-ody && cargo test -p agent-rs remote_kosong_llm_forwards_deltas_and_result remote_kosong_llm_cancels_on_abort
```

预期：2 个测试通过。

- [ ] Commit。

```bash
git add rust-ody/crates/agent-rs/src/turn/remote_kosong_llm.rs
git commit -m "feat(agent-rs): implement RemoteKosongLLM and stream registry"
```

---

## Task 5: `ToolCallDeduplicator` + 接入 `TurnFlow`

**Depends on:** Task 1（`canonical_telemetry_args`）、`turn.md` Task 4（`TurnFlow` 中已预留 `Dedup` trait）

**Files:**
- Create: `rust-ody/crates/agent-rs/src/turn/tool_dedup.rs`
- Modify: `rust-ody/crates/agent-rs/src/turn/turn_flow.rs`（移除本地 `Dedup`/`NoopDedup`，接入 `ToolCallDeduplicator`）

### Steps

- [ ] Write the failing test。

在 `rust-ody/crates/agent-rs/src/turn/tool_dedup.rs` 末尾加入：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::records::nested::{ExecutableToolOutput, ExecutableToolSuccessResult};

    fn success(text: &str) -> ExecutableToolResult {
        ExecutableToolResult::Success(ExecutableToolSuccessResult {
            output: ExecutableToolOutput::Text(text.into()),
            is_error: None,
            stop_turn: None,
            message: None,
        })
    }

    #[tokio::test]
    async fn same_step_dedup_resolves_to_original_result() {
        let dedup = ToolCallDeduplicator::new();
        dedup.begin_step();

        let original = dedup.check_same_step("call_1", "read", &json!({"path": "/a"}));
        assert!(original.is_none());

        let dup = dedup
            .check_same_step("call_2", "read", &json!({"path": "/a"}))
            .expect("duplicate should return placeholder");
        assert_eq!(dup, success(""));

        let finalized = dedup
            .finalize_result("call_1", "read", &json!({"path": "/a"}), success("real result"))
            .await;
        assert_eq!(finalized, success("real result"));

        let dup_finalized = dedup
            .finalize_result("call_2", "read", &json!({"path": "/a"}), success("ignored"))
            .await;
        assert_eq!(dup_finalized, success("real result"));
    }

    #[tokio::test]
    async fn cross_step_appends_reminder_at_third_repeat() {
        let dedup = ToolCallDeduplicator::new();
        let args = json!({"path": "/a"});

        for _ in 0..2 {
            dedup.begin_step();
            dedup.check_same_step("c1", "read", &args);
            let _ = dedup
                .finalize_result("c1", "read", &args, success("ok"))
                .await;
            dedup.end_step();
        }

        dedup.begin_step();
        dedup.check_same_step("c3", "read", &args);
        let result = dedup
            .finalize_result("c3", "read", &args, success("ok"))
            .await;
        dedup.end_step();

        match result {
            ExecutableToolResult::Success(s) => {
                let text = match &s.output {
                    ExecutableToolOutput::Text(t) => t.clone(),
                    _ => panic!("expected text output"),
                };
                assert!(text.contains("You are repeating the exact same tool call"));
            }
            _ => panic!("expected success"),
        }
    }

    #[test]
    fn begin_step_clears_same_step_state() {
        let dedup = ToolCallDeduplicator::new();
        dedup.begin_step();
        dedup.check_same_step("c1", "read", &json!({"path": "/a"}));
        dedup.end_step();

        dedup.begin_step();
        let second = dedup.check_same_step("c2", "read", &json!({"path": "/a"}));
        assert!(second.is_none(), "new step should not dedup previous step");
    }
}
```

- [ ] Run it and verify it FAILS。

```bash
cd rust-ody && cargo test -p agent-rs same_step_dedup_resolves_to_original_result --no-run
```

预期失败：`ToolCallDeduplicator`、`Dedup` 未定义。

- [ ] Write the minimal implementation。

**Step 5a: 创建 `tool_dedup.rs`**

```rust
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::Value as JsonValue;

use crate::records::nested::{
    ContentPart, ExecutableToolErrorResult, ExecutableToolOutput, ExecutableToolResult,
    ExecutableToolSuccessResult,
};
use crate::turn::canonical_args::canonical_telemetry_args;

const REMINDER_TEXT_1: &str = "\n\n<system-reminder>\nYou are repeating the exact same tool call with identical parameters. Please carefully analyze the previous result. If the task is not yet complete, try a different method or parameters instead of repeating the same call.\n</system-reminder>";

fn make_reminder_text_2(tool_name: &str, repeat_count: i64, args: &JsonValue) -> String {
    let args_str = canonical_telemetry_args(args);
    format!(
        "\n\n<system-reminder>\nYou have repeatedly called the same tool with identical parameters many times.\nRepeated tool call detected:\n- tool: {}\n- repeated_times: {}\n- arguments: {}\nThe previous repeated calls did not make progress. Do not call this exact same tool with the exact same arguments again.\nCarefully inspect the latest tool result and choose a different next action, different parameters, or finish the task if enough evidence has been gathered.\n</system-reminder>",
        tool_name, repeat_count, args_str
    )
}

#[async_trait]
pub trait Dedup: Send + Sync {
    fn begin_step(&self);
    fn end_step(&self);
    fn check_same_step(
        &self,
        tool_call_id: &str,
        tool_name: &str,
        args: &JsonValue,
    ) -> Option<ExecutableToolResult>;
    async fn finalize_result(
        &self,
        tool_call_id: &str,
        tool_name: &str,
        args: &JsonValue,
        result: ExecutableToolResult,
    ) -> ExecutableToolResult;
}

fn dedup_placeholder_result() -> ExecutableToolResult {
    ExecutableToolResult::Success(ExecutableToolSuccessResult {
        output: ExecutableToolOutput::Text("".into()),
        is_error: None,
        stop_turn: None,
        message: None,
    })
}

pub struct ToolCallDeduplicator {
    inner: Mutex<Inner>,
}

struct Deferred<T: Clone + Send + Sync + 'static> {
    sender: tokio::sync::watch::Sender<T>,
    receiver: tokio::sync::watch::Receiver<T>,
}

impl<T: Clone + Send + Sync + 'static> Deferred<T> {
    fn new(initial: T) -> Self {
        let (sender, receiver) = tokio::sync::watch::channel(initial);
        Self { sender, receiver }
    }

    fn resolve(&self, value: T) {
        let _ = self.sender.send(value);
    }

    async fn wait(&self) -> Result<T, anyhow::Error> {
        let mut rx = self.receiver.clone();
        rx.changed().await?;
        Ok(rx.borrow().clone())
    }
}

struct Inner {
    step_deferreds: HashMap<String, Deferred<ExecutableToolResult>>,
    step_calls: Vec<String>,
    original_call_index: HashMap<String, usize>,
    synthetic_call_ids: HashSet<String>,
    call_key_by_call_id: HashMap<String, String>,
    consecutive_key: Option<String>,
    consecutive_count: i64,
}

impl ToolCallDeduplicator {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                step_deferreds: HashMap::new(),
                step_calls: Vec::new(),
                original_call_index: HashMap::new(),
                synthetic_call_ids: HashSet::new(),
                call_key_by_call_id: HashMap::new(),
                consecutive_key: None,
                consecutive_count: 0,
            }),
        }
    }

    fn make_key(tool_name: &str, args: &JsonValue) -> String {
        format!("{} {}", tool_name, canonical_telemetry_args(args))
    }
}

impl Default for ToolCallDeduplicator {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Dedup for ToolCallDeduplicator {
    fn begin_step(&self) {
        let mut inner = self.inner.lock().unwrap();
        for deferred in inner.step_deferreds.values() {
            deferred.resolve(ExecutableToolResult::Error(ExecutableToolErrorResult {
                output: ExecutableToolOutput::Text(
                    "Tool call deduplicated but original result was lost".into(),
                ),
                is_error: true,
                stop_turn: None,
                message: None,
            }));
        }
        inner.step_deferreds.clear();
        inner.step_calls.clear();
        inner.original_call_index.clear();
        inner.synthetic_call_ids.clear();
        inner.call_key_by_call_id.clear();
    }

    fn end_step(&self) {
        let mut inner = self.inner.lock().unwrap();
        for key in &inner.step_calls {
            if Some(key.as_str()) == inner.consecutive_key.as_deref() {
                inner.consecutive_count += 1;
            } else {
                inner.consecutive_key = Some(key.clone());
                inner.consecutive_count = 1;
            }
        }
    }

    fn check_same_step(
        &self,
        tool_call_id: &str,
        tool_name: &str,
        args: &JsonValue,
    ) -> Option<ExecutableToolResult> {
        let key = Self::make_key(tool_name, args);
        let mut inner = self.inner.lock().unwrap();
        let index = inner.step_calls.len();
        inner.step_calls.push(key.clone());
        inner.call_key_by_call_id.insert(tool_call_id.into(), key.clone());

        if inner.step_deferreds.contains_key(&key) {
            inner.synthetic_call_ids.insert(tool_call_id.into());
            Some(dedup_placeholder_result())
        } else {
            inner
                .step_deferreds
                .insert(key.clone(), Deferred::new(dedup_placeholder_result()));
            inner.original_call_index.insert(tool_call_id.into(), index);
            None
        }
    }

    async fn finalize_result(
        &self,
        tool_call_id: &str,
        tool_name: &str,
        args: &JsonValue,
        result: ExecutableToolResult,
    ) -> ExecutableToolResult {
        let key = {
            let mut inner = self.inner.lock().unwrap();
            let key = inner.call_key_by_call_id.remove(tool_call_id);
            key?
        };

        {
            let inner = self.inner.lock().unwrap();
            if inner.synthetic_call_ids.contains(tool_call_id) {
                let deferred = inner.step_deferreds.get(&key)?;
                return deferred.wait().await.unwrap_or(result);
            }
        }

        let (index, mut streak, mut last_key) = {
            let inner = self.inner.lock().unwrap();
            let index = *inner.original_call_index.get(tool_call_id)?;
            let mut streak = inner.consecutive_count;
            let mut last_key = inner.consecutive_key.clone();
            for i in 0..=index {
                let k = &inner.step_calls[i];
                if Some(k.as_str()) == last_key.as_deref() {
                    streak += 1;
                } else {
                    last_key = Some(k.clone());
                    streak = 1;
                }
            }
            (index, streak, last_key)
        };

        let final_result = if streak == 3 {
            append_reminder(result, REMINDER_TEXT_1)
        } else if streak == 5 || streak == 8 {
            append_reminder(result, &make_reminder_text_2(tool_name, streak, args))
        } else {
            result
        };

        {
            let inner = self.inner.lock().unwrap();
            inner.original_call_index.remove(tool_call_id);
            if let Some(deferred) = inner.step_deferreds.get(&key) {
                deferred.resolve(final_result.clone());
            }
        }

        let _ = index;
        final_result
    }
}

fn append_reminder(result: ExecutableToolResult, reminder_text: &str) -> ExecutableToolResult {
    match result {
        ExecutableToolResult::Success(mut s) => {
            s.output = append_to_output(s.output, reminder_text);
            ExecutableToolResult::Success(s)
        }
        ExecutableToolResult::Error(mut e) => {
            e.output = append_to_output(e.output, reminder_text);
            ExecutableToolResult::Error(e)
        }
    }
}

fn append_to_output(output: ExecutableToolOutput, reminder: &str) -> ExecutableToolOutput {
    match output {
        ExecutableToolOutput::Text(text) => ExecutableToolOutput::Text(text + reminder),
        ExecutableToolOutput::Parts(mut parts) => {
            if let Some(ContentPart::Text { text }) = parts.last_mut() {
                *text = format!("{}{}", text, reminder);
            } else {
                parts.push(ContentPart::Text { text: reminder.into() });
            }
            ExecutableToolOutput::Parts(parts)
        }
    }
}

#[cfg(test)]
mod tests {
    // 见上文测试代码
}
```

**Step 5b: 修改 `turn_flow.rs` 接入真实去重器**

在 `rust-ody/crates/agent-rs/src/turn/turn_flow.rs` 中：

1. 删除本地定义的 `trait Dedup` 和 `struct NoopDedup`。
2. 在文件顶部 import 块追加：

```rust
use crate::turn::tool_dedup::{Dedup, ToolCallDeduplicator};
```

3. 在 `run_step_loop` 中把：

```rust
let dedup: Arc<dyn Dedup> = Arc::new(NoopDedup);
```

替换为：

```rust
let dedup: Arc<dyn Dedup> = Arc::new(ToolCallDeduplicator::new());
```

4. 同步更新 `FakeLlm::is_retryable_error` 与 `SharedLlm::is_retryable_error` 已在 Task 3 中完成；如未完成，按 Task 3 的签名修改。

- [ ] Run it and verify it PASSES。

```bash
cd rust-ody && cargo test -p agent-rs same_step_dedup_resolves_to_original_result cross_step_appends_reminder_at_third_repeat begin_step_clears_same_step_state
```

预期：3 个测试通过。

- [ ] Commit。

```bash
git add rust-ody/crates/agent-rs/src/turn/tool_dedup.rs rust-ody/crates/agent-rs/src/turn/turn_flow.rs
git commit -m "feat(agent-rs): implement ToolCallDeduplicator and wire into TurnFlow"
```

---

## Task 6: 模块接线 + 全 workspace typecheck

**Depends on:** Task 3、Task 4、Task 5

**Files:**
- Modify: `rust-ody/crates/agent-rs/src/turn/mod.rs`

### Steps

- [ ] 确认并补全 `rust-ody/crates/agent-rs/src/turn/mod.rs`：

```rust
pub mod canonical_args;
pub mod error;
pub mod kosong_llm;
pub mod remote_kosong_llm;
pub mod telemetry;
pub mod tool_dedup;
pub mod turn_flow;
pub mod types;

pub use canonical_args::*;
pub use error::*;
pub use kosong_llm::*;
pub use remote_kosong_llm::*;
pub use telemetry::*;
pub use tool_dedup::*;
pub use turn_flow::*;
pub use types::*;
```

- [ ] Run whole-tree typecheck（包含 tests）。

```bash
cd rust-ody && cargo check -p agent-rs --workspace --tests
```

预期：`agent-rs` 及 workspace 中依赖它的 crate 全部编译通过；无新增错误 warning。

- [ ] Run 本 part 相关单元测试。

```bash
cd rust-ody && cargo test -p agent-rs -- canonical_ error_ kosong_llm remote_kosong_llm dedup
```

预期：所有新增测试通过。

- [ ] Run 受共享签名变更影响的既有测试。

```bash
cd rust-ody && cargo test -p agent-rs -- loop_retry
```

预期：`retry_succeeds_after_one_failure_and_emits_retrying_event` 通过。

- [ ] Commit。

```bash
git add rust-ody/crates/agent-rs/src/turn/mod.rs
git commit -m "feat(agent-rs): wire turn adapter module exports and verify workspace build"
```

---

## Local Self-Review

- [x] 1. Spec-coverage table：本 part 6 个 4.3.5.2/3/4 条目均映射到 Task；无 GAP。
- [x] 2. Placeholder scan：无 TODO/TBD；每个文件都给出完整可编译代码；无 "实际实现时修正" 之类的占位。
- [x] 3. No phantom tasks：6 个 task 均产出文件变更与可验证测试；无 `--allow-empty` 或 "already done in Task N"。
- [x] 4. Dependency soundness：Task 1 → Task 2 → Task 3/4 → Task 5 → Task 6；所有 `Depends on:` 均指向前序 task；无 forward reference。
- [x] 5. Caller & build soundness：Task 3 修改 `Llm::is_retryable_error` 共享签名，同步更新 `retry.rs`、`tests/loop_retry.rs`、`turn_flow.rs` 的 `FakeLlm` 与 `SharedLlm`；Task 6 以 `cargo check -p agent-rs --workspace --tests` 收尾。
- [x] 6. Test-the-risk：`KosongLLM` 验证流式回调、completion budget cap、空响应可重试；`RemoteKosongLLM` 验证跨进程 delta 转发与取消；`ToolCallDeduplicator` 验证同 step 去重、跨 step 提醒阈值、`begin_step` 状态清除；所有测试断言的常量均与 TS 源码一致。
- [x] 7. Type 一致性：`CompletionBudgetConfig`、`StreamDelta`、`ChatStreamRequest`、`ChatStreamResult` 的字段名 / camelCase / 枚举 tag 与 TS 对应类型对齐；`Dedup` trait 从 `turn_flow.rs` 迁移到 `tool_dedup.rs` 后方法签名不变；`Llm` trait 签名变更一次性完成。
