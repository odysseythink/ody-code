import { describe, expect, it } from 'vitest';

import type { WasmExports } from '../../src/utils/wasm-loader';
import {
  callWasmStringFunction,
  readCString,
  writeString,
} from '../../src/utils/wasm-string';

function makeMockExports(): WasmExports {
  const memory = new WebAssembly.Memory({ initial: 1 });
  const buffer = new Uint8Array(memory.buffer);
  let nextPtr = 8;

  return {
    memory,
    alloc(len: number): number {
      if (len === 0) return 0;
      const ptr = nextPtr;
      nextPtr += len + 1;
      return ptr;
    },
    dealloc(_ptr: number, _len: number): void {
      // no-op in mock
    },
    concat(a: number, aLen: number, b: number, bLen: number): number {
      const textA = new TextDecoder().decode(buffer.subarray(a, a + aLen));
      const textB = new TextDecoder().decode(buffer.subarray(b, b + bLen));
      const out = `${textA}|${textB}`;
      const bytes = new TextEncoder().encode(out);
      const ptr = nextPtr;
      nextPtr += bytes.length + 1;
      buffer.set(bytes, ptr);
      buffer[ptr + bytes.length] = 0;
      return ptr;
    },
  } as unknown as WasmExports;
}

describe('writeString + readCString', () => {
  it('round-trips empty string as ptr 0', () => {
    const exports = makeMockExports();
    const { ptr, len } = writeString(exports, '');
    expect(ptr).toBe(0);
    expect(len).toBe(0);
  });

  it('round-trips non-empty string', () => {
    const exports = makeMockExports();
    const { ptr, len } = writeString(exports, 'hello 世界');
    expect(ptr).not.toBe(0);
    expect(len).toBe(new TextEncoder().encode('hello 世界').length);
    expect(readCString(exports, ptr)).toBe('hello 世界');
  });

  it('reads null pointer as empty string', () => {
    const exports = makeMockExports();
    expect(readCString(exports, 0)).toBe('');
  });
});

describe('callWasmStringFunction', () => {
  it('passes multiple inputs and reads NUL-terminated output', () => {
    const exports = makeMockExports();
    const result = callWasmStringFunction(exports, 'concat', 'hello', 'world');
    expect(result).toBe('hello|world');
  });

  it('returns empty string when function returns null', () => {
    const exports = makeMockExports();
    (exports as unknown as Record<string, unknown>)['nullFn'] = () => 0;
    expect(callWasmStringFunction(exports, 'nullFn', 'x')).toBe('');
  });
});
