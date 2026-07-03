# Phase C: L3 fixtures、golden binary、TS driver、对照测试

本部分在 Phase A/B 已落地的无状态引擎上建立 L3 对照门。Task 9/10 分别产出 Rust golden binary 与 TS golden driver，Task 11 注册对照测试并驱动偏差归零。

---

## Phase C 任务依赖图

```text
Phase A/B (agent_loop runtime ready)
        │
        ├──▶ Task 9 (fixtures + Rust loop-l3 binary)
        │
        └──▶ Task 10 (TS loop-l3 driver)
                 │
                 ▼
            Task 11 (L3 parity test + package script)
```

---

### Task 9: 创建 L3 fixtures 与 Rust golden binary

**Depends on:** Phase B Task 7（`run_turn` 与 `LoopEventDispatcher` 已实现且可运行）

**Files:**
- Create: `rust-ody/crates/agent-rs/src/bin/loop_l3.rs`
- Modify: `rust-ody/crates/agent-rs/Cargo.toml`（追加 `[[bin]] loop-l3`）
- Create: `packages/integration-tests/src/parity/fixtures/loop/single-text.json`
- Create: `packages/integration-tests/src/parity/fixtures/loop/single-tool-call.json`
- Create: `packages/integration-tests/src/parity/fixtures/loop/parallel-tool-calls.json`
- Create: `packages/integration-tests/src/parity/fixtures/loop/tool-failure.json`
- Create: `packages/integration-tests/src/parity/fixtures/loop/max-steps.json`
- Create: `packages/integration-tests/src/parity/fixtures/loop/abort-mid-step.json`
- Create: `packages/integration-tests/src/parity/fixtures/loop/retry-recover.json`
- Test: `rust-ody/crates/agent-rs/tests/loop_l3_fixture.rs`

#### Fixture schema

每个 fixture 是自包含的 JSON：

```json
{
  "name": "scenario-name",
  "turnId": "turn-1",
  "maxSteps": 5,
  "maxRetryAttempts": 1,
  "tools": [
    { "name": "add", "description": "...", "parameters": {...}, "behavior": "add" }
  ],
  "responses": [
    {
      "content": [{"type":"text","text":"Hello"}],
      "toolCalls": [{"id":"tc1","name":"add","arguments":"{\"a\":1,\"b\":2}"}],
      "providerFinishReason": "tool_calls",
      "rawFinishReason": "tool_calls",
      "usage": {"inputOther":10,"output":5,"inputCacheRead":0,"inputCacheCreation":0},
      "abortSignal": false,
      "error": null
    }
  ]
}
```

`behavior` 取值：
- `add`：参数 `a`、`b` 为 number，返回 `a + b` 的文本。
- `concat`：参数 `a`、`b` 为 string，返回拼接文本。
- `echo`：返回参数的 JSON 文本。
- `fail`：返回 error result。
- `stopTurn`：返回 success result 且 `stopTurn=true`。

#### 步骤

- [ ] 创建 fixture 文件：

`packages/integration-tests/src/parity/fixtures/loop/single-text.json`：

```json
{
  "name": "single-text",
  "turnId": "turn-single-text",
  "maxSteps": 5,
  "maxRetryAttempts": 1,
  "tools": [],
  "responses": [
    {
      "content": [{"type":"text","text":"Hello from loop"}],
      "toolCalls": [],
      "providerFinishReason": "completed",
      "rawFinishReason": "stop",
      "usage": {"inputOther":10,"output":5,"inputCacheRead":0,"inputCacheCreation":0}
    }
  ]
}
```

`packages/integration-tests/src/parity/fixtures/loop/single-tool-call.json`：

```json
{
  "name": "single-tool-call",
  "turnId": "turn-single-tool-call",
  "maxSteps": 5,
  "maxRetryAttempts": 1,
  "tools": [
    {
      "name": "add",
      "description": "Adds two numbers.",
      "parameters": {"type":"object","properties":{"a":{"type":"number"},"b":{"type":"number"}},"required":["a","b"]},
      "behavior": "add"
    }
  ],
  "responses": [
    {
      "content": [],
      "toolCalls": [{"id":"tc1","name":"add","arguments":"{\"a\":1,\"b\":2}"}],
      "providerFinishReason": "tool_calls",
      "rawFinishReason": "tool_calls",
      "usage": {"inputOther":10,"output":5,"inputCacheRead":0,"inputCacheCreation":0}
    },
    {
      "content": [{"type":"text","text":"done"}],
      "toolCalls": [],
      "providerFinishReason": "completed",
      "rawFinishReason": "stop",
      "usage": {"inputOther":12,"output":3,"inputCacheRead":0,"inputCacheCreation":0}
    }
  ]
}
```

`packages/integration-tests/src/parity/fixtures/loop/parallel-tool-calls.json`：

