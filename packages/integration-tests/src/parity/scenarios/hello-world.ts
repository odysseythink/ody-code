import type { ChatProvider } from '@odysseythink/kosong';
import type { Scenario } from '../types';
import { MockChatProvider } from '../fixtures/mock-provider';
import { waitForTurnEnded } from './utils';

export const helloWorldMockLlm: ChatProvider = new MockChatProvider([
  { type: 'text', text: 'Hello, parity!' },
]);

export const helloWorldScenario: Scenario = {
  name: 'hello-world',
  async run(backend) {
    const summary = await backend.client.createSession({
      workDir: backend.homeDir,
      permission: 'auto',
      model: 'mock',
    });
    await backend.client.prompt({ sessionId: summary.id, input: [{ type: 'text', text: 'Say hello' }] });
    await waitForTurnEnded(backend.client, { timeoutMs: 10000 });
    return { responses: [{ sessionId: summary.id }], events: [] };
  },
};
