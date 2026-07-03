# parity.md — OpenAI Legacy L1 金标对位

## 目标

为 Rust `OpenAILegacyChatProvider` 建立与 TypeScript `OpenAILegacyChatProvider` 的 L1 金标对位：同一份 SSE/HTTP fixture 同时驱动两端实现，断言生成的 `Message`/`usage`/`finishReason` 完全一致。

## 文件结构

- `rust-ody/crates/kosong-rs/src/lib.rs` — 导出 `openai_legacy`、`http_client`、`chat_completions_stream`、`openai_common` 模块。
- `rust-ody/crates/kosong-rs/Cargo.toml` — 声明 `kosong-openai-golden` binary。
- `rust-ody/crates/kosong-rs/src/bin/openai_golden.rs` — Rust 金标 binary。
- `packages/integration-tests/src/parity/kosong-openai-golden.ts` — TypeScript 金标 runner。
- `packages/integration-tests/src/parity/fixtures/kosong-openai/*.json` — 纯文本 / thinking / 单 tool-call / 并行 tool-calls / 截断 / usage / 错误 fixture。
- `packages/integration-tests/test/parity/kosong/l1-openai-golden.test.ts` — L1 对位测试。
- `packages/integration-tests/package.json` — 新增 `test:parity:kosong:openai` 脚本。
- `.github/workflows/rust-host.yml` — CI 新增 parity 步骤。

## 依赖关系

```
core.md Task 4 (OpenAILegacyChatProvider)
    │
    ▼
parity.md Task 1 (导出与 binary 声明)
    │
    ▼
parity.md Task 2 (kosong-openai-golden binary)
    │
    ▼
parity.md Task 3 (TS runner)
    │
    ▼
parity.md Task 4 (fixtures)
    │
    ▼
parity.md Task 5 (L1 test)
    │
    ▼
parity.md Task 6 (CI 脚本)
```

## 风险与未决问题

- `generate()` 在 TS 与 Rust 中对 `finish_reason` 的默认行为需保持一致： fixture 显式提供 `finish_reason`，避免空值差异。
- 错误 fixture 只断言 `error` 存在，不比较字符串细节，因为两端错误格式化的层级不同。
- `stream_options.include_usage` 只在 stream=true 时由 provider 自动注入，fixture 无需关心。

---

## Task 1: 导出 OpenAI Legacy 模块与类型

**Depends on:** `core.md` Task 4（`openai_legacy.rs`、`http_client.rs` 等模块已落地）
**Files:**
- Modify: `rust-ody/crates/kosong-rs/src/lib.rs`
- Test: `rust-ody/crates/kosong-rs/src/lib.rs`（通过 `cargo check` 验证导出）

### 步骤

- [ ] 将 `lib.rs` 替换为以下完整内容，确保 golden binary 可以引用 `OpenAILegacyChatProvider`、`OpenAILegacyOptions`、`MockHttpClient`、`generate` 等符号。
- [ ] 运行 `cargo check -p kosong-rs` 确认无编译错误。
- [ ] Commit: `feat(kosong-rs): export OpenAI Legacy modules for golden binary`

### 实现代码

`rust-ody/crates/kosong-rs/src/lib.rs`：

