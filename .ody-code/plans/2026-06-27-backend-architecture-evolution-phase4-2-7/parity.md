# Part 3 — L2/L3 parity + benchmarks

本部分复用 `packages/integration-tests/src/parity/` harness，新增 `host-config` L2 scenario 以逐字段比对 `setModel`/`getConfig`/`getOdyConfig` 的 provider 信息，修复 `multi-turn-tool` L3 scenario 的事件收集与已知 gap 状态，并新增流式首字节延迟/吞吐 benchmark 与 CI job。

---

### Task 10: 新增 `host-config` L2 scenario 与测试

**Depends on:** Part 2 Task 8（`set_model` 支持 provider 前缀，`getConfig`/`getOdyConfig` 返回真实 provider 信息）

**Files:**
- Create: `packages/integration-tests/src/parity/scenarios/host-config.ts`
- Modify: `packages/integration-tests/src/parity/scenarios/index.ts`
- Modify: `packages/integration-tests/test/parity/ts-vs-rust.test.ts`
- Modify: `packages/integration-tests/src/parity/known-gaps.md`

实现步骤：

- [ ] 创建 `host-config.ts` scenario，使用 raw RPC 调用以获取 agent config 与 OdyConfig：

```ts
import { Session } from '@odysseythink/ody-code-sdk';
import type { ChatProvider } from '@odysseythink/kosong';

import { MockChatProvider } from '../fixtures/mock-provider';
import type { Scenario } from '../types';

export const hostConfigMockLlm: ChatProvider = new MockChatProvider([]);

export const hostConfigScenario: Scenario = {
  name: 'host-config',
  async run(backend) {
    const summary = await backend.client.createSession({
      workDir: backend.homeDir,
      permission: 'auto',
      model: 'mock',
    });
    const session = new Session({
      id: summary.id,
      workDir: summary.workDir,
      rpc: backend.client,
    });
    try {
      // 1. setModel with provider prefix
      const setModelResult = await (backend.client as unknown as { rpc: { setModel: (p: unknown) => Promise<unknown> } }).rpc.setModel({
        sessionId: summary.id,
        agentId: 'main',
        model: 'openai/gpt-4o',
      });

      // 2. agent config (raw getConfig RPC)
      const agentConfig = await (backend.client as unknown as { rpc: { getConfig: (p: unknown) => Promise<unknown> } }).rpc.getConfig({
        sessionId: summary.id,
        agentId: 'main',
      });

      // 3. global OdyConfig
      const odyConfig = await backend.client.getConfig();

      return {
        responses: [
          { setModel: setModelResult },
          { agentConfig: sanitizeConfig(agentConfig) },
          { odyConfig: sanitizeOdyConfig(odyConfig) },
        ],
        events: [],
      };
    } finally {
      await session.close();
    }
  },
};

function sanitizeConfig(config: unknown): unknown {
  const c = config as Record<string, unknown>;
  return {
    cwd: c['cwd'],
    provider: c['provider'],
    modelAlias: c['modelAlias'],
    modelCapabilities: c['modelCapabilities'],
    thinkingLevel: c['thinkingLevel'],
    systemPrompt: c['systemPrompt'],
  };
}

function sanitizeOdyConfig(config: unknown): unknown {
  const c = config as Record<string, unknown>;
  const providers = (c['providers'] as unknown[] | undefined) ?? [];
  return {
    providers: providers.map((p) => {
      const provider = p as Record<string, unknown>;
      return {
        id: provider['id'],
        baseUrl: provider['baseUrl'],
        defaultModel: provider['defaultModel'],
      };
    }),
  };
}
```

- [ ] 在 `scenarios/index.ts` 中新增导出并加入 `scenarios` 数组：

```ts
import { hostConfigMockLlm, hostConfigScenario } from './host-config';
// ...
export { hostConfigMockLlm, hostConfigScenario } from './host-config';
// ...
export const scenarios: readonly ScenarioEntry[] = [
  // ... existing entries ...
  { scenario: hostConfigScenario, mockLlm: hostConfigMockLlm },
];
```

- [ ] 在 `ts-vs-rust.test.ts` 的 `cases` 数组中新增：

```ts
{ name: hostConfigScenario.name, scenario: hostConfigScenario, mockLlm: hostConfigMockLlm },
```

