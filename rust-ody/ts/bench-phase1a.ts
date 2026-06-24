/**
 * Phase 1-A benchmark: measure Wasm compute-hotspot latency vs JS fallbacks.
 *
 * Run: pnpm tsx rust-ody/ts/bench-phase1a.ts
 */
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { dirname } from 'node:path';

import { loadWasmDiffModule } from '../../packages/agent-core/src/utils/wasm-diff';
import { loadWasmGlobMatcher } from '../../packages/agent-core/src/utils/wasm-glob';

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

async function benchDiff(bytes: Uint8Array): Promise<Section> {
  console.log('benchDiff: loading wasm diff with bytes of length', bytes.length);
  const wasm = await loadWasmDiffModule(bytes, { ODY_CODE_EXPERIMENTAL_WASM_DIFF: '1' });
  console.log('benchDiff: wasm loaded, testing small...');
  // Quick sanity check
  wasm.computeTextDiff('a', 'b');
  console.log('benchDiff: sanity ok, loading js...');
  const js = await loadWasmDiffModule(bytes, { ODY_CODE_EXPERIMENTAL_WASM_DIFF: '0' });
  console.log('benchDiff: js loaded');

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
    '> Tokenizer: Wasm BPE suspended — the embedded rank data (~5 MB) exceeded the 2 MB Wasm threshold. The JS heuristic remains the default.',
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
    '- Diff: keep Wasm if it is faster or within 20% of JS; the unified diff from `similar` is higher quality than the JS fallback.',
    '- Glob: the conservative implementation always falls back to picomatch, so expect overhead. If average overhead exceeds 2x, disable `wasm-glob` or add a supported-pattern fast-path.',
  );
  return lines.join('\n');
}

async function main() {
  const bytes = await wasmBytes();
  const sections = await Promise.all([benchDiff(bytes), benchGlob(bytes)]);
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
