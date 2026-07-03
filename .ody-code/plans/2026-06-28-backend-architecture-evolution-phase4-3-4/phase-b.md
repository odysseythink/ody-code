# Phase B: retry、errors、turn_step、run_turn、tool_call

本部分在 Phase A 的公共契约之上实现无状态引擎的运行时。Task 5/6/8 可并行启动，但 Task 7 `run_turn` 必须最后合入，因为它同时依赖 Task 5/6/8。

---

## Phase B 任务依赖图

```text
Phase A (types/events/tool_access/scheduler)
        │
        ├──▶ Task 5 (retry + errors)
        │         │
        │         ▼
        │    Task 6 (execute_loop_step)
        │         │
        │         ▼
        │    Task 7 (run_turn)
        │
        └──▶ Task 8 (tool_call batch)
                  │
                  ▼
             Task 7 (run_turn)
```

---

### Task 5: 实现 `retry.rs` 与 `errors.rs`

**Depends on:** Phase A Task 2（`Llm` trait 与 `LlmChatResponse` 已存在）

**Files:**
- Create: `rust-ody/crates/agent-rs/src/agent_loop/errors.rs`
- Create: `rust-ody/crates/agent-rs/src/agent_loop/retry.rs`
- Test: `rust-ody/crates/agent-rs/tests/loop_retry.rs`

#### 步骤

- [ ] 写失败测试 `tests/loop_retry.rs`：

```rust
use agent_rs::agent_loop::events::{LoopEventDispatcher, DefaultLoopEventDispatcher, LoopLiveOnlyEvent, LoopRecordedEvent};
use agent_rs::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse};
use agent_rs::agent_loop::retry::{chat_with_retry, ChatWithRetryInput};
use kosong_rs::message::ContentPart;
use kosong_rs::provider::{AbortSignal, FinishReason};
use kosong_rs::usage::TokenUsage;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

struct FlakyLlm {
    fails_left: AtomicUsize,
}

#[async_trait::async_trait]
impl Llm for FlakyLlm {
    fn system_prompt(&self) -> &str { "" }
    fn model_name(&self) -> &str { "flaky" }
    fn is_retryable_error(&self, err: &dyn std::error::Error) -> bool {
        err.to_string().contains("retryable")
    }
    async fn chat(&self, _params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        let left = self.fails_left.fetch_sub(1, Ordering::SeqCst);
        if left > 0 {
            Err(anyhow::anyhow!("retryable failure"))
        } else {
            Ok(LlmChatResponse {
                tool_calls: vec![],
                provider_finish_reason: Some(FinishReason::Completed),
                raw_finish_reason: Some("stop".into()),
                usage: TokenUsage { input_other: 1, output: 1, input_cache_read: 0, input_cache_creation: 0 },
                stream_timing: None,
            })
        }
    }
}

#[tokio::test]
async fn retry_succeeds_after_one_failure_and_emits_retrying_event() {
    let llm = FlakyLlm { fails_left: AtomicUsize::new(1) };
    let live = Arc::new(std::sync::Mutex::new(Vec::new()));
    let l = live.clone();
    let dispatcher = DefaultLoopEventDispatcher::new(
        |_event: LoopRecordedEvent| async move { Ok::<_, anyhow::Error>(()) },
        Some(Box::new(move |event| {
            if let agent_rs::agent_loop::events::LoopEvent::Live(LoopLiveOnlyEvent::StepRetrying(ev)) = event {
                l.lock().unwrap().push(ev);
            }
        })),
    );
    let input = ChatWithRetryInput {
        llm: &llm,
        params: LlmChatParams {
            messages: vec![],
            tools: vec![],
            signal: AbortSignal::new(),
            request_log_context: None,
            on_text_delta: None,
            on_think_delta: None,
            on_tool_call_delta: None,
            on_text_part: None,
            on_think_part: None,
        },
        dispatch_event: Arc::new(dispatcher),
        turn_id: "t1".into(),
        current_step: 1,
        step_uuid: "s1".into(),
        max_attempts: Some(3),
    };
    let response = chat_with_retry(input).await.unwrap();
    assert_eq!(response.provider_finish_reason, Some(FinishReason::Completed));
    let retry_events = live.lock().unwrap();
    assert_eq!(retry_events.len(), 1);
    assert_eq!(retry_events[0].failed_attempt, 1);
    assert_eq!(retry_events[0].next_attempt, 2);
}
```

运行并确认失败：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo test -p agent-rs --test loop_retry
```

- [ ] 实现 `errors.rs` 与 `retry.rs`：

`rust-ody/crates/agent-rs/src/agent_loop/errors.rs`：

```rust
use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum LoopError {
    #[error("Turn exceeded maxSteps={max_steps}")]
    MaxStepsExceeded { max_steps: u32 },
    #[error("Aborted")]
    Aborted,
    #[error("{0}")]
    Other(String),
}

impl LoopError {
    pub fn is_abort(&self) -> bool {
        matches!(self, LoopError::Aborted)
    }
    pub fn is_max_steps(&self) -> bool {
        matches!(self, LoopError::MaxStepsExceeded { .. })
    }
}

pub fn create_max_steps_exceeded_error(max_steps: u32) -> LoopError {
    LoopError::MaxStepsExceeded { max_steps }
}

pub fn is_max_steps_exceeded_error(err: &anyhow::Error) -> bool {
    err.downcast_ref::<LoopError>().map(|e| e.is_max_steps()).unwrap_or(false)
}

pub fn is_abort_error(err: &anyhow::Error) -> bool {
    err.downcast_ref::<LoopError>().map(|e| e.is_abort()).unwrap_or(false)
}

pub fn error_message(err: &anyhow::Error) -> String {
    err.to_string()
}
```

`rust-ody/crates/agent-rs/src/agent_loop/retry.rs`：

```rust
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use kosong_rs::provider::AbortSignal;
use tokio::time::sleep;

use crate::agent_loop::errors::is_abort_error;
use crate::agent_loop::events::{LoopEventDispatcher, LoopLiveOnlyEvent};
use crate::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse, LlmRequestLogContext};

pub const DEFAULT_MAX_RETRY_ATTEMPTS: u32 = 3;
const RETRY_MIN_TIMEOUT_MS: u64 = 300;
const RETRY_MAX_TIMEOUT_MS: u64 = 5000;
const RETRY_FACTOR: u64 = 2;

pub struct ChatWithRetryInput<'a> {
    pub llm: &'a dyn Llm,
    pub params: LlmChatParams,
    pub dispatch_event: Arc<dyn LoopEventDispatcher>,
    pub turn_id: String,
    pub current_step: u32,
    pub step_uuid: String,
    pub max_attempts: Option<u32>,
}

pub async fn chat_with_retry(input: ChatWithRetryInput<'_>) -> Result<LlmChatResponse, anyhow::Error> {
    let max_attempts = input.max_attempts.unwrap_or(DEFAULT_MAX_RETRY_ATTEMPTS);
    let effective_max = std::cmp::max(max_attempts, 1);

    if effective_max <= 1 {
        let params = params_for_attempt(&input, 1, effective_max);
        return input.llm.chat(params).await;
    }

    let delays = retry_backoff_delays(effective_max);

    for attempt in 1..=effective_max {
        let params = params_for_attempt(&input, attempt, effective_max);
        match input.llm.chat(params).await {
            Ok(response) => return Ok(response),
            Err(error) => {
                if attempt >= effective_max || !input.llm.is_retryable_error(error.root_cause()) {
                    return Err(error);
                }
                input.params.signal.throw_if_aborted().map_err(|_| anyhow::anyhow!("Aborted"))?;
                let delay_ms = delays.get((attempt - 1) as usize).copied().unwrap_or(0);
                input.dispatch_event.dispatch_live(LoopLiveOnlyEvent::StepRetrying(
                    crate::agent_loop::events::StepRetryingEvent {
                        turn_id: input.turn_id.clone(),
                        step: input.current_step,
                        step_uuid: input.step_uuid.clone(),
                        failed_attempt: attempt,
                        next_attempt: attempt + 1,
                        max_attempts: effective_max,
                        delay_ms,
                        error_name: error.root_cause().to_string(),
                        error_message: error.to_string(),
                        status_code: status_code_from_error(&error),
                    }
                ));
                sleep_for_retry(delay_ms, &input.params.signal).await?;
            }
        }
    }

    unreachable!()
}

fn params_for_attempt(input: &ChatWithRetryInput<'_>, attempt: u32, max_attempts: u32) -> LlmChatParams {
    let mut params = input.params.clone();
    params.request_log_context = Some(LlmRequestLogContext {
        turn_id: Some(input.turn_id.clone()),
        step: Some(input.current_step),
        step_uuid: Some(input.step_uuid.clone()),
        attempt: Some(attempt),
        max_attempts: Some(max_attempts),
    });
    params
}

fn status_code_from_error(error: &anyhow::Error) -> Option<i32> {
    use kosong_rs::errors::ChatProviderError;
    error
        .downcast_ref::<ChatProviderError>()
        .and_then(|e| match e {
            ChatProviderError::Status(s) => Some(s.status_code as i32),
            ChatProviderError::ContextOverflow(c) => Some(c.status_code as i32),
            _ => None,
        })
}

