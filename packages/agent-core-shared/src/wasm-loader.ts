/**
 * Generic dual-track Wasm loader used by tokenizer / diff / glob modules.
 *
 * Design contract:
 *   - If the flag is off, return the JS fallback synchronously (no Wasm I/O).
 *   - If Wasm instantiation or the factory fails, return the JS fallback.
 *   - If the returned function throws at runtime, wrapWithFallback routes the
 *     single call to the JS fallback without mutating global state.
 */
import { readFile } from 'node:fs/promises';

import { FlagResolver } from './flags/resolver';
import type { FlagId } from './flags/registry';

export type WasmFlagId = 'wasm-tokenizer' | 'wasm-diff' | 'wasm-glob';

export interface WasmExports {
  readonly memory: WebAssembly.Memory;
  alloc(len: number): number;
  dealloc(ptr: number, len: number): void;
}

export interface WasmModuleConfig<T> {
  readonly wasmPath: string;
  readonly fallback: T;
  readonly flagId: WasmFlagId;
  readonly factory: (exports: WasmExports) => T;
}

export interface LoadContext {
  readonly [env: string]: string | undefined;
}

export async function loadWasmModule<T>(
  config: WasmModuleConfig<T>,
  wasmBytes?: Uint8Array,
  context: LoadContext = process.env,
): Promise<T> {
  const resolver = new FlagResolver(context);
  if (!resolver.enabled(config.flagId as FlagId)) {
    return config.fallback;
  }

  try {
    const bytes = wasmBytes ?? new Uint8Array(await readFile(config.wasmPath));
    const { instance } = await WebAssembly.instantiate(bytes, {});
    const wasmResult = config.factory(instance.exports as unknown as WasmExports);
    // When the factory returns a function (tokenizer, glob), wrap it so runtime
    // errors fall back to the JS implementation. Object-type factories (diff)
    // are returned as-is.
    if (typeof wasmResult === 'function') {
      return wrapWithFallback(
        wasmResult as (...args: any[]) => any,
        config.fallback as (...args: any[]) => any,
        config.flagId,
      ) as unknown as T;
    }
    return wasmResult;
  } catch {
    return config.fallback;
  }
}

export function wrapWithFallback<T extends (...args: any[]) => any>(
  wasmFn: T,
  fallback: T,
  _flagId: WasmFlagId,
): T {
  return ((...args: Parameters<T>): ReturnType<T> => {
    try {
      return wasmFn(...args);
    } catch {
      return fallback(...args);
    }
  }) as T;
}