```rust
pub mod capability_registry;
pub mod catalog;
pub mod chat_completions_stream;
pub mod errors;
pub mod generate;
pub mod http_client;
pub mod message;
pub mod mock;
pub mod openai_common;
pub mod openai_legacy;
pub mod provider;
pub mod request_auth;
pub mod tool_call_id;
pub mod usage;

// Re-exports for convenience (used by golden binaries)
pub use chat_completions_stream::{
    parse_non_stream_response, parse_stream_response, BufferedChatCompletionToolCall,
};
pub use generate::generate;
pub use http_client::{HttpClient, MockHttpClient, ReqwestClient};
pub use message::{Message, StreamedMessagePart};
pub use mock::MockProvider;
pub use openai_common::{
    convert_content_part, convert_openai_error, extract_usage, normalize_openai_finish_reason,
    thinking_effort_to_reasoning_effort, tool_to_openai, ToolMessageConversion,
};
pub use openai_legacy::{OpenAILegacyChatProvider, OpenAILegacyOptions};
pub use provider::{
    GenerateOptions, ModelCapability, ProviderRequestAuth, ProviderType, Tool,
};
pub use usage::TokenUsage;

// Re-exports for utility modules
pub use capability_registry::{
    get_anthropic_model_capability, get_google_genai_model_capability,
    get_openai_legacy_model_capability, get_openai_responses_model_capability,
    uses_openai_responses_developer_role,
};
pub use request_auth::{
    merge_request_headers, require_provider_api_key, resolve_auth_backed_client,
    AuthBackedClientState,
};
pub use tool_call_id::{
    normalize_tool_call_ids_for_provider, sanitize_openai_responses_call_id, sanitize_tool_call_id,
    ToolCallIdPolicy,
};
pub use catalog::{
    catalog_base_url, catalog_model_to_capability, catalog_provider_models, infer_wire_type,
    Catalog, CatalogModel, CatalogModelEntry, CatalogProviderEntry,
};
```

### 验证命令

```bash
cd rust-ody
cargo check -p kosong-rs
# expected: clean
```

---

## Task 2: `kosong-openai-golden` Rust 金标 binary

**Depends on:** Task 1
**Files:**
- Create: `rust-ody/crates/kosong-rs/src/bin/openai_golden.rs`
- Modify: `rust-ody/crates/kosong-rs/Cargo.toml:21-27`
- Test: `rust-ody/crates/kosong-rs/src/bin/openai_golden.rs`（模块内 `#[cfg(test)]`）

### 步骤

- [ ] 在 `Cargo.toml` 追加 `[[bin]] kosong-openai-golden`。
- [ ] 创建 `src/bin/openai_golden.rs`，包含 fixture schema、HTTP mock 构造、`generate()` 调用与 JSON 输出。
- [ ] 写失败单测：在 binary 内构造一个纯文本 SSE fixture，断言 `run_fixture()` 返回合并后的文本。
- [ ] 运行 `cargo test -p kosong-rs --bin kosong-openai-golden` 确认失败（binary 尚未创建）。
- [ ] 补全实现后运行通过。
- [ ] Commit: `feat(kosong-rs): add kosong-openai-golden binary for L1 parity`

### 实现代码

`Cargo.toml` 变更（在现有 `[[bin]]` 块后追加）：

```toml
[[bin]]
name = "kosong-openai-golden"
path = "src/bin/openai_golden.rs"
```

`src/bin/openai_golden.rs`：