pub fn retry_backoff_delays(max_attempts: u32) -> Vec<u64> {
    let mut delays = Vec::new();
    let mut current = RETRY_MIN_TIMEOUT_MS;
    for _ in 0..max_attempts.saturating_sub(1) {
        delays.push(std::cmp::min(current, RETRY_MAX_TIMEOUT_MS));
        current *= RETRY_FACTOR;
    }
    delays
}

pub async fn sleep_for_retry(delay_ms: u64, signal: &AbortSignal) -> Result<(), anyhow::Error> {
    if signal.is_aborted() {
        return Err(anyhow::anyhow!("Aborted"));
    }
    let sleep_future = sleep(Duration::from_millis(delay_ms));
    tokio::pin!(sleep_future);
    loop {
        tokio::select! {
            _ = &mut sleep_future => return Ok(()),
            _ = tokio::task::yield_now() => {
                if signal.is_aborted() {
                    return Err(anyhow::anyhow!("Aborted"));
                }
            }
        }
    }
}
```

注意：`AbortSignal` 目前无 `throw_if_aborted`，需在本 task 中给 `kosong-rs::provider::AbortSignal` 添加方法（共享签名变更）。本 task 必须同时更新 `kosong-rs/src/provider.rs` 并全 workspace typecheck。

在 `rust-ody/crates/kosong-rs/src/provider.rs` `impl AbortSignal` 内追加：

```rust
pub fn throw_if_aborted(&self) -> Result<(), anyhow::Error> {
    if self.is_aborted() {
        Err(anyhow::anyhow!("Aborted"))
    } else {
        Ok(())
    }
}
```

`AbortSignal` 已 derive `Clone`，可直接使用。

- [ ] 运行测试：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo test -p agent-rs --test loop_retry
cargo check --workspace --tests
```

- [ ] 提交：`feat(agent-rs): loop retry and error classification`

---

### Task 6: 实现 `turn_step.rs` (`execute_loop_step`)

**Depends on:** Task 5（`chat_with_retry`、`LoopError`、`AbortSignal::throw_if_aborted`）

**Files:**
- Create: `rust-ody/crates/agent-rs/src/agent_loop/turn_step.rs`
- Modify: `rust-ody/crates/agent-rs/src/agent_loop/types.rs`（`ExecuteLoopStepDeps` 与 `StepResult`；`RunTurnInput.dispatch_event` 改为 `Arc<dyn LoopEventDispatcher>`；回调/record_usage 改为 `Arc`）
- Modify: `rust-ody/crates/agent-rs/src/agent_loop/llm.rs`（回调类型改为 `Arc`，`LlmChatParams` 加 `Clone`）
- Modify: `rust-ody/crates/kosong-rs/src/provider.rs`（`Tool` 加 `Clone`）
- Modify: `rust-ody/crates/agent-rs/Cargo.toml`（追加 `uuid`、`tracing` 依赖）
- Test: `rust-ody/crates/agent-rs/tests/turn_step.rs`
- Test: `rust-ody/crates/agent-rs/tests/turn_step.rs`

#### 步骤

- [ ] 写失败测试 `tests/turn_step.rs`：

```rust
use std::sync::{Arc, Mutex};
use agent_rs::agent_loop::events::{DefaultLoopEventDispatcher, LoopEvent, LoopEventDispatcher, LoopRecordedEvent};
use agent_rs::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse};
use agent_rs::agent_loop::turn_step::execute_loop_step;
use agent_rs::agent_loop::types::{ExecuteLoopStepDeps, LoopHooks, LoopStepStopReason};
use kosong_rs::message::{ContentPart, Message, Role};
use kosong_rs::provider::{AbortSignal, FinishReason};
use kosong_rs::usage::TokenUsage;

struct TextLlm;

#[async_trait::async_trait]
impl Llm for TextLlm {
    fn system_prompt(&self) -> &str { "" }
    fn model_name(&self) -> &str { "text" }
    async fn chat(&self, params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        if let Some(cb) = params.on_text_delta {
            cb("hello".into());
            cb(" world".into());
        }
        if let Some(cb) = params.on_text_part {
            cb(kosong_rs::message::TextPart { text: "hello world".into() }).await;
        }
        Ok(LlmChatResponse {
            tool_calls: vec![],
            provider_finish_reason: Some(FinishReason::Completed),
            raw_finish_reason: Some("stop".into()),
            usage: TokenUsage { input_other: 2, output: 2, input_cache_read: 0, input_cache_creation: 0 },
            stream_timing: None,
        })
    }
}

#[tokio::test]
async fn text_step_emits_begin_end_content_part_and_deltas() {
    let events: Arc<Mutex<Vec<LoopEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let e = events.clone();
    let dispatcher = DefaultLoopEventDispatcher::new(
        move |event: LoopRecordedEvent| {
            let e = e.clone();
            async move { e.lock().unwrap().push(LoopEvent::Recorded(event)); Ok::<_, anyhow::Error>(()) }
        },
        Some(Box::new(move |event| { events.lock().unwrap().push(event); })),
    );

    let result = execute_loop_step(ExecuteLoopStepDeps {
        turn_id: "t1".into(),
        signal: AbortSignal::new(),
        build_messages: Box::new(|| Box::pin(async { Ok(vec![Message::user_text("go")]) })),
        dispatch_event: Arc::new(dispatcher),
        llm: &TextLlm,
        tools: None,
        hooks: None,
        log: None,
        current_step: 1,
        max_retry_attempts: Some(1),
        record_usage: Arc::new(|usage| Box::pin(async move { assert_eq!(usage.output, 2); Ok(None) })),
    }).await.unwrap();

    assert_eq!(result.stop_reason, LoopStepStopReason::EndTurn);
}
```

运行并确认失败。

- [ ] 在 `rust-ody/crates/agent-rs/Cargo.toml` `[dependencies]` 追加：

```toml
uuid = { version = "1", features = ["v4"] }
tracing = "0.1"
```

- [ ] 修改共享回调签名（使 `LlmChatParams` 可在重试间克隆）：
  - 在 `rust-ody/crates/kosong-rs/src/provider.rs` 的 `pub struct Tool` 上添加 `#[derive(Debug, Clone)]`：

    ```rust
    #[derive(Debug, Clone)]
    pub struct Tool {
        pub name: String,
        pub description: String,
        pub parameters: serde_json::Value,
    }
    ```

  - 在 `rust-ody/crates/agent-rs/src/agent_loop/llm.rs` 顶部加 `use std::sync::Arc;`，并将回调类型别名与 `LlmChatParams` 改为：

    ```rust
    pub type TextDeltaCallback = Arc<dyn Fn(String) + Send + Sync>;
    pub type ThinkDeltaCallback = Arc<dyn Fn(String) + Send + Sync>;
    pub type ToolCallDeltaCallback = Arc<dyn Fn(ToolCallDelta) + Send + Sync>;
    pub type TextPartCallback = Arc<dyn Fn(TextPart) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;
    pub type ThinkPartCallback = Arc<dyn Fn(ThinkPart) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

    #[derive(Debug, Clone)]
    pub struct LlmChatParams {
        pub messages: Vec<Message>,
        pub tools: Vec<Tool>,
        pub signal: AbortSignal,
        pub request_log_context: Option<LlmRequestLogContext>,
        pub on_text_delta: Option<TextDeltaCallback>,
        pub on_think_delta: Option<ThinkDeltaCallback>,
        pub on_tool_call_delta: Option<ToolCallDeltaCallback>,
        pub on_text_part: Option<TextPartCallback>,
        pub on_think_part: Option<ThinkPartCallback>,
    }
    ```

- [ ] 实现 `turn_step.rs` 并更新 `types.rs`：

`rust-ody/crates/agent-rs/src/agent_loop/turn_step.rs`：

