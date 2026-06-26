import { describe, expect, it } from 'vitest';
import { makeTsBackend, createTempHome, cleanupHome } from '../../../src/parity/backends';
import { ParityDriver } from '../../../src/parity/driver';
import { setModelScenario, setModelMockLlm } from '../../../src/parity/scenarios/set-model';

describe('set-model parity scenario', () => {
  it('updates the session model and reflects it in status', async () => {
    const homeDir = await createTempHome('set-model-');
    const backend = await makeTsBackend({ homeDir, mockLlm: setModelMockLlm });
    try {
      const driver = new ParityDriver({ timeoutMs: 10000 });
      const snapshot = await driver.runScenario(backend, setModelScenario);
      expect(snapshot.responses).toHaveLength(1);
      const response = snapshot.responses[0] as {
        readonly sessionId: string;
        readonly model: string | undefined;
      };
      expect(response.sessionId).toBeTypeOf('string');
      expect(response.model).toBe('gpt-4o');
    } finally {
      await backend.close();
      await cleanupHome(homeDir);
    }
  });
});
