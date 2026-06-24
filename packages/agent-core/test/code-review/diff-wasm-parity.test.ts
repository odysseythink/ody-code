import { describe, expect, it } from 'vitest';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { loadWasmDiffModule, computeTextDiff, formatGitDiff, initDiffWasm } from '../../src/utils/wasm-diff';

const WASM_PATH = fileURLToPath(
  new URL('../../../../rust-ody/target/wasm32-unknown-unknown/release/ody_rust.wasm', import.meta.url),
);

async function realWasmBytes(): Promise<Uint8Array> {
  return new Uint8Array(await readFile(WASM_PATH));
}

describe('wasm diff parity', () => {
  it('computeTextDiff produces a unified diff', async () => {
    const diff = await loadWasmDiffModule(await realWasmBytes());
    const out = diff.computeTextDiff('a\nb', 'a\nc\nb');
    expect(out).toContain('@@');
    expect(out).toContain('+c');
    expect(out).toContain('--- old');
    expect(out).toContain('+++ new');
  });

  it('formatGitDiff strips trailing whitespace and preserves structure', async () => {
    const diff = await loadWasmDiffModule(await realWasmBytes());
    const raw = 'diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n+b\n ';
    const out = diff.formatGitDiff(raw);
    expect(out).not.toMatch(/ $/);
    expect(out).toContain('diff --git');
    expect(out).toContain('-a');
    expect(out).toContain('+b');
  });

  it('formatGitDiff drops empty hunks', async () => {
    const diff = await loadWasmDiffModule(await realWasmBytes());
    const raw =
      'diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n context\n context\n';
    const out = diff.formatGitDiff(raw);
    expect(out).not.toContain('@@');
  });
});

describe('wasm diff fallback', () => {
  it('initDiffWasm with flag disabled uses JS fallback', async () => {
    await initDiffWasm({ ODY_CODE_EXPERIMENTAL_WASM_DIFF: '0' });
    const out = computeTextDiff('a\nb', 'a\nc\nb');
    expect(out).toContain('+c');
    expect(out).toContain('--- old');
  });

  it('formatGitDiff JS fallback is identity', async () => {
    await initDiffWasm({ ODY_CODE_EXPERIMENTAL_WASM_DIFF: '0' });
    const raw = 'diff --git a/f b/f\n-a\n+b\n';
    expect(formatGitDiff(raw)).toBe(raw);
  });
});
