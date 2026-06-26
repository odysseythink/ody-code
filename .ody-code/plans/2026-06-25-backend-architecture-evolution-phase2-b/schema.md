# Part 4: JSON Schema Generation & G2-B Smoke Test

**Goal:** 为 `CoreAPI`/`SDKAPI` 生成可消费的 JSON Schema，并通过端到端 smoke test 验证 `ody serve` 能完成“建会话 → 发 prompt → 收事件流”。

**Architecture:** 在 `packages/agent-core/src/rpc/core-api.ts` 与 `sdk-api.ts` 新增 `CoreAPIProtocol`/`SDKAPIProtocol` 映射类型，将每个 RPC 方法的 `payload` 与 `returns` 暴露为普通对象类型；`scripts/gen-rpc-schema.ts` 使用 `ts-json-schema-generator` 从这两个类型产出 `scripts/generated/rpc-schema.json`。G2-B smoke test 位于 `apps/ody-code/test/e2e`，它启动构建后的 `ody serve`，通过 `SDKRpcClient.connect()` 连接，并用 `vi.mock('@odysseythink/kosong')` 注入 fake provider 避免真实 LLM 调用。

**Tech Stack:** TypeScript 6.0 / Node.js ≥24.15 / `ts-json-schema-generator` / Vitest / `SDKRpcClient`。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task). Steps use - [ ] checkboxes for tracking.

---

### Task 13: 生成 `CoreAPI`/`SDKAPI` JSON Schema

**Depends on:** `2026-06-25-backend-architecture-evolution-phase2-b/cli.md` Task 12

**Files:**
- Modify: `packages/agent-core/src/rpc/core-api.ts`（约第 460 行后新增 protocol 类型）
- Modify: `packages/agent-core/src/rpc/sdk-api.ts`（约第 108 行后新增 protocol 类型）
- Create: `scripts/gen-rpc-schema.ts`
- Create: `packages/agent-core/test/scripts/gen-rpc-schema.test.ts`
- Modify: `package.json:23`（新增 npm script）
- Modify: `package.json:30-49`（新增 devDependency）

- [ ] Write the failing test：创建 `packages/agent-core/test/scripts/gen-rpc-schema.test.ts`，运行 `scripts/gen-rpc-schema.ts` 并断言输出文件包含 `createSession` 与 `emitEvent`。

```ts
// packages/agent-core/test/scripts/gen-rpc-schema.test.ts
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, rmSync } from 'node:fs';
import { dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const __dirname = fileURLToPath(new URL('.', import.meta.url));
const repoRoot = dirname(dirname(dirname(dirname(__dirname))));
const outPath = `${repoRoot}/scripts/generated/rpc-schema.json`;

describe('gen-rpc-schema', () => {
  it('generates a schema covering CoreAPI and SDKAPI methods', () => {
    rmSync(outPath, { force: true });
    expect(existsSync(outPath)).toBe(false);

    execFileSync('pnpm', ['-w', 'exec', 'tsx', 'scripts/gen-rpc-schema.ts'], {
      cwd: repoRoot,
      stdio: 'ignore',
    });

    expect(existsSync(outPath)).toBe(true);
    const schema = JSON.parse(readFileSync(outPath, 'utf-8')) as {
      title: string;
      core: { properties: Record<string, unknown> };
      sdk: { properties: Record<string, unknown> };
    };

    expect(schema.title).toBe('Ody Code RPC API');
    expect(schema.core.properties.createSession).toBeDefined();
    expect(schema.sdk.properties.emitEvent).toBeDefined();
  });
});
```

- [ ] Run it and verify it FAILS：

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/scripts/gen-rpc-schema.test.ts
```

Expected failure：`scripts/gen-rpc-schema.ts` 不存在；`ts-json-schema-generator` 未安装。

- [ ] Write the minimal implementation：

```ts
// packages/agent-core/src/rpc/core-api.ts
// 在 CoreAPI 接口定义之后新增（约第 460 行后）
export type CoreAPIProtocol = {
  [K in keyof CoreAPI]: {
    payload: Parameters<CoreAPI[K]>[0];
    returns: Awaited<ReturnType<CoreAPI[K]>>;
  };
};
```

```ts
// packages/agent-core/src/rpc/sdk-api.ts
// 在 SDKAPI 类型定义之后新增（约第 108 行后）
export type SDKAPIProtocol = {
  [K in keyof SDKAPI]: {
    payload: Parameters<SDKAPI[K]>[0];
    returns: Awaited<ReturnType<SDKAPI[K]>>;
  };
};
```

```bash
pnpm add -D ts-json-schema-generator
```

```json
// package.json
// devDependencies 中新增（约第 48 行后）
    "ts-json-schema-generator": "^2.3.0",

// scripts 中新增（约第 23 行后）
    "gen:rpc-schema": "tsx scripts/gen-rpc-schema.ts",
```

```ts
// scripts/gen-rpc-schema.ts
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createGenerator, type Config } from 'ts-json-schema-generator';

const __filename = fileURLToPath(import.meta.url);
const repoRoot = dirname(__filename);

