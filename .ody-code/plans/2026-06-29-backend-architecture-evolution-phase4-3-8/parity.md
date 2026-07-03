# Part 5 — parity.md

## 范围

本部分为 roadmap 4.3.8 的后台任务 / cron 子系统补齐 parity 验证层：

- **归一化正确性**：验证 `normalizeBackgroundCronSnapshot` 不会误杀 must-survive 的语义字段。
- **TS↔TS 自比对**：同一 fixture 用 TS driver 跑两次，归一化后必须完全一致，以发现 driver 本身的非确定性。
- **TS↔Rust 对照**：TS driver 与 Rust `background_cron_l3` binary 对同一 fixture 输出进行 parity 对比。
- **CI / 验证命令**：把 Rust binary 构建、TS typecheck、parity 测试串成可一键执行的脚本，并接入 `rust-host.yml`。

本 Part 不复写 `integration.md` 中已经落地的 `background-cron-l3-parity.test.ts`，而是在其侧翼增加独立的 normalizer 测试、TS↔TS 测试、框架对齐的 cross-check 测试以及 CI 入口。

---

## 依赖总览

- `integration.md` Task 4：提供 `runBackgroundCronL3Fixture`（TS driver）。
- `integration.md` Task 5：提供 fixture 文件、`normalizeBackgroundCronSnapshot`。
- `integration.md` Task 6：提供 `background_cron_l3` binary 与 npm script 注册。
- 既有 `packages/integration-tests/src/parity/assert-parity.ts`：提供通用 diff 断言。

本 Part 无新增外部依赖。

---

## 阶段划分

- **Phase A（归一化正确性）**：Task 1。
- **Phase B（TS↔TS 自比对）**：Task 2。
- **Phase C（TS↔Rust 对照）**：Task 3。
- **Phase D（CI 入口）**：Task 4。

---

## 文件结构

```
packages/integration-tests/
├── src/parity/background-cron-parity.ts          # new：runTsSnapshot / runRustSnapshot / diff 包装
├── test/parity/background-cron-normalize.test.ts # new：normalizer 正确性测试
├── test/parity/background-cron-ts-vs-ts.test.ts  # new：TS↔TS 自比对
├── test/parity/background-cron-cross.test.ts     # new：TS↔Rust cross-check
└── package.json                                  # modify：扩展 test:parity:background-cron

.github/workflows/rust-host.yml                   # modify：新增 background-cron parity job step
```

---

## Task 1：归一化函数正确性测试

**Depends on:** `integration.md` Task 5。

**Files:**
- Create: `packages/integration-tests/test/parity/background-cron-normalize.test.ts`

### 步骤 1.1：写入失败测试

创建 `packages/integration-tests/test/parity/background-cron-normalize.test.ts`：

```ts
import { describe, expect, it } from 'vitest';

import { normalizeBackgroundCronSnapshot } from '../../src/parity/normalize-background-cron';

describe('normalizeBackgroundCronSnapshot', () => {
  it('masks dynamic background ids and timestamps but keeps semantic fields', () => {
    const normalized = normalizeBackgroundCronSnapshot({
      backgroundTasks: [
        {
          taskId: 'bash-a1b2c3d4',
          kind: 'process',
          description: 'echo done',
          status: 'completed',
          startedAt: 1_700_000_000_000,
          endedAt: 1_700_000_001_000,
          stopReason: undefined,
        },
      ],
    });

    const tasks = (normalized as { backgroundTasks: Record<string, unknown>[] }).backgroundTasks;
    expect(tasks[0]!.taskId).toBe('<bg-id>');
    expect(tasks[0]!.startedAt).toBe('<timestamp>');
    expect(tasks[0]!.endedAt).toBe('<timestamp>');
    expect(tasks[0]!.description).toBe('echo done');
    expect(tasks[0]!.status).toBe('completed');
    expect(tasks[0]!.kind).toBe('process');
  });

  it('masks dynamic cron ids and timestamps but keeps semantic fields', () => {
    const normalized = normalizeBackgroundCronSnapshot({
      cronTasks: [
        {
          id: 'deadbeef',
          cron: '* * * * *',
          prompt: 'ping',
          recurring: true,
          createdAt: 1_700_000_000_000,
          lastFiredAt: 1_700_000_060_000,
        },
      ],
    });

    const tasks = (normalized as { cronTasks: Record<string, unknown>[] }).cronTasks;
    expect(tasks[0]!.id).toBe('<cron-id>');
    expect(tasks[0]!.createdAt).toBe('<timestamp>');
    expect(tasks[0]!.lastFiredAt).toBe('<timestamp>');
    expect(tasks[0]!.cron).toBe('* * * * *');
    expect(tasks[0]!.prompt).toBe('ping');
    expect(tasks[0]!.recurring).toBe(true);
  });

  it('replaces cron/background injected XML text with a stable placeholder', () => {
    const normalized = normalizeBackgroundCronSnapshot({
      contextInputs: [
        { text: '<cron-fire cron="* * * * *"><prompt>ping</prompt></cron-fire>', originKind: 'cron_job' },
        { text: 'plain user message', originKind: 'user' },
      ],
    });

    const inputs = (normalized as { contextInputs: Record<string, unknown>[] }).contextInputs;
    expect(inputs[0]!.text).toBe('<injected-xml>');
    expect(inputs[1]!.text).toBe('plain user message');
  });

  it('does not mask must-survive values that happen to look like ids or timestamps', () => {
    const normalized = normalizeBackgroundCronSnapshot({
      meta: {
        // "ping" 与 8-hex id 正则不匹配，必须保留
        prompt: 'ping',
        // 描述里包含 "bash" 但不是 taskId 字段，必须保留
        description: 'bash wrapper script',
        // 状态词必须保留
        status: 'completed',
        // cron 表达式必须保留
        schedule: '*/5 * * * *',
        // 非 timestamp 键的数字必须保留
        retryCount: 3,
      },
    });

    const meta = (normalized as { meta: Record<string, unknown> }).meta;
    expect(meta.prompt).toBe('ping');
    expect(meta.description).toBe('bash wrapper script');
    expect(meta.status).toBe('completed');
    expect(meta.schedule).toBe('*/5 * * * *');
    expect(meta.retryCount).toBe(3);
  });
});
```

