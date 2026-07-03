# Part 3: L1 Golden Parity

本 Part 建立 TS↔Rust 的 L1 golden 对照：新增 `kosong-responses-golden` Rust 二进制与 TS runner，构造 7 组覆盖 text、reasoning summary、单/并行 function-call、incomplete、error 事件、non-stream 的 fixture，最后用 vitest 逐 fixture 断言两侧输出的 `assistantMessage` / `error` 逐值一致。

---

## Dependency Overview (Part 3)

```text
Task 9: Rust golden binary `kosong-responses-golden`
  │
  ├──► Task 10: TS golden runner `kosong-responses-golden.ts`
  │      │
  │      ├──► Task 11: 7 个 JSON golden fixtures
  │      │       │
  │      │       └──► Task 12: L1 比对测试 + package.json script
```

- Task 9 依赖 Part 2 的 `OpenAIResponsesChatProvider::generate()` 与 `OpenAIResponsesStreamedMessage`。
- Task 10 依赖 Task 9 确定的 fixture schema 与 Rust 输出格式。
- Task 11 依赖 Task 10 的 runner 能解析 fixture 字段。
- Task 12 依赖 Task 9–11 全部就绪，作为集成验证。

---

### Task 9: `kosong-responses-golden` Rust 二进制

**Depends on:** Part 2 Task 8（`generate()` 真实解析 + `OpenAIResponsesStreamedMessage`）

**Files:**
- Modify: `rust-ody/crates/kosong-rs/Cargo.toml`（新增 `[[bin]]`）
- Create: `rust-ody/crates/kosong-rs/src/bin/responses_golden.rs`
- Test: `rust-ody/crates/kosong-rs/src/bin/responses_golden.rs` 内 `#[cfg(test)]`

步骤：

- [ ] **写失败测试**：新建 `responses_golden.rs` 时先写入骨架 + 内联 fixture 测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn text_stream_fixture() -> Fixture {
        serde_json::from_value(serde_json::json!({
            "systemPrompt": "",
            "history": [{"role":"user","content":[{"type":"text","text":"Hi"}],"toolCalls":[]}],
            "providerOptions": {"model": "gpt-4o-mini"},
            "response": {
                "status": 200,
                "stream": true,
                "body": "{\"type\":\"response.created\",\"response\":{\"id\":\"resp-test\"}}\n{\"type\":\"response.output_text.delta\",\"delta\":\"Hello\"}\n{\"type\":\"response.completed\",\"response\":{\"id\":\"resp-test\",\"status\":\"completed\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n"
            }
        })).unwrap()
    }

    #[tokio::test]
    async fn binary_runs_text_stream_fixture() {
        let fixture = text_stream_fixture();
        let message = run_fixture(fixture).await.unwrap();
        let text = kosong_rs::message::extract_text(&message, "");
        assert_eq!(text, "Hello");
    }
}
```

- [ ] **运行并确认失败**：

```bash
cd rust-ody && cargo test -p kosong-rs --bin kosong-responses-golden
```

预期失败：二进制与 `run_fixture` 尚未实现。

- [ ] **写最小实现**：完整 `responses_golden.rs`：

```rust
use std::env;
use std::fs;
use std::sync::Arc;

