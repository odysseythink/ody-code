/**
 * Raw-ABI string helpers shared by all Wasm modules.
 *
 * Convention (mirrors rust-ody/src/abi.rs):
 *   - writeString uses exports.alloc(len); empty strings use ptr 0.
 *   - Rust functions returning strings use alloc_cstring, which allocates
 *     len+1 bytes and writes a NUL terminator.
 *   - readCString reads until NUL; callWasmStringFunction then calls
 *     exports.dealloc(outPtr, decodedLen + 1).
 */
import type { WasmExports } from './wasm-loader';

export type { WasmExports };

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export interface StringAllocation {
  readonly ptr: number;
  readonly len: number;
}

export function writeString(exports: WasmExports, text: string): StringAllocation {
  const bytes = encoder.encode(text);
  const len = bytes.length;
  if (len === 0) {
    return { ptr: 0, len: 0 };
  }
  const ptr = exports.alloc(len);
  if (ptr === 0) {
    throw new Error('wasm alloc failed');
  }
  new Uint8Array(exports.memory.buffer, ptr, len).set(bytes);
  return { ptr, len };
}

export function readCString(exports: WasmExports, ptr: number): string {
  if (ptr === 0) {
    return '';
  }
  const view = new Uint8Array(exports.memory.buffer);
  let end = ptr;
  while (view[end] !== 0) {
    end += 1;
  }
  const bytes = view.subarray(ptr, end);
  return decoder.decode(bytes);
}

export function callWasmStringFunction(
  exports: WasmExports,
  fnName: string,
  ...inputStrings: string[]
): string {
  const allocations: StringAllocation[] = [];
  try {
    for (const str of inputStrings) {
      allocations.push(writeString(exports, str));
    }
    const args = allocations.flatMap(({ ptr, len }) => [ptr, len]);
    const wasmFn = (exports as unknown as Record<string, unknown>)[fnName] as (...args: number[]) => number;
    const outPtr = wasmFn(...args);
    const result = readCString(exports, outPtr);
    if (outPtr !== 0) {
      exports.dealloc(outPtr, result.length + 1);
    }
    return result;
  } finally {
    for (const { ptr, len } of allocations) {
      if (ptr !== 0) {
        exports.dealloc(ptr, len);
      }
    }
  }
}

/**
 * Call a Wasm function that takes N UTF-8 strings and returns a u32 scalar.
 * Input allocations are always freed; output is a plain number.
 */
export function callWasmU32Function(
  exports: WasmExports,
  fnName: string,
  ...inputStrings: string[]
): number {
  const allocations: StringAllocation[] = [];
  try {
    for (const str of inputStrings) {
      allocations.push(writeString(exports, str));
    }
    const args = allocations.flatMap(({ ptr, len }) => [ptr, len]);
    const wasmFn = (exports as unknown as Record<string, unknown>)[fnName] as (...args: number[]) => number;
    return wasmFn(...args);
  } finally {
    for (const { ptr, len } of allocations) {
      if (ptr !== 0) {
        exports.dealloc(ptr, len);
      }
    }
  }
}
