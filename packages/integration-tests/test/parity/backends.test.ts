import { describe, expect, it } from 'vitest';
import { makeTsBackend, createTempHome, cleanupHome } from '../../src/parity/backends';

describe('makeTsBackend', () => {
  it('creates a session and exposes the same homeDir', async () => {
    const homeDir = await createTempHome('ts-');
    const backend = await makeTsBackend({ homeDir });
    try {
      expect(backend.kind).toBe('ts');
      expect(backend.homeDir).toBe(homeDir);
      const summary = await backend.client.createSession({ workDir: homeDir });
      expect(summary.id).toBeDefined();
      expect(typeof summary.id).toBe('string');
    } finally {
      await backend.close();
      await cleanupHome(homeDir);
    }
  });
});