运行：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/background-cron-normalize.test.ts
```

**预期结果：** 若 `integration.md` Task 5 尚未实现，测试因无法解析 `normalizeBackgroundCronSnapshot` 而失败；若已实现，则测试开始运行并可能因实现细节差异而失败（用于驱动修正）。

### 步骤 1.2：修正归一化常量（如需要）

如果失败是由 `normalize-background-cron.ts` 的常量误杀 must-survive 输入导致，按以下顺序修正：

1. 检查 `normalizeScalar` 中 id/timestamp 正则是否过于宽泛。
2. 检查对象字段白名单是否遗漏了 `schedule` / `retryCount` 等 must-survive 字段。
3. 重新运行测试直到通过。

> 本任务只改测试和（必要时）归一化实现，不改 fixture 或 driver。

### 步骤 1.3：运行测试

```bash
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/background-cron-normalize.test.ts
```

**预期结果：** 4 个测试全部通过。

---

## Task 2：TS↔TS 自比对

**Depends on:** `integration.md` Task 4、Task 5、Task 1。

**Files:**
- Create: `packages/integration-tests/src/parity/background-cron-parity.ts`
- Create: `packages/integration-tests/test/parity/background-cron-ts-vs-ts.test.ts`

### 步骤 2.1：写入失败测试

创建 `packages/integration-tests/test/parity/background-cron-ts-vs-ts.test.ts`：

```ts
import { describe, expect, it } from 'vitest';

import {
  assertNoDiff,
  backgroundCronFixtures,
  runTsSnapshot,
} from '../../src/parity/background-cron-parity';

describe('background-cron TS↔TS parity', () => {
  it.each(backgroundCronFixtures)(
    '%s produces identical normalized snapshots on two TS runs',
    async (fixtureName) => {
      // 串行运行，避免同时修改 process.env 的 ODY_CRON_CLOCK / ODY_CRON_MANUAL_TICK
      const first = await runTsSnapshot(fixtureName);
      const second = await runTsSnapshot(fixtureName);

      expect(() => assertNoDiff(fixtureName, first, second)).not.toThrow();
    },
    120_000,
  );
});
```

运行：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/background-cron-ts-vs-ts.test.ts
```

**预期结果：** 编译/解析失败，`background-cron-parity.ts` 不存在。

### 步骤 2.2：实现 parity 包装器

创建 `packages/integration-tests/src/parity/background-cron-parity.ts`：

