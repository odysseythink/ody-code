# Part 6: 基准报告与最终验证

Scope: 建立 Phase 1-A 三热点的性能/收益基准脚本，输出 Markdown 报告；在 `Agent` 构造时触发 Wasm 初始化，使 `estimateTokens` 与 `globMatch` 在运行期自动切换到 Wasm；最后执行全树类型检查、单测与基准，确认无回归。

---

### Task 16: Phase 1-A 基准脚本与报告生成

**Depends on:** Part 3/4/5 (Task 9/12/15)

**Files:**
- Create: `rust-ody/ts/bench-phase1a.ts:1-180`

**Goal:** 编写可独立运行的基准脚本，对比 Tokenizer / Diff / Glob 在 Wasm 与 JS fallback 下的单调用延迟，输出控制台摘要并写入 `.ody-code/reports/phase1a-bench.md`。

**Step-by-step:**

- [ ] 创建 `rust-ody/ts/bench-phase1a.ts`：

```typescript
/**
 * Phase 1-A benchmark: measure Wasm compute-hotspot latency vs JS fallbacks.
 *
 * Run: pnpm tsx rust-ody/ts/bench-phase1a.ts
 */
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname } from 'pathe';

import { loadWasmTokenizerEstimator } from '../../packages/agent-core/src/utils/wasm-tokenizer.ts';
import { loadWasmDiffModule } from '../../packages/agent-core/src/utils/wasm-diff.ts';
import { loadWasmGlobMatcher } from '../../packages/agent-core/src/utils/wasm-glob.ts';

const WASM_PATH = fileURLToPath(
  new URL('../target/wasm32-unknown-unknown/release/ody_rust.wasm', import.meta.url),
);

async function wasmBytes(): Promise<Uint8Array> {
  return new Uint8Array(await readFile(WASM_PATH));
}

function makeCodeSample(size: number): string {
  const unit = 'function add(a: number, b: number): number { return a + b; } // 计算两数之和\n';
  return unit.repeat(Math.max(1, Math.ceil(size / unit.length))).slice(0, size);
}

function timeIt(fn: () => void, iterations: number): number {
  for (let i = 0; i < Math.min(iterations, 1000); i++) fn();
  const start = process.hrtime.bigint();
  for (let i = 0; i < iterations; i++) fn();
  return Number(process.hrtime.bigint() - start) / iterations;
}

interface Row {
  readonly name: string;
  readonly size: number;
  readonly iterations: number;
  readonly jsNs: number;
  readonly wasmNs: number;
  readonly speedup: number;
}

interface Section {
  readonly title: string;
  readonly rows: readonly Row[];
}

async function benchTokenizer(bytes: Uint8Array): Promise<Section> {
  const wasm = await loadWasmTokenizerEstimator(bytes, {
    ODY_CODE_EXPERIMENTAL_WASM_TOKENIZER: '1',
  });
  const js = await loadWasmTokenizerEstimator(bytes, {
    ODY_CODE_EXPERIMENTAL_WASM_TOKENIZER: '0',
  });

  const sizes = [
    { name: 'tiny', size: 12 },
    { name: 'small', size: 200 },
    { name: 'medium', size: 4 * 1024 },
    { name: 'large', size: 64 * 1024 },
  ];
  const rows: Row[] = [];
  for (const { name, size } of sizes) {
    const text = makeCodeSample(size);
    const iterations = size <= 200 ? 200_000 : size <= 4096 ? 50_000 : 5_000;
    const jsNs = timeIt(() => js(text), iterations);
    const wasmNs = timeIt(() => wasm(text), iterations);
    rows.push({ name, size, iterations, jsNs, wasmNs, speedup: jsNs / wasmNs });
  }
  return { title: 'Tokenizer (BPE vs heuristic)', rows };
}

async function benchDiff(bytes: Uint8Array): Promise<Section> {
  const wasm = await loadWasmDiffModule(bytes, { ODY_CODE_EXPERIMENTAL_WASM_DIFF: '1' });
  const js = await loadWasmDiffModule(bytes, { ODY_CODE_EXPERIMENTAL_WASM_DIFF: '0' });

  const sizes = [
    { name: 'small', size: 200 },
    { name: 'medium', size: 4 * 1024 },
    { name: 'large', size: 64 * 1024 },
  ];
  const rows: Row[] = [];
  for (const { name, size } of sizes) {
    const base = makeCodeSample(size);
    const changed = base.replaceAll('add', 'sum');
    const iterations = size <= 200 ? 50_000 : size <= 4096 ? 10_000 : 1_000;
    const jsNs = timeIt(() => js.computeTextDiff(base, changed), iterations);
    const wasmNs = timeIt(() => wasm.computeTextDiff(base, changed), iterations);
    rows.push({ name, size, iterations, jsNs, wasmNs, speedup: jsNs / wasmNs });
  }
  return { title: 'Diff (similar vs JS LCS)', rows };
}

async function benchGlob(bytes: Uint8Array): Promise<Section> {
  const wasm = await loadWasmGlobMatcher(bytes, { ODY_CODE_EXPERIMENTAL_WASM_GLOB: '1' });
  const js = await loadWasmGlobMatcher(bytes, { ODY_CODE_EXPERIMENTAL_WASM_GLOB: '0' });

  const samples = [
    { name: 'short-match', value: 'src/main.ts', pattern: '*.ts' },
    { name: 'short-no-match', value: 'src/main.js', pattern: '*.ts' },
    { name: 'long-match', value: 'packages/agent-core/src/utils/wasm-tokenizer.ts', pattern: 'packages/**/*.ts' },
    { name: 'brace', value: 'a/b.ts', pattern: 'a/{b,c}.ts' },
  ];
  const rows: Row[] = [];
  for (const { name, value, pattern } of samples) {
    const iterations = 200_000;
    const jsNs = timeIt(() => js(value, pattern), iterations);
    const wasmNs = timeIt(() => wasm(value, pattern), iterations);
    rows.push({ name, size: value.length, iterations, jsNs, wasmNs, speedup: jsNs / wasmNs });
  }
  return { title: 'Glob (globset+picomatch vs picomatch)', rows };
}

function formatNs(ns: number): string {
  if (ns < 1000) return `${ns.toFixed(1)} ns`;
  if (ns < 1_000_000) return `${(ns / 1000).toFixed(2)} µs`;
  return `${(ns / 1_000_000).toFixed(2)} ms`;
}

function renderSection(section: Section): string {
  const lines = [
    `### ${section.title}`,
    '',
    '| name | size | iterations | JS | Wasm | speedup |',
    '|---|---:|---:|---:|---:|---:|',
  ];
  for (const r of section.rows) {
    const verdict =
      r.speedup >= 1
        ? `${r.speedup.toFixed(2)}x faster`
        : `${(1 / r.speedup).toFixed(2)}x slower`;
    lines.push(
      `| ${r.name} | ${r.size} | ${r.iterations.toLocaleString()} | ${formatNs(r.jsNs)} | ${formatNs(
        r.wasmNs,
      )} | ${verdict} |`,
    );
  }
  return lines.join('\n');
}