```rust
use std::env;
use std::fs;
use std::sync::Arc;

use kosong_rs::{
    generate, ChatProviderError, GenerateOptions, Message, MockHttpClient, OpenAILegacyChatProvider,
    OpenAILegacyOptions, ProviderRequestAuth, Tool,
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
    #[serde(default)]
    stream: bool,
    #[serde(default)]
    reasoning_key: Option<String>,
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
    let response_bytes = build_response_bytes(&fixture.response);
    let client = Arc::new(MockHttpClient::new(fixture.response.status, response_bytes));

    let options = OpenAILegacyOptions {
        api_key: Some(fixture.provider_options.api_key.unwrap_or_else(|| "sk-test".into())),
        base_url: Some(fixture.provider_options.base_url.unwrap_or_else(|| "http://mock".into())),
        model: fixture.provider_options.model,
        stream: Some(fixture.provider_options.stream),
        max_tokens: None,
        reasoning_key: fixture.provider_options.reasoning_key,
        default_headers: None,
        tool_message_conversion: None,
        http_client: Some(client),
    };
    let provider = OpenAILegacyChatProvider::new(options);

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
    let body = response.body.as_ref().expect("success response must have body");
    if response.stream {
        body.as_bytes().to_vec()
    } else {
        let value: Value = serde_json::from_str(body).expect("non-stream body must be valid JSON");
        serde_json::to_vec(&value).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_stream_fixture() -> Fixture {
        let body = "data: {\"id\":\"1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hello\"}}]}\n\n\
                    data: {\"id\":\"2\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"}}]}\n\n\
                    data: {\"id\":\"3\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
                    data: [DONE]\n\n";
        serde_json::from_value(serde_json::json!({
            "systemPrompt": "",
            "history": [{ "role": "user", "content": [{ "type": "text", "text": "Hi" }], "toolCalls": [] }],
            "providerOptions": { "model": "gpt-4o-mini", "stream": true },
            "response": { "status": 200, "stream": true, "body": body }
        })).unwrap()
    }

    #[tokio::test]
    async fn binary_runs_text_stream_fixture() {
        let fixture = text_stream_fixture();
        let message = run_fixture(fixture).await.unwrap();
        let text = kosong_rs::message::extract_text(&message, "");
        assert_eq!(text, "Hello world");
    }

    #[tokio::test]
    async fn binary_reports_error_for_failed_response() {
        let fixture: Fixture = serde_json::from_value(serde_json::json!({
            "systemPrompt": "",
            "history": [{ "role": "user", "content": [{ "type": "text", "text": "Hi" }], "toolCalls": [] }],
            "providerOptions": { "model": "gpt-4o-mini", "stream": true },
            "response": { "status": 401, "error": { "message": "Invalid auth" } },
            "expectError": true
        })).unwrap();
        assert!(run_fixture(fixture).await.is_err());
    }
}
```

### 验证命令

```bash
cd rust-ody
cargo test -p kosong-rs --bin kosong-openai-golden
# expected: 2 passed
cargo build -p kosong-rs --bin kosong-openai-golden
# expected: binary built at target/debug/kosong-openai-golden
```

---

## Task 3: TypeScript 金标 runner

**Depends on:** Task 2（fixture schema 已稳定）
**Files:**
- Create: `packages/integration-tests/src/parity/kosong-openai-golden.ts`
- Create: `packages/integration-tests/test/parity/kosong/openai-golden-runner.test.ts`
- Test: `packages/integration-tests/test/parity/kosong/openai-golden-runner.test.ts`

### 步骤

- [ ] 创建 runner，解析同一份 fixture JSON，用 `clientFactory` 注入 mock OpenAI 客户端。
- [ ] 写失败单测：用内联纯文本 SSE fixture 调用 runner，断言返回文本为 `"Hi there"`（实现前会失败）。
- [ ] 运行 `pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/kosong/openai-golden-runner.test.ts` 确认失败。
- [ ] 补全 runner 后运行通过。
- [ ] Commit: `feat(integration-tests): add TS runner for OpenAI Legacy golden parity`

### 实现代码

`packages/integration-tests/src/parity/kosong-openai-golden.ts`：

