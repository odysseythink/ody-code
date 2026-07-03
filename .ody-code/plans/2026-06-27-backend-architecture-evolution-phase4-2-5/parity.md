# Part 5: L1 Golden Fixtures + TS↔Rust Parity

**Scope:** Add provider-specific golden binaries for Kimi, DeepSeek, and GLM in `rust-ody/crates/kosong-rs`, TS mock runners in `packages/integration-tests/src/parity`, fixture JSON files, and vitest parity suites. Each provider gets at least one streaming and one error fixture; the test compares the parsed Rust `assistantMessage`/`error` against the TS `generate()` output.

**Depends on:** Parts 2–4 (Kimi, DeepSeek, GLM providers and their public exports). No new provider logic is introduced here — only golden harness wiring and fixtures.

---

## Task 5.1: Kimi golden binary + fixtures + TS runner + parity test

**Depends on:** Part 2 (`kimi.md`) — `KimiChatProvider`, `KimiOptions`, and `get_kimi_model_capability` must be exported from `kosong_rs`.

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/bin/kimi_golden.rs`
- Create: `packages/integration-tests/src/parity/kosong-kimi-golden.ts`
- Create: `packages/integration-tests/test/parity/kosong/l1-kimi-golden.test.ts`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-kimi/l1-kimi-text-stream.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-kimi/l1-kimi-tool-stream.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong-kimi/l1-kimi-error.json`
- Modify: `rust-ody/crates/kosong-rs/Cargo.toml` (add `[[bin]]` for `kosong-kimi-golden`)
- Test: `packages/integration-tests/test/parity/kosong/l1-kimi-golden.test.ts`

### Fixture shape

```typescript
export interface Fixture {
  systemPrompt?: string;
  tools?: Tool[];
  history: Message[];
  options?: { auth?: { apiKey?: string; headers?: Record<string, string> } };
  providerOptions: {
    model: string;
    apiKey?: string;
    baseUrl?: string;
    stream?: boolean;
    maxTokens?: number;
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
```

- [ ] **Create the Rust golden binary.** Write `rust-ody/crates/kosong-rs/src/bin/kimi_golden.rs`:

```rust
use std::env;
use std::fs;
use std::sync::Arc;

use kosong_rs::{
    generate, ChatProviderError, GenerateOptions, KimiChatProvider, KimiOptions, Message,
    MockHttpClient, ProviderRequestAuth, Tool,
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
    max_tokens: Option<i64>,
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
    #[serde(default)]
    code: Option<String>,
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

async fn run_fixture(fixture: Fixture) -> Result<kosong_rs::message::Message, ChatProviderError> {
    let response_bytes = build_response_bytes(&fixture.response);
    let client = Arc::new(MockHttpClient::new(fixture.response.status, response_bytes));
    let options = KimiOptions {
        api_key: Some(fixture.provider_options.api_key.unwrap_or_else(|| "sk-test".into())),
        base_url: Some(fixture.provider_options.base_url.unwrap_or_else(|| "http://mock".into())),
        model: fixture.provider_options.model,
        stream: Some(fixture.provider_options.stream),
        max_tokens: fixture.provider_options.max_tokens,
        default_headers: None,
        http_client: Some(client),
        reasoning_key: fixture.provider_options.reasoning_key,
    };
    let provider = KimiChatProvider::new(options);
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
            "error": { "message": error.message, "type": "invalid_request_error", "code": error.code }
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
```

- [ ] **Register the binary in `Cargo.toml`.** Append:

```toml
[[bin]]
name = "kosong-kimi-golden"
path = "src/bin/kimi_golden.rs"
```

- [ ] **Create the TS runner.** Write `packages/integration-tests/src/parity/kosong-kimi-golden.ts`:

