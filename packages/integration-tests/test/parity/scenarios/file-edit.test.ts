import { readFile } from 'node:fs/promises';
import { join } from 'pathe';
import { describe, expect, it } from 'vitest';

import { makeTsBackend, createTempHome, cleanupHome } from '../../../src/parity/backends';
import { fileEditScenario, fileEditMockLlm } from '../../../src/parity/scenarios/file-edit';

describe('file-edit parity scenario', () => {
  it('writes a file through the Write tool', async () => {
    const homeDir = await createTempHome('file-edit-');
    const backend = await makeTsBackend({ homeDir, mockLlm: fileEditMockLlm });
    try {
      await fileEditScenario.run(backend);
      const content = await readFile(join(homeDir, 'hello.txt'), 'utf8');
      expect(content).toBe('hello world');
    } finally {
      await backend.close();
      await cleanupHome(homeDir);
    }
  });
});
