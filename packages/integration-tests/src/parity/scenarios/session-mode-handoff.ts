import { Session } from '@odysseythink/ody-code-sdk';
import type { ChatProvider } from '@odysseythink/kosong';

import { MockChatProvider } from '../fixtures/mock-provider';
import type { Scenario } from '../types';

export const sessionModeHandoffMockLlm: ChatProvider = new MockChatProvider([]);

interface RawRpc {
  rpc: {
    enterPlan: (p: unknown) => Promise<unknown>;
    getPlan: (p: unknown) => Promise<unknown>;
    clearPlan: (p: unknown) => Promise<unknown>;
  };
}

export const sessionModeHandoffScenario: Scenario = {
  name: 'session-mode-handoff',
  async run(backend) {
    const summary = await backend.client.createSession({
      workDir: backend.homeDir,
      permission: 'auto',
      model: 'mock',
      id: 'session-mode-handoff-001',
    });
    try {
      const rpc = (backend.client as unknown as RawRpc).rpc;

      // Enter plan mode via RPC
      await rpc.enterPlan({ sessionId: summary.id, agentId: 'main' });
      const plan1 = await rpc.getPlan({ sessionId: summary.id, agentId: 'main' });

      // Exit plan mode
      await rpc.clearPlan({ sessionId: summary.id, agentId: 'main' });
      const plan2 = await rpc.getPlan({ sessionId: summary.id, agentId: 'main' });

      return {
        responses: [
          { planAfterEnter: normalizePlan(plan1) },
          { planAfterClear: normalizePlan(plan2) },
        ],
        events: [],
      };
    } finally {
      await backend.client.closeSession({ sessionId: summary.id }).catch(() => {});
    }
  },
};

function normalizePlan(result: unknown): unknown {
  const r = result as Record<string, unknown> | null | undefined;
  if (r === null || r === undefined) return { active: false, kind: null, filePath: null };
  return {
    active: Boolean(r['active'] ?? false),
    kind: r['kind'] ?? r['mode'] ?? null,
    filePath: typeof r['filePath'] === 'string' ? '<file>' : null,
  };
}
