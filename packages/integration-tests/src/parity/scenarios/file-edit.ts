import type { ChatProvider } from '@odysseythink/kosong';

import { MockChatProvider } from '../fixtures/mock-provider';
import type { Scenario } from '../types';
import { waitForTurnEnded } from './utils';

export const fileEditMockLlm: ChatProvider = new MockChatProvider([
  [
    {
      type: 'function',
      id: 'tc-write-1',
      name: 'Write',
      arguments: JSON.stringify({ path: 'hello.txt', content: 'hello world' }),
    },
  ],
  [{ type: 'text', text: 'Created hello.txt' }],
]);

export const fileEditScenario: Scenario = {
  name: 'file-edit',
  async run(backend) {
    const summary = await backend.client.createSession({
      workDir: backend.homeDir,
      permission: 'auto',
      model: 'mock',
    });
    await backend.client.prompt({
      sessionId: summary.id,
      input: [{ type: 'text', text: 'Create hello.txt with "hello world"' }],
    });
    await waitForTurnEnded(backend.client, { timeoutMs: 10000 });
    return { responses: [{ sessionId: summary.id }], events: [] };
  },
};
