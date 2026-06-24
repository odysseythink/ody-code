import { describe, expect, it } from 'vitest';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import {
  globMatch,
  pathGlobMatch,
  initGlobWasm,
} from '../../../src/tools/support/path-glob-match';
import { loadWasmGlobMatcher } from '../../../src/utils/wasm-glob';

const WASM_PATH = fileURLToPath(
  new URL('../../../../rust-ody/target/wasm32-unknown-unknown/release/ody_rust.wasm', import.meta.url),
);

async function realWasmBytes(): Promise<Uint8Array> {
  return new Uint8Array(await readFile(WASM_PATH));
}

describe('wasm glob parity', () => {
  it('matches simple star patterns', async () => {
    await initGlobWasm();
    expect(globMatch('main.ts', '*.ts')).toBe(true);
    expect(globMatch('src/main.ts', '*.ts')).toBe(false);
  });

  it('matches double-star patterns', async () => {
    await initGlobWasm();
    expect(globMatch('src/deep/main.ts', 'src/**/*.ts')).toBe(true);
    expect(globMatch('main.ts', 'src/**/*.ts')).toBe(false);
  });

  it('matches brace expansion', async () => {
    await initGlobWasm();
    expect(globMatch('a/b.ts', 'a/{b,c}.ts')).toBe(true);
    expect(globMatch('a/c.ts', 'a/{b,c}.ts')).toBe(true);
    expect(globMatch('a/d.ts', 'a/{b,c}.ts')).toBe(false);
  });

  it('matches escaped specials and question mark', async () => {
    await initGlobWasm();
    expect(globMatch('a*b', 'a\\*b')).toBe(true);
    expect(globMatch('aXb', 'a?b')).toBe(true);
    expect(globMatch('a/b', 'a?b')).toBe(false);
  });

  it('matches character class', async () => {
    await initGlobWasm();
    expect(globMatch('abc', 'a[bc]c')).toBe(true);
    expect(globMatch('adc', 'a[bc]c')).toBe(false);
  });

  it('honours nocase option', async () => {
    await initGlobWasm();
    expect(globMatch('MAIN.TS', '*.ts', { nocase: true })).toBe(true);
    expect(globMatch('MAIN.TS', '*.ts', { nocase: false })).toBe(false);
  });

  it('falls back to picomatch for unsupported nested braces', async () => {
    await initGlobWasm();
    expect(globMatch('a/c.ts', 'a/{b,{c,d}}.ts')).toBe(true);
    expect(globMatch('a/z.ts', 'a/{b,{c,d}}.ts')).toBe(false);
  });

  it('falls back to picomatch for leading-dot-slash variants', async () => {
    await initGlobWasm();
    expect(globMatch('./main.ts', '*.ts')).toBe(true);
  });
});

describe('wasm glob fallback', () => {
  it('initGlobWasm with flag disabled uses JS', async () => {
    await initGlobWasm({ ODY_CODE_EXPERIMENTAL_WASM_GLOB: '0' });
    expect(globMatch('main.ts', '*.ts')).toBe(true);
    expect(globMatch('src/main.ts', '*.ts')).toBe(false);
    expect(globMatch('./main.ts', '*.ts')).toBe(true);
  });

  it('loadWasmGlobMatcher with missing wasm path falls back to JS', async () => {
    const matcher = await loadWasmGlobMatcher(undefined, { ODY_CODE_EXPERIMENTAL_WASM_GLOB: '0' });
    expect(matcher('main.ts', '*.ts')).toBe(true);
  });
});

describe('pathGlobMatch integration', () => {
  it('still normalizes paths with wasm enabled', async () => {
    await initGlobWasm();
    expect(pathGlobMatch('./main.ts', '*.ts', { cwd: '/repo' })).toBe(true);
  });
});
