/**
 * PoC benchmark: pure-JS `estimateTokens` vs the Rust/Wasm version.
 *
 * Run: pnpm tsx rust-ody/ts/bench.ts
 *
 * Two things are validated:
 *   1. Correctness — Wasm output must equal JS output for every sample.
 *   2. Performance — across input sizes, to expose the JS<->Wasm boundary cost
 *      on small inputs vs the compute win on large inputs.
 */
import { estimateTokens as estimateTokensJs } from '../../packages/agent-core/src/utils/tokens.ts';
import { loadWasmTokenEstimator } from './wasm-tokens.ts';

function makeSample(size: number): string {
  // Mixed ASCII + CJK, roughly mirroring source code + comments + prose.
  const unit = 'function add(a, b) { return a + b; } // 计算两数之和，返回结果。\n';
  return unit.repeat(Math.max(1, Math.ceil(size / unit.length))).slice(0, size);
}

function timeIt(label: string, fn: () => void, iterations: number): number {
  // warmup
  for (let i = 0; i < Math.min(iterations, 1000); i++) fn();
  const start = process.hrtime.bigint();
  for (let i = 0; i < iterations; i++) fn();
  const ns = Number(process.hrtime.bigint() - start);
  const perCall = ns / iterations;
  console.log(`  ${label.padEnd(10)} ${(perCall).toFixed(1).padStart(10)} ns/call`);
  return perCall;
}

async function main() {
  const estimateTokensWasm = await loadWasmTokenEstimator();

  const sizes = [
    { name: 'tiny (12B)', size: 12 },
    { name: 'small (200B)', size: 200 },
    { name: 'medium (4KB)', size: 4 * 1024 },
    { name: 'large (64KB)', size: 64 * 1024 },
    { name: 'huge (512KB)', size: 512 * 1024 },
  ];

  console.log('=== correctness (Wasm must equal JS) ===');
  let allOk = true;
  for (const { name, size } of sizes) {
    const sample = makeSample(size);
    const js = estimateTokensJs(sample);
    const wasm = estimateTokensWasm(sample);
    const ok = js === wasm;
    allOk &&= ok;
    console.log(`  ${ok ? 'OK ' : 'FAIL'} ${name.padEnd(14)} js=${js} wasm=${wasm}`);
  }
  // edge cases
  for (const s of ['', 'a', 'abcd', 'abcde', '你好世界', '🚀🔥', 'a你b好c']) {
    const ok = estimateTokensJs(s) === estimateTokensWasm(s);
    allOk &&= ok;
    if (!ok) console.log(`  FAIL edge ${JSON.stringify(s)}: js=${estimateTokensJs(s)} wasm=${estimateTokensWasm(s)}`);
  }
  console.log(`  -> ${allOk ? 'ALL MATCH' : 'MISMATCH DETECTED'}\n`);

  console.log('=== performance (lower is better) ===');
  for (const { name, size } of sizes) {
    const sample = makeSample(size);
    // Fewer iterations for big inputs to keep total runtime sane.
    const iterations = size <= 200 ? 2_000_000 : size <= 4096 ? 200_000 : size <= 65536 ? 20_000 : 3_000;
    console.log(`${name} (x${iterations.toLocaleString()}):`);
    const js = timeIt('JS', () => estimateTokensJs(sample), iterations);
    const wasm = timeIt('Wasm', () => estimateTokensWasm(sample), iterations);
    const ratio = js / wasm;
    const verdict = ratio >= 1 ? `Wasm ${ratio.toFixed(2)}x faster` : `Wasm ${(1 / ratio).toFixed(2)}x SLOWER`;
    console.log(`  -> ${verdict}\n`);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
