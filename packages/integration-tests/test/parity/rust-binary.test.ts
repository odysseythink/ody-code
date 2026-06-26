import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { describe, expect, it } from 'vitest';
import { resolveRustBinaryPath } from '../../src/parity/rust-binary';

describe('resolveRustBinaryPath', () => {
  it('prefers ODY_HOST_BINARY_PATH when the file exists', () => {
    const dir = mkdtempSync(join(tmpdir(), 'parity-bin-'));
    const fakeBinary = join(dir, 'ody-host');
    writeFileSync(fakeBinary, '');
    const prev = process.env['ODY_HOST_BINARY_PATH'];
    process.env['ODY_HOST_BINARY_PATH'] = fakeBinary;
    try {
      expect(resolveRustBinaryPath()).toBe(fakeBinary);
    } finally {
      process.env['ODY_HOST_BINARY_PATH'] = prev;
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('throws a helpful error when no candidate exists', () => {
    const dir = mkdtempSync(join(tmpdir(), 'parity-root-'));
    const prev = process.env['ODY_HOST_BINARY_PATH'];
    delete process.env['ODY_HOST_BINARY_PATH'];
    try {
      expect(() => resolveRustBinaryPath(dir)).toThrow('Rust host binary not found');
    } finally {
      process.env['ODY_HOST_BINARY_PATH'] = prev;
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