```ts
import { generate } from '@odysseythink/kosong';
import type { Message, Tool } from '@odysseythink/kosong';
import { OpenAILegacyChatProvider } from '@odysseythink/kosong/providers/openai-legacy';

export interface Fixture {
  systemPrompt?: string;
  tools?: Tool[];
  history: Message[];
  options?: {
    auth?: { apiKey?: string; headers?: Record<string, string> };
  };
  providerOptions: {
    model: string;
    apiKey?: string;
    baseUrl?: string;
    stream?: boolean;
    reasoningKey?: string;
  };
  response: {
    status: number;
    stream?: boolean;
    body?: string;
    error?: { message: string; code?: string };
  };
  expectError?: boolean;
}

export interface GoldenResult {
  assistantMessage: unknown | null;
  error: string | null;
}

type ClientFactory = NonNullable<
  ConstructorParameters<typeof OpenAILegacyChatProvider>[0]['clientFactory']
>;
type OpenAIClient = ReturnType<ClientFactory>;

export async function runTsKosongOpenAIGolden(
  fixture: Fixture,
): Promise<GoldenResult> {
  const provider = new OpenAILegacyChatProvider({
    model: fixture.providerOptions.model,
    apiKey: fixture.providerOptions.apiKey ?? 'sk-test',
    baseUrl: fixture.providerOptions.baseUrl ?? 'http://mock',
    stream: fixture.providerOptions.stream ?? true,
    reasoningKey: fixture.providerOptions.reasoningKey,
    clientFactory: () => createMockClient(fixture.response) as unknown as OpenAIClient,
  });

  try {
    const result = await generate(
      provider,
      fixture.systemPrompt ?? '',
      fixture.tools ?? [],
      fixture.history,
      undefined,
      fixture.options,
    );
    return { assistantMessage: result.message, error: null };
  } catch (e) {
    return {
      assistantMessage: null,
      error: e instanceof Error ? e.message : String(e),
    };
  }
}

function createMockClient(response: Fixture['response']) {
  return {
    chat: {
      completions: {
        create: async (_params: unknown, _options?: unknown) => {
          if (response.error) {
            throw new Error(response.error.message);
          }
          if (response.stream) {
            return parseSSE(response.body ?? '');
          }
          return JSON.parse(response.body ?? '{}');
        },
      },
    },
  };
}

async function* parseSSE(body: string): AsyncIterable<unknown> {
  for (const line of body.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed.startsWith('data: ')) continue;
    const data = trimmed.slice(6);
    if (data === '[DONE]') break;
    yield JSON.parse(data);
  }
}
```

`packages/integration-tests/test/parity/kosong/openai-golden-runner.test.ts`：

```ts
import { describe, it, expect } from 'vitest';
import {
  runTsKosongOpenAIGolden,
  type Fixture,
} from '../../../src/parity/kosong-openai-golden';

const TEXT_STREAM_BODY =
  'data: {"id":"1","choices":[{"index":0,"delta":{"role":"assistant","content":"Hi"}}]}\n\n' +
  'data: {"id":"2","choices":[{"index":0,"delta":{"content":" there"}}]}\n\n' +
  'data: {"id":"3","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}\n\n' +
  'data: [DONE]\n\n';

const FIXTURE: Fixture = {
  systemPrompt: '',
  history: [
    {
      role: 'user',
      content: [{ type: 'text', text: 'Hello' }],
      toolCalls: [],
    },
  ],
  providerOptions: { model: 'gpt-4o-mini', stream: true },
  response: { status: 200, stream: true, body: TEXT_STREAM_BODY },
};

describe('kosong-openai-golden runner', () => {
  it('merges streamed text', async () => {
    const result = await runTsKosongOpenAIGolden(FIXTURE);
    expect(result.error).toBeNull();
    expect(result.assistantMessage).toMatchObject({
      role: 'assistant',
      content: [{ type: 'text', text: 'Hi there' }],
      toolCalls: [],
    });
  });
});
```

### 验证命令

```bash
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/kosong/openai-golden-runner.test.ts
# expected: 1 passed
```

---

## Task 4: L1 SSE/HTTP fixtures

**Depends on:** Task 2、Task 3
**Files:**
- Create: `packages/integration-tests/src/parity/fixtures/kosong-openai/l1-openai-text.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-openai/l1-openai-thinking.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-openai/l1-openai-tool-call-single.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-openai/l1-openai-tool-call-parallel.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-openai/l1-openai-truncated.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-openai/l1-openai-usage.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-openai/l1-openai-error.json`

### 步骤

- [ ] 创建目录 `packages/integration-tests/src/parity/fixtures/kosong-openai/`。
- [ ] 写入 7 个 fixture 文件，覆盖 roadmap 4.2.2.4 要求的全部场景。
- [ ] 手动验证 Rust binary 能解析 text fixture 并输出 `"Hello world"`。
- [ ] Commit: `test(integration-tests): add OpenAI Legacy L1 golden fixtures`

