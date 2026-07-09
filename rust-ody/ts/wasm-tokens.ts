/**
 * Dual-track loader for the Rust/Wasm `estimate_tokens` PoC.
 *
 * Production shape: `loadWasmTokenEstimator()` returns a function with the same
 * signature as the existing JS `estimateTokens`. If the Wasm module fails to
 * load (missing asset, unsupported runtime, etc.) the caller falls back to the
 * pure-JS implementation, so behaviour degrades safely.
 *
 * In a SEA build the `.wasm` bytes would come from `sea.getAsset('ody_rust.wasm')`
 * instead of `readFile`; everything below the byte source is identical.
 */
import { readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';

interface WasmExports {
  memory: WebAssembly.Memory;
  alloc(len: number): number;
  dealloc(ptr: number, len: number): void;
  estimate_tokens(ptr: number, len: number): number;
}

export type TokenEstimator = (text: string) => number;

const DEFAULT_WASM_PATH = fileURLToPath(
  new URL('../target/wasm32-unknown-unknown/release/ody_rust.wasm', import.meta.url),
);

const encoder = new TextEncoder();

export async function loadWasmTokenEstimator(
  wasmBytes?: Uint8Array,
): Promise<TokenEstimator> {
  const bytes = wasmBytes ?? new Uint8Array(await readFile(DEFAULT_WASM_PATH));
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const exports = instance.exports as unknown as WasmExports;

  return (text: string): number => {
    const utf8 = encoder.encode(text);
    const len = utf8.length;
    if (len === 0) return 0;

    const ptr = exports.alloc(len);
    if (ptr === 0) return 0;
    try {
      // Re-view memory each call: the buffer can detach if wasm memory grows.
      new Uint8Array(exports.memory.buffer, ptr, len).set(utf8);
      return exports.estimate_tokens(ptr, len) >>> 0;
    } finally {
      exports.dealloc(ptr, len);
    }
  };
}