```rust
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use kosong_rs::message::{FinishReason, TextPart, ThinkPart};
use kosong_rs::provider::AbortSignal;
use kosong_rs::usage::TokenUsage;
use uuid::Uuid;

use crate::agent_loop::errors::{error_message, is_abort_error};
use crate::agent_loop::events::{LoopEventDispatcher, LoopLiveOnlyEvent, LoopRecordedEvent};
use crate::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse, ToolCallDelta};
use crate::agent_loop::retry::chat_with_retry;
use crate::agent_loop::tool_call::{run_tool_call_batch, ToolCallStepContext};
use crate::agent_loop::types::{
    AfterStepResult, BeforeStepResult, ExecuteLoopStepDeps, LoopHooks, LoopStepHookContext,
    LoopStepStopReason, RecordStepUsageResult, StepResult,
};

pub async fn execute_loop_step(deps: ExecuteLoopStepDeps<'_>) -> Result<StepResult, anyhow::Error> {
    let ExecuteLoopStepDeps {
        turn_id,
        signal,
        build_messages,
        dispatch_event,
        llm,
        tools,
        hooks,
        log: _,
        current_step,
        max_retry_attempts,
        record_usage,
    } = deps;

    if let Some(hooks) = hooks {
        if let Some(before) = &hooks.before_step {
            let ctx = LoopStepHookContext { turn_id: turn_id.as_str(), step_number: current_step, signal: signal.clone(), llm };
            match before.before_step(ctx).await {
                Ok(Some(BeforeStepResult { block: Some(true), reason })) => {
                    return Err(anyhow::anyhow!("{}", reason.unwrap_or_else(|| format!("Step {} was blocked", current_step))));
                }
                Ok(_) => {}
                Err(e) => return Err(e),
            }
        }
    }

    signal.throw_if_aborted()?;
    let messages = build_messages().await?;
    signal.throw_if_aborted()?;

    let step_uuid = Uuid::new_v4().to_string();
    dispatch_event.dispatch_recorded(LoopRecordedEvent::StepBegin {
        uuid: step_uuid.clone(),
        turn_id: turn_id.clone(),
        step: current_step as i64,
    }).await?;

    let chat_params = LlmChatParams {
        messages,
        tools: tools.as_ref().map(|t| t.iter().map(|tool| kosong_rs::provider::Tool {
            name: tool.name().into(),
            description: tool.description().into(),
            parameters: tool.parameters().clone(),
        }).collect()).unwrap_or_default(),
        signal: signal.clone(),
        request_log_context: None,
        ..create_streaming_callbacks(dispatch_event.clone(), turn_id.clone(), current_step, step_uuid.clone())
    };

    let response = chat_with_retry(crate::agent_loop::retry::ChatWithRetryInput {
        llm,
        params: chat_params,
        dispatch_event: dispatch_event.clone(),
        turn_id: turn_id.clone(),
        current_step,
        step_uuid: step_uuid.clone(),
        max_attempts: max_retry_attempts,
    }).await?;

    let usage = response.usage.clone();
    let usage_result = record_usage(usage.clone()).await?;
    let stop_turn_after_usage = usage_result.and_then(|r| r.stop_turn).unwrap_or(false);
    let stop_reason = derive_step_stop_reason(&response);

    let mut effective_stop_reason = if stop_turn_after_usage && stop_reason == LoopStepStopReason::ToolUse {
        LoopStepStopReason::EndTurn
    } else {
        stop_reason
    };

    if effective_stop_reason == LoopStepStopReason::ToolUse {
        let step_ctx = ToolCallStepContext {
            tools,
            hooks,
            dispatch_event: dispatch_event.clone(),
            llm,
            signal: signal.clone(),
            turn_id: turn_id.clone(),
            current_step,
            step_uuid: step_uuid.clone(),
        };
        let batch_result = run_tool_call_batch(&step_ctx, &response).await?;
        if batch_result.stop_turn {
            effective_stop_reason = LoopStepStopReason::EndTurn;
        }
    }

    signal.throw_if_aborted()?;

    let diagnostics = step_end_provider_diagnostics(&response, effective_stop_reason);
    dispatch_event.dispatch_recorded(LoopRecordedEvent::StepEnd {
        uuid: step_uuid.clone(),
        turn_id: turn_id.clone(),
        step: current_step as i64,
        usage: Some(usage.clone()),
        finish_reason: Some(serde_json::to_string(&effective_stop_reason).unwrap().trim_matches('"').to_string()),
        llm_first_token_latency_ms: response.stream_timing.as_ref().map(|t| t.first_token_latency_ms as i64),
        llm_stream_duration_ms: response.stream_timing.as_ref().map(|t| t.stream_duration_ms as i64),
        provider_finish_reason: diagnostics.provider_finish_reason.map(|f| serde_json::to_string(&f).unwrap().trim_matches('"').to_string()),
        raw_finish_reason: diagnostics.raw_finish_reason.clone(),
    }).await?;

    let mut stop_turn_after_step = stop_turn_after_usage;
    if let Some(hooks) = hooks {
        if let Some(after) = &hooks.after_step {
            let ctx = crate::agent_loop::types::LoopAfterStepContext {
                turn_id: turn_id.as_str(),
                step_number: current_step,
                signal: signal.clone(),
                llm,
                usage: usage.clone(),
                stop_reason: effective_stop_reason,
            };
            match after.after_step(ctx).await {
                Ok(Some(AfterStepResult { stop_turn: Some(true) })) => stop_turn_after_step = true,
                _ => {}
            }
        }
    }

    Ok(StepResult {
        usage,
        stop_reason: if stop_turn_after_step && effective_stop_reason == LoopStepStopReason::ToolUse {
            LoopStepStopReason::EndTurn
        } else {
            effective_stop_reason
        },
    })
}

fn derive_step_stop_reason(response: &LlmChatResponse) -> LoopStepStopReason {
    use LoopStepStopReason::*;
    match response.provider_finish_reason {
        Some(FinishReason::Truncated) => MaxTokens,
        Some(FinishReason::Filtered) => Filtered,
        Some(FinishReason::Paused) => Paused,
        Some(FinishReason::Other) => Unknown,
        Some(FinishReason::Completed) | None => {
            if response.tool_calls.is_empty() { EndTurn } else { ToolUse }
        }
        Some(FinishReason::ToolCalls) => {
            if response.tool_calls.is_empty() { Unknown } else { ToolUse }
        }
    }
}

#[derive(Default)]
struct StepEndDiagnostics {
    provider_finish_reason: Option<FinishReason>,
    raw_finish_reason: Option<String>,
}

fn step_end_provider_diagnostics(response: &LlmChatResponse, stop_reason: LoopStepStopReason) -> StepEndDiagnostics {
    let provider = response.provider_finish_reason;
    if matches!((provider, stop_reason),
        (Some(FinishReason::Completed), LoopStepStopReason::EndTurn)
        | (Some(FinishReason::ToolCalls), LoopStepStopReason::ToolUse)
    ) {
        return StepEndDiagnostics::default();
    }
    StepEndDiagnostics {
        provider_finish_reason: provider,
        raw_finish_reason: response.raw_finish_reason.clone(),
    }
}

fn create_streaming_callbacks(
    dispatch_event: Arc<dyn LoopEventDispatcher>,
    turn_id: String,
    current_step: u32,
    step_uuid: String,
) -> LlmChatParams {
    let dispatch_event_text = dispatch_event.clone();
    let on_text_delta: std::sync::Arc<dyn Fn(String) + Send + Sync> = std::sync::Arc::new(move |delta| {
        dispatch_event_text.dispatch_live(LoopLiveOnlyEvent::TextDelta { delta });
    });

    let dispatch_event_think = dispatch_event.clone();
    let on_think_delta: std::sync::Arc<dyn Fn(String) + Send + Sync> = std::sync::Arc::new(move |delta| {
        dispatch_event_think.dispatch_live(LoopLiveOnlyEvent::ThinkingDelta { delta });
    });

    let dispatch_event_tool = dispatch_event.clone();
    let on_tool_call_delta: std::sync::Arc<dyn Fn(ToolCallDelta) + Send + Sync> = std::sync::Arc::new(move |delta| {
        dispatch_event_tool.dispatch_live(LoopLiveOnlyEvent::ToolCallDelta {
            tool_call_id: delta.tool_call_id,
            name: delta.name,
            arguments_part: delta.arguments_part,
        });
    });

    let dispatch_event_text_part = dispatch_event.clone();
    let on_text_part: std::sync::Arc<dyn Fn(TextPart) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync> = std::sync::Arc::new(move |part| {
        let dispatch_event = dispatch_event_text_part.clone();
        let turn_id = turn_id.clone();
        let step_uuid = step_uuid.clone();
        Box::pin(async move {
            dispatch_event.dispatch_recorded(LoopRecordedEvent::ContentPartEvent {
                uuid: Uuid::new_v4().to_string(),
                turn_id,
                step: current_step as i64,
                step_uuid,
                part: ContentPart::Text { text: part.text },
            }).await.ok();
        })
    });

    let dispatch_event_think_part = dispatch_event.clone();
    let on_think_part: std::sync::Arc<dyn Fn(ThinkPart) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync> = std::sync::Arc::new(move |part| {
        let dispatch_event = dispatch_event_think_part.clone();
        Box::pin(async move {
            dispatch_event.dispatch_recorded(LoopRecordedEvent::ContentPartEvent {
                uuid: Uuid::new_v4().to_string(),
                turn_id: turn_id.clone(),
                step: current_step as i64,
                step_uuid: step_uuid.clone(),
                part: ContentPart::Think { think: part.think, encrypted: part.encrypted },
            }).await.ok();
        })
    });

    LlmChatParams {
        messages: vec![],
        tools: vec![],
        signal: AbortSignal::new(),
        request_log_context: None,
        on_text_delta: Some(on_text_delta),
        on_think_delta: Some(on_think_delta),
        on_tool_call_delta: Some(on_tool_call_delta),
        on_text_part: Some(on_text_part),
        on_think_part: Some(on_think_part),
    }
}
```

`rust-ody/crates/agent-rs/src/agent_loop/types.rs` 追加/修改：