### 实现代码

`l1-openai-text.json`：

```json
{
  "systemPrompt": "You are helpful.",
  "history": [
    { "role": "user", "content": [{ "type": "text", "text": "Hello" }], "toolCalls": [] }
  ],
  "providerOptions": { "model": "gpt-4o-mini", "stream": true },
  "response": {
    "status": 200,
    "stream": true,
    "body": "data: {\"id\":\"chatcmpl-text\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hello\"}}]}\n\ndata: {\"id\":\"chatcmpl-text\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"}}]}\n\ndata: {\"id\":\"chatcmpl-text\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n"
  }
}
```

`l1-openai-thinking.json`：

```json
{
  "systemPrompt": "",
  "history": [
    { "role": "user", "content": [{ "type": "text", "text": "2+2=?" }], "toolCalls": [] }
  ],
  "providerOptions": { "model": "gpt-4o-mini", "stream": true },
  "response": {
    "status": 200,
    "stream": true,
    "body": "data: {\"id\":\"chatcmpl-think\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"The answer is 4.\",\"reasoning_content\":\"Let's think: 2+2=4.\"}}]}\n\ndata: {\"id\":\"chatcmpl-think\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n"
  }
}
```

`l1-openai-tool-call-single.json`：

```json
{
  "systemPrompt": "",
  "tools": [
    {
      "name": "add",
      "description": "Add two integers.",
      "parameters": {
        "type": "object",
        "properties": { "a": { "type": "integer" }, "b": { "type": "integer" } },
        "required": ["a", "b"]
      }
    }
  ],
  "history": [
    { "role": "user", "content": [{ "type": "text", "text": "Add 1 and 2" }], "toolCalls": [] }
  ],
  "providerOptions": { "model": "gpt-4o-mini", "stream": true },
  "response": {
    "status": 200,
    "stream": true,
    "body": "data: {\"id\":\"chatcmpl-tc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"add\"}}]}}]}\n\ndata: {\"id\":\"chatcmpl-tc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"a\\\":1\"}}]}}]}\n\ndata: {\"id\":\"chatcmpl-tc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"b\\\":2}\"}}]}}]}\n\ndata: {\"id\":\"chatcmpl-tc\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n\n"
  }
}
```

`l1-openai-tool-call-parallel.json`：

```json
{
  "systemPrompt": "",
  "tools": [
    {
      "name": "read",
      "description": "Read a file.",
      "parameters": { "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"] }
    },
    {
      "name": "write",
      "description": "Write a file.",
      "parameters": { "type": "object", "properties": { "path": { "type": "string" }, "content": { "type": "string" } }, "required": ["path", "content"] }
    }
  ],
  "history": [
    { "role": "user", "content": [{ "type": "text", "text": "Read and write" }], "toolCalls": [] }
  ],
  "providerOptions": { "model": "gpt-4o-mini", "stream": true },
  "response": {
    "status": 200,
    "stream": true,
    "body": "data: {\"id\":\"chatcmpl-ptc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_r\",\"type\":\"function\",\"function\":{\"name\":\"read\"}}]}}]}\n\ndata: {\"id\":\"chatcmpl-ptc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":1,\"id\":\"call_w\",\"type\":\"function\",\"function\":{\"name\":\"write\"}}]}}]}\n\ndata: {\"id\":\"chatcmpl-ptc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"path\\\":\"}}]}}]}\n\ndata: {\"id\":\"chatcmpl-ptc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":1,\"function\":{\"arguments\":\"{\\\"path\\\":\"}}]}}]}\n\ndata: {\"id\":\"chatcmpl-ptc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"a.txt\\\"}\"}}]}}]}\n\ndata: {\"id\":\"chatcmpl-ptc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":1,\"function\":{\"arguments\":\"\\\"b.txt\\\",\\\"content\\\":\"}}]}}]}\n\ndata: {\"id\":\"chatcmpl-ptc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":1,\"function\":{\"arguments\":\"\\\"hello\\\"}\"}}]}}]}\n\ndata: {\"id\":\"chatcmpl-ptc\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n\n"
  }
}
```