function renderReport(sections: readonly Section[]): string {
  const lines = [
    '# Phase 1-A Wasm Hotspot Benchmark Report',
    '',
    `Generated: ${new Date().toISOString()}`,
    '',
    '## Summary',
    '',
  ];
  for (const s of sections) {
    const avg = s.rows.reduce((a, r) => a + r.speedup, 0) / s.rows.length;
    lines.push(`- ${s.title}: average speedup ${avg.toFixed(2)}x`);
  }
  lines.push('', '## Details', '');
  for (const s of sections) {
    lines.push(renderSection(s));
    lines.push('');
  }
  lines.push(
    '## Recommendations',
    '',
    '- Tokenizer: Wasm BPE returns exact token counts. Accept if 64 KB latency stays below ~1 ms; otherwise investigate batching or caching.',
    '- Diff: keep Wasm if it is faster or within 20% of JS; the unified diff from `similar` is higher quality than the JS fallback.',
    '- Glob: the conservative implementation always falls back to picomatch, so expect overhead. If average overhead exceeds 2x, disable `wasm-glob` or add a supported-pattern fast-path.',
  );
  return lines.join('\n');
}

async function main() {
  const bytes = await wasmBytes();
  const sections = await Promise.all([benchTokenizer(bytes), benchDiff(bytes), benchGlob(bytes)]);
  const report = renderReport(sections);
  console.log(report);

  const outPath = fileURLToPath(
    new URL('../../.ody-code/reports/phase1a-bench.md', import.meta.url),
  );
  await mkdir(dirname(outPath), { recursive: true });
  await writeFile(outPath, report, 'utf-8');
  console.log(`\nReport written to ${outPath}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
```

- [ ] 确认 `.wasm` 已构建：

```bash
cd rust-ody && cargo build --release --target wasm32-unknown-unknown
```

- [ ] 运行基准脚本：

```bash
pnpm tsx rust-ody/ts/bench-phase1a.ts
```

- [ ] **Manual verification:** 控制台打印三个热点的 latency 表格；目录 `.ody-code/reports/` 下生成 `phase1a-bench.md`，内容包含 Summary、Details、Recommendations。

- [ ] Commit: `feat(rust-ody): phase1a wasm hotspot benchmark`

---

### Task 17: Agent 启动时初始化 Tokenizer / Glob Wasm

**Depends on:** Part 3/5 (Task 8/14)

**Files:**
- Modify: `packages/agent-core/src/agent/index.ts:1-250`

**Goal:** `Agent` 构造时以 fire-and-forget 方式触发 `initTokenizerWasm()` 与 `initGlobWasm()`，让后续 `estimateTokens` / `globMatch` 在 Wasm 就绪后自动切换；不阻塞同步构造，失败时静默回退到 JS。

**Step-by-step:**

- [ ] 修改 `packages/agent-core/src/agent/index.ts`，在现有 `'../utils/tokens'` 导入中追加 `initTokenizerWasm`：

```typescript
import {
  estimateTokens,
  estimateTokensForMessages,
  estimateTokensForTools,
  initTokenizerWasm,
} from '../utils/tokens';
```

- [ ] 新增导入 `initGlobWasm`：

```typescript
import { initGlobWasm } from '../utils/wasm-glob';
```

- [ ] 在 `Agent` 构造函数末尾（`this._setUserLanguageCallback = options.setUserLanguage;` 之后）添加 Wasm 初始化触发：

```typescript
    this.userLanguage = options.userLanguage;
    this._setUserLanguageCallback = options.setUserLanguage;

    // Fire-and-forget: load Wasm compute hotspots in the background. estimateTokens
    // and globMatch already fall back to JS implementations while Wasm is loading or
    // if it fails, so this never blocks construction and never breaks standalone usage.
    void initTokenizerWasm().catch(() => {
      /* fallback is automatic */
    });
    void initGlobWasm().catch(() => {
      /* fallback is automatic */
    });
  }
```

> `initDiffWasm` 不在构造时加载，因为 `formatGitDiff` 仅在 `fetchDiff` 返回前调用一次，不构成持续热点；若后续需要可在 `fetchDiff` 调用前按需初始化。

- [ ] 搜索 `Agent` 构造函数调用点，确认无需新增参数：

```bash
rg -n "new Agent\(" packages/agent-core/src packages/agent-core/test apps/ody-code/src packages/node-sdk/src
```

Expected: 所有调用点仍使用现有 `AgentOptions`；`new Agent({ ... })` 签名未变。

- [ ] 运行类型检查：

```bash
pnpm tsc --noEmit -p packages/agent-core/tsconfig.json
```

Expected: 成功退出。

- [ ] **Manual verification:** 启动 ody-code CLI（或运行一个创建 `Agent` 的集成测试），观察 `estimateTokens` 在首次调用后不再回退到 JS（可通过在 `wasm-tokenizer.ts` 中临时加 `console.log` 验证）；Wasm 加载失败时不抛未处理异常。

- [ ] Commit: `feat(agent-core): init tokenizer/glob wasm on agent construction`

---

### Task 18: 全树验证与回归测试

**Depends on:** Task 16/17 + 所有 Part 1-5

**Files:**
- 无需新增或修改源文件；仅运行命令。

**Goal:** 确认 Rust/Wasm 构建、类型检查、单测、基准均通过；若 `wasm-glob` 报告显示显著倒退，则在 `registry.ts` 中关闭其默认 flag。

**Step-by-step:**

- [ ] Rust 全量测试 + Wasm 构建：

```bash
cd rust-ody && cargo test --quiet && cargo build --release --target wasm32-unknown-unknown
```

Expected: `cargo test` 全部通过；`cargo build` 成功退出。

- [ ] 全树类型检查：

```bash
pnpm typecheck
```

Expected: 成功退出。

- [ ] Phase 1-A parity 与加载器测试：

```bash
pnpm vitest run \
  packages/agent-core/test/utils/wasm-loader.test.ts \
  packages/agent-core/test/utils/wasm-string.test.ts \
  packages/agent-core/test/utils/tokens-wasm-parity.test.ts \
  packages/agent-core/test/code-review/diff-wasm-parity.test.ts \
  packages/agent-core/test/tools/support/glob-wasm-parity.test.ts
```

Expected: 全部通过。

- [ ] 运行基准并检查报告：

```bash
pnpm tsx rust-ody/ts/bench-phase1a.ts
```

Expected: 脚本退出码 0；`.ody-code/reports/phase1a-bench.md` 已生成。

- [ ] **Manual verification / 回退决策:**

  1. 打开 `.ody-code/reports/phase1a-bench.md`。
  2. Tokenizer：确认 64 KB 样本 Wasm 延迟 <1 ms 且结果正确（BPE token 数合理）。
  3. Diff：确认 Wasm 不比 JS 慢超过 20%。
  4. Glob：若平均速度比 JS 慢超过 2x，说明保守回退策略开销过大，执行子步骤 (5)；否则跳过。
  5. （条件执行）关闭 `wasm-glob` 默认 flag：修改 `packages/agent-core/src/flags/registry.ts` 中 `wasm-glob` 的 `default` 为 `false`，运行 `pnpm vitest run packages/agent-core/test/tools/support/glob-wasm-parity.test.ts` 确认 flag 关闭路径通过，提交：`chore(agent-core): disable wasm-glob by default due to overhead`。

- [ ] Commit: `chore(agent-core): phase1a final verification green`

---

## Local Self-Review

- [ ] 1. Spec-coverage: Part 6 覆盖 "G1-A 收益基准报告"、"Agent 启动流程调用 init"、"全树验证与回退决策"。
- [ ] 2. Placeholder scan: 无 TODO/TBD；`bench-phase1a.ts` 给出完整采样、计时、渲染、写文件逻辑；Task 17 给出完整构造器改动。
- [ ] 3. No phantom tasks: Task 16 创建基准脚本；Task 17 修改 Agent 构造器；Task 18 运行验证命令并做回退决策。
- [ ] 4. Dependency soundness: Task 16 依赖 Part 3/4/5 的 `loadWasm*` 函数；Task 17 依赖 Part 3/5 的 `initTokenizerWasm`/`initGlobWasm`；Task 18 依赖 Task 16/17 与所有前置实现。
- [ ] 5. Caller & build soundness: Task 17 未改变 `Agent` 构造器签名；搜索 `new Agent(` 调用点确认无需修改；Task 18 以 `pnpm typecheck` 全树检查收尾。
- [ ] 6. Test-the-risk: 性能风险通过基准脚本量化；正确性风险通过 Task 18 的 parity 测试覆盖；回退决策有明确的 2x 阈值与执行步骤。
- [ ] 7. Type一致性: `loadWasmTokenizerEstimator`、`loadWasmDiffModule`、`loadWasmGlobMatcher` 的导入路径与参数（`wasmBytes`、`LoadContext`）与 Part 3/4/5 一致；`initTokenizerWasm`/`initGlobWasm` 无参数或接受可选 `LoadContext`（构造器使用无参数版本）。
