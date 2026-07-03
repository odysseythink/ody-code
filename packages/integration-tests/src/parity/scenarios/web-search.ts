import type { ChatProvider } from '@odysseythink/kosong';
import { MockChatProvider } from '../fixtures/mock-provider';
import type { Scenario } from '../types';
import { waitForTurnEnded } from './utils';

interface RawRpcClient {
  rpc: Record<string, (payload: unknown) => Promise<unknown>>;
}

export const webSearchMockLlm: ChatProvider = new MockChatProvider([
  [
    {
      type: 'function',
      id: 'tc-search-1',
      name: 'WebSearch',
      arguments: JSON.stringify({ query: 'ody code parity', limit: 1 }),
    },
  ],
  [{ type: 'text', text: 'Found mock result.' }],
]);

export const webSearchScenario: Scenario = {
  name: 'web-search',
  async run(backend) {
    const rpc = (backend.client as unknown as RawRpcClient).rpc;
    const summary = await backend.client.createSession({
      workDir: backend.homeDir,
      id: 'web-search-001',
      permission: 'auto',
      model: 'mock',
    });

    // Register a user-level WebSearch tool so the parity harness can echo
    // without hitting the network. This tests tool registration + call shape.
    await rpc['registerTool']!({
      sessionId: summary.id,
      agentId: 'main',
      name: 'WebSearch',
      description: 'Search the web (mock).',
      parameters: {
        type: 'object',
        properties: {
          query: { type: 'string' },
          limit: { type: 'number' },
        },
        required: ['query'],
      },
    });

    await backend.client.prompt({
      sessionId: summary.id,
      input: [{ type: 'text', text: 'Search for ody code parity' }],
    });
    await waitForTurnEnded(backend.client, { timeoutMs: 10000 });

    return { responses: [{ sessionId: summary.id }], events: [] };
  },
};
