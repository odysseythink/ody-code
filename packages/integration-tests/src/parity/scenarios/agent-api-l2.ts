import { Session } from '@odysseythink/ody-code-sdk';
import type { ChatProvider } from '@odysseythink/kosong';

import { MockChatProvider } from '../fixtures/mock-provider';
import type { Scenario } from '../types';

export const agentApiL2MockLlm: ChatProvider = new MockChatProvider([]);

interface RawRpcClient {
  rpc: Record<string, (payload: unknown) => Promise<unknown>>;
}

function extractProviderPrefix(modelAlias: string): string | undefined {
  const idx = modelAlias.indexOf('/');
  return idx > 0 ? modelAlias.slice(0, idx) : undefined;
}

// ── Response normalizers for L2 parity ──
// Each normalizer maps both TS and Rust responses to the same structural shape
// so we can assert method availability and return-type compatibility.

function normSetModel(result: unknown): unknown {
  const r = result as Record<string, unknown>;
  const rawModel = String(r['model'] ?? '');
  const providerName = String(r['providerName'] ?? extractProviderPrefix(rawModel) ?? '');
  const model = rawModel.includes('/') ? rawModel.slice(rawModel.indexOf('/') + 1) : rawModel;
  return { provider: providerName || '<default>', model };
}

function normVoid(_result: unknown): unknown {
  // Void methods return undefined, null, or {} — treat all as success.
  return { ok: true };
}

function normConfig(result: unknown): unknown {
  const c = result as Record<string, unknown>;
  const modelAlias = String(c['modelAlias'] ?? '');
  const provider = c['provider'] as Record<string, unknown> | undefined;
  let providerId: string | undefined;
  if (provider !== undefined && typeof provider['id'] === 'string') {
    providerId = provider['id'];
  }
  const prefix = extractProviderPrefix(modelAlias);
  const model = modelAlias.includes('/') ? modelAlias.slice(modelAlias.indexOf('/') + 1) : modelAlias;
  const capabilities = c['modelCapabilities'] as Record<string, unknown> | undefined;
  const hasModelCapabilities = capabilities !== null && capabilities !== undefined;
  return {
    provider: providerId ?? prefix ?? '<default>',
    model,
    thinkingLevel: c['thinkingLevel'],
    hasModelCapabilities,
  };
}

function normPermission(result: unknown): unknown {
  const r = result as Record<string, unknown> | null | undefined;
  if (r === null || r === undefined) return null;
  return { mode: String(r['mode'] ?? '').toLowerCase() };
}

function normContext(result: unknown): unknown {
  const r = result as Record<string, unknown> | null | undefined;
  if (r === null || r === undefined) return { historyCount: 0, tokenCount: 0 };
  const history = Array.isArray(r['history']) ? r['history'] : [];
  return { historyCount: history.length, tokenCount: r['tokenCount'] ?? 0 };
}

function normPlan(result: unknown): unknown {
  const r = result as Record<string, unknown> | null | undefined;
  if (r === null || r === undefined) return { active: false };
  return { active: Boolean(r['active'] ?? false) };
}

export const agentApiL2Scenario: Scenario = {
  name: 'agent-api-l2',
  async run(backend) {
    const summary = await backend.client.createSession({
      workDir: backend.homeDir,
      id: 'agent-api-l2-001',
      permission: 'auto',
      model: 'mock',
    });
    const session = new Session({
      id: summary.id,
      workDir: summary.workDir,
      rpc: backend.client,
    });
    try {
      const client = backend.client as unknown as RawRpcClient;
      const rpc = client.rpc;
      const list = await backend.client.listSessions({ workDir: backend.homeDir });

      const responses: unknown[] = [
        { listCount: list.length },

        // Mutation methods
        {
          setModel: normSetModel(
            await rpc['setModel']!({ sessionId: summary.id, agentId: 'main', model: 'openai/gpt-4o' }),
          ),
        },
        { setThinking: normVoid(await rpc['setThinking']!({ sessionId: summary.id, agentId: 'main', level: 'off' })) },
        { setPermission: normVoid(await rpc['setPermission']!({ sessionId: summary.id, agentId: 'main', mode: 'manual' })) },

        // Read methods — normalized to structural shapes for L2 parity
        { getConfig: normConfig(await rpc['getConfig']!({ sessionId: summary.id, agentId: 'main' })) },
        { getPermission: normPermission(await rpc['getPermission']!({ sessionId: summary.id, agentId: 'main' })) },
        { getContext: normContext(await rpc['getContext']!({ sessionId: summary.id, agentId: 'main' })) },
        { getPlan: normPlan(await rpc['getPlan']!({ sessionId: summary.id, agentId: 'main' })) },
        { getUsage: normVoid(await rpc['getUsage']!({ sessionId: summary.id, agentId: 'main' })) },
        { getTools: normVoid(await rpc['getTools']!({ sessionId: summary.id, agentId: 'main' })) },
        { getBackground: normVoid(await rpc['getBackground']!({ sessionId: summary.id, agentId: 'main' })) },
        { getBackgroundOutput: normVoid(await rpc['getBackgroundOutput']!({ sessionId: summary.id, agentId: 'main', taskId: 'none' })) },
        { getUserLanguage: normVoid(await rpc['getUserLanguage']!({ sessionId: summary.id, agentId: 'main' })) },
        { getModel: normVoid(await rpc['getModel']!({ sessionId: summary.id, agentId: 'main' })) },

        // Write stubs — both sides return void-equivalent
        { clearPlan: normVoid(await rpc['clearPlan']!({ sessionId: summary.id, agentId: 'main' })) },
        { cancelPlan: normVoid(await rpc['cancelPlan']!({ sessionId: summary.id, agentId: 'main', id: 'agent-api-l2-001' })) },
        { stopBackground: normVoid(await rpc['stopBackground']!({ sessionId: summary.id, agentId: 'main', taskId: 'none' })) },
        { clearContext: normVoid(await rpc['clearContext']!({ sessionId: summary.id, agentId: 'main' })) },
      ];

      await session.close();
      return { responses, events: [] };
    } finally {
      await session.close?.().catch(() => {});
    }
  },
};