```ts
import { execFile } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

import { assertParity } from './assert-parity';
import { runBackgroundCronL3Fixture } from './background-cron-l3-driver';
import { normalizeBackgroundCronSnapshot } from './normalize-background-cron';
import type { BackgroundCronSnapshot } from './background-cron-fixture';
import type { NormalizedSnapshot } from './types';

const execFileAsync = promisify(execFile);

const __dirname = dirname(fileURLToPath(import.meta.url));
const fixturesDir = join(__dirname, '../../test/parity/fixtures/background-cron');
const projectRoot = dirname(dirname(dirname(__dirname)));

export const backgroundCronFixtures = [
  'cron-fire.json',
  'background-process-completes.json',
  'cron-remove-last.json',
];

export async function runTsSnapshot(fixtureName: string): Promise<NormalizedSnapshot> {
  const snapshot = await runBackgroundCronL3Fixture(join(fixturesDir, fixtureName));
  return normalize(snapshot);
}

export async function runRustSnapshot(fixtureName: string): Promise<NormalizedSnapshot> {
  const fixturePath = join(fixturesDir, fixtureName);
  const { stdout } = await execFileAsync(
    'cargo',
    ['run', '--quiet', '--bin', 'background_cron_l3', '--', fixturePath],
    { cwd: join(projectRoot, 'rust-ody') },
  );
  const snapshot = JSON.parse(stdout) as BackgroundCronSnapshot;
  return normalize(snapshot);
}

export function assertNoDiff(name: string, a: NormalizedSnapshot, b: NormalizedSnapshot): void {
  const diff = assertParity(name, a, b);
  if (diff !== null) {
    throw new Error(`parity diff in ${name}: ${JSON.stringify(diff.diffs, null, 2)}`);
  }
}

function normalize(snapshot: BackgroundCronSnapshot): NormalizedSnapshot {
  return normalizeBackgroundCronSnapshot(snapshot) as NormalizedSnapshot;
}
```

### 步骤 2.3：运行测试

```bash
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/background-cron-ts-vs-ts.test.ts
```

**预期结果：** 三个 fixture 全部通过。如果失败，先检查 `runBackgroundCronL3Fixture` 是否使用了非确定性 ID 或时间戳未被 `normalizeBackgroundCronSnapshot` 覆盖，再决定是否调整 driver 或 normalizer。

---

## Task 3：TS↔Rust 对照

**Depends on:** Task 2、`integration.md` Task 3/5/6。

**Files:**
- Create: `packages/integration-tests/test/parity/background-cron-cross.test.ts`

### 步骤 3.1：写入测试

创建 `packages/integration-tests/test/parity/background-cron-cross.test.ts`：

```ts
import { describe, expect, it } from 'vitest';

import {
  assertNoDiff,
  backgroundCronFixtures,
  runRustSnapshot,
  runTsSnapshot,
} from '../../src/parity/background-cron-parity';

describe('background-cron TS↔Rust parity', () => {
  it.each(backgroundCronFixtures)(
    '%s matches between TS driver and Rust binary',
    async (fixtureName) => {
      const tsSnapshot = await runTsSnapshot(fixtureName);
      const rustSnapshot = await runRustSnapshot(fixtureName);

      expect(() => assertNoDiff(fixtureName, tsSnapshot, rustSnapshot)).not.toThrow();
    },
    120_000,
  );
});
```

运行：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/background-cron-cross.test.ts
```

**预期结果：** 第一次运行时，`cargo run` 会自动编译 `background_cron_l3`（若未编译），随后执行对比。若 Rust/TS snapshot 在归一化后仍有差异，测试失败并打印 diff 路径。

### 步骤 3.2：收敛 shape 差异

如果测试失败：

1. 查看 `assertNoDiff` 抛出的 diff 路径。
2. 若差异来自时间戳、id、pid、exitCode、command 等已知非语义字段，优先扩展 `normalizeBackgroundCronSnapshot`（见 `integration.md` Task 5）。
3. 若差异来自事件顺序或字段命名（例如 `cronTasks` vs `cron_tasks`），统一 TS/Rust driver 的输出字段名，或在 normalizer 中做 key 映射。
4. 重新运行直到通过。

### 步骤 3.3：运行测试

```bash
pnpm --filter @odysseythink/integration-tests exec vitest run test/parity/background-cron-cross.test.ts
```

**预期结果：** 三个 fixture 全部通过。

---

## Task 4：CI / 验证命令

**Depends on:** Task 2、Task 3、`integration.md` Task 6。

**Files:**
- Modify: `packages/integration-tests/package.json`
- Modify: `packages/integration-tests/src/parity/background-cron-parity.ts`
- Modify: `.github/workflows/rust-host.yml`

> 本任务为 wiring / CI 配置，采用“完整代码 + 构建 + 手动验证”模式。

### 步骤 4.1：扩展 npm script

修改 `packages/integration-tests/package.json` 中的 `test:parity:background-cron`：

```json
    "test:parity:background-cron": "vitest run test/parity/background-cron-l3-parity.test.ts test/parity/background-cron-normalize.test.ts test/parity/background-cron-ts-vs-ts.test.ts test/parity/background-cron-cross.test.ts",
