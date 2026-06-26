import { describe, expect, it } from 'vitest';
import { makeTsBackend, createTempHome, cleanupHome } from '../../../src/parity/backends';
import { ParityDriver } from '../../../src/parity/driver';
import {
  sessionLifecycleScenario,
  sessionLifecycleMockLlm,
} from '../../../src/parity/scenarios/session-lifecycle';

describe('session-lifecycle parity scenario', () => {
  it('creates, lists, and closes a session with a fixed id', async () => {
    const homeDir = await createTempHome('session-lifecycle-');
    const backend = await makeTsBackend({ homeDir, mockLlm: sessionLifecycleMockLlm });
    try {
      const driver = new ParityDriver({ timeoutMs: 10000 });
      const snapshot = await driver.runScenario(backend, sessionLifecycleScenario);
      expect(snapshot.responses).toHaveLength(1);
      const response = snapshot.responses[0] as {
        readonly createdId: string;
        readonly listedCount: number;
        readonly found: boolean;
      };
      expect(response.createdId).toBe('parity-session-001');
      expect(response.listedCount).toBeGreaterThan(0);
      expect(response.found).toBe(true);
    } finally {
      await backend.close();
      await cleanupHome(homeDir);
    }
  });
});