function generateSchema(
  tsconfig: string,
  entry: string,
  type: string,
): ReturnType<ReturnType<typeof createGenerator>['createSchema']> {
  const config: Config = {
    path: entry,
    tsconfig,
    type,
    rootPath: repoRoot,
  };
  const generator = createGenerator(config);
  return generator.createSchema(type);
}

function getCliVersion(): string {
  const pkg = JSON.parse(
    readFileSync(join(repoRoot, 'apps/ody-code/package.json'), 'utf-8'),
  ) as { version: string };
  return pkg.version;
}

const coreSchema = generateSchema(
  join(repoRoot, 'packages/agent-core/tsconfig.json'),
  join(repoRoot, 'packages/agent-core/src/rpc/core-api.ts'),
  'CoreAPIProtocol',
);

const sdkSchema = generateSchema(
  join(repoRoot, 'packages/agent-core/tsconfig.json'),
  join(repoRoot, 'packages/agent-core/src/rpc/sdk-api.ts'),
  'SDKAPIProtocol',
);

const fullSchema = {
  $id: 'https://ody-code.dev/rpc-schema.json',
  title: 'Ody Code RPC API',
  version: getCliVersion(),
  core: coreSchema,
  sdk: sdkSchema,
};

const outPath = join(repoRoot, 'scripts/generated/rpc-schema.json');
mkdirSync(dirname(outPath), { recursive: true });
writeFileSync(outPath, `${JSON.stringify(fullSchema, null, 2)}\n`);
console.log(`Wrote ${outPath}`);
```

- [ ] Run it and verify it PASSES：

```bash
pnpm install
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/scripts/gen-rpc-schema.test.ts
```

Expected：测试全绿，`scripts/generated/rpc-schema.json` 被创建且包含 `createSession` / `emitEvent`。

- [ ] Commit：`git add -A && git commit -m "feat(rpc): generate JSON schema for CoreAPI and SDKAPI"`

- [ ] Whole-tree typecheck：

```bash
pnpm -r typecheck
```

Expected：全绿（新增的 protocol 类型是纯类型，不影响运行时）。

---

### Task 14: G2-B Smoke Test（端到端 `ody serve`）

**Depends on:** `2026-06-25-backend-architecture-evolution-phase2-b/cli.md` Task 12

**Files:**
- Create: `apps/ody-code/test/e2e/g2b-smoke.e2e.test.ts`
- Modify: `apps/ody-code/package.json:79-89`（新增 devDependency `@odysseythink/kosong`）

- [ ] Write the failing test：创建 G2-B e2e 测试，启动构建后的 `ody serve`，用 `SDKRpcClient.connect()` 建会话、发 prompt、收事件。

```ts
// apps/ody-code/test/e2e/g2b-smoke.e2e.test.ts
import { spawn, type ChildProcess } from 'node:child_process';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type * as KosongModule from '@odysseythink/kosong';
import { SDKRpcClient } from '@odysseythink/ody-code-sdk';

const ENABLED = process.env['ODY_E2E'] === '1';

const fakeProviderState = vi.hoisted(() => ({
  calls: 0,
  responseText: 'hello from g2b smoke',
}));

vi.mock('@odysseythink/kosong', async (importOriginal) => {
  const actual = await importOriginal<typeof KosongModule>();
  return {
    ...actual,
    createProvider: () => ({
      name: 'fake',
      modelName: 'fake-model',
      thinkingEffort: null,
      async generate(_systemPrompt: string, _tools: unknown, _history: unknown) {
        fakeProviderState.calls += 1;
        return {
          id: 'fake-response',
          usage: {
            inputOther: 0,
            output: 1,
            inputCacheRead: 0,
            inputCacheCreation: 0,
          },
          finishReason: 'completed',
          rawFinishReason: 'stop',
          async *[Symbol.asyncIterator]() {
            yield { type: 'text', text: fakeProviderState.responseText };
          },
        };
      },
      withThinking() {
        return this;
      },
    }),
  };
});

async function writeConfig(homeDir: string): Promise<void> {
  const config = `
default_model = 'mock'

[providers.mock]
type = 'openai'
api_key = 'test-key'
base_url = 'http://127.0.0.1:99999/v1'

[models.mock]
provider = 'mock'
model = 'mock-model'
max_context_size = 128000
max_output_size = 4096
`;
  await writeFile(join(homeDir, 'config.toml'), config);
}

function waitForEvent(
  client: SDKRpcClient,
  predicate: (event: { type: string }) => boolean,
  timeoutMs = 30_000,
): Promise<{ type: string }> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      unsubscribe();
      reject(new Error('Timed out waiting for event'));
    }, timeoutMs);
    const unsubscribe = client.onEvent((event) => {
      if (predicate(event as { type: string })) {
        clearTimeout(timer);
        unsubscribe();
        resolve(event as { type: string });
      }
    });
  });
}

