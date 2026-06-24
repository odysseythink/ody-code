import { describe, expect, it } from 'vitest';

import { BytesBuffer } from '../../../src/rpc/transports/bytes-buffer';

function s(text: string): Uint8Array {
  return new TextEncoder().encode(text);
}

describe('BytesBuffer', () => {
  it('appends and slices across chunk boundaries', () => {
    const buf = new BytesBuffer();
    buf.append(s('hello'));
    buf.append(s(' world'));
    expect(buf.length).toBe(11);
    expect(new TextDecoder().decode(buf.slice(0, 5))).toBe('hello');
    expect(new TextDecoder().decode(buf.slice(6, 11))).toBe('world');
  });

  it('finds newline and discards bytes', () => {
    const buf = new BytesBuffer();
    buf.append(s('abc'));
    buf.append(s('d\nef'));
    expect(buf.indexOf(0x0a)).toBe(4);
    buf.discard(5);
    expect(buf.length).toBe(2);
    expect(new TextDecoder().decode(buf.slice(0, 2))).toBe('ef');
  });

  it('expands internal capacity when appending large chunks', () => {
    const buf = new BytesBuffer();
    const big = new Uint8Array(4096).fill(0xab);
    buf.append(big);
    expect(buf.length).toBe(4096);
    expect(buf.slice(0, 1)[0]).toBe(0xab);
    expect(buf.slice(4095, 4096)[0]).toBe(0xab);
  });
});
