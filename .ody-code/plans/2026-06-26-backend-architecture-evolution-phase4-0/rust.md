# Part 3 — TS-vs-Rust Parity Tests

本 Part 让 Part 2 的三个 scenario 同时跑在 TS 内存后端与 Rust `ody-host` 二进制后端上，通过 `known-gaps.md` 登记当前不对齐的 layer，实现「差异可预期、过期 gap 自动报警」。

---

## Part 3 依赖图

```
C1 Rust binary resolver
  │
  ▼
C2 runParityWithGaps (KnownGaps integration)
  │
  ▼
C3 TS-vs-Rust parity tests
```

C1 依赖 `core.md` A5 的 `makeRustBackend` 接口；C2 依赖 `core.md` A3 与 Part 2 B5 的 `runParity`；C3 依赖 C1、C2 与 Part 2 的全部 scenario。

---

## Part 3 范围说明

- **覆盖**：Rust host 二进制路径解析、`runParity` 的 known-gap 集成、TS-vs-Rust 端到端 parity 测试。
- **不覆盖**：CLI 参数开关（Part 4）、CI pipeline 接入（Part 5）。
- **共享签名**：本 Part 不改现有共享签名，只扩展 `run-parity.ts` 新增函数，并在 `known-gaps.md` 追加登记项。

---

### Task C1: Rust Host Binary Resolver

**Depends on:** none（工具函数）

**Files:**
- Create: `packages/integration-tests/src/parity/rust-binary.ts`
- Create: `packages/integration-tests/test/parity/rust-binary.test.ts`

**Goal:** 在测试启动时定位 Rust `ody-host` 二进制：优先 `ODY_HOST_BINARY_PATH` 环境变量，其次探测 `rust-ody/target/release/ody-host` 与 `rust-ody/target/debug/ody-host`。

- [ ] 在 `packages/integration-tests/test/parity/rust-binary.test.ts` 写入测试：

```ts
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { describe, expect, it } from 'vitest';
import { resolveRustBinaryPath } from '../../src/parity/rust-binary';

describe('resolveRustBinaryPath', () => {
  it('prefers ODY_HOST_BINARY_PATH when the file exists', () => {
    const dir = mkdtempSync(join(tmpdir(), 'parity-bin-'));
    const fakeBinary = join(dir, 'ody-host');
    writeFileSync(fakeBinary, '');
    const prev = process.env['ODY_HOST_BINARY_PATH'];
    process.env['ODY_HOST_BINARY_PATH'] = fakeBinary;
    try {
      expect(resolveRustBinaryPath()).toBe(fakeBinary);
    } finally {
      process.env['ODY_HOST_BINARY_PATH'] = prev;
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('throws a helpful error when no candidate exists', () => {
    const prev = process.env['ODY_HOST_BINARY_PATH'];
    delete process.env['ODY_HOST_BINARY_PATH'];
    try {
      expect(() => resolveRustBinaryPath()).toThrow('Rust host binary not found');
    } finally {
      process.env['ODY_HOST_BINARY_PATH'] = prev;
    }
  });
});
```

- [ ] 运行测试并确认失败：

```bash
pnpm --filter integration-tests vitest run test/parity/rust-binary.test.ts
```

预期失败：`rust-binary.ts` 不存在。

- [ ] 在 `packages/integration-tests/src/parity/rust-binary.ts` 写入实现：

```ts
import { existsSync } from 'node:fs';
import { join } from 'pathe';

const CANDIDATES: Array<() => string | undefined> = [
  () => process.env['ODY_HOST_BINARY_PATH'],
  () => join(process.cwd(), 'rust-ody', 'target', 'release', 'ody-host'),
  () => join(process.cwd(), 'rust-ody', 'target', 'debug', 'ody-host'),
];

export function resolveRustBinaryPath(): string {
  for (const candidate of CANDIDATES) {
    const path = candidate();
    if (path !== undefined && existsSync(path)) {
      return path;
    }
  }
  throw new Error(
    'Rust host binary not found. Set ODY_HOST_BINARY_PATH or build with `pnpm run build:host`.',
  );
}
```