```json
{
  "name": "parallel-tool-calls",
  "turnId": "turn-parallel-tool-calls",
  "maxSteps": 5,
  "maxRetryAttempts": 1,
  "tools": [
    {
      "name": "add",
      "description": "Adds two numbers.",
      "parameters": {"type":"object","properties":{"a":{"type":"number"},"b":{"type":"number"}},"required":["a","b"]},
      "behavior": "add"
    }
  ],
  "responses": [
    {
      "content": [],
      "toolCalls": [
        {"id":"tc1","name":"add","arguments":"{\"a\":1,\"b\":2}"},
        {"id":"tc2","name":"add","arguments":"{\"a\":3,\"b\":4}"}
      ],
      "providerFinishReason": "tool_calls",
      "rawFinishReason": "tool_calls",
      "usage": {"inputOther":10,"output":5,"inputCacheRead":0,"inputCacheCreation":0}
    },
    {
      "content": [{"type":"text","text":"done"}],
      "toolCalls": [],
      "providerFinishReason": "completed",
      "rawFinishReason": "stop",
      "usage": {"inputOther":12,"output":3,"inputCacheRead":0,"inputCacheCreation":0}
    }
  ]
}
```

`packages/integration-tests/src/parity/fixtures/loop/tool-failure.json`：

```json
{
  "name": "tool-failure",
  "turnId": "turn-tool-failure",
  "maxSteps": 5,
  "maxRetryAttempts": 1,
  "tools": [
    {
      "name": "fail",
      "description": "Always fails.",
      "parameters": {"type":"object","properties":{"message":{"type":"string"}},"required":["message"]},
      "behavior": "fail"
    }
  ],
  "responses": [
    {
      "content": [],
      "toolCalls": [{"id":"tc1","name":"fail","arguments":"{\"message\":\"intentional failure\"}"}],
      "providerFinishReason": "tool_calls",
      "rawFinishReason": "tool_calls",
      "usage": {"inputOther":10,"output":5,"inputCacheRead":0,"inputCacheCreation":0}
    },
    {
      "content": [{"type":"text","text":"done"}],
      "toolCalls": [],
      "providerFinishReason": "completed",
      "rawFinishReason": "stop",
      "usage": {"inputOther":12,"output":3,"inputCacheRead":0,"inputCacheCreation":0}
    }
  ]
}
```

`packages/integration-tests/src/parity/fixtures/loop/max-steps.json`：

```json
{
  "name": "max-steps",
  "turnId": "turn-max-steps",
  "maxSteps": 2,
  "maxRetryAttempts": 1,
  "tools": [
    {
      "name": "echo",
      "description": "Echoes input.",
      "parameters": {"type":"object","properties":{"x":{"type":"string"}},"required":["x"]},
      "behavior": "echo"
    }
  ],
  "responses": [
    {
      "content": [],
      "toolCalls": [{"id":"tc1","name":"echo","arguments":"{\"x\":\"step1\"}"}],
      "providerFinishReason": "tool_calls",
      "rawFinishReason": "tool_calls",
      "usage": {"inputOther":10,"output":5,"inputCacheRead":0,"inputCacheCreation":0}
    },
    {
      "content": [],
      "toolCalls": [{"id":"tc2","name":"echo","arguments":"{\"x\":\"step2\"}"}],
      "providerFinishReason": "tool_calls",
      "rawFinishReason": "tool_calls",
      "usage": {"inputOther":10,"output":5,"inputCacheRead":0,"inputCacheCreation":0}
    }
  ]
}
```

`packages/integration-tests/src/parity/fixtures/loop/abort-mid-step.json`：

```json
{
  "name": "abort-mid-step",
  "turnId": "turn-abort-mid-step",
  "maxSteps": 5,
  "maxRetryAttempts": 1,
  "tools": [
    {
      "name": "add",
      "description": "Adds two numbers.",
      "parameters": {"type":"object","properties":{"a":{"type":"number"},"b":{"type":"number"}},"required":["a","b"]},
      "behavior": "add"
    }
  ],
  "responses": [
    {
      "content": [],
      "toolCalls": [{"id":"tc1","name":"add","arguments":"{\"a\":1,\"b\":2}"}],
      "providerFinishReason": "tool_calls",
      "rawFinishReason": "tool_calls",
      "usage": {"inputOther":10,"output":5,"inputCacheRead":0,"inputCacheCreation":0},
      "abortSignal": true
    }
  ]
}
```

`packages/integration-tests/src/parity/fixtures/loop/retry-recover.json`：

```json
{
  "name": "retry-recover",
  "turnId": "turn-retry-recover",
  "maxSteps": 5,
  "maxRetryAttempts": 3,
  "tools": [],
  "responses": [
    {
      "error": {"statusCode":429,"message":"rate limit"}
    },
    {
      "content": [{"type":"text","text":"recovered"}],
      "toolCalls": [],
      "providerFinishReason": "completed",
      "rawFinishReason": "stop",
      "usage": {"inputOther":10,"output":5,"inputCacheRead":0,"inputCacheCreation":0}
    }
  ]
}
```