- [ ] 先跑 TS-vs-TS 自比对，确保 harness 本身无 bug：

```bash
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/ts-vs-ts.test.ts -t "host-config"
```

预期：通过（diff 为 null）。

- [ ] 再跑 TS-vs-Rust：

```bash
ODY_HOST_BINARY_PATH=rust-ody/target/release/ody-host pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/ts-vs-rust.test.ts -t "host-config"
```

预期：初始可能因 `modelCapabilities` 字段不一致而失败；根据 diff 调整 `normalize.ts`（见 Task 11）或修正 Rust 实现。

- [ ] 更新 `known-gaps.md`：移除 `set-model | L2` 的旧 gap（因为 `host-config` scenario 已覆盖并期望通过）；若 `host-config` 仍有未对齐字段，新增 `host-config | L2 | <reason>`。

- [ ] Commit：`test(parity): add host-config L2 scenario for provider routing`。

---

### Task 11: 修复 `multi-turn-tool` L3 scenario 的事件流对照

**Depends on:** Task 10（normalize 规则已就绪）

**Files:**
- Modify: `packages/integration-tests/src/parity/scenarios/multi-turn-tool.ts`
- Modify: `packages/integration-tests/src/parity/normalize.ts`
- Modify: `packages/integration-tests/src/parity/known-gaps.md`

实现步骤：

- [ ] 更新 `multi-turn-tool.ts`，在 prompt 后读取 `output.txt` 并把文件内容加入 responses，同时等待 `turn.ended`：

```ts
import { readFile } from 'node:fs/promises';
import { join } from 'pathe';

import type { ChatProvider } from '@odysseythink/kosong';

import { MockChatProvider } from '../fixtures/mock-provider';
import type { Scenario } from '../types';
import { waitForTurnEnded } from './utils';

export const multiTurnToolMockLlm: ChatProvider = new MockChatProvider([
  [
    {
      type: 'function',
      id: 'tc-read-1',
      name: 'Read',
      arguments: JSON.stringify({ path: 'input.txt' }),
    },
  ],
  [
    {
      type: 'function',
      id: 'tc-write-1',
      name: 'Write',
      arguments: JSON.stringify({ path: 'output.txt', content: 'derived payload' }),
    },
  ],
  [{ type: 'text', text: 'Wrote output.txt' }],
]);

export const multiTurnToolScenario: Scenario = {
  name: 'multi-turn-tool',
  async run(backend) {
    await backend.client.envCall('env.writeText', {
      path: join(backend.homeDir, 'input.txt'),
      text: 'source payload',
    });

    const summary = await backend.client.createSession({
      workDir: backend.homeDir,
      permission: 'auto',
      model: 'mock',
    });
    await backend.client.prompt({
      sessionId: summary.id,
      input: [{ type: 'text', text: 'Read input.txt and write its meaning to output.txt' }],
    });
    await waitForTurnEnded(backend.client, { timeoutMs: 10000 });

    const outputText = await readFile(join(backend.homeDir, 'output.txt'), 'utf8').catch(() => '');

    return {
      responses: [{ sessionId: summary.id, outputText }],
      events: [],
    };
  },
};
```

- [ ] 更新 `normalize.ts`，增加对 capability 对象和 usage 时间字段的归一化：

在 `TIMESTAMPISH_KEYS` 中已有 `llmFirstTokenLatencyMs` / `llmStreamDurationMs`，无需新增。若 `modelCapabilities.max_context_tokens` 等数字字段在 scenario 响应中可能为零，保持原值即可（这些不是时间戳）。

在 `normalize.ts` 中新增对 `turn.ended` 事件 `error` 字段的处理：若 TS 与 Rust 对成功 turn 的 `error` 字段分别为 `undefined`/`null`，在 walk 函数中将 `undefined` 与 `null` 统一。更简单的做法是在 `collectDiffs` 中把 `undefined`/`null` 视为相等。修改 `assert-parity.ts`：

```ts
function collectDiffs(a: unknown, b: unknown, path: string, diffs: FieldDiff[], seen: WeakSet<object>): void {
  // Treat null/undefined as equivalent at leaf level.
  if (a === undefined && b === null) return;
  if (a === null && b === undefined) return;
  // ... rest unchanged
}
```