```typescript
import { generate } from '@odysseythink/kosong';
import type { Message, Tool } from '@odysseythink/kosong';
import { KimiChatProvider } from '@odysseythink/kosong/providers/kimi';
import type { ProviderRequestAuth } from '@odysseythink/kosong';
import type OpenAI from 'openai';

export interface Fixture {
  systemPrompt?: string;
  tools?: Tool[];
  history: Message[];
  options?: { auth?: ProviderRequestAuth };
  providerOptions: {
    model: string;
    apiKey?: string;
    baseUrl?: string;
    stream?: boolean;
    maxTokens?: number;
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

type ClientFactory = NonNullable<ConstructorParameters<typeof KimiChatProvider>[0]['clientFactory']>;
type KimiClient = ReturnType<ClientFactory>;

export async function runTsKosongKimiGolden(fixture: Fixture): Promise<GoldenResult> {
  const provider = new KimiChatProvider({
    model: fixture.providerOptions.model,
    apiKey: fixture.providerOptions.apiKey ?? 'sk-test',
    baseUrl: fixture.providerOptions.baseUrl ?? 'http://mock',
    stream: fixture.providerOptions.stream ?? true,
    maxTokens: fixture.providerOptions.maxTokens,
    reasoningKey: fixture.providerOptions.reasoningKey,
    clientFactory: () => createMockClient(fixture.response) as unknown as KimiClient,
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
    return { assistantMessage: null, error: e instanceof Error ? e.message : String(e) };
  }
}

function createMockClient(response: Fixture['response']): unknown {
  return {
    chat: {
      completions: {
        create: async (_params: unknown, _options?: unknown) => {
          if (response.error) return {};
          if (response.stream) return parseSSE(response.body ?? '');
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

- [ ] **Create fixtures.**

`packages/integration-tests/src/parity/fixtures/kosong-kimi/l1-kimi-text-stream.json`:

```json
{
  "systemPrompt": "You are helpful.",
  "history": [
    { "role": "user", "content": [{ "type": "text", "text": "Hello" }], "toolCalls": [] }
  ],
  "providerOptions": { "model": "kimi-k2", "stream": true, "maxTokens": 512 },
  "response": {
    "status": 200,
    "stream": true,
    "body": "data: {\"id\":\"chatcmpl-kimi-text\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hello\"}}]}\n\ndata: {\"id\":\"chatcmpl-kimi-text\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"}}]}\n\ndata: {\"id\":\"chatcmpl-kimi-text\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n"
  }
}
```

`packages/integration-tests/src/parity/fixtures/kosong-kimi/l1-kimi-tool-stream.json`:

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
  "providerOptions": { "model": "kimi-k2", "stream": true },
  "response": {
    "status": 200,
    "stream": true,
    "body": "data: {\"id\":\"chatcmpl-kimi-tc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"add\"}}]}}]}\n\ndata: {\"id\":\"chatcmpl-kimi-tc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"a\\\":1\"}}]}}]}\n\ndata: {\"id\":\"chatcmpl-kimi-tc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"b\\\":2}\"}}]}}]}\n\ndata: {\"id\":\"chatcmpl-kimi-tc\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n\n"
  }
}
```

`packages/integration-tests/src/parity/fixtures/kosong-kimi/l1-kimi-error.json`:

```json
{
  "systemPrompt": "",
  "history": [
    { "role": "user", "content": [{ "type": "text", "text": "Hi" }], "toolCalls": [] }
  ],
  "providerOptions": { "model": "kimi-k2", "stream": true },
  "response": {
    "status": 401,
    "error": { "message": "Invalid auth", "code": "invalid_api_key" }
  },
  "expectError": true
}
```

- [ ] **Create the parity test.** Write `packages/integration-tests/test/parity/kosong/l1-kimi-golden.test.ts`:

```typescript
import { existsSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { beforeAll, describe, expect, it } from 'vitest';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { runTsKosongKimiGolden, type Fixture } from '../../../src/parity/kosong-kimi-golden';

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
  'kosong-kimi',
);
const fixtures: Array<{ name: string; expectError: boolean }> = [
  { name: 'l1-kimi-text-stream.json', expectError: false },
  { name: 'l1-kimi-tool-stream.json', expectError: false },
  { name: 'l1-kimi-error.json', expectError: true },
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

describe('kosong-kimi L1 golden parity', () => {
  beforeAll(() => {
    const binaryPath = process.env.ODY_KIMI_GOLDEN_BINARY_PATH ?? join(rootDir, 'rust-ody', 'target', 'debug', 'kosong-kimi-golden');
    if (existsSync(binaryPath)) return;
    spawnSync('cargo', ['build', '-p', 'kosong-rs', '--bin', 'kosong-kimi-golden'], {
      cwd: join(rootDir, 'rust-ody'),
      stdio: 'inherit',
    });
  });

  const binaryPath = process.env.ODY_KIMI_GOLDEN_BINARY_PATH ?? join(rootDir, 'rust-ody', 'target', 'debug', 'kosong-kimi-golden');

  it.each(fixtures)('$name TS matches Rust', async ({ name, expectError }) => {
    const fixture = loadFixture(name);
    const ts = await runTsKosongKimiGolden(fixture);
    const result = spawnSync(binaryPath, [join(fixturesDir, name)], { encoding: 'utf8' });

    if (expectError) {
      expect(ts.error).toBeTruthy();
      if (result.status === 0) {
        const rust = JSON.parse(result.stdout);
        expect(rust.error).toBeTruthy();
      }
      return;
    }

    if (result.status !== 0) {
      throw new Error(`kosong-kimi-golden exited ${result.status}: ${result.stderr}`);
    }
    const rust = JSON.parse(result.stdout);
    expect(sortKeys(rust)).toStrictEqual(sortKeys(ts));
  });
});
```

- [ ] **Build and run the parity test.**

```bash
cd /Users/ranwei/workspace/ody-code && pnpm --filter @odysseythink/integration-tests test:parity -- test/parity/kosong/l1-kimi-golden.test.ts
```

Expected: 3 passing tests.

- [ ] **Commit.**

```bash
git add rust-ody/crates/kosong-rs/src/bin/kimi_golden.rs rust-ody/crates/kosong-rs/Cargo.toml packages/integration-tests/src/parity/kosong-kimi-golden.ts packages/integration-tests/test/parity/kosong/l1-kimi-golden.test.ts packages/integration-tests/src/parity/fixtures/kosong-kimi/
git commit -m "feat(kosong-rs): add Kimi L1 golden parity fixtures and harness"
```

---

## Task 5.2: DeepSeek golden binary + fixtures + TS runner + parity test

**Depends on:** Part 3 (`deepseek.md`) — `DeepSeekChatProvider`, `DeepSeekOptions` exported from `kosong_rs`.

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/bin/deepseek_golden.rs`
- Create: `packages/integration-tests/src/parity/kosong-deepseek-golden.ts`
- Create: `packages/integration-tests/test/parity/kosong/l1-deepseek-golden.test.ts`
- Create:
  - `packages/integration-tests/src/parity/fixtures/kosong-deepseek/l1-deepseek-text-stream.json`
  - `packages/integration-tests/src/parity/fixtures/kosong-deepseek/l1-deepseek-tool-stream.json`
  - `packages/integration-tests/src/parity/fixtures/kosong-deepseek/l1-deepseek-error.json`
- Modify: `rust-ody/crates/kosong-rs/Cargo.toml` (add `[[bin]]` for `kosong-deepseek-golden`)
- Test: `packages/integration-tests/test/parity/kosong/l1-deepseek-golden.test.ts`

- [ ] **Create the Rust golden binary.** Write `rust-ody/crates/kosong-rs/src/bin/deepseek_golden.rs`:

```rust
use std::env;
use std::fs;
use std::sync::Arc;