- [ ] 写失败测试 `rust-ody/crates/agent-rs/tests/loop_l3_fixture.rs`：

```rust
use std::path::PathBuf;

#[derive(serde::Deserialize)]
struct Fixture {
    name: String,
    turn_id: String,
    #[serde(default)]
    max_steps: Option<u32>,
    #[serde(default)]
    max_retry_attempts: Option<u32>,
    #[serde(default)]
    tools: Vec<serde_json::Value>,
    responses: Vec<serde_json::Value>,
}

fn fixtures_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // agent-rs crate -> rust-ody/crates/agent-rs -> workspace root
    manifest.parent().unwrap().parent().unwrap().parent().unwrap()
        .join("packages/integration-tests/src/parity/fixtures/loop")
}

#[test]
fn all_l3_fixtures_parse() {
    let dir = fixtures_dir();
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let raw = std::fs::read_to_string(&path).unwrap();
        let fixture: Fixture = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("failed to parse {}: {}", path.display(), e));
        assert!(!fixture.name.is_empty(), "{}", path.display());
        assert!(!fixture.turn_id.is_empty(), "{}", path.display());
        count += 1;
    }
    assert!(count >= 7, "expected at least 7 fixtures, got {}", count);
}
```

运行并确认失败：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo test -p agent-rs --test loop_l3_fixture
```

预期失败：`No such file or directory`（fixtures 目录或文件尚未创建）或 `Fixture` 字段不匹配。

- [ ] 实现 Rust golden binary：`rust-ody/crates/agent-rs/src/bin/loop_l3.rs`

```rust
use std::env;
use std::fs;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use agent_rs::agent_loop::events::{
    DefaultLoopEventDispatcher, LoopEvent, LoopEventDispatcher, LoopLiveOnlyEvent,
    LoopRecordedEvent,
};
use agent_rs::agent_loop::llm::{Llm, LlmChatParams, LlmChatResponse, LlmStreamTiming, ToolCallDelta};
use agent_rs::agent_loop::run_turn::{run_turn, RunTurnInput};
use agent_rs::agent_loop::tool_access::ToolAccesses;
use agent_rs::agent_loop::types::{
    ExecutableTool, ExecutableToolContext, ExecutableToolErrorResult, ExecutableToolOutput,
    ExecutableToolResult, ExecutableToolSuccessResult, RunnableToolExecution, ToolExecution,
};
use kosong_rs::message::{ContentPart, ToolCall};
use kosong_rs::provider::{AbortSignal, FinishReason};
use kosong_rs::usage::TokenUsage;
use serde::Deserialize;
use serde_json::Value as JsonValue;

#[derive(Deserialize)]
struct Fixture {
    name: String,
    turn_id: String,
    #[serde(default)]
    max_steps: Option<u32>,
    #[serde(default)]
    max_retry_attempts: Option<u32>,
    #[serde(default)]
    tools: Vec<FixtureTool>,
    responses: Vec<FixtureResponse>,
}

#[derive(Deserialize)]
struct FixtureTool {
    name: String,
    description: String,
    parameters: JsonValue,
    behavior: String,
}

#[derive(Deserialize)]
struct FixtureResponse {
    #[serde(default)]
    content: Vec<FixtureContentPart>,
    #[serde(default, rename = "toolCalls")]
    tool_calls: Vec<FixtureToolCall>,
    #[serde(rename = "providerFinishReason", default)]
    provider_finish_reason: Option<String>,
    #[serde(rename = "rawFinishReason", default)]
    raw_finish_reason: Option<String>,
    #[serde(default)]
    usage: TokenUsage,
    #[serde(default)]
    abort_signal: bool,
    #[serde(default)]
    error: Option<FixtureError>,
}

#[derive(Deserialize)]
struct FixtureError {
    #[serde(rename = "statusCode")]
    status_code: i32,
    message: String,
}

#[derive(Deserialize)]
struct FixtureContentPart {
    #[serde(rename = "type")]
    part_type: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    think: Option<String>,
}

#[derive(Deserialize)]
struct FixtureToolCall {
    id: String,
    name: String,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug)]
struct RetryableError;

impl std::fmt::Display for RetryableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "retryable error")
    }
}

impl std::error::Error for RetryableError {}

struct MockLlm {
    responses: Mutex<Vec<FixtureResponse>>,
}

#[async_trait::async_trait]
impl Llm for MockLlm {
    fn system_prompt(&self) -> &str { "" }
    fn model_name(&self) -> &str { "mock" }

    fn is_retryable_error(&self, err: &dyn std::error::Error) -> bool {
        err.downcast_ref::<RetryableError>().is_some()
    }