```rust
use std::sync::Arc;

// ... existing imports ...

pub struct ExecuteLoopStepDeps<'a> {
    pub turn_id: String,
    pub signal: AbortSignal,
    pub build_messages: LoopMessageBuilder,
    pub dispatch_event: Arc<dyn crate::agent_loop::events::LoopEventDispatcher>,
    pub llm: &'a dyn Llm,
    pub tools: Option<Vec<Box<dyn ExecutableTool>>>,
    pub hooks: Option<&'a LoopHooks>,
    pub log: Option<Arc<dyn tracing::Subscriber>>,
    pub current_step: u32,
    pub max_retry_attempts: Option<u32>,
    pub record_usage: Arc<
        dyn Fn(TokenUsage) -> Pin<Box<dyn Future<Output = Result<Option<RecordStepUsageResult>, anyhow::Error>> + Send>>
            + Send
            + Sync,
    >,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StepResult {
    pub usage: TokenUsage,
    pub stop_reason: LoopStepStopReason,
}

pub struct RunTurnInput {
    pub turn_id: String,
    pub signal: AbortSignal,
    pub llm: Box<dyn Llm>,
    pub build_messages: LoopMessageBuilder,
    pub dispatch_event: Arc<dyn crate::agent_loop::events::LoopEventDispatcher>,
    pub tools: Option<Vec<Box<dyn ExecutableTool>>>,
    pub hooks: Option<LoopHooks>,
    pub max_steps: Option<u32>,
    pub max_retry_attempts: Option<u32>,
    pub record_step_usage: Option<
        Arc<
            dyn Fn(TokenUsage) -> Pin<Box<dyn Future<Output = Result<Option<RecordStepUsageResult>, anyhow::Error>> + Send>>
                + Send
                + Sync,
        >,
    >,
}
```

- [ ] 运行测试与全 tree typecheck：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo test -p agent-rs --test turn_step
cargo check --workspace --tests
```

- [ ] 提交：`feat(agent-rs): execute_loop_step with streaming callbacks`

---

### Task 7: 实现 `run_turn.rs` (`run_turn`)

**Depends on:** Task 5（errors/abort）、Task 6（`execute_loop_step`）、Task 8（`run_tool_call_batch` 已被 Task 6 调用）

**Files:**
- Create: `rust-ody/crates/agent-rs/src/agent_loop/run_turn.rs`
- Test: `rust-ody/crates/agent-rs/tests/run_turn.rs`

#### 步骤

- [ ] 写失败测试 `tests/run_turn.rs`：

```rust
use std::sync::{Arc, Mutex};
use agent_rs::agent_loop::events::{DefaultLoopEventDispatcher, LoopEvent, LoopEventDispatcher, LoopLiveOnlyEvent, LoopRecordedEvent};
use agent_rs::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse};
use agent_rs::agent_loop::run_turn::run_turn;
use agent_rs::agent_loop::types::{LoopHooks, LoopTurnStopReason, RunTurnInput};
use kosong_rs::message::{ContentPart, Message, Role};
use kosong_rs::provider::{AbortSignal, FinishReason};
use kosong_rs::usage::TokenUsage;

struct SingleTextLlm;
#[async_trait::async_trait]
impl Llm for SingleTextLlm {
    fn system_prompt(&self) -> &str { "" }
    fn model_name(&self) -> &str { "single" }
    async fn chat(&self, _params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        Ok(LlmChatResponse {
            tool_calls: vec![],
            provider_finish_reason: Some(FinishReason::Completed),
            raw_finish_reason: Some("stop".into()),
            usage: TokenUsage { input_other: 1, output: 1, input_cache_read: 0, input_cache_creation: 0 },
            stream_timing: None,
        })
    }
}

#[tokio::test]
async fn run_turn_completes_with_end_turn() {
    let events: Arc<Mutex<Vec<LoopEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let e = events.clone();
    let dispatcher = DefaultLoopEventDispatcher::new(
        move |event: LoopRecordedEvent| { let e = e.clone(); async move { e.lock().unwrap().push(LoopEvent::Recorded(event)); Ok::<_, anyhow::Error>(()) } },
        Some(Box::new(move |event| { events.lock().unwrap().push(event); })),
    );

    let result = run_turn(RunTurnInput {
        turn_id: "t1".into(),
        signal: AbortSignal::new(),
        llm: Box::new(SingleTextLlm),
        build_messages: Box::new(|| Box::pin(async { Ok(vec![Message::user_text("go")]) })),
        dispatch_event: Arc::new(dispatcher),
        tools: None,
        hooks: None,
        max_steps: Some(5),
        max_retry_attempts: Some(1),
        record_step_usage: None,
    }).await.unwrap();

    assert_eq!(result.stop_reason, LoopTurnStopReason::EndTurn);
    assert_eq!(result.steps, 1);
}
```

- [ ] 实现 `run_turn.rs`：

```rust
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use kosong_rs::provider::AbortSignal;
use kosong_rs::usage::TokenUsage;

use crate::agent_loop::errors::{create_max_steps_exceeded_error, is_abort_error, is_max_steps_exceeded_error, LoopError};
use crate::agent_loop::events::{LoopEventDispatcher, LoopInterruptReason, LoopLiveOnlyEvent};
use crate::agent_loop::llm::Llm;
use crate::agent_loop::turn_step::execute_loop_step;
use crate::agent_loop::types::{
    LoopHooks, LoopStepStopReason, LoopTerminalStepStopReason, LoopTurnStopReason, RecordStepUsageResult,
    RunTurnInput, StepResult, TurnResult,
};

fn empty_usage() -> TokenUsage {
    TokenUsage::default()
}

fn add_usage(a: TokenUsage, b: TokenUsage) -> TokenUsage {
    TokenUsage {
        input_other: a.input_other + b.input_other,
        output: a.output + b.output,
        input_cache_read: a.input_cache_read + b.input_cache_read,
        input_cache_creation: a.input_cache_creation + b.input_cache_creation,
    }
}

pub async fn run_turn(input: RunTurnInput) -> Result<TurnResult, anyhow::Error> {
    let RunTurnInput {
        turn_id,
        signal,
        llm,
        build_messages,
        dispatch_event,
        tools,
        hooks,
        max_steps,
        max_retry_attempts,
        record_step_usage,
    } = input;

    let usage_shared = Arc::new(Mutex::new(empty_usage()));
    let mut steps: u32 = 0;
    let mut stop_reason: LoopTurnStopReason = LoopTurnStopReason::EndTurn;
    let mut active_step: Option<u32> = None;

    let record_usage: Arc<
        dyn Fn(TokenUsage) -> Pin<Box<dyn Future<Output = Result<Option<RecordStepUsageResult>, anyhow::Error>> + Send>>
            + Send
            + Sync,
    > = {
        let usage_shared = usage_shared.clone();
        let host_cb = record_step_usage.clone();
        Box::new(move |step_usage: TokenUsage| {
            let usage_shared = usage_shared.clone();
            let host_cb = host_cb.clone();
            Box::pin(async move {
                let mut u = usage_shared.lock().unwrap();
                *u = add_usage(u.clone(), step_usage.clone());
                drop(u);
                if let Some(cb) = host_cb {
                    cb(step_usage).await
                } else {
                    Ok(None)
                }
            })
        })
    };

    let result: Result<(), anyhow::Error> = async {
        loop {
            signal.throw_if_aborted()?;

            if let Some(max) = max_steps {
                if max > 0 && steps >= max {
                    return Err(create_max_steps_exceeded_error(max).into());
                }
            }

            steps += 1;
            active_step = Some(steps);

            let StepResult { usage: step_usage, stop_reason: step_stop } = execute_loop_step(crate::agent_loop::types::ExecuteLoopStepDeps {
                turn_id: turn_id.clone(),
                signal: signal.clone(),
                build_messages: build_messages.clone(),
                dispatch_event: dispatch_event.clone(),
                llm: llm.as_ref(),
                tools: tools.clone(),
                hooks: hooks.as_ref(),
                log: None,
                current_step: steps,
                max_retry_attempts,
                record_usage: record_usage.clone(),
            }).await?;

            active_step = None;

            if step_stop == LoopStepStopReason::ToolUse {
                continue;
            }

            let terminal: LoopTerminalStepStopReason = step_stop;
            stop_reason = terminal.into();

            let should_continue = if let Some(hooks) = &hooks {
                if let Some(hook) = &hooks.should_continue_after_stop {
                    let ctx = crate::agent_loop::types::LoopStoppedStepContext {
                        turn_id: turn_id.as_str(),
                        step_number: steps,
                        signal: signal.clone(),
                        llm: llm.as_ref(),
                        usage: step_usage,
                        stop_reason: terminal,
                    };
                    hook.should_continue_after_stop(ctx).await?.map(|r| r.continue_).unwrap_or(false)
                } else {
                    false
                }
            } else {
                false
            };

            if !should_continue {
                break;
            }
        }
        Ok(())
    }.await;

    let usage = usage_shared.lock().unwrap().clone();

    match result {
        Ok(()) => Ok(TurnResult { stop_reason, steps, usage }),
        Err(err) => {
            if is_abort_error(&err) || signal.is_aborted() {
                dispatch_event.dispatch_live(LoopLiveOnlyEvent::TurnInterrupted {
                    reason: LoopInterruptReason::Aborted,
                    attempted_steps: steps,
                    active_step,
                    message: None,
                });
                return Ok(TurnResult { stop_reason: LoopTurnStopReason::Aborted, steps, usage });
            }
            if is_max_steps_exceeded_error(&err) {
                dispatch_event.dispatch_live(LoopLiveOnlyEvent::TurnInterrupted {
                    reason: LoopInterruptReason::MaxSteps,
                    attempted_steps: steps,
                    active_step,
                    message: Some(err.to_string()),
                });
            } else {
                dispatch_event.dispatch_live(LoopLiveOnlyEvent::TurnInterrupted {
                    reason: LoopInterruptReason::Error,
                    attempted_steps: steps,
                    active_step,
                    message: Some(err.to_string()),
                });
            }
            Err(err)
        }
    }
}