use kosong_rs::{
    generate, ChatProviderError, DeepSeekChatProvider, DeepSeekOptions, GenerateOptions, Message,
    MockHttpClient, ProviderRequestAuth, Tool,
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
    #[serde(default)]
    code: Option<String>,
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

async fn run_fixture(
    fixture: Fixture,
) -> Result<kosong_rs::message::Message, ChatProviderError> {
    let response_bytes = build_response_bytes(&fixture.response);
    let client = Arc::new(MockHttpClient::new(fixture.response.status, response_bytes));
    let options = DeepSeekOptions {
        api_key: Some(fixture.provider_options.api_key.unwrap_or_else(|| "sk-test".into())),
        base_url: Some(fixture.provider_options.base_url.unwrap_or_else(|| "http://mock".into())),
        model: fixture.provider_options.model,
        stream: Some(fixture.provider_options.stream),
        http_client: Some(client),
    };
    let provider = DeepSeekChatProvider::new(options);
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
            "error": { "message": error.message, "type": "invalid_request_error", "code": error.code }
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
```

- [ ] **Register the binary in `Cargo.toml`.** Append:

```toml
[[bin]]
name = "kosong-deepseek-golden"
path = "src/bin/deepseek_golden.rs"
```

- [ ] **Create the TS runner.** Write `packages/integration-tests/src/parity/kosong-deepseek-golden.ts`:

```typescript
import { generate } from '@odysseythink/kosong';
import type { Message, Tool } from '@odysseythink/kosong';
import { DeepSeekChatProvider } from '@odysseythink/kosong/providers/deepseek';
import type { ProviderRequestAuth } from '@odysseythink/kosong';
import type OpenAI from 'openai';

export interface Fixture {
  systemPrompt?: string;
  tools?: Tool[];
  history: Message[];
  options?: { auth?: ProviderRequestAuth };
  providerOptions: {
    model: string;
    apiKey?: string;
    baseUrl?: string;
    stream?: boolean;
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

type ClientFactory = NonNullable<ConstructorParameters<typeof DeepSeekChatProvider>[0]['clientFactory']>;
type DeepSeekClient = ReturnType<ClientFactory>;

export async function runTsKosongDeepSeekGolden(fixture: Fixture): Promise<GoldenResult> {
  const provider = new DeepSeekChatProvider({
    model: fixture.providerOptions.model,
    apiKey: fixture.providerOptions.apiKey ?? 'sk-test',
    baseUrl: fixture.providerOptions.baseUrl ?? 'http://mock',
    stream: fixture.providerOptions.stream ?? true,
    clientFactory: () => createMockClient(fixture.response) as unknown as DeepSeekClient,
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
    return { assistantMessage: null, error: e instanceof Error ? e.message : String(e) };
  }
}

function createMockClient(response: Fixture['response']): unknown {
  return {
    chat: {
      completions: {
        create: async (_params: unknown, _options?: unknown) => {
          if (response.error) return {};
          if (response.stream) return parseSSE(response.body ?? '');
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

- [ ] **Create fixtures.**

`packages/integration-tests/src/parity/fixtures/kosong-deepseek/l1-deepseek-text-stream.json`:

```json
{
  "systemPrompt": "You are helpful.",
  "history": [
    { "role": "user", "content": [{ "type": "text", "text": "Hello" }], "toolCalls": [] }
  ],
  "providerOptions": { "model": "deepseek-chat", "stream": true },
  "response": {
    "status": 200,
    "stream": true,
    "body": "data: {\"id\":\"chatcmpl-ds-text\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hello\"}}]}\n\ndata: {\"id\":\"chatcmpl-ds-text\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"}}]}\n\ndata: {\"id\":\"chatcmpl-ds-text\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n"
  }
}
```

`packages/integration-tests/src/parity/fixtures/kosong-deepseek/l1-deepseek-tool-stream.json`:

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
  "providerOptions": { "model": "deepseek-chat", "stream": true },
  "response": {
    "status": 200,
    "stream": true,
    "body": "data: {\"id\":\"chatcmpl-ds-tc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"add\"}}]}}]}\n\ndata: {\"id\":\"chatcmpl-ds-tc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"a\\\":1\"}}]}}]}\n\ndata: {\"id\":\"chatcmpl-ds-tc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"b\\\":2}\"}}]}}]}\n\ndata: {\"id\":\"chatcmpl-ds-tc\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n\n"
  }
}
```

`packages/integration-tests/src/parity/fixtures/kosong-deepseek/l1-deepseek-error.json`:

```json
{
  "systemPrompt": "",
  "history": [
    { "role": "user", "content": [{ "type": "text", "text": "Hi" }], "toolCalls": [] }
  ],
  "providerOptions": { "model": "deepseek-chat", "stream": true },
  "response": {
    "status": 401,
    "error": { "message": "Invalid auth", "code": "invalid_api_key" }
  },
  "expectError": true
}
```

- [ ] **Create the parity test.** Write `packages/integration-tests/test/parity/kosong/l1-deepseek-golden.test.ts` (mirror Kimi test with `kosong-deepseek-golden` binary and `runTsKosongDeepSeekGolden`).

- [ ] **Run the parity test.**

```bash
cd /Users/ranwei/workspace/ody-code && pnpm --filter @odysseythink/integration-tests test:parity -- test/parity/kosong/l1-deepseek-golden.test.ts
```

Expected: 3 passing tests.

- [ ] **Commit.**

```bash
git add rust-ody/crates/kosong-rs/src/bin/deepseek_golden.rs rust-ody/crates/kosong-rs/Cargo.toml packages/integration-tests/src/parity/kosong-deepseek-golden.ts packages/integration-tests/test/parity/kosong/l1-deepseek-golden.test.ts packages/integration-tests/src/parity/fixtures/kosong-deepseek/
git commit -m "feat(kosong-rs): add DeepSeek L1 golden parity fixtures and harness"
```

---

## Task 5.3: GLM golden binary + fixtures + TS runner + parity test

**Depends on:** Part 4 (`glm.md`) — `GLMChatProvider`, `GLMOptions` exported from `kosong_rs`.

**Files:**
- Create: `rust-ody/crates/kosong-rs/src/bin/glm_golden.rs`
- Create: `packages/integration-tests/src/parity/kosong-glm-golden.ts`
- Create: `packages/integration-tests/test/parity/kosong/l1-glm-golden.test.ts`
- Create:
  - `packages/integration-tests/src/parity/fixtures/kosong-glm/l1-glm-text-stream.json`
  - `packages/integration-tests/src/parity/fixtures/kosong-glm/l1-glm-tool-stream.json`
  - `packages/integration-tests/src/parity/fixtures/kosong-glm/l1-glm-error.json`
- Modify: `rust-ody/crates/kosong-rs/Cargo.toml` (add `[[bin]]` for `kosong-glm-golden`)
- Test: `packages/integration-tests/test/parity/kosong/l1-glm-golden.test.ts`

- [ ] **Create the Rust golden binary.** Write `rust-ody/crates/kosong-rs/src/bin/glm_golden.rs`:

```rust
use std::env;
use std::fs;
use std::sync::Arc;

use kosong_rs::{
    generate, ChatProviderError, GenerateOptions, GLMChatProvider, GLMOptions, Message,
    MockHttpClient, ProviderRequestAuth, Tool,
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
    max_tokens: Option<i64>,
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
    #[serde(default)]
    code: Option<String>,
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

async fn run_fixture(fixture: Fixture) -> Result<kosong_rs::message::Message, ChatProviderError> {
    let response_bytes = build_response_bytes(&fixture.response);
    let client = Arc::new(MockHttpClient::new(fixture.response.status, response_bytes));
    let options = GLMOptions {
        api_key: Some(fixture.provider_options.api_key.unwrap_or_else(|| "sk-test".into())),
        base_url: Some(fixture.provider_options.base_url.unwrap_or_else(|| "http://mock".into())),
        model: fixture.provider_options.model,
        stream: Some(fixture.provider_options.stream),
        max_tokens: fixture.provider_options.max_tokens,
        default_headers: None,
        http_client: Some(client),
    };
    let provider = GLMChatProvider::new(options);
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
            "error": { "message": error.message, "type": "invalid_request_error", "code": error.code }
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
```

- [ ] **Register the binary in `Cargo.toml`.** Append:

```toml
[[bin]]
name = "kosong-glm-golden"
path = "src/bin/glm_golden.rs"
```

- [ ] **Create the TS runner.** Write `packages/integration-tests/src/parity/kosong-glm-golden.ts`:

```typescript
import { generate } from '@odysseythink/kosong';
import type { Message, Tool } from '@odysseythink/kosong';
import { GLMChatProvider } from '@odysseythink/kosong/providers/glm';
import type { ProviderRequestAuth } from '@odysseythink/kosong';

export interface Fixture {
  systemPrompt?: string;
  tools?: Tool[];
  history: Message[];
  options?: { auth?: ProviderRequestAuth };
  providerOptions: {
    model: string;
    apiKey?: string;
    baseUrl?: string;
    stream?: boolean;
    maxTokens?: number;
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

export async function runTsKosongGLMGolden(fixture: Fixture): Promise<GoldenResult> {
  const provider = new GLMChatProvider({
    model: fixture.providerOptions.model,
    apiKey: fixture.providerOptions.apiKey ?? 'sk-test',
    baseUrl: fixture.providerOptions.baseUrl ?? 'http://mock',
    stream: fixture.providerOptions.stream ?? true,
    maxTokens: fixture.providerOptions.maxTokens,
    httpClient: createMockHttpClient(fixture.response),
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
    return { assistantMessage: null, error: e instanceof Error ? e.message : String(e) };
  }
}

function createMockHttpClient(response: Fixture['response']): unknown {
  return {
    fetch: async (_url: string, _init: unknown): Promise<Response> => {
      if (response.error) {
        return new Response(
          JSON.stringify({ error: { message: response.error.message, code: response.error.code } }),
          { status: response.status, headers: { 'Content-Type': 'application/json' } },
        );
      }
      const body = response.body ?? '';
      return new Response(body, {
        status: response.status,
        headers: { 'Content-Type': response.stream ? 'text/event-stream' : 'application/json' },
      });
    },
  };
}
```

- [ ] **Create fixtures.**

`packages/integration-tests/src/parity/fixtures/kosong-glm/l1-glm-text-stream.json`:

```json
{
  "systemPrompt": "You are helpful.",
  "history": [
    { "role": "user", "content": [{ "type": "text", "text": "Hello" }], "toolCalls": [] }
  ],
  "providerOptions": { "model": "glm-4-plus", "stream": true, "maxTokens": 512 },
  "response": {
    "status": 200,
    "stream": true,
    "body": "data: {\"id\":\"chatcmpl-glm-text\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hello\"}}]}\n\ndata: {\"id\":\"chatcmpl-glm-text\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"}}]}\n\ndata: {\"id\":\"chatcmpl-glm-text\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n"
  }
}
```

`packages/integration-tests/src/parity/fixtures/kosong-glm/l1-glm-tool-stream.json`:

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
  "providerOptions": { "model": "glm-4-plus", "stream": true },
  "response": {
    "status": 200,
    "stream": true,
    "body": "data: {\"id\":\"chatcmpl-glm-tc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"add\"}}]}}]}\n\ndata: {\"id\":\"chatcmpl-glm-tc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"a\\\":1\"}}]}}]}\n\ndata: {\"id\":\"chatcmpl-glm-tc\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"b\\\":2}\"}}]}}]}\n\ndata: {\"id\":\"chatcmpl-glm-tc\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\ndata: [DONE]\n\n"
  }
}
```

`packages/integration-tests/src/parity/fixtures/kosong-glm/l1-glm-error.json`:

```json
{
  "systemPrompt": "",
  "history": [
    { "role": "user", "content": [{ "type": "text", "text": "Hi" }], "toolCalls": [] }
  ],
  "providerOptions": { "model": "glm-4-plus", "stream": true },
  "response": {
    "status": 401,
    "error": { "message": "Invalid auth", "code": "invalid_api_key" }
  },
  "expectError": true
}
```

- [ ] **Create the parity test.** Write `packages/integration-tests/test/parity/kosong/l1-glm-golden.test.ts` (mirror Kimi test with `kosong-glm-golden` binary and `runTsKosongGLMGolden`).

- [ ] **Run the parity test.**

```bash
cd /Users/ranwei/workspace/ody-code && pnpm --filter @odysseythink/integration-tests test:parity -- test/parity/kosong/l1-glm-golden.test.ts
```

Expected: 3 passing tests.

- [ ] **Commit.**

```bash
git add rust-ody/crates/kosong-rs/src/bin/glm_golden.rs rust-ody/crates/kosong-rs/Cargo.toml packages/integration-tests/src/parity/kosong-glm-golden.ts packages/integration-tests/test/parity/kosong/l1-glm-golden.test.ts packages/integration-tests/src/parity/fixtures/kosong-glm/
git commit -m "feat(kosong-rs): add GLM L1 golden parity fixtures and harness"
```

---

## Task 5.4: Whole-tree verification and integration scripts

**Depends on:** Tasks 5.1, 5.2, 5.3.

**Files:**
- Modify: `packages/integration-tests/package.json`
- Test: whole-tree `cargo check --workspace --tests` and `pnpm -r typecheck`

- [ ] **Add convenience parity scripts.** In `packages/integration-tests/package.json`, add under `scripts`:

```json
"test:parity:kosong:kimi": "vitest run test/parity/kosong/l1-kimi-golden.test.ts",
"test:parity:kosong:deepseek": "vitest run test/parity/kosong/l1-deepseek-golden.test.ts",
"test:parity:kosong:glm": "vitest run test/parity/kosong/l1-glm-golden.test.ts",
```

- [ ] **Run Rust whole-tree typecheck.**

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody && cargo check --workspace --tests
```

Expected: `Finished dev [unoptimized + debuginfo] target(s) in ...`.

- [ ] **Run TypeScript whole-tree typecheck.**

```bash
cd /Users/ranwei/workspace/ody-code && pnpm -r typecheck
```

Expected: no `tsc` errors (note: this is a whole-tree check per the shared-signature rule).

- [ ] **Run all new parity suites together.**

```bash
cd /Users/ranwei/workspace/ody-code && pnpm --filter @odysseythink/integration-tests test:parity -- test/parity/kosong/l1-kimi-golden.test.ts test/parity/kosong/l1-deepseek-golden.test.ts test/parity/kosong/l1-glm-golden.test.ts
```

Expected: 9 passing tests.

- [ ] **Commit.**

```bash
git add packages/integration-tests/package.json
git commit -m "chore(integration-tests): parity scripts for kimi/deepseek/glm golden suites"
```

---

## Local Self-Review (Part 5)

- [ ] 1. **Spec-coverage table:**
  | Requirement | Covered by |
  |---|---|
  | Kimi streaming text parity | Task 5.1 fixture + test |
  | Kimi streaming tool-call parity | Task 5.1 fixture + test |
  | Kimi error-path parity | Task 5.1 fixture + test |
  | DeepSeek streaming text parity | Task 5.2 fixture + test |
  | DeepSeek streaming tool-call parity | Task 5.2 fixture + test |
  | DeepSeek error-path parity | Task 5.2 fixture + test |
  | GLM streaming text parity | Task 5.3 fixture + test |
  | GLM streaming tool-call parity | Task 5.3 fixture + test |
  | GLM error-path parity | Task 5.3 fixture + test |
  | Rust golden binaries wired in Cargo.toml | Tasks 5.1–5.3 + 5.4 |
  | Whole-tree Rust typecheck | Task 5.4 |
  | Whole-tree TypeScript typecheck | Task 5.4 |

- [ ] 2. **Placeholder scan:** No TODO/TBD; all code, fixture JSON, and test paths are explicit.
- [ ] 3. **No phantom tasks:** Every task produces files, passing tests, or build verification.
- [ ] 4. **Dependency soundness:** Task 5.1/5.2/5.3 depend on provider implementation in earlier parts; Task 5.4 depends on 5.1–5.3.
- [ ] 5. **Caller & build soundness:** `Cargo.toml` shared-signature change (adding `[[bin]]` entries) does not affect existing callers; whole-tree typechecks cover Rust and TS.
- [ ] 6. **Test-the-risk:** Each parity test asserts behavioral equality of `assistantMessage` and `error` between TS and Rust for streaming text, tool calls, and HTTP errors.
- [ ] 7. **Type consistency:** Fixture interfaces, Rust `*Options` structs, and binary names match provider definitions from Parts 2–4.