```

> 若 `integration.md` Task 6 尚未写入该脚本，则直接新增上述键值对。

### 步骤 4.2：让 `runRustSnapshot` 支持预编译 binary

修改 `packages/integration-tests/src/parity/background-cron-parity.ts` 中的 `runRustSnapshot`，优先使用环境变量 `ODY_BACKGROUND_CRON_BINARY_PATH`，未设置时回退到 `cargo run`：

```ts
export async function runRustSnapshot(fixtureName: string): Promise<NormalizedSnapshot> {
  const fixturePath = join(fixturesDir, fixtureName);
  const binaryPath = process.env['ODY_BACKGROUND_CRON_BINARY_PATH'];

  let stdout: string;
  if (binaryPath !== undefined && binaryPath.length > 0) {
    ({ stdout } = await execFileAsync(binaryPath, [fixturePath]));
  } else {
    ({ stdout } = await execFileAsync(
      'cargo',
      ['run', '--quiet', '--bin', 'background_cron_l3', '--', fixturePath],
      { cwd: join(projectRoot, 'rust-ody') },
    ));
  }

  const snapshot = JSON.parse(stdout) as BackgroundCronSnapshot;
  return normalize(snapshot);
}
```

### 步骤 4.3：更新 GitHub Actions workflow

在 `.github/workflows/rust-host.yml` 中，紧邻 `Build kaos-golden binary` 之后新增两步：

```yaml
      - name: Build background_cron_l3 binary
        run: cargo build -p agent-rs --bin background_cron_l3
        working-directory: rust-ody

      - name: background-cron L3 parity
        id: background-cron-parity
        run: pnpm --filter @odysseythink/integration-tests test:parity:background-cron
        shell: bash
        env:
          ODY_BACKGROUND_CRON_BINARY_PATH: ${{ github.workspace }}/rust-ody/target/debug/background_cron_l3
```

再在上传的 artifact 步骤中增加 background-cron 失败产物（可选，但推荐）：

```yaml
      - name: Upload background-cron parity diff artifacts
        if: failure() && steps.background-cron-parity.outcome == 'failure'
        uses: actions/upload-artifact@v4
        with:
          name: background-cron-parity-diffs-${{ matrix.target }}-${{ matrix.transport }}
          path: .ody-code/reports/parity/**
          if-no-files-found: ignore
```

### 步骤 4.4：手动验证

构建 Rust binary：

```bash
cd /Users/ranwei/workspace/ody-code/rust-ody
cargo build -p agent-rs --bin background_cron_l3
```

**预期结果：** 编译成功。

TS typecheck：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm --filter @odysseythink/integration-tests typecheck
```

**预期结果：** `tsc` 无错。

运行完整 background-cron parity suite：

```bash
export ODY_BACKGROUND_CRON_BINARY_PATH=/Users/ranwei/workspace/ody-code/rust-ody/target/debug/background_cron_l3
pnpm --filter @odysseythink/integration-tests test:parity:background-cron
```

**预期结果：** 所有测试通过。

---

## Self-Review（本 Part）

### Spec-coverage 表

| 需求 | 覆盖任务 | 状态 |
|---|---|---|
| 验证 `normalizeBackgroundCronSnapshot` 不误杀 must-survive 字段 | Task 1 | covered |
| TS driver 两次运行输出一致（自比对） | Task 2 | covered |
| TS driver 与 Rust binary 对同一 fixture 输出一致 | Task 3 | covered |
| npm script 一键运行完整 background-cron parity suite | Task 4 | covered |
| CI workflow 构建 binary 并运行 parity | Task 4 | covered |

### 七项检查

- [ ] 1. Spec-coverage 表已覆盖本 Part 全部目标，无 GAP。
- [ ] 2. Placeholder 扫描：无 `TODO`/`TBD`/“后续实现”，所有代码均可直接编译运行。
- [ ] 3. 无 phantom task：每个任务都产生可验证的变更（测试通过 / script 可用 / CI step 可执行）。
- [ ] 4. 依赖正确：`Depends on:` 均指向前置任务或 `integration.md` 已定义产物；Task 4 的 wrapper 修改在同一任务内完成，无跨任务悬空依赖。
- [ ] 5. Caller & build 正确性：本 Part 无共享签名变更；Task 4 修改的 `runRustSnapshot` 是其所在文件内部实现，调用方（Task 2/3 测试）接口不变。最终需以 `pnpm --filter @odysseythink/integration-tests typecheck` 做全包类型检查。
- [ ] 6. 测试了风险：Task 1 显式枚举 must-survive 输入（`ping`、`bash wrapper script`、`completed`、`*/5 * * * *`、`retryCount`）并验证它们不会被误杀；Task 2/3 用 `assertParity` 断言两个 snapshot 完全一致，diff 非空即失败。
- [ ] 7. 类型一致性：Task 2 包装器复用 `integration.md` 定义的 `BackgroundCronSnapshot` 与 `normalizeBackgroundCronSnapshot` 类型；`assertParity` 使用既有 `NormalizedSnapshot` 类型，运行时通过类型断言兼容 background-cron 的自定义 snapshot 形状。
