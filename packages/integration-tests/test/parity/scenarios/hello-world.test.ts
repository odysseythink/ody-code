import { describe, expect, it } from 'vitest';
import { makeTsBackend, createTempHome, cleanupHome } from '../../../src/parity/backends';
import { ParityDriver } from '../../../src/parity/driver';
import { helloWorldScenario, helloWorldMockLlm } from '../../../src/parity/scenarios/hello-world';

describe('hello-world scenario', () => {
  it('produces the expected assistant text', async () => {
    const homeDir = await createTempHome('hello-');
    const backend = await makeTsBackend({ homeDir, mockLlm: helloWorldMockLlm });
    try {
      const driver = new ParityDriver({ timeoutMs: 10000 });
      const snapshot = await driver.runScenario(backend, helloWorldScenario);
      const deltas = snapshot.events.filter((e: any) => e.type === 'assistant.delta');
      expect(deltas.length).toBeGreaterThan(0);
      const text = deltas.map((e: any) => e.delta).join('');
      expect(text).toContain('Hello, parity!');
    } finally {
      await backend.close();
      await cleanupHome(homeDir);
    }
  });
});