    async fn chat(&self, params: LlmChatParams) -> Result<LlmChatResponse, anyhow::Error> {
        let response = {
            let mut guard = self.responses.lock().unwrap();
            if guard.is_empty() {
                return Err(anyhow::anyhow!("MockLlm exhausted"));
            }
            guard.remove(0)
        };

        if response.abort_signal {
            params.signal.abort();
        }

        if response.error.is_some() {
            return Err(anyhow::anyhow!(RetryableError));
        }

        let content: Vec<ContentPart> = response
            .content
            .into_iter()
            .filter_map(|p| match p.part_type.as_str() {
                "text" => Some(ContentPart::Text { text: p.text.unwrap_or_default() }),
                "think" => Some(ContentPart::Think {
                    think: p.think.unwrap_or_default(),
                    encrypted: None,
                }),
                _ => None,
            })
            .collect();

        for part in &content {
            if let ContentPart::Text { text } = part {
                if let Some(cb) = &params.on_text_delta {
                    cb(text.clone());
                }
            }
        }
        if let Some(cb) = &params.on_text_part {
            for part in content.clone() {
                if let ContentPart::Text { text } = part {
                    cb(kosong_rs::message::TextPart { text }).await;
                }
            }
        }

        let tool_calls: Vec<ToolCall> = response
            .tool_calls
            .into_iter()
            .map(|tc| {
                if let Some(cb) = &params.on_tool_call_delta {
                    cb(ToolCallDelta {
                        tool_call_id: tc.id.clone(),
                        name: Some(tc.name.clone()),
                        arguments_part: tc.arguments.clone(),
                    });
                }
                ToolCall {
                    call_type: "function".into(),
                    id: tc.id,
                    name: tc.name,
                    arguments: tc.arguments,
                    extras: None,
                    stream_index: None,
                }
            })
            .collect();

        let provider_finish_reason = response
            .provider_finish_reason
            .and_then(|s| serde_json::from_str(&format!("\"{}\"", s)).ok());

        Ok(LlmChatResponse {
            tool_calls,
            provider_finish_reason,
            raw_finish_reason: response.raw_finish_reason,
            usage: response.usage,
            stream_timing: Some(LlmStreamTiming {
                first_token_latency_ms: 1,
                stream_duration_ms: 1,
            }),
        })
    }
}

struct BuiltinTool {
    name: String,
    description: String,
    parameters: JsonValue,
    behavior: String,
}

impl kosong_rs::provider::Tool for BuiltinTool {
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> &str { &self.description }
    fn parameters(&self) -> &JsonValue { &self.parameters }
}

#[async_trait::async_trait]
impl ExecutableTool for BuiltinTool {
    async fn resolve_execution(&self, input: JsonValue) -> Result<ToolExecution, anyhow::Error> {
        match self.behavior.as_str() {
            "fail" => {
                let msg = input.get("message").and_then(|v| v.as_str()).unwrap_or("failed");
                Ok(ToolExecution::Error(ExecutableToolErrorResult {
                    output: ExecutableToolOutput::Text(msg.into()),
                    is_error: true,
                    stop_turn: None,
                    message: None,
                }))
            }
            _ => {
                let output = match self.behavior.as_str() {
                    "add" => {
                        let a = input.get("a").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let b = input.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        ExecutableToolOutput::Text(format!("{}", a + b))
                    }
                    "concat" => {
                        let a = input.get("a").and_then(|v| v.as_str()).unwrap_or("");
                        let b = input.get("b").and_then(|v| v.as_str()).unwrap_or("");
                        ExecutableToolOutput::Text(format!("{}{}", a, b))
                    }
                    "echo" => ExecutableToolOutput::Text(serde_json::to_string(&input).unwrap_or_default()),
                    "stopTurn" => ExecutableToolOutput::Text("stop".into()),
                    other => ExecutableToolOutput::Text(format!("unknown behavior: {}", other)),
                };
                let stop_turn = self.behavior == "stopTurn";
                Ok(ToolExecution::Runnable(RunnableToolExecution {
                    is_error: None,
                    accesses: Some(ToolAccesses::none()),
                    display: None,
                    description: None,
                    stop_batch_after_this: if stop_turn { Some(true) } else { None },
                    approval_rule: "auto".into(),
                    matches_rule: None,
                    execute: Box::new(move |_ctx| Box::pin(async move {
                        Ok(ExecutableToolResult::Success(ExecutableToolSuccessResult {
                            output,
                            is_error: None,
                            stop_turn: if stop_turn { Some(true) } else { None },
                            message: None,
                        }))
                    })),
                }))
            }
        }
    }
}

fn build_tools(fixture_tools: Vec<FixtureTool>) -> Vec<Box<dyn ExecutableTool>> {
    fixture_tools
        .into_iter()
        .map(|t| {
            Box::new(BuiltinTool {
                name: t.name,
                description: t.description,
                parameters: t.parameters,
                behavior: t.behavior,
            }) as Box<dyn ExecutableTool>
        })
        .collect()
}