use futures_util::StreamExt;
use kosong_rs::{
    generate, ChatProviderError, GenerateOptions, Message, MockHttpClient,
    MockProvider, OpenAIResponsesChatProvider, OpenAIResponsesOptions,
    OpenAIResponsesStreamedMessage, ProviderRequestAuth, StreamedMessagePart, Tool,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    system_prompt: Option<String>,
    #[serde(default)]
    tools: Vec<Tool>,
    history: Vec<Message>,
    #[serde(default)]
    options: FixtureOptions,
    provider_options: ProviderOptionsFixture,
    response: ResponseFixture,
    #[serde(default)]
    expect_error: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct FixtureOptions {
    auth: Option<ProviderRequestAuth>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderOptionsFixture {
    model: String,
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    base_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResponseFixture {
    status: u16,
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    error: Option<ErrorFixture>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ErrorFixture {
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GoldenResult {
    assistant_message: Option<Value>,
    error: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = env::args().nth(1).expect("fixture path required");
    let fixture = load_fixture(&path)?;
    let result = run_fixture(fixture).await;
    let output = GoldenResult {
        assistant_message: result.as_ref().ok().map(|m| serde_json::to_value(m).unwrap()),
        error: result.as_ref().err().map(|e| format!("{}", e)),
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

fn load_fixture(path: &str) -> anyhow::Result<Fixture> {
    let input = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&input)?)
}

async fn run_fixture(fixture: Fixture) -> Result<Message, ChatProviderError> {
    if fixture.response.stream {
        run_through_provider(fixture).await
    } else {
        run_direct_parser(fixture).await
    }
}

async fn run_through_provider(fixture: Fixture) -> Result<Message, ChatProviderError> {
    let response_bytes = build_response_bytes(&fixture.response);
    let client = Arc::new(MockHttpClient::new(fixture.response.status, response_bytes));
    let options = OpenAIResponsesOptions {
        api_key: Some(
            fixture
                .provider_options
                .api_key
                .unwrap_or_else(|| "sk-test".into()),
        ),
        base_url: Some(
            fixture
                .provider_options
                .base_url
                .unwrap_or_else(|| "http://mock".into()),
        ),
        model: fixture.provider_options.model,
        max_output_tokens: None,
        default_headers: None,
        tool_message_conversion: None,
        http_client: Some(client),
    };
    let provider = OpenAIResponsesChatProvider::new(options);
    let gen_options = fixture.options.auth.map(|auth| GenerateOptions {
        auth: Some(auth),
        ..Default::default()
    });
    let result = generate(
        &provider,
        &fixture.system_prompt.unwrap_or_default(),
        &fixture.tools,
        &fixture.history,
        None,
        gen_options.as_ref(),
    )
    .await?;
    Ok(result.message)
}

fn build_response_bytes(response: &ResponseFixture) -> Vec<u8> {
    if let Some(error) = &response.error {
        let body = serde_json::json!({
            "error": { "message": error.message, "type": "invalid_request_error" }
        });
        return serde_json::to_vec(&body).unwrap();
    }
    let body = response
        .body
        .as_ref()
        .expect("success response must have body");
    if response.stream {
        body.as_bytes().to_vec()
    } else {
        let value: Value = serde_json::from_str(body).expect("non-stream body must be valid JSON");
        serde_json::to_vec(&value).unwrap()
    }
}

async fn run_direct_parser(fixture: Fixture) -> Result<Message, ChatProviderError> {
    let body = fixture
        .response
        .body
        .as_ref()
        .expect("non-stream body required")
        .as_bytes()
        .to_vec();
    let stream = OpenAIResponsesStreamedMessage::from_bytes(body, false)?;
    let mut message = Message::assistant(vec![], vec![]);
    futures_util::pin_mut!(stream);
    while let Some(part) = stream.next().await {
        match part {
            StreamedMessagePart::Content(c) => message.content.push(c),
            StreamedMessagePart::ToolCall(mut tc) => {
                tc.stream_index = None;
                message.tool_calls.push(tc);
            }
            StreamedMessagePart::ToolCallPart(_) => {}
        }
    }
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_stream_fixture() -> Fixture {
        serde_json::from_value(serde_json::json!({
            "systemPrompt": "",
            "history": [{"role":"user","content":[{"type":"text","text":"Hi"}],"toolCalls":[]}],
            "providerOptions": {"model": "gpt-4o-mini"},
            "response": {
                "status": 200,
                "stream": true,
                "body": "{\"type\":\"response.created\",\"response\":{\"id\":\"resp-test\"}}\n{\"type\":\"response.output_text.delta\",\"delta\":\"Hello\"}\n{\"type\":\"response.completed\",\"response\":{\"id\":\"resp-test\",\"status\":\"completed\",\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n"
            }
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn binary_runs_text_stream_fixture() {
        let fixture = text_stream_fixture();
        let message = run_fixture(fixture).await.unwrap();
        let text = kosong_rs::message::extract_text(&message, "");
        assert_eq!(text, "Hello");
    }

    #[tokio::test]
    async fn binary_reports_error_for_failed_response() {
        let fixture: Fixture = serde_json::from_value(serde_json::json!({
            "systemPrompt": "",
            "history": [{"role":"user","content":[{"type":"text","text":"Hi"}],"toolCalls":[]}],
            "providerOptions": {"model": "gpt-4o-mini"},
            "response": {
                "status": 401,
                "stream": true,
                "error": {"message": "Invalid auth"}
            },
            "expectError": true
        }))
        .unwrap();
        assert!(run_fixture(fixture).await.is_err());
    }
}
```

并在 `Cargo.toml` 末尾追加：

```toml
[[bin]]
name = "kosong-responses-golden"
path = "src/bin/responses_golden.rs"
```

- [ ] **运行并确认通过**：

```bash
cd rust-ody && cargo test -p kosong-rs --bin kosong-responses-golden
```

预期：2 个测试通过。

- [ ] **运行 crate 级测试保证无回归**：

```bash
cd rust-ody && cargo test -p kosong-rs
```

预期：所有已有测试通过。

- [ ] **Commit**：`feat(kosong-rs): add kosong-responses-golden binary`

---

### Task 10: TS golden runner

**Depends on:** Task 9（fixture schema 与输出格式已确定）

**Files:**
- Modify: `packages/kosong/src/providers/openai-responses.ts`（导出 `OpenAIResponsesStreamedMessage`）
- Create: `packages/integration-tests/src/parity/kosong-responses-golden.ts`

步骤：

- [ ] **写实现代码**：

1. 在 `packages/kosong/src/providers/openai-responses.ts` 中，将：

```ts
export class OpenAIResponsesChatProvider implements ChatProvider {
```

上方的：

```ts
class OpenAIResponsesStreamedMessage implements StreamedMessage {
```

改为：

```ts
export class OpenAIResponsesStreamedMessage implements StreamedMessage {
```

2. 新建 `packages/integration-tests/src/parity/kosong-responses-golden.ts`：

```ts
import { generate, type Message, type StreamedMessagePart, type Tool, type ContentPart, type ToolCall, isContentPart, isToolCall } from '@odysseythink/kosong';
import { OpenAIResponsesChatProvider, OpenAIResponsesStreamedMessage } from '@odysseythink/kosong/providers/openai-responses';

export interface Fixture {
  systemPrompt?: string;
  tools?: Tool[];
  history: Message[];
  options?: { auth?: { apiKey?: string; headers?: Record<string, string> } };
  providerOptions: { model: string; apiKey?: string; baseUrl?: string };
  response: { status: number; stream?: boolean; body?: string; error?: { message: string; code?: string } };
  expectError?: boolean;
}

export interface GoldenResult { assistantMessage: unknown | null; error: string | null; }

type ClientFactory = NonNullable<ConstructorParameters<typeof OpenAIResponsesChatProvider>[0]['clientFactory']>;
type OpenAIClient = ReturnType<ClientFactory>;

export async function runTsKosongResponsesGolden(fixture: Fixture): Promise<GoldenResult> {
  if (fixture.response.error) {
    return runThroughProvider(fixture);
  }
  if (fixture.response.stream) {
    return runThroughProvider(fixture);
  }
  return runDirectParser(fixture);
}

async function runThroughProvider(fixture: Fixture): Promise<GoldenResult> {
  const provider = new OpenAIResponsesChatProvider({
    model: fixture.providerOptions.model,
    apiKey: fixture.providerOptions.apiKey ?? 'sk-test',
    baseUrl: fixture.providerOptions.baseUrl ?? 'http://mock',
    clientFactory: () => createMockClient(fixture.response) as unknown as OpenAIClient,
  });
  try {
    const result = await generate(provider, fixture.systemPrompt ?? '', fixture.tools ?? [], fixture.history, undefined, fixture.options);
    return { assistantMessage: result.message, error: null };
  } catch (e) {
    return { assistantMessage: null, error: e instanceof Error ? e.message : String(e) };
  }
}

async function runDirectParser(fixture: Fixture): Promise<GoldenResult> {
  try {
    const response = JSON.parse(fixture.response.body ?? '{}');
    const stream = new OpenAIResponsesStreamedMessage(response, false);
    const message = await partsToMessage(stream);
    return { assistantMessage: message, error: null };
  } catch (e) {
    return { assistantMessage: null, error: e instanceof Error ? e.message : String(e) };
  }
}

async function partsToMessage(stream: AsyncIterable<StreamedMessagePart>): Promise<Message> {
  const content: ContentPart[] = [];
  const toolCalls: ToolCall[] = [];
  for await (const part of stream) {
    if (isContentPart(part)) {
      content.push(part);
    } else if (isToolCall(part)) {
      const { _streamIndex, ...tc } = part as ToolCall & { _streamIndex?: number | string };
      toolCalls.push(tc as ToolCall);
    }
  }
  return {
    role: 'assistant',
    content,
    toolCalls,
  };
}

function createMockClient(response: Fixture['response']) {
  return {
    responses: {
      create: async (_params: unknown, _options?: unknown) => {
        if (response.error) return JSON.parse('{}');
        if (response.stream) return parseResponsesStream(response.body ?? '');
        return JSON.parse(response.body ?? '{}');
      },
    },
  };
}

async function* parseResponsesStream(body: string): AsyncIterable<unknown> {
  for (const line of body.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (trimmed.length === 0) continue;
    yield JSON.parse(trimmed);
  }
}
```

- [ ] **类型检查（build step）**：

```bash
pnpm --filter @odysseythink/kosong typecheck
pnpm --filter @odysseythink/integration-tests typecheck
```

预期：两个 package 的 `tsc --noEmit` 均通过。

- [ ] **Commit**：`feat(integration-tests): add kosong-responses-golden TS runner`

---

### Task 11: L1 golden fixtures

**Depends on:** Task 10（runner 可解析 fixture schema）

**Files:**
- Create: `packages/integration-tests/src/parity/fixtures/kosong-responses/l1-responses-text.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-responses/l1-responses-thinking.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-responses/l1-responses-tool-call-single.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-responses/l1-responses-tool-call-parallel.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-responses/l1-responses-incomplete.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-responses/l1-responses-error.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-responses/l1-responses-nonstream.json`

步骤：

- [ ] **写 fixture 文件**：

`l1-responses-text.json`：

```json
{"systemPrompt":"You are helpful.","history":[{"role":"user","content":[{"type":"text","text":"Hello"}],"toolCalls":[]}],"providerOptions":{"model":"gpt-4o-mini"},"response":{"status":200,"stream":true,"body":"{\"type\":\"response.created\",\"response\":{\"id\":\"resp-text\"}}\n{\"type\":\"response.output_text.delta\",\"delta\":\"Hello\"}\n{\"type\":\"response.output_text.delta\",\"delta\":\" world\"}\n{\"type\":\"response.completed\",\"response\":{\"id\":\"resp-text\",\"status\":\"completed\",\"usage\":{\"input_tokens\":5,\"output_tokens\":2}}}\n"}}
```

`l1-responses-thinking.json`：

```json
{"systemPrompt":"","history":[{"role":"user","content":[{"type":"text","text":"Think"}],"toolCalls":[]}],"providerOptions":{"model":"o3-mini"},"response":{"status":200,"stream":true,"body":"{\"type\":\"response.created\",\"response\":{\"id\":\"resp-think\"}}\n{\"type\":\"response.reasoning_summary_part.added\"}\n{\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"Reasoning\"}\n{\"type\":\"response.output_text.delta\",\"delta\":\"Answer\"}\n{\"type\":\"response.completed\",\"response\":{\"id\":\"resp-think\",\"status\":\"completed\",\"usage\":{\"input_tokens\":3,\"output_tokens\":2}}}\n"}}
```

`l1-responses-tool-call-single.json`：

```json
{"systemPrompt":"","tools":[{"name":"add","description":"Add two integers.","parameters":{"type":"object","properties":{"a":{"type":"integer"},"b":{"type":"integer"}},"required":["a","b"]}}],"history":[{"role":"user","content":[{"type":"text","text":"Add 1 and 2"}],"toolCalls":[]}],"providerOptions":{"model":"gpt-4o-mini"},"response":{"status":200,"stream":true,"body":"{\"type\":\"response.created\",\"response\":{\"id\":\"resp-tc\"}}\n{\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"function_call\",\"id\":\"fc-1\",\"call_id\":\"call_1\",\"name\":\"add\",\"arguments\":\"\"}}\n{\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc-1\",\"output_index\":0,\"delta\":\"{\\\"a\\\":1\"}\n{\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc-1\",\"output_index\":0,\"delta\":\"\\\"b\\\":2}\"}\n{\"type\":\"response.function_call_arguments.done\",\"item_id\":\"fc-1\",\"output_index\":0,\"arguments\":\"{\\\"a\\\":1,\\\"b\\\":2}\"}\n{\"type\":\"response.completed\",\"response\":{\"id\":\"resp-tc\",\"status\":\"completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}}\n"}}
```

`l1-responses-tool-call-parallel.json`：

```json
{"systemPrompt":"","tools":[{"name":"read","description":"Read.","parameters":{"type":"object"}},{"name":"write","description":"Write.","parameters":{"type":"object"}}],"history":[{"role":"user","content":[{"type":"text","text":"Read and write"}],"toolCalls":[]}],"providerOptions":{"model":"gpt-4o-mini"},"response":{"status":200,"stream":true,"body":"{\"type\":\"response.created\",\"response\":{\"id\":\"resp-para\"}}\n{\"type\":\"response.output_item.added\",\"item\":{\"type\":\"function_call\",\"id\":\"fc-a\",\"call_id\":\"call_a\",\"name\":\"read\",\"arguments\":\"\"}}\n{\"type\":\"response.output_item.added\",\"item\":{\"type\":\"function_call\",\"id\":\"fc-b\",\"call_id\":\"call_b\",\"name\":\"write\",\"arguments\":\"\"}}\n{\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc-b\",\"delta\":\"{\\\"b\\\":2\"}\n{\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc-a\",\"delta\":\"{\\\"a\\\":1\"}\n{\"type\":\"response.function_call_arguments.done\",\"item_id\":\"fc-b\",\"arguments\":\"{\\\"b\\\":2}\"}\n{\"type\":\"response.function_call_arguments.done\",\"item_id\":\"fc-a\",\"arguments\":\"{\\\"a\\\":1}\"}\n{\"type\":\"response.completed\",\"response\":{\"id\":\"resp-para\",\"status\":\"completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5}}}\n"}}
```

`l1-responses-incomplete.json`：

```json
{"systemPrompt":"","history":[{"role":"user","content":[{"type":"text","text":"Long"}],"toolCalls":[]}],"providerOptions":{"model":"gpt-4o-mini"},"response":{"status":200,"stream":true,"body":"{\"type\":\"response.created\",\"response\":{\"id\":\"resp-inc\"}}\n{\"type\":\"response.output_text.delta\",\"delta\":\"Cut\"}\n{\"type\":\"response.incomplete\",\"response\":{\"id\":\"resp-inc\",\"status\":\"incomplete\",\"incomplete_details\":{\"reason\":\"max_output_tokens\"},\"usage\":{\"input_tokens\":2,\"output_tokens\":1}}}\n"}}
```

`l1-responses-error.json`：

```json
{"systemPrompt":"","history":[{"role":"user","content":[{"type":"text","text":"Hi"}],"toolCalls":[]}],"providerOptions":{"model":"gpt-4o-mini"},"response":{"status":200,"stream":true,"body":"{\"type\":\"response.created\",\"response\":{\"id\":\"resp-err\"}}\n{\"type\":\"error\",\"code\":\"rate_limit_exceeded\",\"message\":\"Rate limited\",\"param\":null}\n"},"expectError":true}
```

`l1-responses-nonstream.json`：

```json
{"systemPrompt":"","tools":[{"name":"add","description":"Add.","parameters":{"type":"object"}}],"history":[{"role":"user","content":[{"type":"text","text":"Add"}],"toolCalls":[]}],"providerOptions":{"model":"gpt-4o-mini"},"response":{"status":200,"stream":false,"body":"{\"id\":\"resp-nonstream\",\"status\":\"completed\",\"output\":[{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"Result:\"}]},{\"type\":\"function_call\",\"id\":\"fc-1\",\"call_id\":\"call_1\",\"name\":\"add\",\"arguments\":\"{\\\"a\\\":1}\"}],\"usage\":{\"input_tokens\":4,\"output_tokens\":3}}"}}
```

- [ ] **手动验证**：运行 L1 测试（Task 12），预期 7/7 fixture 通过。

- [ ] **Commit**：`test(integration-tests): add kosong-responses golden fixtures`

---

### Task 12: L1 parity测试与 package.json script

**Depends on:** Task 11（fixtures 已就位）

**Files:**
- Create: `packages/integration-tests/test/parity/kosong/l1-responses-golden.test.ts`
- Modify: `packages/integration-tests/package.json`（新增 `test:parity:kosong:responses` script）

步骤：

- [ ] **写测试**：

```ts
import { existsSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { beforeAll, describe, expect, it } from 'vitest';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { runTsKosongResponsesGolden, type Fixture } from '../../../src/parity/kosong-responses-golden';

function findProjectRoot(): string {
  let current = dirname(fileURLToPath(import.meta.url));
  while (current !== dirname(current)) {
    if (existsSync(join(current, '.git'))) return current;
    current = dirname(current);
  }
  return process.cwd();
}

const rootDir = findProjectRoot();
const fixturesDir = join(rootDir, 'packages', 'integration-tests', 'src', 'parity', 'fixtures', 'kosong-responses');
const fixtures: Array<{ name: string; expectError: boolean }> = [
  { name: 'l1-responses-text.json', expectError: false },
  { name: 'l1-responses-thinking.json', expectError: false },
  { name: 'l1-responses-tool-call-single.json', expectError: false },
  { name: 'l1-responses-tool-call-parallel.json', expectError: false },
  { name: 'l1-responses-incomplete.json', expectError: false },
  { name: 'l1-responses-error.json', expectError: true },
  { name: 'l1-responses-nonstream.json', expectError: false },
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

describe('kosong-responses L1 golden parity', () => {
  beforeAll(() => {
    const binaryPath = process.env.ODY_RESPONSES_GOLDEN_BINARY_PATH ?? join(rootDir, 'rust-ody', 'target', 'debug', 'kosong-responses-golden');
    if (existsSync(binaryPath)) return;
    spawnSync('cargo', ['build', '-p', 'kosong-rs', '--bin', 'kosong-responses-golden'], { cwd: join(rootDir, 'rust-ody'), stdio: 'inherit' });
  });

  const binaryPath = process.env.ODY_RESPONSES_GOLDEN_BINARY_PATH ?? join(rootDir, 'rust-ody', 'target', 'debug', 'kosong-responses-golden');

  it.each(fixtures)('$name TS matches Rust', async ({ name, expectError }) => {
    const fixture = loadFixture(name);
    const ts = await runTsKosongResponsesGolden(fixture);
    const result = spawnSync(binaryPath, [join(fixturesDir, name)], { encoding: 'utf8' });

    if (expectError) {
      expect(ts.error).toBeTruthy();
      if (result.status === 0) {
        const rust = JSON.parse(result.stdout);
        expect(rust.error).toBeTruthy();
      }
      return;
    }

    if (result.status !== 0) throw new Error(`kosong-responses-golden exited ${result.status}: ${result.stderr}`);
    const rust = JSON.parse(result.stdout);

    expect(sortKeys(rust)).toStrictEqual(sortKeys(ts));
  });
});
```

- [ ] **修改 package.json**：在 `scripts` 中追加：

```json
"test:parity:kosong:responses": "vitest run test/parity/kosong/l1-responses-golden.test.ts"
```

- [ ] **运行并确认失败**：

```bash
pnpm --filter @odysseythink/integration-tests test:parity:kosong:responses
```

预期失败：fixture / runner / binary 尚未全部就绪时部分用例失败；全部就绪后应 7/7 通过。

- [ ] **运行并确认通过**：在 Task 9–11 完成后再次运行：

```bash
pnpm --filter @odysseythink/integration-tests test:parity:kosong:responses
```

预期：7 个 fixture 全部通过。

- [ ] **运行完整 parity 套件**：

```bash
pnpm --filter @odysseythink/integration-tests test:parity
```

预期：所有 parity 测试通过。

- [ ] **Commit**：`test(integration-tests): add kosong-responses L1 parity test`

---

## Local Self-Review

- [x] 1. Spec-coverage table（索引中）：Part 3 覆盖 4.2.3.5（L1 SSE + non-stream fixture）与门 G4-2-3。
- [x] 2. Placeholder scan：无 TODO/TBD；所有 fixture body、runner、binary、test 代码完整给出。
- [x] 3. No phantom tasks：Task 9–12 均产生可验证变更（binary、runner、fixtures、test、script）。
- [x] 4. Dependency soundness：Task 9 → Task 10 → Task 11 → Task 12；无向后依赖。
- [x] 5. Caller & build soundness：新增 `OpenAIResponsesStreamedMessage` 导出为新增 export，不改变既有共享签名；Task 9 以 `cargo test -p kosong-rs` 结束，Task 10 以 `pnpm typecheck` 结束，Task 12 以 `pnpm test:parity` 结束。
- [x] 6. Test-the-risk：L1 测试逐 fixture 比较 TS 与 Rust 的 assistantMessage/error；error fixture 断言两侧均产生错误；non-stream fixture 验证直接解析路径的跨语言一致性。
- [x] 7. Type一致性：Fixture schema 与 Task 9/10 实现一致；输出字段 `assistantMessage`/`error` 在 Rust（`GoldenResult`）与 TS（`GoldenResult`）中同名同类型；`Message` 序列化字段在两侧均为 camelCase。
