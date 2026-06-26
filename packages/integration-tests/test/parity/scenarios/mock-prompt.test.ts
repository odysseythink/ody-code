import { describe, expect, it } from 'vitest';
import { makeTsBackend, createTempHome, cleanupHome } from '../../../src/parity/backends';
import { ParityDriver } from '../../../src/parity/driver';
import { mockPromptScenario, mockPromptMockLlm } from '../../../src/parity/scenarios/mock-prompt';

describe('mock-prompt parity scenario', () => {
  it('emits turn events and produces assistant text', async () => {
    const homeDir = await createTempHome('mock-prompt-');
    const backend = await makeTsBackend({ homeDir, mockLlm: mockPromptMockLlm });
    try {
      const driver = new ParityDriver({ timeoutMs: 10000 });
      const snapshot = await driver.runScenario(backend, mockPromptScenario);
      expect(snapshot.events.some((e: any) => e.type === 'turn.started')).toBe(true);
      expect(snapshot.events.some((e: any) => e.type === 'turn.ended')).toBe(true);
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