fn print_jsonl(value: &serde_json::Value) {
    println!("{}", serde_json::to_string(value).unwrap());
}

fn event_to_value(event: &LoopEvent) -> serde_json::Value {
    serde_json::to_value(event).unwrap()
}

#[tokio::main]
async fn main() {
    let path = env::args().nth(1).expect("fixture path argument required");
    let raw = fs::read_to_string(&path).expect("read fixture");
    let fixture: Fixture = serde_json::from_str(&raw).expect("parse fixture");

    let signal = AbortSignal::new();
    let llm = Box::new(MockLlm {
        responses: Mutex::new(fixture.responses),
    });
    let tools = if fixture.tools.is_empty() {
        None
    } else {
        Some(build_tools(fixture.tools))
    };

    let events: Arc<Mutex<Vec<LoopEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let e = events.clone();
    let dispatcher = DefaultLoopEventDispatcher::new(
        move |event: LoopRecordedEvent| {
            let e = e.clone();
            async move {
                e.lock().unwrap().push(LoopEvent::Recorded(event));
                Ok::<_, anyhow::Error>(())
            }
        },
        Some(Box::new(move |event| {
            events.lock().unwrap().push(event);
        })),
    );

    let result = run_turn(RunTurnInput {
        turn_id: fixture.turn_id,
        signal: signal.clone(),
        llm,
        build_messages: Box::new(|| Box::pin(async { Ok::<_, anyhow::Error>(Vec::new()) })),
        dispatch_event: Arc::new(dispatcher),
        tools,
        hooks: None,
        max_steps: fixture.max_steps,
        max_retry_attempts: fixture.max_retry_attempts,
        record_step_usage: None,
    }).await;

    for event in events.lock().unwrap().iter() {
        print_jsonl(&event_to_value(event));
    }

    match result {
        Ok(turn_result) => {
            print_jsonl(&serde_json::json!({
                "type": "turn.result",
                "stopReason": serde_json::to_value(&turn_result.stop_reason).unwrap(),
                "steps": turn_result.steps,
                "usage": turn_result.usage,
            }));
        }
        Err(err) => {
            print_jsonl(&serde_json::json!({
                "type": "turn.error",
                "message": err.to_string(),
            }));
        }
    }
}
```

注意：`RunTurnInput.dispatch_event` 在 Phase B Task 6 已改为 `Arc<dyn LoopEventDispatcher>`；本 binary 直接使用该签名。

- [ ] 在 `rust-ody/crates/agent-rs/Cargo.toml` 末尾追加 bin 注册：

```toml
[[bin]]
name = "loop-l3"
path = "src/bin/loop_l3.rs"
```

- [ ] 编译与 fixture 解析测试：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo test -p agent-rs --test loop_l3_fixture
cargo check -p agent-rs --bin loop-l3
```

预期 `loop_l3_fixture` 通过，`cargo check --bin loop-l3` 无类型错误。

- [ ] 手动验证 golden binary：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo run -p agent-rs --bin loop-l3 -- \
  ../../packages/integration-tests/src/parity/fixtures/loop/single-text.json \
  | jq -c '.type'
```

预期输出（顺序必须一致）：

```
"step.begin"
"text.delta"
"content.part"
"step.end"
"turn.result"
```

- [ ] 提交：`feat(agent-rs): L3 loop golden binary and fixtures`

---

### Task 10: 创建 TS `loop-l3.ts` golden driver

**Depends on:** Task 9（fixtures schema 已固定）

**Files:**
- Create: `packages/integration-tests/src/parity/loop-l3.ts`
- Modify: `packages/agent-core/src/index.ts`（导出 `ToolAccesses`，使 driver 不必绕过 barrel）

#### 步骤

- [ ] 在 `packages/agent-core/src/index.ts` 中追加 re-export：

```ts
export { ToolAccesses } from './loop';
```

（位置：放在 `LoopRecordedEvent` 等类型导出附近即可；本变更仅新增导出，不修改已有签名。）

- [ ] 实现 TS driver：`packages/integration-tests/src/parity/loop-l3.ts`

```ts
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

import {
  runTurn,
  createLoopEventDispatcher,
  ToolAccesses,
  KosongLLM,
  type ExecutableTool,
  type ExecutableToolContext,
  type ExecutableToolResult,
  type ExecutableToolSuccessResult,
  type ExecutableToolErrorResult,
  type RunnableToolExecution,
  type ToolExecution,
  type LoopEvent,
  type LoopRecordedEvent,
  type LoopLiveOnlyEvent,
  type TurnResult,
  type RunTurnInput,
} from '@odysseythink/agent-core';
import {
  APIStatusError,
  type ChatProvider,
  type GenerateOptions,
  type Message,
  type ModelCapability,
  type StreamedMessage,
  type StreamedMessagePart,
  type TokenUsage,
  type Tool,
} from '@odysseythink/kosong';