`l1-openai-truncated.json`：

```json
{
  "systemPrompt": "",
  "history": [
    { "role": "user", "content": [{ "type": "text", "text": "Tell a story" }], "toolCalls": [] }
  ],
  "providerOptions": { "model": "gpt-4o-mini", "stream": true },
  "response": {
    "status": 200,
    "stream": true,
    "body": "data: {\"id\":\"chatcmpl-trunc\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Once upon\"}}]}\n\ndata: {\"id\":\"chatcmpl-trunc\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"length\"}]}\n\ndata: [DONE]\n\n"
  }
}
```

`l1-openai-usage.json`：

```json
{
  "systemPrompt": "",
  "history": [
    { "role": "user", "content": [{ "type": "text", "text": "ok" }], "toolCalls": [] }
  ],
  "providerOptions": { "model": "gpt-4o-mini", "stream": true },
  "response": {
    "status": 200,
    "stream": true,
    "body": "data: {\"id\":\"chatcmpl-usage\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"ok\"}}]}\n\ndata: {\"id\":\"chatcmpl-usage\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: {\"id\":\"chatcmpl-usage\",\"choices\":[],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":1,\"total_tokens\":11}}\n\ndata: [DONE]\n\n"
  }
}
```

`l1-openai-error.json`：

```json
{
  "systemPrompt": "",
  "history": [
    { "role": "user", "content": [{ "type": "text", "text": "Hi" }], "toolCalls": [] }
  ],
  "providerOptions": { "model": "gpt-4o-mini", "stream": false },
  "response": {
    "status": 401,
    "error": { "message": "Incorrect API key provided" }
  },
  "expectError": true
}
```

### 验证命令

```bash
cd rust-ody
cargo build -p kosong-rs --bin kosong-openai-golden
./target/debug/kosong-openai-golden \
  ../packages/integration-tests/src/parity/fixtures/kosong-openai/l1-openai-text.json
# expected: JSON with assistantMessage.content[0].text == "Hello world" and error == null
```

---

## Task 5: L1 对位测试

**Depends on:** Task 2–Task 4
**Files:**
- Create: `packages/integration-tests/test/parity/kosong/l1-openai-golden.test.ts`
- Test: `packages/integration-tests/test/parity/kosong/l1-openai-golden.test.ts`

### 步骤

- [ ] 创建 L1 测试文件，自动发现 `kosong-openai` fixture 目录下的所有 `.json` 文件。
- [ ] 对每个 fixture 同时调用 TS runner 与 Rust binary，排序键后做 `toStrictEqual` 比较。
- [ ] 对 `expectError: true` 的 fixture 仅断言两端都产生非空 `error`。
- [ ] 写好后先运行一次：此时若实现尚未对齐，会看到 fixture 输出差异（预期失败）。
- [ ] 待前面任务全部完成后再次运行，应全部通过。
- [ ] Commit: `test(integration-tests): add OpenAI Legacy L1 golden parity test`

### 实现代码

`packages/integration-tests/test/parity/kosong/l1-openai-golden.test.ts`：

