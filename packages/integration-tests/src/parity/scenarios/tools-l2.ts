import type { ChatProvider } from '@odysseythink/kosong';
import { MockChatProvider } from '../fixtures/mock-provider';
import type { Scenario } from '../types';

interface RawRpcClient {
  rpc: Record<string, (payload: unknown) => Promise<unknown>>;
}

interface NormalizedToolInfo {
  name: string;
  active: boolean;
  source: string;
}

// Focus L2 parity on a small set of tools that exist on both TS and Rust backends.
// TS registers additional mode-specific tools (game-design, office-hours, etc.) that
// the Rust host does not expose yet; comparing the full inventory would be a parity
// gap unrelated to the register/active/unregister shapes we are testing.
const COMMON_TOOLS = new Set(['Echo', 'Read', 'Write']);

function normalizeToolList(result: unknown): NormalizedToolInfo[] {
  const arr = Array.isArray(result) ? result : [];
  return arr
    .map((t) => ({
      name: String((t as Record<string, unknown>)['name'] ?? ''),
      active: Boolean((t as Record<string, unknown>)['active'] ?? false),
      source: String((t as Record<string, unknown>)['source'] ?? ''),
    }))
    .filter((t) => COMMON_TOOLS.has(t.name))
    .sort((a, b) => a.name.localeCompare(b.name));
}

export const toolsL2MockLlm: ChatProvider = new MockChatProvider([]);

export const toolsL2Scenario: Scenario = {
  name: 'tools-l2',
  async run(backend) {
    const rpc = (backend.client as unknown as RawRpcClient).rpc;
    const summary = await backend.client.createSession({
      workDir: backend.homeDir,
      id: 'tools-l2-001',
      permission: 'auto',
      model: 'mock',
    });
    const base = { sessionId: summary.id, agentId: 'main' };

    const baseline = normalizeToolList(await rpc['getTools']!(base));

    await rpc['registerTool']!({
      ...base,
      name: 'Echo',
      description: 'Echo the input text back.',
      parameters: {
        type: 'object',
        properties: { text: { type: 'string' } },
        required: ['text'],
      },
    });
    const afterRegister = normalizeToolList(await rpc['getTools']!(base));

    await rpc['setActiveTools']!({ ...base, names: ['Read', 'Write'] });
    const afterActive = normalizeToolList(await rpc['getTools']!(base));

    await rpc['unregisterTool']!({ ...base, name: 'Echo' });
    const afterUnregister = normalizeToolList(await rpc['getTools']!(base));

    return {
      responses: [{ baseline, afterRegister, afterActive, afterUnregister }],
      events: [],
    };
  },
};