interface Fixture {
  readonly name: string;
  readonly turnId: string;
  readonly maxSteps?: number | undefined;
  readonly maxRetryAttempts?: number | undefined;
  readonly tools: readonly FixtureTool[];
  readonly responses: readonly FixtureResponse[];
}

interface FixtureTool {
  readonly name: string;
  readonly description: string;
  readonly parameters: unknown;
  readonly behavior: string;
}

interface FixtureResponse {
  readonly content?: readonly FixtureContentPart[] | undefined;
  readonly toolCalls?: readonly FixtureToolCall[] | undefined;
  readonly providerFinishReason?: string | undefined;
  readonly rawFinishReason?: string | undefined;
  readonly usage: TokenUsage;
  readonly abortSignal?: boolean | undefined;
  readonly error?: { readonly statusCode: number; readonly message: string } | undefined;
}

interface FixtureContentPart {
  readonly type: string;
  readonly text?: string | undefined;
  readonly think?: string | undefined;
}

interface FixtureToolCall {
  readonly id: string;
  readonly name: string;
  readonly arguments?: string | undefined;
}

class BuiltinTool implements ExecutableTool {
  readonly name: string;
  readonly description: string;
  readonly parameters: unknown;
  private readonly behavior: string;

  constructor(tool: FixtureTool) {
    this.name = tool.name;
    this.description = tool.description;
    this.parameters = tool.parameters;
    this.behavior = tool.behavior;
  }

  resolveExecution(input: unknown): ToolExecution {
    if (this.behavior === 'fail') {
      const msg =
        typeof input === 'object' &&
        input !== null &&
        'message' in input &&
        typeof (input as Record<string, unknown>).message === 'string'
          ? String((input as Record<string, unknown>).message)
          : 'failed';
      return {
        output: msg,
        isError: true,
        stopTurn: undefined,
        message: undefined,
      } as ExecutableToolErrorResult;
    }

    let output = '';
    if (this.behavior === 'add') {
      const a = Number((input as Record<string, unknown>).a ?? 0);
      const b = Number((input as Record<string, unknown>).b ?? 0);
      output = String(a + b);
    } else if (this.behavior === 'concat') {
      const a = String((input as Record<string, unknown>).a ?? '');
      const b = String((input as Record<string, unknown>).b ?? '');
      output = `${a}${b}`;
    } else if (this.behavior === 'echo') {
      output = JSON.stringify(input);
    } else if (this.behavior === 'stopTurn') {
      output = 'stop';
    } else {
      output = `unknown behavior: ${this.behavior}`;
    }

    const runnable: RunnableToolExecution = {
      approvalRule: 'auto',
      accesses: ToolAccesses.none(),
      display: undefined,
      description: undefined,
      stopBatchAfterThis: this.behavior === 'stopTurn' ? true : undefined,
      execute: async (_ctx: ExecutableToolContext): Promise<ExecutableToolResult> => {
        return {
          output,
          isError: false,
          stopTurn: this.behavior === 'stopTurn' ? true : undefined,
          message: undefined,
        } as ExecutableToolSuccessResult;
      },
    };
    return runnable;
  }
}

class MockChatProvider implements ChatProvider {
  private responses: FixtureResponse[];

  constructor(responses: readonly FixtureResponse[]) {
    this.responses = [...responses];
  }

  name(): string { return 'mock'; }
  modelName(): string { return 'mock'; }
  thinkingEffort() { return undefined; }
  withThinking(): ChatProvider { return this; }
  getCapability(): ModelCapability {
    return {
      imageIn: false,
      videoIn: false,
      audioIn: false,
      thinking: false,
      toolUse: true,
      maxContextTokens: 4096,
      maxOutputTokens: 4096,
    };
  }

  async generate(
    _systemPrompt: string,
    _tools: Tool[],
    _history: Message[],
    options?: GenerateOptions,
  ): Promise<StreamedMessage> {
    if (this.responses.length === 0) {
      throw new Error('MockChatProvider exhausted');
    }
    const response = this.responses.shift()!;

    if (response.abortSignal) {
      options?.signal?.abort();
    }

    if (response.error) {
      throw new APIStatusError(response.error.statusCode, response.error.message);
    }

    const parts: StreamedMessagePart[] = [];
    for (const c of response.content ?? []) {
      if (c.type === 'text') {
        parts.push({ type: 'text', text: c.text ?? '' });
      } else if (c.type === 'think') {
        parts.push({ type: 'think', think: c.think ?? '', encrypted: undefined });
      }
    }
    for (const tc of response.toolCalls ?? []) {
      parts.push({
        type: 'function',
        id: tc.id,
        name: tc.name,
        arguments: tc.arguments ?? null,
        _streamIndex: undefined,
      });
    }

    const iterator = (async function* () {
      for (const p of parts) {
        yield p;
      }
    })();

    return {
      [Symbol.asyncIterator]: () => iterator[Symbol.asyncIterator](),
      id: null,
      usage: response.usage,
      finishReason: response.providerFinishReason as any,
      rawFinishReason: response.rawFinishReason ?? null,
    } as StreamedMessage;
  }
}