- [ ] 运行测试并确认通过：

```bash
pnpm --filter integration-tests vitest run test/parity/rust-binary.test.ts
```

预期：2 个用例通过。

- [ ] 提交：

```bash
git add packages/integration-tests/src/parity/rust-binary.ts \
           packages/integration-tests/test/parity/rust-binary.test.ts
git commit -m "feat(integration-tests): rust host binary resolver"
```

---

### Task C2: runParityWithGaps (Known-Gap Integration)

**Depends on:** `core.md: Task A3`, `scenarios.md: Task B5`

**Files:**
- Modify: `packages/integration-tests/src/parity/run-parity.ts`
- Create: `packages/integration-tests/test/parity/run-parity-with-gaps.test.ts`

**Goal:** 在不改变 `runParity` 签名的前提下，新增 `runParityWithGaps`：若 diff 为 null 但存在已登记 gap，则抛 `StaleGapError`；若 diff 非 null 且存在 gap，则标记为 known-failure 但仍让测试通过。

- [ ] 在 `packages/integration-tests/test/parity/run-parity-with-gaps.test.ts` 写入测试：

```ts
import { describe, expect, it } from 'vitest';
import { runParityWithGaps } from '../../src/parity/run-parity';
import { StaleGapError } from '../../src/parity/known-gaps';
import type { ParityBackend, Scenario } from '../../src/parity/types';

function fakeBackend(homeDir: string, kind: 'ts' | 'rust'): ParityBackend {
  const listeners = new Set<(event: unknown) => void>();
  return {
    kind,
    homeDir,
    client: {
      onEvent(listener: (event: unknown) => void) {
        listeners.add(listener);
        return () => listeners.delete(listener);
      },
    } as any,
    close: async () => {},
  };
}

const passingScenario: Scenario = {
  name: 'passing',
  async run() {
    return { responses: ['ok'], events: [] };
  },
};

const failingScenario: Scenario = {
  name: 'failing',
  async run(backend) {
    return { responses: [backend.kind], events: [] };
  },
};

describe('runParityWithGaps', () => {
  it('passes when diff is null and no gap is registered', async () => {
    const result = await runParityWithGaps({
      scenario: passingScenario,
      mockLlm: {} as any,
      makeA: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
      makeB: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
      knownGaps: [],
    });
    expect(result.passed).toBe(true);
    expect(result.diff).toBeNull();
    expect(result.gapReason).toBeUndefined();
  });

  it('passes when diff exists but an L3 gap is registered', async () => {
    const result = await runParityWithGaps({
      scenario: failingScenario,
      mockLlm: {} as any,
      makeA: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
      makeB: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'rust')),
      knownGaps: [{ scenario: 'failing', layer: 'L3', reason: 'mock mismatch' }],
    });
    expect(result.passed).toBe(true);
    expect(result.diff).not.toBeNull();
    expect(result.gapReason).toBe('mock mismatch');
  });

  it('passes when diff exists but an L4 wildcard gap is registered', async () => {
    const result = await runParityWithGaps({
      scenario: failingScenario,
      mockLlm: {} as any,
      makeA: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
      makeB: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'rust')),
      knownGaps: [{ scenario: '*', layer: 'L4', reason: 'records not migrated' }],
    });
    expect(result.passed).toBe(true);
    expect(result.diff).not.toBeNull();
  });

  it('throws StaleGapError when diff is null but gap is registered', async () => {
    await expect(
      runParityWithGaps({
        scenario: passingScenario,
        mockLlm: {} as any,
        makeA: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
        makeB: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
        knownGaps: [{ scenario: 'passing', layer: 'L3', reason: 'mock mismatch' }],
      }),
    ).rejects.toBeInstanceOf(StaleGapError);
  });
});
```

