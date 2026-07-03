# Part 3: 黄金对照测试（Golden Parity）

**范围：** 搭建 Rust `kosong-golden` 可执行文件、TS 黄金运行器、L1 黄金 fixture，以及端到端 TS↔Rust 对照测试，验证 `kosong-rs` 的 `generate()` 与 TS `kosong` 行为一致。

---

## Local Dependency Overview

```
Task 9  kosong-golden binary
       │
       ▼
Task 10 TS kosong-golden runner
       │
       ▼
Task 11 L1 golden fixtures
       │
       ▼
Task 12 TS↔Rust parity test suite
```

- Part 3 依赖 Part 1（消息/错误/Provider 类型）和 Part 2（`generate()` 循环、`MockProvider`）。
- Task 9 与 Task 10 可并行，但 Task 11/12 必须顺序执行。

---

### Task 9: 创建 `kosong-golden` Rust 可执行文件

**Depends on:** `generate-loop.md`: Task 5–8
**Files:**
- Create: `rust-ody/crates/kosong-rs/src/bin/golden.rs`
- Modify: `rust-ody/crates/kosong-rs/Cargo.toml`
- Test: `cargo run -p kosong-rs --bin kosong-golden -- /tmp/test.json`

#### Background

`kaos-rs` 已提供黄金模式：一个独立 binary 接收 fixture 文件路径作为参数，输出 JSON 结果。Part 3 为 `kosong-rs` 复制同一模式，使得 TS 侧 parity runner 可以通过文件调用 Rust binary 并比较输出。

#### Fixture Format

每个 fixture 是单个 `generate()` 调用的输入与预期：

```json
{
  "systemPrompt": "",
  "tools": [],
  "history": [{ "role": "user", "content": [{ "type": "text", "text": "hi" }] }],
  "options": {},
  "providerStep": {
    "id": "resp_1",
    "parts": [{ "type": "text", "text": "hello" }],
    "usage": { "inputOther": 10, "output": 8, "inputCacheRead": 0, "inputCacheCreation": 0 },
    "finishReason": "completed",
    "rawFinishReason": "stop"
  },
  "expectError": null
}
```

字段说明：
- `systemPrompt`: 系统提示，对应 `generate()` 第 2 参数。
- `tools`: `Tool[]` 扁平对象数组（`{name, description, parameters}`）。
- `history`: `Message[]`，与 TS JSON shape 一致。
- `options`: 仅 serializable 字段，目前支持 `auth`；`signal`/`onRequestStart`/`onStreamEnd` 不在 fixture 中覆盖。
- `providerStep`: 构造 `MockProvider` 的一次产出；`finishReason` 使用归一化值（`completed`/`tool_calls`/...）。
- `expectError`: 非空时期望 `generate()` 抛出错误（parity 测试只断言双方都有错误，不比较 message）。

#### Steps

- [ ] 在 `Cargo.toml` 中添加 `[[bin]]` 条目：

```toml
[[bin]]
name = "kosong-golden"
path = "src/bin/golden.rs"
```

- [ ] 创建 `rust-ody/crates/kosong-rs/src/bin/golden.rs`：