export async function runLoopL3(fixturePath: string): Promise<{ readonly lines: readonly string[] }> {
  const raw = readFileSync(fixturePath, 'utf8');
  const fixture: Fixture = JSON.parse(raw);

  const provider = new MockChatProvider(fixture.responses);
  const llm = new KosongLLM({
    provider,
    modelName: 'mock',
    systemPrompt: '',
  });

  const events: LoopEvent[] = [];
  const dispatcher = createLoopEventDispatcher({
    appendTranscriptRecord: async (record: LoopRecordedEvent) => {
      events.push(record as unknown as LoopEvent);
    },
    emitLiveEvent: (event: LoopLiveOnlyEvent) => {
      events.push(event as unknown as LoopEvent);
    },
  });

  const input: RunTurnInput = {
    turnId: fixture.turnId,
    signal: new AbortController().signal,
    llm,
    buildMessages: () => [],
    dispatchEvent: dispatcher,
    tools: fixture.tools.map((t) => new BuiltinTool(t)),
    hooks: undefined,
    maxSteps: fixture.maxSteps,
    maxRetryAttempts: fixture.maxRetryAttempts,
  };

  try {
    const result: TurnResult = await runTurn(input);
    events.push({ type: 'turn.result', ...result } as unknown as LoopEvent);
  } catch (error) {
    events.push({
      type: 'turn.error',
      message: error instanceof Error ? error.message : String(error),
    } as unknown as LoopEvent);
  }

  return { lines: events.map((e) => JSON.stringify(e)) };
}

async function main() {
  const fixturePath = process.argv[2];
  if (fixturePath === undefined) {
    console.error('usage: loop-l3.ts <fixture.json>');
    process.exit(1);
  }
  const { lines } = await runLoopL3(fixturePath);
  for (const line of lines) {
    console.log(line);
  }
}

if (fileURLToPath(import.meta.url) === process.argv[1]) {
  void main();
}
```

- [ ] 编译/类型检查：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm -r typecheck
```

预期无类型错误（新增导出后 driver 可通过 agent-core barrel 取到 `ToolAccesses`）。

- [ ] 手动验证 TS driver：

```bash
cd /Users/ranwei/workspace/ody-code/packages/integration-tests
pnpm exec tsx src/parity/loop-l3.ts \
  src/parity/fixtures/loop/single-text.json \
  | jq -c '.type'
```

预期输出：

```
"step.begin"
"text.delta"
"content.part"
"step.end"
"turn.result"
```

- [ ] 提交：`feat(integration-tests): TS loop L3 golden driver`

---

### Task 11: 注册 package script 并运行 TS-vs-Rust L3 对照

**Depends on:** Task 9（Rust binary 已可编译运行）、Task 10（TS driver 已可运行）

**Files:**
- Modify: `packages/integration-tests/package.json`（新增 script）
- Create: `packages/integration-tests/test/parity/loop/l3-loop-engine.test.ts`

#### 步骤

- [ ] 注册 script：在 `packages/integration-tests/package.json` 的 `scripts` 对象中追加：

```json
"test:parity:loop:l3": "vitest run test/parity/loop/l3-loop-engine.test.ts"
```

- [ ] 写测试 `packages/integration-tests/test/parity/loop/l3-loop-engine.test.ts`：

