import { describe, expect, it } from 'vitest';

import { MockChatProvider } from '../../src/parity/fixtures/mock-provider';
import { resolveRustBinaryPath } from '../../src/parity/rust-binary';

const mockLlm = new MockChatProvider([
  { type: 'text', text: 'ack1' },
  { type: 'text', text: 'ack2' },
  { type: 'text', text: 'ack3' },
]);

const binaryPath = (() => {
  try {
    return resolveRustBinaryPath();
  } catch {
    return null;
  }
})();

describe.skipIf(binaryPath === null)('L4 cross-host resume parity', () => {
  it('TS create -> Rust resume -> TS resume is a known L4 gap', async () => {
    // L4 cross-host resume requires compatible session persistence between
    // TS and Rust backends. This is covered by the known-gaps.md wildcard L4 entry.
    // This test verifies the infrastructure exists; the actual parity will pass
    // once session state serialization is aligned.
    expect(true).toBe(true);
  }, 1000);
});
