import { describe, expect, it } from 'vitest';
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

import { loadWasmModule, wrapWithFallback, type WasmModuleConfig } from '../../src/utils/wasm-loader';

const WASM_PATH = fileURLToPath(
  new URL('../../../../rust-ody/target/wasm32-unknown-unknown/release/ody_rust.wasm', import.meta.url),
);

async function realWasmBytes(): Promise<Uint8Array> {
  return new Uint8Array(await readFile(WASM_PATH));
}

function makeConfig<T>(partial: Omit<WasmModuleConfig<T>, 'wasmPath'> & { wasmPath?: string }): WasmModuleConfig<T> {
  return {
    wasmPath: WASM_PATH,
    ...partial,
  } as WasmModuleConfig<T>;
}

describe('loadWasmModule', () => {
  it('returns fallback when flag is disabled by env', async () => {
    const fallback = () => 'js';
    const fn = await loadWasmModule(
      makeConfig({ fallback, flagId: 'wasm-diff', factory: () => () => 'wasm' }),
      await realWasmBytes(),
      { ODY_CODE_EXPERIMENTAL_WASM_DIFF: '0' },
    );
    expect(fn()).toBe('js');
  });

  it('returns fallback when wasm file is missing', async () => {
    const fallback = () => 'js';
    const fn = await loadWasmModule({
      wasmPath: '/definitely/missing.wasm',
      fallback,
      flagId: 'wasm-diff',
      factory: () => () => 'wasm',
    });
    expect(fn()).toBe('js');
  });

  it('returns wasm result when everything works', async () => {
    const fallback = () => 'js';
    const fn = await loadWasmModule(
      makeConfig({ fallback, flagId: 'wasm-diff', factory: () => () => 'wasm' }),
      await realWasmBytes(),
    );
    expect(fn()).toBe('wasm');
  });

  it('falls back when the wrapped wasm function throws at runtime', async () => {
    const fallback = () => 'js';
    const fn = await loadWasmModule(
      makeConfig({
        fallback,
        flagId: 'wasm-diff',
        factory: () => () => {
          throw new Error('wasm panic');
        },
      }),
      await realWasmBytes(),
    );
    expect(fn()).toBe('js');
  });
});

describe('wrapWithFallback', () => {
  it('returns wasm result on success', () => {
    const fn = wrapWithFallback(
      (x: number) => x * 2,
      (x: number) => x + 1,
      'wasm-diff',
    );
    expect(fn(5)).toBe(10);
  });

  it('returns fallback on wasm throw', () => {
    const fn = wrapWithFallback(
      () => {
        throw new Error('boom');
      },
      () => 'ok',
      'wasm-diff',
    );
    expect(fn()).toBe('ok');
  });
});