```ts
import { execSync } from 'node:child_process';
import { existsSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'pathe';
import { describe, expect, it } from 'vitest';

import { assertParity } from '../../../src/parity/assert-parity';
import { normalize } from '../../../src/parity/normalize';
import type { NormalizedSnapshot, ScenarioSnapshot } from '../../../src/parity/types';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const workspaceRoot = join(__dirname, '..', '..', '..', '..');
const fixtureDir = join(workspaceRoot, 'packages/integration-tests/src/parity/fixtures/loop');
const tsDriver = join(workspaceRoot, 'packages/integration-tests/src/parity/loop-l3.ts');

function findRustBinary(): string | null {
  const candidates = [
    join(workspaceRoot, 'rust-ody/target/release/loop-l3'),
    join(workspaceRoot, 'rust-ody/target/debug/loop-l3'),
  ];
  for (const candidate of candidates) {
    if (existsSync(candidate)) {
      return candidate;
    }
  }
  return null;
}

function runTs(fixturePath: string): string {
  return execSync(`pnpm exec tsx "${tsDriver}" "${fixturePath}"`, {
    cwd: join(workspaceRoot, 'packages/integration-tests'),
    encoding: 'utf8',
    maxBuffer: 10 * 1024 * 1024,
  });
}

function runRust(binaryPath: string, fixturePath: string): string {
  return execSync(`"${binaryPath}" "${fixturePath}"`, {
    encoding: 'utf8',
    maxBuffer: 10 * 1024 * 1024,
  });
}

interface LoopL3Snapshot {
  readonly events: unknown[];
  readonly result: unknown | undefined;
}

function normalizeOutput(output: string): LoopL3Snapshot {
  const parsed = output
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0)
    .map((line) => JSON.parse(line));

  const last = parsed[parsed.length - 1];
  const hasResult =
    last !== undefined &&
    typeof last === 'object' &&
    last !== null &&
    (last.type === 'turn.result' || last.type === 'turn.error');

  const events = hasResult ? parsed.slice(0, -1) : parsed;
  const result = hasResult ? last : undefined;

  const normalized = normalize(
    { responses: [], events: events as any[], records: undefined, fsTree: undefined } as ScenarioSnapshot,
    { homeDir: '', tmpDir: '' },
  );

  return { events: normalized.events as unknown[], result };
}

const binaryPath = findRustBinary();
const fixtures = readdirSync(fixtureDir)
  .filter((name) => name.endsWith('.json'))
  .map((name) => join(fixtureDir, name));

describe.skipIf(binaryPath === null)('loop engine L3 parity', () => {
  it.each(fixtures)(
    '%s matches between TS and Rust',
    async (fixturePath) => {
      const ts = normalizeOutput(runTs(fixturePath));
      const rust = normalizeOutput(runRust(binaryPath!, fixturePath));

      const diff = assertParity(
        fixturePath,
        { responses: [], events: ts.events, records: undefined, fsTree: undefined } as NormalizedSnapshot,
        { responses: [], events: rust.events, records: undefined, fsTree: undefined } as NormalizedSnapshot,
      );
      expect(diff).toBeNull();
      expect(rust.result).toEqual(ts.result);
    },
    120000,
  );
});
```

- [ ] 构建 Rust binary 并运行对照：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo build -p agent-rs --bin loop-l3

cd /Users/ranwei/workspace/ody-code/packages/integration-tests
pnpm run test:parity:loop:l3
```

预期首次运行可能暴露字段/顺序偏差（例如 `text.delta` 与 `content.part` 顺序、`finishReason` 字符串化差异）。修复策略：

1. 若某字段仅在 TS 侧出现（如 `llmFirstTokenLatencyMs`），但 Rust 侧省略——检查 `normalize.ts` 的 `TIMESTAMPISH_KEYS` 是否已包含该键；它会把数值归零、字符串替换为 `<ts>`。
2. 若 `turn.error` 消息文本不同——在 driver 中统一错误消息来源（都使用 `error.to_string()` / `error.message`）。
3. 若事件顺序不同——优先修复实现侧使顺序与 TS 一致；若确属合法非确定性（如并发 tool 结果），在 normalize 阶段排序或 fixture 中避免触发该分支。
4. 每修复一个偏差后重新运行 `pnpm run test:parity:loop:l3`，直到全部 7 个 fixture 绿。

- [ ] 最终全 tree typecheck：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm -r typecheck
cd rust-ody && cargo check --workspace --tests
```

- [ ] 提交：`test(integration-tests): L3 loop engine TS-vs-Rust parity`

---

## Phase C 本地自审

- [ ] 1. Spec-coverage：4.3.4.6 L3 fixture 与 G4-3-4 对照门已映射到 Task 9/10/11。
- [ ] 2. Placeholder scan：所有 fixture、binary、driver、测试代码完整给出，无 TODO/TBD/"后续补全"。
- [ ] 3. No phantom tasks：Task 9 产出 fixtures + binary + fixture 解析测试；Task 10 产出 TS driver；Task 11 产出 package script + 对照测试。
- [ ] 4. Dependency soundness：Task 9 依赖 Phase B Task 7；Task 10 依赖 Task 9 的 fixture schema；Task 11 依赖 Task 9/10。
- [ ] 5. Caller & build soundness：Task 10 在 `@odysseythink/agent-core` 新增 `ToolAccesses` 导出，未改动已有签名，无需更新 caller；Task 9/10/11 各自以 `pnpm -r typecheck` / `cargo check --workspace --tests` 结尾。
- [ ] 6. Test-the-risk：Task 9 的 fixture 解析测试确保所有 L3 fixture 可被反序列化；Task 11 的行为测试比较 TS/Rust 同一份 fixture 的事件序列与 turn 结果。
- [ ] 7. Type 一致性：binary/driver 使用的 `RunTurnInput`、`LoopEventDispatcher`、`ExecutableTool`、`TokenUsage`、`ToolCall` 字段名与 Phase A/B 定义一致；`turn.result`/`turn.error` 为对照测试私有包装，不影响引擎类型。
