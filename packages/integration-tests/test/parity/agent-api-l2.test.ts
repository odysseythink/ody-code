import { describe, expect, it } from 'vitest';

import {
  agentApiL2MockLlm,
  agentApiL2Scenario,
} from '../../src/parity/scenarios/agent-api-l2';
import { runL2Parity } from '../../src/parity/l2-parity';
import { resolveRustBinaryPath } from '../../src/parity/rust-binary';

const binaryPath = (() => {
  try {
    return resolveRustBinaryPath();
  } catch {
    return null;
  }
})();

describe.skipIf(binaryPath === null)('AgentAPI L2 parity', () => {
  it('TS and Rust return the same AgentAPI response shapes', async () => {
    const diff = await runL2Parity(agentApiL2Scenario, agentApiL2MockLlm);
    expect(diff).toBeNull();
  }, 120000);
});
