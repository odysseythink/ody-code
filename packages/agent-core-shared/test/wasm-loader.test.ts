import { describe, expect, it } from 'vitest';

import { loadWasmModule, wrapWithFallback, type WasmModuleConfig } from '../src/wasm-loader';

describe('loadWasmModule', () => {
  it('returns fallback when flag is disabled by env', async () => {
    const fallback = () => 'js';
    const fn = await loadWasmModule(
      {
        wasmPath: '/definitely/missing.wasm',
        fallback,
        flagId: 'wasm-diff',
        factory: () => () => 'wasm',
      } satisfies WasmModuleConfig<() => string>,
      undefined,
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
    } satisfies WasmModuleConfig<() => string>);
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