- [ ] 运行测试并确认失败：

```bash
pnpm --filter integration-tests vitest run test/parity/run-parity-with-gaps.test.ts
```

预期失败：`runParityWithGaps` 未导出。

- [ ] 修改 `packages/integration-tests/src/parity/run-parity.ts`，追加：

```ts
import { checkGapState, findGap, type KnownGap, StaleGapError } from './known-gaps';

export interface RunParityWithGapsResult {
  readonly diff: ParityDiff | null;
  readonly gapReason: string | undefined;
  readonly passed: boolean;
}

export async function runParityWithGaps(
  options: RunParityOptions & { readonly knownGaps: readonly KnownGap[] },
): Promise<RunParityWithGapsResult> {
  const { knownGaps, scenario } = options;
  const diff = await runParity(options);

  const l3Reason = findGap(knownGaps, scenario.name, 'L3');
  const l4Reason = findGap(knownGaps, scenario.name, 'L4');
  const gapReason = l3Reason ?? l4Reason;

  if (diff === null) {
    if (l3Reason !== undefined) checkGapState(knownGaps, scenario.name, 'L3', true);
    if (l4Reason !== undefined) checkGapState(knownGaps, scenario.name, 'L4', true);
  }

  return {
    diff,
    gapReason,
    passed: diff === null || gapReason !== undefined,
  };
}
```

- [ ] 运行测试并确认通过：

```bash
pnpm --filter integration-tests vitest run test/parity/run-parity-with-gaps.test.ts
```

预期：4 个用例通过。

- [ ] 运行 `integration-tests` typecheck：

```bash
pnpm --filter integration-tests typecheck
```

预期：无编译错误。

- [ ] 提交：

```bash
git add packages/integration-tests/src/parity/run-parity.ts \
           packages/integration-tests/test/parity/run-parity-with-gaps.test.ts
git commit -m "feat(integration-tests): runParityWithGaps known-gap integration"
```

---

### Task C3: TS-vs-Rust Parity Tests

**Depends on:** Task C1, Task C2, `scenarios.md: Task B5`

**Files:**
- Modify: `packages/integration-tests/src/parity/known-gaps.md`
- Create: `packages/integration-tests/test/parity/ts-vs-rust.test.ts`

**Goal:** 对三个 scenario 运行 TS-vs-Rust 比对；为 Rust 后端 mock provider 未对齐的 L3 差异登记 known gap；若二进制不存在则优雅跳过。

- [ ] 修改 `packages/integration-tests/src/parity/known-gaps.md`，最终内容：

```markdown
# Parity Known Gaps

| Scenario | Layer | Reason |
|---|---|---|
| hello-world | L3 | Rust 后端 mock provider 未注入，事件 payload 暂不对齐 |
| file-edit | L3 | Rust 后端 mock provider 未注入，事件 payload 暂不对齐 |
| multi-turn-tool | L3 | Rust 后端 mock provider 未注入，事件 payload 暂不对齐 |
| * | L4 | records 持久化 4.3 才迁移 |
```

- [ ] 在 `packages/integration-tests/test/parity/ts-vs-rust.test.ts` 写入测试：