```rust
use std::env;
use std::fs;

use kosong_rs::{
    generate, GenerateOptions, Message, MockProvider, StreamedMessagePart, TokenUsage,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixture {
    system_prompt: Option<String>,
    tools: Option<Value>,
    history: Vec<Message>,
    options: Option<GenerateOptionsFixture>,
    provider_step: ProviderStep,
    expect_error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerateOptionsFixture {
    auth: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderStep {
    id: Option<String>,
    parts: Vec<StreamedMessagePart>,
    usage: Option<TokenUsage>,
    finish_reason: Option<kosong_rs::provider::FinishReason>,
    raw_finish_reason: Option<String>,
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
    let input = fs::read_to_string(&path)?;
    let fixture: Fixture = serde_json::from_str(&input)?;

    let mut provider = MockProvider::new("mock", "m1").with_parts(fixture.provider_step.parts);
    if let Some(id) = fixture.provider_step.id {
        provider = provider.with_id(id);
    }
    if let Some(usage) = fixture.provider_step.usage {
        provider = provider.with_usage(usage);
    }
    if let Some(finish_reason) = fixture.provider_step.finish_reason {
        provider = provider.with_finish_reason(finish_reason);
    }
    if let Some(raw) = fixture.provider_step.raw_finish_reason {
        provider = provider.with_raw_finish_reason(raw);
    }

    let system_prompt = fixture.system_prompt.unwrap_or_default();
    let tools: Vec<kosong_rs::provider::Tool> = fixture
        .tools
        .and_then(|t| serde_json::from_value(t).ok())
        .unwrap_or_default();
    let options = fixture.options.map(|o| GenerateOptions {
        auth: o.auth.and_then(|a| serde_json::from_value(a).ok()),
        ..Default::default()
    });

    let result = generate(
        &provider,
        &system_prompt,
        &tools,
        &fixture.history,
        None,
        options.as_ref(),
    )
    .await;

    let output = match result {
        Ok(r) => GoldenResult {
            assistant_message: Some(serde_json::to_value(&r.message)?),
            error: None,
        },
        Err(e) => GoldenResult {
            assistant_message: None,
            error: Some(format!("{}", e)),
        },
    };

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

- [ ] 添加 binary 依赖：

```toml
[[bin]]
name = "kosong-golden"
path = "src/bin/golden.rs"

[dependencies]
anyhow = "1"
```

- [ ] 创建临时 fixture 文件 `/tmp/kosong-smoke.json`：

```json
{
  "history": [{ "role": "user", "content": [{ "type": "text", "text": "hi" }] }],
  "providerStep": {
    "parts": [{ "type": "text", "text": "hello" }]
  }
}
```

- [ ] 编译并运行，确认输出包含 `assistantMessage.text == "hello"`：

```bash
cargo run -p kosong-rs --bin kosong-golden -- /tmp/kosong-smoke.json
```

- [ ] Commit：

```bash
git add rust-ody/crates/kosong-rs/src/bin/golden.rs rust-ody/crates/kosong-rs/Cargo.toml
git commit -m "feat(kosong-rs): add kosong-golden binary for parity fixtures"
```

---

### Task 10: 创建 TS `kosong-golden` runner

**Depends on:** Task 9
**Files:**
- Create: `packages/integration-tests/src/parity/kosong-golden.ts`
- Test: `pnpm vitest run packages/integration-tests/test/parity/kosong/l1-golden.test.ts`

#### Background

TS 侧需要一个与 Rust binary 对等的 runner：读取同一 fixture，调用 TS `packages/kosong/src/generate.ts` 的 `generate()`，输出同形结果。

#### Steps

- [ ] 创建 `packages/integration-tests/src/parity/kosong-golden.ts`：

```ts
import type { ChatProvider, FinishReason, Message, StreamedMessage, ThinkingEffort, Tool } from '@odysseythink/kosong';
import { generate } from '@odysseythink/kosong';

export interface Fixture {
  systemPrompt?: string;
  tools?: Tool[];
  history: Message[];
  options?: {
    auth?: { apiKey?: string; headers?: Record<string, string> };
  };
  providerStep: {
    id?: string;
    parts: unknown[];
    usage?: {
      inputOther: number;
      output: number;
      inputCacheRead: number;
      inputCacheCreation: number;
    };
    finishReason?: string;
    rawFinishReason?: string;
  };
  expectError?: string | null;
}

class MockProvider implements ChatProvider {
  readonly name = 'mock';
  readonly modelName = 'm1';
  readonly thinkingEffort = null;

  constructor(private readonly step: Fixture['providerStep']) {}

  async generate(
    _systemPrompt: string,
    _tools: Tool[],
    _history: Message[],
    _options?: { signal?: AbortSignal; auth?: { apiKey?: string; headers?: Record<string, string> } },
  ): Promise<StreamedMessage> {
    const step = this.step;
    return {
      id: step.id ?? null,
      usage: step.usage ?? null,
      finishReason: (step.finishReason ?? null) as FinishReason | null,
      rawFinishReason: step.rawFinishReason ?? null,
      async *[Symbol.asyncIterator]() {
        for (const part of step.parts) {
          yield part as never;
        }
      },
    } as StreamedMessage;
  }