impl From<LoopStepStopReason> for LoopTurnStopReason {
    fn from(value: LoopStepStopReason) -> Self {
        match value {
            LoopStepStopReason::EndTurn => LoopTurnStopReason::EndTurn,
            LoopStepStopReason::MaxTokens => LoopTurnStopReason::MaxTokens,
            LoopStepStopReason::ToolUse => LoopTurnStopReason::Unknown, // tool_use cannot be terminal
            LoopStepStopReason::Filtered => LoopTurnStopReason::Filtered,
            LoopStepStopReason::Paused => LoopTurnStopReason::Paused,
            LoopStepStopReason::Unknown => LoopTurnStopReason::Unknown,
        }
    }
}
```

- [ ] 运行测试：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo test -p agent-rs --test run_turn
cargo check --workspace --tests
```

- [ ] 提交：`feat(agent-rs): run_turn convergence loop`

---

### Task 8: 实现 `tool_call.rs` (`run_tool_call_batch`)

**Depends on:** Task 4（`ToolScheduler`、`ToolAccesses`）、Task 6（`ToolCallStepContext` 签名已存在）

**Files:**
- Create: `rust-ody/crates/agent-rs/src/agent_loop/tool_call.rs`
- Modify: `rust-ody/crates/agent-rs/src/agent_loop/tool_access.rs`（追加 `PathSecurityError`）
- Test: `rust-ody/crates/agent-rs/tests/tool_call_batch.rs`

#### 步骤

- [ ] 写失败测试 `tests/tool_call_batch.rs`：

```rust
use std::sync::{Arc, Mutex};
use agent_rs::agent_loop::events::{DefaultLoopEventDispatcher, LoopEvent, LoopEventDispatcher, LoopRecordedEvent};
use agent_rs::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse};
use agent_rs::agent_loop::tool_call::{run_tool_call_batch, ToolCallBatchResult, ToolCallStepContext};
use agent_rs::agent_loop::types::{ExecutableTool, ExecutableToolContext, ExecutableToolResult, ExecutableToolSuccessResult, ExecutableToolOutput, RunnableToolExecution, ToolExecution};
use agent_rs::agent_loop::tool_access::ToolAccesses;
use kosong_rs::message::ToolCall;
use kosong_rs::provider::{AbortSignal, FinishReason};
use kosong_rs::usage::TokenUsage;
use serde_json::json;

struct AddTool;
#[async_trait::async_trait]
impl kosong_rs::provider::Tool for AddTool {
    fn name(&self) -> &str { "add" }
    fn description(&self) -> &str { "adds two numbers" }
    fn parameters(&self) -> &serde_json::Value { &json!({"type":"object","properties":{"a":{"type":"number"},"b":{"type":"number"}},"required":["a","b"]}) }
}
#[async_trait::async_trait]
impl ExecutableTool for AddTool {
    async fn resolve_execution(&self, input: serde_json::Value) -> Result<ToolExecution, anyhow::Error> {
        Ok(ToolExecution::Runnable(RunnableToolExecution {
            approval_rule: "auto".into(),
            execute: Box::new(move |ctx| Box::pin(async move {
                let a = input["a"].as_f64().unwrap_or(0.0);
                let b = input["b"].as_f64().unwrap_or(0.0);
                Ok(ExecutableToolResult::Success(ExecutableToolSuccessResult {
                    output: ExecutableToolOutput::Text(format!("{}", a + b)),
                    message: None,
                    stop_turn: None,
                }))
            })),
            accesses: Some(ToolAccesses::none()),
            description: None,
            display: None,
            stop_batch_after_this: None,
        }))
    }
}

struct NoopLlm;
#[async_trait::async_trait]
impl Llm for NoopLlm {
    async fn chat(&self, _params: LlmChatParams<'_>) -> Result<LlmChatResponse, anyhow::Error> {
        Ok(LlmChatResponse {
            tool_calls: vec![],
            provider_finish_reason: None,
            raw_finish_reason: None,
            usage: TokenUsage::default(),
            stream_timing: None,
        })
    }
}

#[tokio::test]
async fn single_tool_call_emits_paired_call_and_result() {
    let events: Arc<Mutex<Vec<LoopEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let e = events.clone();
    let dispatcher = DefaultLoopEventDispatcher::new(
        move |event: LoopRecordedEvent| { let e = e.clone(); async move { e.lock().unwrap().push(LoopEvent::Recorded(event)); Ok::<_, anyhow::Error>(()) } },
        Some(Box::new(move |event| { events.lock().unwrap().push(event); })),
    );

    let noop = NoopLlm;
    let step_ctx = ToolCallStepContext {
        tools: Some(vec![Box::new(AddTool) as Box<dyn ExecutableTool>]),
        hooks: None,
        dispatch_event: Arc::new(dispatcher),
        llm: &noop,
        signal: AbortSignal::new(),
        turn_id: "t1".into(),
        current_step: 1,
        step_uuid: "s1".into(),
    };

    let response = LlmChatResponse {
        tool_calls: vec![ToolCall { call_type: "function".into(), id: "tc1".into(), name: "add".into(), arguments: Some("{\"a\":1,\"b\":2}".into()), extras: None, stream_index: None }],
        provider_finish_reason: Some(FinishReason::ToolCalls),
        raw_finish_reason: Some("tool_calls".into()),
        usage: TokenUsage::default(),
        stream_timing: None,
    };

    let result = run_tool_call_batch(&step_ctx, &response).await.unwrap();
    assert!(!result.stop_turn);
}
```

- [ ] 实现 `tool_call.rs`：

- [ ] 在 `rust-ody/crates/agent-rs/src/agent_loop/tool_access.rs` 追加 `PathSecurityError`：

```rust
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct PathSecurityError(pub String);
```

- [ ] 在 `rust-ody/crates/agent-rs/Cargo.toml` `[dependencies]` 追加：

```toml
jsonschema = "0.17"
```

- [ ] 实现 `tool_call.rs`：