```ts
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'pathe';
import { describe, expect, it } from 'vitest';
import { makeTsBackend, makeRustBackend } from '../../src/parity/backends';
import { parseKnownGaps } from '../../src/parity/known-gaps';
import { runParityWithGaps } from '../../src/parity/run-parity';
import {
  helloWorldScenario,
  helloWorldMockLlm,
  fileEditScenario,
  fileEditMockLlm,
  multiTurnToolScenario,
  multiTurnToolMockLlm,
} from '../../src/parity/scenarios';
import { resolveRustBinaryPath } from '../../src/parity/rust-binary';

const knownGapsSource = readFileSync(
  join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'src', 'parity', 'known-gaps.md'),
  'utf8',
);
const knownGaps = parseKnownGaps(knownGapsSource);

const binaryPath = (() => {
  try {
    return resolveRustBinaryPath();
  } catch {
    return null;
  }
})();

const cases = [
  { scenario: helloWorldScenario, mockLlm: helloWorldMockLlm },
  { scenario: fileEditScenario, mockLlm: fileEditMockLlm },
  { scenario: multiTurnToolScenario, mockLlm: multiTurnToolMockLlm },
];

describe.skipIf(binaryPath === null)('TS-vs-Rust parity', () => {
  it.each(cases)('$scenario.name passes or is covered by a known gap', async ({ scenario, mockLlm }) => {
    const result = await runParityWithGaps({
      scenario,
      mockLlm,
      makeA: (homeDir) => makeTsBackend({ homeDir, mockLlm }),
      makeB: (homeDir) =>
        makeRustBackend({
          homeDir,
          binaryPath: binaryPath!,
          transport: 'stdio',
        }),
      knownGaps,
      timeoutMs: 60000,
    });
    expect(result.passed).toBe(true);
  });
});
```

- [ ] 构建 Rust host（若尚未构建）：

```bash
pnpm run build:host
```

预期：二进制生成到 `rust-ody/target/release/ody-host`（或项目配置的等效路径）。

- [ ] 运行测试：

```bash
pnpm --filter integration-tests vitest run test/parity/ts-vs-rust.test.ts
```

预期：若二进制存在，3 个用例全部通过（当前因 known gap 覆盖 L3 差异）；若二进制不存在，测试套件被跳过。

- [ ] 手动验证：临时移除 `known-gaps.md` 中任意 scenario 的 L3 行，重新运行上一条命令，确认对应 scenario 失败（证明 gap 确实在生效）。完成后恢复该行。

- [ ] 提交：

```bash
git add packages/integration-tests/src/parity/known-gaps.md \
           packages/integration-tests/test/parity/ts-vs-rust.test.ts
git commit -m "feat(integration-tests): TS-vs-Rust parity tests with known gaps"
```

---

## Part 3 本地 Self-Review

| 检查项 | 结论 |
|---|---|
| 1. Spec-coverage | Rust 二进制解析 → C1；known-gap 集成 → C2；TS-vs-Rust 端到端 → C3。 |
| 2. Placeholder scan | 无 TODO/TBD；所有代码均可执行。 |
| 3. No phantom tasks | 每个任务都产生可运行测试或可验证配置。 |
| 4. Dependency soundness | C1 独立；C2 依赖 A3 + B5；C3 依赖 C1 + C2 + B2/B3/B4。 |
| 5. Caller & build soundness | 仅扩展 `run-parity.ts` 新增导出，未改变 `runParity` 签名；C2 结束有 typecheck。 |
| 6. Test-the-risk | C2 覆盖「diff+gap 通过」「无 gap 失败」「gap 过期抛错」三种状态；C3 手动验证 gap 生效。 |
| 7. Type consistency | `KnownGap`、`ParityDiff`、`ParityBackend` 均复用 Part 1 类型；`makeRustBackend` 复用 A5 签名。 |

- [ ] 1. Spec-coverage table: binary resolver → C1, known-gap integration → C2, TS-vs-Rust tests → C3.
- [ ] 2. Placeholder scan: 无 TODO/TBD/占位符。
- [ ] 3. No phantom tasks: 每个任务都有新文件或可验证配置变更。
- [ ] 4. Dependency soundness: C1 独立；C2 依赖 `core.md` A3 与 `scenarios.md` B5；C3 依赖 C1/C2 与 Part 2 scenarios。
- [ ] 5. Caller & build soundness: `runParity` 签名不变；C2 运行 `pnpm --filter integration-tests typecheck`。
- [ ] 6. Test-the-risk: C2 覆盖 diff+gap、无 gap、stale gap 三种状态；C3 手动验证 gap 生效。
- [ ] 7. Type一致性: 复用 Part 1/Part 2 已定义类型与签名。