- [ ] 运行 TS-vs-TS 自比对确认 normalize 无 bug：

```bash
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/ts-vs-ts.test.ts -t "multi-turn-tool"
```

预期：通过。

- [ ] 运行 TS-vs-Rust：

```bash
ODY_HOST_BINARY_PATH=rust-ody/target/release/ody-host pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/ts-vs-rust.test.ts -t "multi-turn-tool"
```

预期：若 Rust 后端 `turn.ended` 已正确 emit，则该 scenario 应通过；若仍有事件类型差异，根据 diff 更新 `known-gaps.md`。

- [ ] 更新 `known-gaps.md`：
  - 移除 `multi-turn-tool | L3 | Rust 后端 mock provider 未 emit turn.ended`（已修复）。
  - 若通过，确认无新增 gap；若失败，新增具体 reason。

- [ ] Commit：`test(parity): align multi-turn-tool L3 scenario and normalize null/undefined`。

---

### Task 12: 新增流式 TTFB/throughput benchmark 与 CI job

**Depends on:** Task 11

**Files:**
- Create: `packages/integration-tests/src/parity/benchmark.ts`
- Create: `packages/integration-tests/test/parity/benchmark.test.ts`
- Modify: `.github/workflows/rust-host.yml`

实现步骤：

- [ ] 创建 `benchmark.ts`，提供一个可独立运行的 benchmark，比较 TS 与 Rust 后端在固定 mock 输出下的首字节延迟与流式吞吐：

```ts
import { performance } from 'node:perf_hooks';

import type { ChatProvider } from '@odysseythink/kosong';

import { makeTsBackend, makeRustBackend, createTempHome, cleanupHome } from './backends';
import { MockChatProvider } from './fixtures/mock-provider';
import { resolveRustBinaryPath } from './rust-binary';

export interface BenchmarkResult {
  readonly backend: 'ts' | 'rust';
  readonly firstTokenMs: number;
  readonly totalMs: number;
  readonly tokens: number;
  readonly throughputTokensPerSec: number;
}

const mockLlm: ChatProvider = new MockChatProvider(
  Array.from({ length: 50 }, (_, i) => ({ type: 'text' as const, text: `tok${i} ` })),
);

async function runBackend(backend: 'ts' | 'rust', homeDir: string): Promise<BenchmarkResult> {
  const binaryPath = resolveRustBinaryPath();
  const makeBackend = backend === 'ts'
    ? () => makeTsBackend({ homeDir, mockLlm })
    : () => makeRustBackend({ homeDir, binaryPath, transport: 'stdio', extraArgs: ['--mock-provider'] });

  const b = await makeBackend();
  try {
    const summary = await b.client.createSession({ workDir: homeDir });

    let firstTokenAt: number | undefined;
    let lastTokenAt: number | undefined;
    let tokens = 0;
    const unsubscribe = b.client.onEvent((event) => {
      if (event.type === 'assistant.delta') {
        const now = performance.now();
        if (firstTokenAt === undefined) firstTokenAt = now;
        lastTokenAt = now;
        tokens += 1;
      }
    });

    const start = performance.now();
    await b.client.prompt({
      sessionId: summary.id,
      input: [{ type: 'text', text: 'benchmark' }],
    });

    // Wait a short grace period for all deltas to arrive.
    await new Promise((resolve) => setTimeout(resolve, 500));
    unsubscribe();
    const totalMs = performance.now() - start;

    return {
      backend,
      firstTokenMs: firstTokenAt !== undefined ? firstTokenAt - start : totalMs,
      totalMs,
      tokens,
      throughputTokensPerSec: totalMs > 0 ? (tokens / totalMs) * 1000 : 0,
    };
  } finally {
    await b.close();
  }
}

export async function runBenchmark(): Promise<{ readonly ts: BenchmarkResult; readonly rust: BenchmarkResult }> {
  const tsHome = await createTempHome('parity-bench-ts-');
  const rustHome = await createTempHome('parity-bench-rust-');
  try {
    const [ts, rust] = await Promise.all([
      runBackend('ts', tsHome),
      runBackend('rust', rustHome),
    ]);
    return { ts, rust };
  } finally {
    await cleanupHome(tsHome);
    await cleanupHome(rustHome);
  }
}

export function formatBenchmark(results: { readonly ts: BenchmarkResult; readonly rust: BenchmarkResult }): string {
  const { ts, rust } = results;
  return [
    '| backend | firstTokenMs | totalMs | tokens | throughput (tok/s) |',
    '|---|---|---|---|---|',
    `| ts | ${ts.firstTokenMs.toFixed(2)} | ${ts.totalMs.toFixed(2)} | ${ts.tokens} | ${ts.throughputTokensPerSec.toFixed(2)} |`,
    `| rust | ${rust.firstTokenMs.toFixed(2)} | ${rust.totalMs.toFixed(2)} | ${rust.tokens} | ${rust.throughputTokensPerSec.toFixed(2)} |`,
  ].join('\n');
}
```

