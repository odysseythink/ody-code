import { describe, expect, it } from 'vitest';
import { toolsL2MockLlm, toolsL2Scenario } from '../../src/parity/scenarios/tools-l2';
import { runL2Parity } from '../../src/parity/l2-parity';
import { resolveRustBinaryPath } from '../../src/parity/rust-binary';

const binaryPath = (() => {
  try { return resolveRustBinaryPath(); } catch { return null; }
})();

describe.skipIf(binaryPath === null)('Tools L2 parity', () => {
  it('TS and Rust return the same tool RPC response shapes', async () => {
    const diff = await runL2Parity(toolsL2Scenario, toolsL2MockLlm);
    expect(diff).toBeNull();
  }, 120000);
});
