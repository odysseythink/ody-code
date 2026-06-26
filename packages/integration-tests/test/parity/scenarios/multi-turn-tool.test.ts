import { readFile } from 'node:fs/promises';
import { join } from 'pathe';
import { describe, expect, it } from 'vitest';

import { makeTsBackend, createTempHome, cleanupHome } from '../../../src/parity/backends';
import { multiTurnToolScenario, multiTurnToolMockLlm } from '../../../src/parity/scenarios/multi-turn-tool';

describe('multi-turn-tool parity scenario', () => {
  it('reads and writes across multiple tool steps', async () => {
    const homeDir = await createTempHome('multi-turn-tool-');
    const backend = await makeTsBackend({ homeDir, mockLlm: multiTurnToolMockLlm });
    try {
      await multiTurnToolScenario.run(backend);
      const content = await readFile(join(homeDir, 'output.txt'), 'utf8');
      expect(content).toBe('derived payload');
    } finally {
      await backend.close();
      await cleanupHome(homeDir);
    }
  });
});