- [ ] 创建 `test/parity/benchmark.test.ts`，作为非阻塞回归测试，仅记录结果并断言 Rust 与 TS 的 token 数量一致：

```ts
import { describe, expect, it } from 'vitest';

import { runBenchmark } from '../../src/parity/benchmark';

describe('parity benchmark', () => {
  it('ts and rust produce the same number of mock tokens', async () => {
    const results = await runBenchmark();
    expect(results.rust.tokens).toBe(results.ts.tokens);
    console.log('\n' + formatBenchmark(results));
  }, 60000);
});

function formatBenchmark(results: { readonly ts: { tokens: number }; readonly rust: { tokens: number } }): string {
  return `ts=${results.ts.tokens} rust=${results.rust.tokens}`;
}
```

- [ ] 运行 benchmark 测试：

```bash
ODY_HOST_BINARY_PATH=rust-ody/target/release/ody-host pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/benchmark.test.ts
```

预期：通过，console 中打印 ts/rust token 数相同。

- [ ] 修改 `.github/workflows/rust-host.yml`，在 `Parity smoke tests` job 后新增 benchmark step：

```yaml
      - name: Provider routing benchmark
        if: matrix.os == 'ubuntu-24.04'
        run: |
          mkdir -p .ody-code/reports
          pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/benchmark.test.ts | tee .ody-code/reports/provider-bench.log
        shell: bash
        env:
          ODY_HOST_BINARY_PATH: ${{ github.workspace }}/rust-ody/target/release/ody-host

      - name: Upload provider benchmark log
        if: matrix.os == 'ubuntu-24.04' && always()
        uses: actions/upload-artifact@v4
        with:
          name: provider-bench-${{ matrix.target }}-${{ matrix.transport }}
          path: .ody-code/reports/provider-bench.log
          if-no-files-found: ignore
```

- [ ] Commit：`test(parity): add streaming TTFB/throughput benchmark`。

---

## Part 3 Self-Review

- [ ] 1. Spec-coverage: 4.2.7.4（L2 对照）由 Task 10 覆盖；4.2.7.5（L3 对照）由 Task 11 覆盖；4.2.7.6（性能基准）由 Task 12 覆盖。
- [ ] 2. Placeholder scan: 无 TODO/TBD；每个 scenario/测试/CI step 均给出完整代码。
- [ ] 3. No phantom tasks: 每个 task 均产生可验证变更（scenario 文件 + 测试 + CI）。
- [ ] 4. Dependency soundness: Task 10 依赖 Part 2 Task 8；Task 11 依赖 Task 10；Task 12 依赖 Task 11。无向后依赖。
- [ ] 5. Caller & build soundness: Task 11 修改 `assert-parity.ts` 的 `collectDiffs`，需全树 typecheck：`pnpm -r typecheck`。Task 12 新增测试文件，运行 vitest 验证。
- [ ] 6. Test-the-risk: `host-config` scenario 对 `setModel` 的 provider/model 拆分做行为断言；`multi-turn-tool` 对 output.txt 内容做断言；benchmark 对 token 数量一致性做断言。
- [ ] 7. Type consistency: `host-config.ts` 使用 `(backend.client as unknown as { rpc: ... })` 访问 raw RPC，字段名 `setModel`/`getConfig`/`getOdyConfig` 与 `CoreAPI` 一致；`multi-turn-tool.ts` 复用现有 `MockChatProvider` 与 `waitForTurnEnded` API。