  withThinking(_effort: ThinkingEffort): ChatProvider {
    return this;
  }
}

export async function runTsKosongGolden(fixture: Fixture): Promise<{
  assistantMessage: unknown | null;
  error: string | null;
}> {
  const provider = new MockProvider(fixture.providerStep);
  try {
    const result = await generate(
      provider,
      fixture.systemPrompt ?? '',
      fixture.tools ?? [],
      fixture.history,
      undefined,
      {
        auth: fixture.options?.auth,
      },
    );
    return { assistantMessage: result.message, error: null };
  } catch (e) {
    return { assistantMessage: null, error: String(e) };
  }
}
```

- [ ] 写临时测试验证 runner 可运行：

```ts
// packages/integration-tests/test/parity/kosong/l1-golden.test.ts
import { describe, expect, it } from 'vitest';
import { runTsKosongGolden } from '../../../src/parity/kosong-golden';

describe('kosong TS golden runner', () => {
  it('runs a minimal fixture', async () => {
    const result = await runTsKosongGolden({
      history: [{ role: 'user', content: [{ type: 'text', text: 'hi' }] }],
      providerStep: { parts: [{ type: 'text', text: 'hello' }] },
    });
    expect(result.assistantMessage).toMatchObject({ role: 'assistant', content: [{ type: 'text', text: 'hello' }] });
  });
});
```

- [ ] 运行测试并确认通过：

```bash
pnpm vitest run packages/integration-tests/test/parity/kosong/l1-golden.test.ts
```

- [ ] Commit：

```bash
git add packages/integration-tests/src/parity/kosong-golden.ts \
  packages/integration-tests/test/parity/kosong/l1-golden.test.ts
