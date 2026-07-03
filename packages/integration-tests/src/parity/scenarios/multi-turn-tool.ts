import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'pathe';

import type { ChatProvider } from '@odysseythink/kosong';

import { MockChatProvider } from '../fixtures/mock-provider';
import type { Scenario } from '../types';
import { waitForTurnEnded } from './utils';

export const multiTurnToolMockLlm: ChatProvider = new MockChatProvider([
  [
    {
      type: 'function',
      id: 'tc-read-1',
      name: 'Read',
      arguments: JSON.stringify({ path: 'input.txt' }),
    },
  ],
  [
    {
      type: 'function',
      id: 'tc-write-1',
      name: 'Write',
      arguments: JSON.stringify({ path: 'output.txt', content: 'derived payload' }),
    },
  ],
  [{ type: 'text', text: 'Wrote output.txt' }],
]);

export const multiTurnToolScenario: Scenario = {
  name: 'multi-turn-tool',
  async run(backend) {
    await writeFile(join(backend.homeDir, 'input.txt'), 'source payload', 'utf8');

    const summary = await backend.client.createSession({
      workDir: backend.homeDir,
      permission: 'auto',
      model: 'mock',
    });
    await backend.client.prompt({
      sessionId: summary.id,
      input: [{ type: 'text', text: 'Read input.txt and write its meaning to output.txt' }],
    });
    await waitForTurnEnded(backend.client, { timeoutMs: 10000 });

    const outputText = await readFile(join(backend.homeDir, 'output.txt'), 'utf8').catch(() => '');

    return { responses: [{ sessionId: summary.id, outputText }], events: [] };
  },
};