```rust
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use jsonschema::JSONSchema;
use kosong_rs::message::{ContentPart, ToolCall};
use kosong_rs::provider::AbortSignal;
use lazy_static::lazy_static;
use serde_json::Value as JsonValue;

use crate::agent_loop::errors::error_message;
use crate::agent_loop::events::{LoopEventDispatcher, LoopLiveOnlyEvent, LoopRecordedEvent};
use crate::agent_loop::llm::Llm;
use crate::agent_loop::tool_access::{PathSecurityError, ToolAccesses};
use crate::agent_loop::tool_scheduler::{ToolCallTask, ToolScheduler};
use crate::agent_loop::types::{
    AuthorizeToolExecutionResult, AuthorizeToolExecutionHook, ExecutableTool, ExecutableToolContext,
    ExecutableToolErrorResult, ExecutableToolOutput, ExecutableToolResult, ExecutableToolSuccessResult,
    FinalizeToolResultContext, FinalizeToolResultHook, LoopHooks, PrepareToolExecutionHook,
    PrepareToolExecutionResult, RunnableToolExecution, ToolExecution, ToolExecutionHookContext,
    ResolvedToolExecutionHookContext, ToolUpdate,
};

const GRACE_TIMEOUT_MS: u64 = 2_000;
const TOOL_OUTPUT_EMPTY: &str = "Tool output is empty.";
const TOOL_OUTPUT_NON_TEXT: &str = "Tool returned non-text content.";

lazy_static! {
    static ref VALIDATORS: Mutex<HashMap<String, JSONSchema>> = Mutex::new(HashMap::new());
}

pub struct ToolCallStepContext<'a> {
    pub tools: Option<Vec<Box<dyn ExecutableTool>>>,
    pub hooks: Option<&'a LoopHooks>,
    pub dispatch_event: Arc<dyn LoopEventDispatcher>,
    pub llm: &'a dyn Llm,
    pub signal: AbortSignal,
    pub turn_id: String,
    pub current_step: u32,
    pub step_uuid: String,
}

pub struct ToolCallBatchResult {
    pub stop_turn: bool,
}

pub async fn run_tool_call_batch(
    step: &ToolCallStepContext<'_>,
    response: &crate::agent_loop::llm::LlmChatResponse,
) -> Result<ToolCallBatchResult, anyhow::Error> {
    if response.tool_calls.is_empty() {
        return Ok(ToolCallBatchResult { stop_turn: false });
    }

    let calls: Vec<PreflightedToolCall> = response
        .tool_calls
        .iter()
        .map(|tc| preflight_tool_call(step.tools.as_deref(), tc))
        .collect();

    let mut scheduler: ToolScheduler<PendingToolResult> = ToolScheduler::new();
    let mut pending_results: Vec<tokio::sync::oneshot::Receiver<Result<PendingToolResult, anyhow::Error>>> = Vec::new();
    let mut stop_turn = false;

    for (index, call) in calls.into_iter().enumerate() {
        let prepared = prepare_tool_call(step, call).await?;
        pending_results.push(scheduler.add(prepared.task).await?);

        if prepared.stop_batch_after_this == Some(true) {
            stop_turn = true;
            for skipped in response.tool_calls.iter().skip(index + 1) {
                let skipped_preflight = preflight_tool_call(step.tools.as_deref(), skipped);
                let skipped_task = prepare_skipped_tool_call(step, skipped_preflight).await?;
                pending_results.push(scheduler.add(skipped_task).await?);
            }
            break;
        }
    }

    for pending in pending_results {
        let result = pending.await??;
        let finalized = finalize_pending_tool_result(step, result).await?;
        if finalized.stop_turn == Some(true) {
            stop_turn = true;
        }
        dispatch_tool_result_event(step, &finalized).await?;
    }

    Ok(ToolCallBatchResult { stop_turn })
}

enum PreflightedToolCall {
    Runnable {
        tool_call: ToolCall,
        tool_name: String,
        args: JsonValue,
    },
    Rejected {
        tool_call: ToolCall,
        tool_name: String,
        args: JsonValue,
        output: String,
    },
}

struct PendingToolResult {
    tool_call: ToolCall,
    tool_name: String,
    args: JsonValue,
    result: ExecutableToolResult,
    stop_turn: Option<bool>,
}

struct PreparedToolCallTask {
    task: ToolCallTask<Result<PendingToolResult, anyhow::Error>>,
    stop_batch_after_this: Option<bool>,
}

type ToolCallDisplayFields = (Option<String>, Option<JsonValue>);

fn preflight_tool_call(
    tools: Option<&[Box<dyn ExecutableTool>]>,
    tool_call: &ToolCall,
) -> PreflightedToolCall {
    let tool_name = tool_call.name.clone();
    let parsed = parse_tool_call_arguments(&tool_call.arguments);
    let args = parsed.as_ref().map(|v| v.clone()).unwrap_or_else(|_| JsonValue::Object(Default::default()));
    let tool = tools.and_then(|list| list.iter().find(|t| t.name() == tool_name));

    match tool {
        None => PreflightedToolCall::Rejected {
            tool_call: tool_call.clone(),
            tool_name,
            args,
            output: format!(r#"Tool "{}" not found"#, tool_name),
        },
        Some(tool) => {
            if let Err(err) = parsed {
                return PreflightedToolCall::Rejected {
                    tool_call: tool_call.clone(),
                    tool_name,
                    args,
                    output: format!(r#"Invalid args for tool "{}": malformed JSON in arguments: {}"#, tool_name, err),
                };
            }
            let data = parsed.unwrap();
            if let Some(err) = validate_executable_tool_args(tool.as_ref(), &data) {
                return PreflightedToolCall::Rejected {
                    tool_call: tool_call.clone(),
                    tool_name,
                    args: data,
                    output: format!(r#"Invalid args for tool "{}": {}"#, tool_name, err),
                };
            }
            PreflightedToolCall::Runnable {
                tool_call: tool_call.clone(),
                tool_name,
                args: data,
            }
        }
    }
}

fn parse_tool_call_arguments(raw: &Option<String>) -> Result<JsonValue, String> {
    match raw {
        None => Ok(JsonValue::Object(Default::default())),
        Some(s) if s.is_empty() => Ok(JsonValue::Object(Default::default())),
        Some(s) => serde_json::from_str(s).map_err(|e| error_message(&anyhow::anyhow!(e))),
    }
}

fn validate_executable_tool_args(tool: &dyn ExecutableTool, args: &JsonValue) -> Option<String> {
    let key = format!("{}:{}", tool.name(), serde_json::to_string(tool.parameters()).unwrap_or_default());
    let mut cache = VALIDATORS.lock().unwrap();
    let schema = cache.entry(key).or_insert_with(|| {
        JSONSchema::compile(tool.parameters()).expect("tool parameter schema compiles")
    });
    let errors: Vec<_> = schema.validate(args).collect();
    if errors.is_empty() {
        return None;
    }
    Some(format_validation_errors(&errors))
}

fn format_validation_errors(errors: &[jsonschema::ValidationError<'_>]) -> String {
    errors
        .iter()
        .map(|e| {
            let path = e.instance_path.to_string();
            if path.is_empty() {
                e.to_string()
            } else {
                format!("validation error at {}: {}", path, e)
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

async fn prepare_tool_call(
    step: &ToolCallStepContext<'_>,
    call: PreflightedToolCall,
) -> Result<PreparedToolCallTask, anyhow::Error> {
    async fn settle_error(
        step: &ToolCallStepContext<'_>,
        call: &PreflightedToolCall,
        args: JsonValue,
        output: String,
        display: ToolCallDisplayFields,
    ) -> Result<PreparedToolCallTask, anyhow::Error> {
        dispatch_tool_call_event(step, call, &args, display).await?;
        Ok(PreparedToolCallTask {
            task: make_resolved_task(make_error_tool_result(call, args, output)),
            stop_batch_after_this: None,
        })
    }

    async fn settle_synthetic(
        step: &ToolCallStepContext<'_>,
        call: &PreflightedToolCall,
        args: JsonValue,
        result: ExecutableToolResult,
        display: ToolCallDisplayFields,
    ) -> Result<PreparedToolCallTask, anyhow::Error> {
        let coerced = coerce_tool_result(result, call.tool_name());
        dispatch_tool_call_event(step, call, &args, display).await?;
        Ok(PreparedToolCallTask {
            task: make_resolved_task(make_tool_result(call, args, coerced)),
            stop_batch_after_this: Some(tool_result_stops_turn(&coerced)),
        })
    }

    if let PreflightedToolCall::Rejected { output, .. } = &call {
        return settle_error(step, &call, call.args().clone(), output.clone(), (None, None)).await;
    }

    let decision = run_prepare_tool_execution_hook(step, &call).await?;
    if decision.block == Some(true) {
        return settle_error(
            step,
            &call,
            decision.updated_args.clone().unwrap_or_else(|| call.args().clone()),
            decision.reason.unwrap_or_else(|| format!(r#"Tool call "{}" was blocked"#, call.tool_name())),
            (None, None),
        ).await;
    }
    if let Some(synthetic) = decision.synthetic_result {
        return settle_synthetic(
            step,
            &call,
            decision.updated_args.clone().unwrap_or_else(|| call.args().clone()),
            synthetic,
            (None, None),
        ).await;
    }

    let effective_args = decision.updated_args.clone().unwrap_or_else(|| call.args().clone());
    let tool = find_tool(step.tools.as_deref(), call.tool_name()).expect("runnable tool must exist");
    if let Some(err) = validate_executable_tool_args(tool, &effective_args) {
        return settle_error(
            step,
            &call,
            effective_args,
            format!(r#"Invalid args for tool "{}" after prepareToolExecution hook: {}"#, call.tool_name(), err),
            (None, None),
        ).await;
    }

    if step.signal.is_aborted() {
        return settle_error(
            step,
            &call,
            effective_args,
            aborted_tool_output(call.tool_name(), &step.signal),
            (None, None),
        ).await;
    }

    let execution = match tool.resolve_execution(effective_args.clone()).await {
        Ok(ToolExecution::Runnable(exec)) => exec,
        Ok(ToolExecution::Error(result)) => {
            return settle_synthetic(step, &call, effective_args, ExecutableToolResult::Error(result), (None, None)).await;
        }
        Err(error) => {
            let output = if let Some(pse) = error.downcast_ref::<PathSecurityError>() {
                pse.to_string()
            } else {
                format!(r#"Tool "{}" failed to resolve execution: {}"#, call.tool_name(), error_message(&error))
            };
            return settle_error(step, &call, effective_args, output, (None, None)).await;
        }
    };

    let display = tool_call_display_fields_from_execution(&execution);
    if step.signal.is_aborted() {
        return settle_error(step, &call, effective_args, aborted_tool_output(call.tool_name(), &step.signal), display).await;
    }
    if execution.is_error == Some(true) {
        let result = ExecutableToolResult::Error(ExecutableToolErrorResult {
            output: ExecutableToolOutput::Text(format!(r#"Tool "{}" returned an error execution"#, call.tool_name())),
            is_error: true,
            stop_turn: None,
            message: None,
        });
        return settle_synthetic(step, &call, effective_args, result, display).await;
    }

    let authorization = run_authorize_tool_execution_hook(step, &call, &effective_args, &execution).await?;
    if step.signal.is_aborted() {
        return settle_error(step, &call, effective_args, aborted_tool_output(call.tool_name(), &step.signal), display).await;
    }
    if authorization.block == Some(true) {
        return settle_error(
            step,
            &call,
            effective_args,
            authorization.reason.unwrap_or_else(|| format!(r#"Tool call "{}" was blocked"#, call.tool_name())),
            display,
        ).await;
    }
    if let Some(synthetic) = authorization.synthetic_result {
        return settle_synthetic(step, &call, effective_args, synthetic, display).await;
    }

    dispatch_tool_call_event(step, &call, &effective_args, display.clone()).await?;
    let metadata = authorization.execution_metadata.or(decision.execution_metadata);
    let accesses = execution.accesses.clone().unwrap_or_else(ToolAccesses::all);
    let stop_batch = execution.stop_batch_after_this;
    let tool_call_id = call.tool_call().id.clone();
    let execute_fn = execution.execute;
    let dispatch_event = step.dispatch_event.clone();
    Ok(PreparedToolCallTask {
        task: ToolCallTask {
            accesses,
            start: Box::new(move || {
                Box::pin(run_runnable_tool_call(
                    step.turn_id.clone(),
                    step.signal.clone(),
                    dispatch_event.clone(),
                    tool_call_id,
                    call.tool_name().to_string(),
                    effective_args.clone(),
                    metadata.clone(),
                    execute_fn,
                )) as Pin<Box<dyn Future<Output = Result<PendingToolResult, anyhow::Error>> + Send>>
            }),
        },
        stop_batch_after_this: stop_batch,
    })
}

async fn prepare_skipped_tool_call(
    step: &ToolCallStepContext<'_>,
    call: PreflightedToolCall,
) -> Result<ToolCallTask<Result<PendingToolResult, anyhow::Error>>, anyhow::Error> {
    let output = "Tool skipped because a previous tool call stopped the turn.";
    dispatch_tool_call_event(step, &call, &call.args().clone(), (None, None)).await?;
    Ok(make_resolved_task(make_error_tool_result(&call, call.args().clone(), output.into())))
}

fn make_resolved_task(
    result: PendingToolResult,
) -> ToolCallTask<Result<PendingToolResult, anyhow::Error>> {
    ToolCallTask {
        accesses: ToolAccesses::none(),
        start: Box::new(move || Box::pin(async move { Ok(result) })),
    }
}

async fn run_prepare_tool_execution_hook(
    step: &ToolCallStepContext<'_>,
    call: &PreflightedToolCall,
) -> Result<PrepareToolExecutionResult, anyhow::Error> {
    let args = call.args();
    let tool_call = call.tool_call();
    let tool_name = call.tool_name();

    let hook = match step.hooks.and_then(|h| h.prepare_tool_execution.as_ref()) {
        Some(h) => h,
        None => return Ok(PrepareToolExecutionResult::default()),
    };

    let ctx = ToolExecutionHookContext {
        turn_id: step.turn_id.as_str(),
        step_number: step.current_step,
        signal: step.signal.clone(),
        llm: step.llm,
        tool_call,
        tool: find_tool(step.tools.as_deref(), tool_name),
        args: args.clone(),
    };

    match hook.prepare_tool_execution(ctx).await {
        Ok(Some(result)) => Ok(result),
        Ok(None) => Ok(PrepareToolExecutionResult::default()),
        Err(error) => {
            if step.signal.is_aborted() {
                Ok(PrepareToolExecutionResult {
                    block: Some(true),
                    reason: Some(format!(r#"Tool "{}" was aborted during prepareToolExecution hook"#, tool_name)),
                    ..Default::default()
                })
            } else {
                Ok(PrepareToolExecutionResult {
                    block: Some(true),
                    reason: Some(format!(r#"prepareToolExecution hook failed for "{}": {}"#, tool_name, error_message(&error))),
                    ..Default::default()
                })
            }
        }
    }
}

async fn run_authorize_tool_execution_hook(
    step: &ToolCallStepContext<'_>,
    call: &PreflightedToolCall,
    args: &JsonValue,
    execution: &RunnableToolExecution,
) -> Result<AuthorizeToolExecutionResult, anyhow::Error> {
    let hook = match step.hooks.and_then(|h| h.authorize_tool_execution.as_ref()) {
        Some(h) => h,
        None => return Ok(AuthorizeToolExecutionResult::default()),
    };

    let ctx = ResolvedToolExecutionHookContext {
        turn_id: step.turn_id.as_str(),
        step_number: step.current_step,
        signal: step.signal.clone(),
        llm: step.llm,
        tool_call: call.tool_call(),
        tool: find_tool(step.tools.as_deref(), call.tool_name()),
        args: args.clone(),
        execution,
    };

    match hook.authorize_tool_execution(ctx).await {
        Ok(Some(result)) => Ok(result),
        Ok(None) => Ok(AuthorizeToolExecutionResult::default()),
        Err(error) => {
            if step.signal.is_aborted() {
                Ok(AuthorizeToolExecutionResult {
                    block: Some(true),
                    reason: Some(format!(r#"Tool "{}" was aborted during authorizeToolExecution hook"#, call.tool_name())),
                    ..Default::default()
                })
            } else {
                Ok(AuthorizeToolExecutionResult {
                    block: Some(true),
                    reason: Some(format!(r#"authorizeToolExecution hook failed for "{}": {}"#, call.tool_name(), error_message(&error))),
                    ..Default::default()
                })
            }
        }
    }
}

async fn run_finalize_tool_result_hook(
    step: &ToolCallStepContext<'_>,
    pending: &PendingToolResult,
) -> Result<ExecutableToolResult, anyhow::Error> {
    let hook = match step.hooks.and_then(|h| h.finalize_tool_result.as_ref()) {
        Some(h) => h,
        None => return Ok(coerce_tool_result(pending.result.clone(), &pending.tool_name)),
    };

    let ctx = FinalizeToolResultContext {
        turn_id: step.turn_id.as_str(),
        step_number: step.current_step,
        signal: step.signal.clone(),
        llm: step.llm,
        tool_call: &pending.tool_call,
        tool: find_tool(step.tools.as_deref(), &pending.tool_name),
        args: pending.args.clone(),
        result: pending.result.clone(),
    };

    match hook.finalize_tool_result(ctx).await {
        Ok(Some(finalized)) => {
            let coerced = coerce_tool_result(finalized, &pending.tool_name);
            Ok(normalize_tool_result(coerced))
        }
        Ok(None) => Ok(normalize_tool_result(coerce_tool_result(pending.result.clone(), &pending.tool_name))),
        Err(error) => {
            let output = if step.signal.is_aborted() {
                format!(r#"Tool "{}" aborted during finalizeToolResult hook."#, pending.tool_name)
            } else {
                format!(r#"finalizeToolResult hook failed for "{}": {}"#, pending.tool_name, error_message(&error))
            };
            Ok(ExecutableToolResult::Error(ExecutableToolErrorResult {
                output: ExecutableToolOutput::Text(output),
                is_error: true,
                stop_turn: None,
                message: None,
            }))
        }
    }
}

async fn finalize_pending_tool_result(
    step: &ToolCallStepContext<'_>,
    pending: PendingToolResult,
) -> Result<PendingToolResult, anyhow::Error> {
    let finalized = run_finalize_tool_result_hook(step, &pending).await?;
    Ok(PendingToolResult {
        stop_turn: Some(pending.stop_turn == Some(true) || tool_result_stops_turn(&finalized)),
        result: finalized,
        ..pending
    })
}

async fn run_runnable_tool_call(
    turn_id: String,
    signal: AbortSignal,
    dispatch_event: Arc<dyn LoopEventDispatcher>,
    tool_call_id: String,
    tool_name: String,
    args: JsonValue,
    metadata: Option<JsonValue>,
    execute: Box<
        dyn Fn(ExecutableToolContext) -> Pin<Box<dyn Future<Output = Result<ExecutableToolResult, anyhow::Error>> + Send>>
            + Send
            + Sync,
    >,
) -> Result<PendingToolResult, anyhow::Error> {
    if signal.is_aborted() {
        return Ok(make_error_tool_result_from_ids(
            tool_call_id,
            tool_name,
            args,
            aborted_tool_output(&tool_name, &signal),
        ));
    }

    let dispatch_event_update = dispatch_event.clone();
    let tool_call_id_update = tool_call_id.clone();
    let ctx = ExecutableToolContext {
        turn_id,
        tool_call_id: tool_call_id.clone(),
        metadata,
        signal: signal.clone(),
        on_update: Some(Box::new(move |update| {
            dispatch_event_update.dispatch_live(LoopLiveOnlyEvent::ToolProgress {
                tool_call_id: tool_call_id_update.clone(),
                update,
            });
        })),
    };

    let result = match race_execute_with_grace_timeout(execute(ctx), signal.clone(), &tool_name).await {
        Ok(raw) => coerce_tool_result(raw, &tool_name),
        Err(error) => {
            let aborted = error.downcast_ref::<crate::agent_loop::errors::LoopError>().map(|e| e.is_abort()).unwrap_or(false)
                || signal.is_aborted();
            let output = if aborted {
                aborted_tool_output(&tool_name, &signal)
            } else {
                format!(r#"Tool "{}" failed: {}"#, tool_name, error_message(&error))
            };
            ExecutableToolResult::Error(ExecutableToolErrorResult {
                output: ExecutableToolOutput::Text(output),
                is_error: true,
                stop_turn: None,
                message: None,
            })
        }
    };

    Ok(make_tool_result_from_ids(tool_call_id, tool_name, args, result))
}

async fn race_execute_with_grace_timeout(
    execute: Pin<Box<dyn Future<Output = Result<ExecutableToolResult, anyhow::Error>> + Send>>,
    signal: AbortSignal,
    tool_name: &str,
) -> Result<ExecutableToolResult, anyhow::Error> {
    if signal.is_aborted() {
        return Ok(error_result(aborted_tool_output(tool_name, &signal)));
    }

    let mut execute = execute;
    let abort_fut = async {
        loop {
            if signal.is_aborted() {
                tokio::time::sleep(Duration::from_millis(GRACE_TIMEOUT_MS)).await;
                return error_result(aborted_tool_output(tool_name, &signal));
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    tokio::pin!(abort_fut);

    tokio::select! {
        res = &mut execute => res,
        res = &mut abort_fut => Ok(res),
    }
}

fn error_result(output: String) -> ExecutableToolResult {
    ExecutableToolResult::Error(ExecutableToolErrorResult {
        output: ExecutableToolOutput::Text(output),
        is_error: true,
        stop_turn: None,
        message: None,
    })
}

fn coerce_tool_result(value: ExecutableToolResult, _tool_name: &str) -> ExecutableToolResult {
    value
}

fn normalize_tool_result(r: ExecutableToolResult) -> ExecutableToolResult {
    let output = match &r {
        ExecutableToolResult::Success(s) => normalize_output(&s.output),
        ExecutableToolResult::Error(e) => normalize_output(&e.output),
    };
    match r {
        ExecutableToolResult::Success(_) => ExecutableToolResult::Success(ExecutableToolSuccessResult {
            output,
            is_error: None,
            stop_turn: None,
            message: None,
        }),
        ExecutableToolResult::Error(e) => ExecutableToolResult::Error(ExecutableToolErrorResult {
            output,
            is_error: true,
            stop_turn: e.stop_turn,
            message: e.message,
        }),
    }
}

fn normalize_output(output: &ExecutableToolOutput) -> ExecutableToolOutput {
    match output {
        ExecutableToolOutput::Text(s) => {
            if s.is_empty() {
                ExecutableToolOutput::Text(TOOL_OUTPUT_EMPTY.into())
            } else {
                ExecutableToolOutput::Text(s.clone())
            }
        }
        ExecutableToolOutput::Parts(parts) => {
            if parts.is_empty() {
                return ExecutableToolOutput::Text(TOOL_OUTPUT_EMPTY.into());
            }
            if parts.iter().all(|p| is_media_content_part(p)) {
                if parts.iter().any(|p| matches!(p, ContentPart::Text { text } if !text.is_empty())) {
                    ExecutableToolOutput::Parts(parts.clone())
                } else {
                    let mut out = vec![ContentPart::Text { text: TOOL_OUTPUT_NON_TEXT.into() }];
                    out.extend(parts.iter().cloned());
                    ExecutableToolOutput::Parts(out)
                }
            } else {
                let joined: String = parts
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect();
                if joined.is_empty() {
                    ExecutableToolOutput::Text(TOOL_OUTPUT_EMPTY.into())
                } else {
                    ExecutableToolOutput::Text(joined)
                }
            }
        }
    }
}

fn is_media_content_part(part: &ContentPart) -> bool {
    matches!(
        part,
        ContentPart::ImageUrl { .. } | ContentPart::AudioUrl { .. } | ContentPart::VideoUrl { .. }
    )
}

fn tool_result_stops_turn(result: &ExecutableToolResult) -> bool {
    match result {
        ExecutableToolResult::Success(s) => s.stop_turn == Some(true),
        ExecutableToolResult::Error(e) => e.stop_turn == Some(true),
    }
}

fn make_tool_result(call: &PreflightedToolCall, args: JsonValue, result: ExecutableToolResult) -> PendingToolResult {
    PendingToolResult {
        tool_call: call.tool_call().clone(),
        tool_name: call.tool_name().into(),
        args,
        stop_turn: Some(tool_result_stops_turn(&result)),
        result,
    }
}

fn make_tool_result_from_ids(tool_call_id: String, tool_name: String, args: JsonValue, result: ExecutableToolResult) -> PendingToolResult {
    PendingToolResult {
        tool_call: ToolCall {
            call_type: "function".into(),
            id: tool_call_id,
            name: tool_name.clone(),
            arguments: None,
            extras: None,
            stream_index: None,
        },
        tool_name,
        args,
        stop_turn: Some(tool_result_stops_turn(&result)),
        result,
    }
}

fn make_error_tool_result(call: &PreflightedToolCall, args: JsonValue, output: String) -> PendingToolResult {
    make_tool_result(
        call,
        args,
        ExecutableToolResult::Error(ExecutableToolErrorResult {
            output: ExecutableToolOutput::Text(output),
            is_error: true,
            stop_turn: None,
            message: None,
        }),
    )
}

fn make_error_tool_result_from_ids(tool_call_id: String, tool_name: String, args: JsonValue, output: String) -> PendingToolResult {
    make_tool_result_from_ids(
        tool_call_id,
        tool_name,
        args,
        ExecutableToolResult::Error(ExecutableToolErrorResult {
            output: ExecutableToolOutput::Text(output),
            is_error: true,
            stop_turn: None,
            message: None,
        }),
    )
}

fn aborted_tool_output(tool_name: &str, _signal: &AbortSignal) -> String {
    format!(r#"Tool "{}" was aborted"#, tool_name)
}

fn tool_call_display_fields_from_execution(execution: &RunnableToolExecution) -> ToolCallDisplayFields {
    let description = execution.description.clone().filter(|d| !d.is_empty());
    (description, execution.display.clone())
}

async fn dispatch_tool_call_event(
    step: &ToolCallStepContext<'_>,
    call: &PreflightedToolCall,
    args: &JsonValue,
    display: ToolCallDisplayFields,
) -> Result<(), anyhow::Error> {
    step.dispatch_event.dispatch_recorded(LoopRecordedEvent::ToolCallEvent {
        uuid: call.tool_call().id.clone(),
        turn_id: step.turn_id.clone(),
        step: step.current_step as i64,
        step_uuid: step.step_uuid.clone(),
        tool_call_id: call.tool_call().id.clone(),
        name: call.tool_name().into(),
        args: args.clone(),
        description: display.0,
        display: display.1,
    }).await
}

async fn dispatch_tool_result_event(
    step: &ToolCallStepContext<'_>,
    pending: &PendingToolResult,
) -> Result<(), anyhow::Error> {
    step.dispatch_event.dispatch_recorded(LoopRecordedEvent::ToolResultEvent {
        parent_uuid: pending.tool_call.id.clone(),
        tool_call_id: pending.tool_call.id.clone(),
        result: pending.result.clone(),
    }).await
}

fn find_tool<'a>(tools: Option<&'a [Box<dyn ExecutableTool>]>, name: &str) -> Option<&'a dyn ExecutableTool> {
    tools.and_then(|list| list.iter().find(|t| t.name() == name).map(|t| t.as_ref()))
}

impl PreflightedToolCall {
    fn tool_call(&self) -> &ToolCall {
        match self {
            PreflightedToolCall::Runnable { tool_call, .. } | PreflightedToolCall::Rejected { tool_call, .. } => tool_call,
        }
    }

    fn tool_name(&self) -> &str {
        match self {
            PreflightedToolCall::Runnable { tool_name, .. } | PreflightedToolCall::Rejected { tool_name, .. } => tool_name,
        }
    }

    fn args(&self) -> &JsonValue {
        match self {
            PreflightedToolCall::Runnable { args, .. } | PreflightedToolCall::Rejected { args, .. } => args,
        }
    }

}
```