git commit -m "test(integration-tests): add kosong TS golden runner"
```

---

### Task 11: 编写 L1 黄金 fixture 覆盖 `generate()` 关键路径

**Depends on:** Task 10
**Files:**
- Create: `packages/integration-tests/src/parity/fixtures/kosong/l1-generate-text.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong/l1-tool-call-single.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong/l1-tool-call-parallel.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong/l1-empty-rejection.json`
- Create: `packages/integration-tests/src/parity/fixtures/kosong/l1-thinking-only-rejection.json`
- Modify: `packages/integration-tests/test/parity/kosong/l1-golden.test.ts`
- Test: `pnpm vitest run packages/integration-tests/test/parity/kosong/l1-golden.test.ts`

#### Steps

- [ ] 创建 `l1-generate-text.json`：

```json
{
  "systemPrompt": "",
  "tools": [],
  "history": [
    { "role": "user", "content": [{ "type": "text", "text": "hello" }] }
  ],
  "providerStep": {
    "id": "resp_1",
    "parts": [
      { "type": "text", "text": "Hi! " },
      { "type": "text", "text": "How can I help?" }
    ],
    "usage": { "inputOther": 10, "output": 8, "inputCacheRead": 0, "inputCacheCreation": 0 },
    "finishReason": "completed",
    "rawFinishReason": "stop"
  },
  "expectError": null
}
```

- [ ] 创建 `l1-tool-call-single.json`：

```json
{
  "systemPrompt": "",
  "tools": [
    { "name": "get_weather", "description": "weather", "parameters": { "type": "object", "properties": {} } }
  ],
  "history": [
    { "role": "user", "content": [{ "type": "text", "text": "get weather" }] }
  ],
  "providerStep": {
    "id": "resp_1",
    "parts": [
      { "type": "function", "id": "call_1", "name": "get_weather", "arguments": null, "_streamIndex": 0 },
      { "type": "tool_call_part", "argumentsPart": "{\"city\": \"Beijing\"}", "index": 0 }
    ],
    "usage": { "inputOther": 12, "output": 6, "inputCacheRead": 0, "inputCacheCreation": 0 },
    "finishReason": "tool_calls"
  },
  "expectError": null
}
```

- [ ] 创建 `l1-tool-call-parallel.json`：

```json
{
  "systemPrompt": "",
  "tools": [
    { "name": "get_weather", "description": "weather", "parameters": { "type": "object", "properties": {} } }
  ],
  "history": [
    { "role": "user", "content": [{ "type": "text", "text": "weather in two cities" }] }
  ],
  "providerStep": {
    "id": "resp_1",
    "parts": [
      { "type": "function", "id": "call_a", "name": "get_weather", "arguments": null, "_streamIndex": 0 },
      { "type": "tool_call_part", "argumentsPart": "{\"city\": \"Beijing\"}", "index": 0 },
      { "type": "function", "id": "call_b", "name": "get_weather", "arguments": null, "_streamIndex": 1 },
      { "type": "tool_call_part", "argumentsPart": "{\"city\": \"Shanghai\"}", "index": 1 }
    ],
    "usage": { "inputOther": 15, "output": 10, "inputCacheRead": 0, "inputCacheCreation": 0 },
    "finishReason": "tool_calls"
  },
  "expectError": null
}
```

- [ ] 创建 `l1-empty-rejection.json`：

```json
{
  "systemPrompt": "",
  "tools": [],
  "history": [
    { "role": "user", "content": [{ "type": "text", "text": "hi" }] }
  ],
  "providerStep": {
    "parts": [],
    "finishReason": "completed",
    "rawFinishReason": "stop"
  },
  "expectError": "empty"
}
```

- [ ] 创建 `l1-thinking-only-rejection.json`：

```json
{
  "systemPrompt": "",
  "tools": [],
  "history": [
    { "role": "user", "content": [{ "type": "text", "text": "hi" }] }
  ],
  "providerStep": {
    "parts": [{ "type": "think", "think": "...", "encrypted": "sig" }],
    "finishReason": "completed",
    "rawFinishReason": "stop"
  },
  "expectError": "empty"
}
```

- [ ] 更新 `packages/integration-tests/test/parity/kosong/l1-golden.test.ts` 为只验证 Rust binary 能跑通所有 fixture：

```ts
import { existsSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { beforeAll, describe, expect, it } from 'vitest';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';

function findProjectRoot(): string {
  let current = dirname(fileURLToPath(import.meta.url));
  while (current !== dirname(current)) {
    if (existsSync(join(current, '.git'))) return current;
    current = dirname(current);
  }
  return process.cwd();
}

const rootDir = findProjectRoot();
const fixturesDir = join(rootDir, 'packages', 'integration-tests', 'src', 'parity', 'fixtures', 'kosong');
const fixtures = [
  'l1-generate-text.json',
  'l1-tool-call-single.json',
  'l1-tool-call-parallel.json',
  'l1-empty-rejection.json',
  'l1-thinking-only-rejection.json',
];

describe('kosong L1 golden fixtures (rust only)', () => {
  beforeAll(() => {
    spawnSync('cargo', ['build', '-p', 'kosong-rs', '--bin', 'kosong-golden'], {
      cwd: join(rootDir, 'rust-ody'),
      stdio: 'inherit',
    });
  });

  const binaryPath = join(rootDir, 'rust-ody', 'target', 'debug', 'kosong-golden');

  it.each(fixtures)('%s runs without panic', (name) => {
    const fixturePath = join(fixturesDir, name);
    const result = spawnSync(binaryPath, [fixturePath], { encoding: 'utf8' });
    if (result.status !== 0) {
      throw new Error(`kosong-golden exited ${result.status}: ${result.stderr}`);
    }
    const parsed = JSON.parse(result.stdout);
    expect(parsed).toHaveProperty('assistantMessage');
    expect(parsed).toHaveProperty('error');
  });
});
```

- [ ] 运行测试确认所有 fixture 被 Rust binary 成功消费：

```bash
pnpm vitest run packages/integration-tests/test/parity/kosong/l1-golden.test.ts
```

- [ ] Commit：

```bash
git add packages/integration-tests/src/parity/fixtures/kosong/ \
  packages/integration-tests/test/parity/kosong/l1-golden.test.ts
git commit -m "test(integration-tests): add kosong L1 golden fixtures"
```

---

### Task 12: 编写 TS↔Rust parity 对照断言

**Depends on:** Task 11
**Files:**
- Modify: `packages/integration-tests/test/parity/kosong/l1-golden.test.ts`
- Test: `pnpm vitest run packages/integration-tests/test/parity/kosong/l1-golden.test.ts`

#### Steps

- [ ] 将测试改为同时调用 TS runner 与 Rust binary，并深比较输出：

```ts
import { existsSync, readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { beforeAll, describe, expect, it } from 'vitest';
import { dirname, join } from 'pathe';
import { fileURLToPath } from 'node:url';
import { runTsKosongGolden, type Fixture } from '../../../src/parity/kosong-golden';

function findProjectRoot(): string {
  let current = dirname(fileURLToPath(import.meta.url));
  while (current !== dirname(current)) {
    if (existsSync(join(current, '.git'))) return current;
    current = dirname(current);
  }
  return process.cwd();
}

const rootDir = findProjectRoot();
const fixturesDir = join(rootDir, 'packages', 'integration-tests', 'src', 'parity', 'fixtures', 'kosong');
const fixtures: Array<{ name: string; expectError?: boolean }> = [
  { name: 'l1-generate-text.json' },
  { name: 'l1-tool-call-single.json' },
  { name: 'l1-tool-call-parallel.json' },
  { name: 'l1-empty-rejection.json', expectError: true },
  { name: 'l1-thinking-only-rejection.json', expectError: true },
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
      sorted[key] = sortKeys((obj as Record<string, unknown>)[key]);
    }
    return sorted;
  }
  return obj;
}

