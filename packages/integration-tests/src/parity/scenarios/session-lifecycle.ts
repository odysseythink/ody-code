import type { ChatProvider } from '@odysseythink/kosong';

import { MockChatProvider } from '../fixtures/mock-provider';
import type { Scenario } from '../types';

export const sessionLifecycleMockLlm: ChatProvider = new MockChatProvider([]);

export const sessionLifecycleScenario: Scenario = {
  name: 'session-lifecycle',
  async run(backend) {
    const fixedId = 'parity-session-001';
    const created = await backend.client.createSession({
      workDir: backend.homeDir,
      id: fixedId,
    });
    const listed = await backend.client.listSessions({ workDir: backend.homeDir });
    const found = listed.some((s) => s.id === created.id);
    await backend.client.closeSession({ sessionId: created.id });
    return {
      responses: [{ createdId: created.id, listedCount: listed.length, found }],
      events: [],
    };
  },
};
