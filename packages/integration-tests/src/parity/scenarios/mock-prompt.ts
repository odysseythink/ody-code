import { Session } from '@odysseythink/ody-code-sdk';
import type { ChatProvider } from '@odysseythink/kosong';

import { MockChatProvider } from '../fixtures/mock-provider';
import type { Scenario } from '../types';
import { waitForTurnEnded } from './utils';

export const mockPromptMockLlm: ChatProvider = new MockChatProvider([
  { type: 'text', text: 'Hello, parity!' },
]);

export const mockPromptScenario: Scenario = {
  name: 'mock-prompt',
  async run(backend) {
    const summary = await backend.client.createSession({
      workDir: backend.homeDir,
      permission: 'auto',
      model: 'mock',
    });
    const session = new Session({
      id: summary.id,
      workDir: summary.workDir,
      rpc: backend.client,
    });
    try {
      await session.prompt('hello');
      await waitForTurnEnded(backend.client, { timeoutMs: 10000 });
      return {
        responses: [{ sessionId: summary.id }],
        events: [],
      };
    } finally {
      await session.close();
    }
  },
};