describe('kosong L1 golden parity', () => {
  beforeAll(() => {
    spawnSync('cargo', ['build', '-p', 'kosong-rs', '--bin', 'kosong-golden'], {
      cwd: join(rootDir, 'rust-ody'),
      stdio: 'inherit',
    });
  });

  const binaryPath = join(rootDir, 'rust-ody', 'target', 'debug', 'kosong-golden');

  it.each(fixtures)('$name TS matches Rust', async ({ name, expectError }) => {
    const fixture = loadFixture(name);
    const ts = await runTsKosongGolden(fixture);
    const result = spawnSync(binaryPath, [join(fixturesDir, name)], { encoding: 'utf8' });
    if (result.status !== 0) {
      throw new Error(`kosong-golden exited ${result.status}: ${result.stderr}`);
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

- [ ] 运行测试，预期最初可能因字段顺序或默认值不一致而失败。逐个修复：

1. 若 Rust `Message` 序列化后字段顺序与 TS 不同，使用 `sortKeys` 已归一化；
2. 若 `ToolCall.arguments` 在 TS 为 `null`、Rust 为 `None`（均序列化为 `null`），应一致；
3. 若 `usage` 字段 Rust 输出 `null` 而 TS 输出对象，检查 MockProvider 是否正确传递 `usage`。

- [ ] 全部通过后运行 workspace typecheck：

```bash
pnpm -r typecheck
```

- [ ] Commit：

```bash
git add packages/integration-tests/test/parity/kosong/l1-golden.test.ts
git commit -m "test(integration-tests): add kosong TS↔Rust parity assertions"
```

---

## Local Self-Review

- [ ] 1. Spec-coverage table：Part 3 覆盖 golden binary（4.2.0-⑨）、TS runner（4.2.0-⑩）、L1 fixture（4.2.0-⑪）、parity 测试（4.2.0-⑫）。
- [ ] 2. Placeholder scan：无 TODO/TBD；所有代码片段完整。
- [ ] 3. No phantom tasks：每个任务都产生文件、测试和 commit。
- [ ] 4. Dependency soundness：Task 10 依赖 Task 9；Task 11 依赖 Task 10；Task 12 依赖 Task 11。
- [ ] 5. Caller & build soundness：Task 12 修改测试文件，结束时运行 `pnpm -r typecheck`。
- [ ] 6. Test-the-risk：fixture 覆盖合并顺序、parallel tool-call 路由、空响应/thinking-only 拒绝；parity 测试断言结构相等。
- [ ] 7. Type consistency：fixture 字段名（camelCase、`type` 标签）与 Part 1/2 中 Rust 类型序列化一致；`generate()` 签名与 TS 一致（provider, systemPrompt, tools, history, callbacks?, options?）。