```ts
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { beforeAll, describe, expect, it } from 'vitest';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import {
  runTsKosongOpenAIGolden,
  type Fixture,
} from '../../../src/parity/kosong-openai-golden';

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
  'kosong-openai',
);

const fixtures: Array<{ name: string; expectError: boolean }> = readdirSync(fixturesDir)
  .filter((name) => name.endsWith('.json'))
  .map((name) => {
    const raw = readFileSync(join(fixturesDir, name), 'utf8');
    const parsed: Fixture = JSON.parse(raw);
    return { name, expectError: parsed.expectError ?? false };
  });

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

describe('kosong-openai L1 golden parity', () => {
  beforeAll(() => {
    const binaryPath =
      process.env.ODY_OPENAI_GOLDEN_BINARY_PATH ??
      join(rootDir, 'rust-ody', 'target', 'debug', 'kosong-openai-golden');
    if (existsSync(binaryPath)) return;
    spawnSync('cargo', ['build', '-p', 'kosong-rs', '--bin', 'kosong-openai-golden'], {
      cwd: join(rootDir, 'rust-ody'),
      stdio: 'inherit',
    });
  });

  const binaryPath =
    process.env.ODY_OPENAI_GOLDEN_BINARY_PATH ??
    join(rootDir, 'rust-ody', 'target', 'debug', 'kosong-openai-golden');

  it.each(fixtures)('$name TS matches Rust', async ({ name, expectError }) => {
    const fixture = loadFixture(name);
    const ts = await runTsKosongOpenAIGolden(fixture);
    const result = spawnSync(binaryPath, [join(fixturesDir, name)], { encoding: 'utf8' });
    if (result.status !== 0) {
      throw new Error(`kosong-openai-golden exited ${result.status}: ${result.stderr}`);
    }
    const rust = JSON.parse(result.stdout);

    if (expectError) {
      expect(ts.error).toBeTruthy();
      expect(rust.error).toBeTruthy();
      return;
    }

    expect(sortKeys(rust)).toStrictEqual(sortKeys(ts));
  });
});
```

### 验证命令

```bash
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/kosong/l1-openai-golden.test.ts
# expected: 7 passed
pnpm --filter @odysseythink/integration-tests run typecheck
# expected: clean
```

---

## Task 6: 集成脚本与 CI

**Depends on:** Task 5
**Files:**
- Modify: `packages/integration-tests/package.json:17-25`
- Modify: `package.json:13-15`
- Modify: `.github/workflows/rust-host.yml:54-58`

### 步骤

- [ ] 在 `packages/integration-tests/package.json` 增加 `test:parity:kosong:openai` 脚本。
- [ ] 在根 `package.json` 增加同名便捷脚本。
- [ ] 在 `.github/workflows/rust-host.yml` 的 `kaos L1 golden parity` 步骤后增加 build + parity 步骤。
- [ ] 运行脚本验证 7 个 fixture 全部绿。
- [ ] Commit: `ci: add kosong-openai L1 golden parity job`

### 实现代码

`packages/integration-tests/package.json` 脚本区变更：

```json
"scripts": {
  "test": "vitest run",
  "test:parity": "vitest run test/parity",
  "test:parity:kaos": "vitest run test/parity/kaos",
  "test:parity:kosong:openai": "vitest run test/parity/kosong/l1-openai-golden.test.ts",
  "test:parity:ts-vs-ts": "vitest run test/parity/ts-vs-ts.test.ts",
  "test:parity:ts-vs-rust": "vitest run test/parity/ts-vs-rust.test.ts",
  "typecheck": "tsc -p tsconfig.json --noEmit",
  "clean": "rm -rf dist"
}
```

根 `package.json` 脚本区变更（在 `test:parity:ts-vs-rust` 后新增一行，并补逗号）：

```json
"test:parity": "pnpm --filter integration-tests test:parity",
"test:parity:ts-vs-ts": "pnpm --filter integration-tests test:parity:ts-vs-ts",
"test:parity:ts-vs-rust": "pnpm --filter integration-tests test:parity:ts-vs-rust",
"test:parity:kosong:openai": "pnpm --filter integration-tests test:parity:kosong:openai",
"proto:rust-host": "pnpm run build:host && ODY_HOST_BINARY_PATH=$(pwd)/rust-ody/target/release/ody-host pnpm -C apps/ody-code run dev:cli-only --host=rust --host-stdio"
```