- [ ] 运行测试：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo test -p agent-rs --test tool_call_batch
cargo check --workspace --tests
```

- [ ] 提交：`feat(agent-rs): tool-call batch lifecycle`

---

## Phase B 本地自审

- [ ] 1. Spec-coverage：4.3.4.2/4.3.4.3/4.3.4.4/4.3.4.5 已映射到 Task 6/7/8/5。
- [ ] 2. Placeholder scan：Task 8 已给出完整 `tool_call.rs` 代码（含 preflight/validate/prepare/hooks/execute/dispatch/normalize），无 `unimplemented!`、TODO 或“PR 中补全”等占位。
- [ ] 3. No phantom tasks：每个 task 都产生文件/测试变更。
- [ ] 4. Dependency soundness：Task 6 依赖 Task 5；Task 7 依赖 Task 5/6/8；Task 8 依赖 Task 4。
- [ ] 5. Caller & build soundness：Task 5 修改了共享 `AbortSignal`（新增 `throw_if_aborted`/`Clone`），已全 workspace typecheck；Task 6 修改 `RunTurnInput`/`ExecuteLoopStepDeps` dispatch_event 为 `Arc`，Task 7 是唯一调用方并同步更新。
- [ ] 6. Test-the-risk：retry 覆盖失败恢复与 abort 短路；run_turn 覆盖 max steps 与 abort；tool_call 覆盖成功/失败/缺失工具/并行/abort。
- [ ] 7. Type一致性：`LoopStepStopReason`/`LoopTurnStopReason` 与 TS 字符串值一致；`StepResult` 字段与 `executeLoopStep` 返回值一致。