describe.skipIf(!ENABLED)('G2-B smoke: ody serve end-to-end', () => {
  let homeDir: string;
  let workDir: string;
  let proc: ChildProcess | undefined;

  beforeEach(async () => {
    homeDir = await mkdtemp(join(tmpdir(), 'ody-g2b-home-'));
    workDir = await mkdtemp(join(tmpdir(), 'ody-g2b-work-'));
    await writeConfig(homeDir);
  });

  afterEach(async () => {
    proc?.kill();
    await rm(homeDir, { recursive: true, force: true });
    await rm(workDir, { recursive: true, force: true });
  });

  it('creates a session, sends a prompt, and receives events', async () => {
    const cliPath = fileURLToPath(
      new URL('../../../dist/main.mjs', import.meta.url),
    );

    proc = spawn('node', [
      cliPath,
      'serve',
      '--host',
      '127.0.0.1',
      '--port',
      '0',
      '--home-dir',
      homeDir,
    ], {
      stdio: ['pipe', 'pipe', 'pipe'],
    });

    const ready = await new Promise<{
      host: string;
      port: number;
      token: string;
    }>((resolve, reject) => {
      const onData = (chunk: Buffer) => {
        const lines = chunk.toString('utf8').split('\n');
        for (const line of lines) {
          if (!line.trim()) continue;
          try {
            const msg = JSON.parse(line) as {
              type: string;
              host: string;
              port: number;
              token: string;
            };
            if (msg.type === 'ready') {
              proc!.stderr!.off('data', onData);
              resolve(msg);
              return;
            }
          } catch {
            // ignore non-JSON stderr lines
          }
        }
      };
      proc!.stderr!.on('data', onData);
      proc!.on('error', reject);
      proc!.on('exit', (code) =>
        reject(new Error(`ody serve exited with ${String(code)}`)),
      );
    });

    const client = await SDKRpcClient.connect({
      transport: { host: ready.host, port: ready.port },
      token: ready.token,
      homeDir,
    });

    const session = await client.createSession({
      workDir,
      id: 'g2b-session',
      permission: 'yolo',
    });
    expect(session.id).toBe('g2b-session');

    const events: { type: string }[] = [];
    client.onEvent((event) => {
      events.push(event as { type: string });
    });

    await client.prompt({
      sessionId: session.id,
      input: [{ type: 'text' as const, text: 'hello' }],
    });

    await waitForEvent(client, (event) => event.type === 'turn.ended');
    expect(events.length).toBeGreaterThan(0);
    expect(fakeProviderState.calls).toBeGreaterThan(0);
  }, 60_000);
});
```

- [ ] Run it and verify it FAILS：

```bash
ODY_E2E=1 pnpm --filter ody-code exec vitest run test/e2e/g2b-smoke.e2e.test.ts
```

Expected failure：测试文件不存在；`@odysseythink/kosong` 未在 `apps/ody-code` 中声明，导致 `vi.mock` 解析失败。

- [ ] Write the minimal implementation：

```json
// apps/ody-code/package.json
// devDependencies 中新增（约第 88 行后）
    "@odysseythink/kosong": "workspace:^",
```

```bash
pnpm install
```

测试文件已在上面创建，无需额外实现代码。

- [ ] Run it and verify it PASSES：

```bash
pnpm --filter ody-code run e2e -- test/e2e/g2b-smoke.e2e.test.ts
```

Expected：e2e 脚本先构建 packages，然后运行 smoke test；测试通过，`fakeProviderState.calls > 0`，事件列表非空。

- [ ] Commit：`git add -A && git commit -m "test(e2e): add G2-B smoke test for ody serve"`

- [ ] Whole-tree typecheck + full e2e：

```bash
pnpm -r typecheck
ODY_E2E=1 pnpm --filter ody-code run e2e
```

Expected：typecheck 全绿；e2e 全绿（包括新 smoke test）。

---

## Local Self-Review

- [ ] 1. Spec-coverage table（本 Part）：JSON Schema 生成 → Task 13；schema 覆盖 CoreAPI/SDKAPI 方法 → Task 13；G2-B smoke test（建会话→发 prompt→收事件流）→ Task 14。
- [ ] 2. Placeholder scan：本 Part 无 `TODO`/`TBD`/`implement later`；所有实现、测试、依赖安装命令已完整给出。
- [ ] 3. No phantom tasks：每个 Task 均产出文件变更、测试/验证步骤与 commit；无 `--allow-empty`。
- [ ] 4. Dependency soundness：Task 13 依赖 Part 3 Task 12（CLI 已实现）；Task 14 依赖 Task 13；无向后引用。
- [ ] 5. Caller & build soundness：Task 13 新增的 `CoreAPIProtocol`/`SDKAPIProtocol` 是纯类型，不影响现有调用；Task 14 新增 devDependency 并运行 `pnpm -r typecheck` 与完整 e2e；新增测试不修改共享签名。
- [ ] 6. Test-the-risk：schema 测试断言输出文件存在且包含关键方法属性；smoke test 断言会话创建、prompt 触发 fake provider、事件流非空，覆盖端到端路径。
- [ ] 7. Type consistency：`CoreAPIProtocol`/`SDKAPIProtocol` 使用 Part 1-2 已定义的 `CoreAPI`/`SDKAPI` 方法签名；`SDKRpcClient.connect()` 选项与 Part 2 定义一致。