`.github/workflows/rust-host.yml` 变更（插入到 `kaos L1 golden parity` 步骤之后）：

```yaml
      - name: Build kosong-openai-golden binary
        run: cargo build -p kosong-rs --bin kosong-openai-golden
        working-directory: rust-ody

      - name: kosong OpenAI Legacy L1 golden parity
        run: pnpm --filter @odysseythink/integration-tests test:parity:kosong:openai
        env:
          ODY_OPENAI_GOLDEN_BINARY_PATH: ${{ github.workspace }}/rust-ody/target/debug/kosong-openai-golden
```

### 验证命令

```bash
pnpm run test:parity:kosong:openai
# expected: 7 passed
pnpm -r typecheck
# expected: clean
```

---

## Local Self-Review

- [ ] 1. Spec-coverage table（对应 roadmap 4.2.2 与 G4-2-2）：
  | 条目 | 覆盖任务 | 状态 |
  |---|---|---|
  | 4.2.2.4 L1 SSE fixture（纯文本） | Task 4 `l1-openai-text.json` + Task 5 | covered |
  | 4.2.2.4 L1 SSE fixture（thinking） | Task 4 `l1-openai-thinking.json` + Task 5 | covered |
  | 4.2.2.4 L1 SSE fixture（单 tool-call） | Task 4 `l1-openai-tool-call-single.json` + Task 5 | covered |
  | 4.2.2.4 L1 SSE fixture（并行 tool-calls） | Task 4 `l1-openai-tool-call-parallel.json` + Task 5 | covered |
  | 4.2.2.4 L1 SSE fixture（截断） | Task 4 `l1-openai-truncated.json` + Task 5 | covered |
  | 4.2.2.4 L1 SSE fixture（usage） | Task 4 `l1-openai-usage.json` + Task 5 | covered |
  | 4.2.2.4 L1 SSE fixture（错误） | Task 4 `l1-openai-error.json` + Task 5 | covered |
  | G4-2-2 OpenAI Legacy 全部 SSE fixture L1 绿 | Task 5 + Task 6 | covered |
  | G4-2-2 Chat Completions 共享解析器被后续复用 | `core.md` Task 3/4，本 part 通过 L1 锁定行为 | covered |
- [ ] 2. Placeholder scan：`parity.md` 无 `TODO`/`TBD`，所有代码、fixture、命令均给出完整内容。
- [ ] 3. No phantom tasks：Task 1–6 每个都产生可验证的文件改动；无 `--allow-empty` 或 "already done in Task N"。
- [ ] 4. Dependency soundness：Task 2 依赖 Task 1 的导出；Task 3 依赖 Task 2 的 fixture schema；Task 4 依赖 Task 2/3；Task 5 依赖 Task 2–4；Task 6 依赖 Task 5。
- [ ] 5. Caller & build soundness：本 part 未修改任何跨 crate/跨 package 的共享签名；新增符号仅在本 crate / integration-tests 内部使用。Task 6 在 CI 与本地脚本中引用路径与 binary 名一致（`kosong-openai-golden`），且通过 `ODY_OPENAI_GOLDEN_BINARY_PATH` 统一。
- [ ] 6. Test-the-risk：
  - `kosong-openai-golden` binary 自带两个单元测试验证纯文本流与错误路径；
  - TS runner 自带单元测试验证 SSE 文本合并；
  - L1 测试对每个 fixture 同时断言 TS 与 Rust 输出（含 thinking、tool-call、并行 delta、truncated、usage、错误）。
- [ ] 7. Type consistency：
  - Rust fixture schema 中的 `systemPrompt`、`providerOptions.model`、`response.stream`、`response.body` 与 TS `Fixture` 接口字段名、类型一致；
  - `expectError` 在两端均作为可选布尔处理；
  - 错误 fixture 中 TS runner 抛出的 `Error` 与 Rust `ChatProviderError` 在 L1 测试中仅比较存在性，不比较格式。
