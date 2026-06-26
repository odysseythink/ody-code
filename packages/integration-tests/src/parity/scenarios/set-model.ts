import { Session } from '@odysseythink/ody-code-sdk';
import type { ChatProvider } from '@odysseythink/kosong';

import { MockChatProvider } from '../fixtures/mock-provider';
import type { Scenario } from '../types';

export const setModelMockLlm: ChatProvider = new MockChatProvider([]);

export const setModelScenario: Scenario = {
  name: 'set-model',
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
      await session.setModel('gpt-4o');
      const status = await session.getStatus();
      return {
        responses: [{ sessionId: summary.id, model: status.model }],
        events: [],
      };
    } finally {
      await session.close();
    }
  },
};
