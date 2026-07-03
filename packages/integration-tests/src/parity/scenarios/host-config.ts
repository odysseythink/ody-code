import { Session } from '@odysseythink/ody-code-sdk';
import type { ChatProvider } from '@odysseythink/kosong';

import { MockChatProvider } from '../fixtures/mock-provider';
import type { Scenario } from '../types';

export const hostConfigMockLlm: ChatProvider = new MockChatProvider([]);

interface RawRpc {
  rpc: {
    setModel: (p: unknown) => Promise<unknown>;
    setThinking: (p: unknown) => Promise<unknown>;
    getConfig: (p: unknown) => Promise<unknown>;
  };
}

export const hostConfigScenario: Scenario = {
  name: 'host-config',
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
      const rawClient = backend.client as unknown as RawRpc;

      const setModelResult = await rawClient.rpc.setModel({
        sessionId: summary.id,
        agentId: 'main',
        model: 'openai/gpt-4o',
      });

      await rawClient.rpc.setThinking({
        sessionId: summary.id,
        agentId: 'main',
        level: 'off',
      });

      const agentConfig = await rawClient.rpc.getConfig({
        sessionId: summary.id,
        agentId: 'main',
      });

      const odyConfig = await backend.client.getConfig();

      return {
        responses: [
          { setModel: normalizeSetModel(setModelResult) },
          { agentConfig: normalizeAgentConfig(agentConfig) },
          { odyConfig: normalizeOdyConfig(odyConfig, 'openai') },
        ],
        events: [],
      };
    } finally {
      await session.close();
    }
  },
};

function normalizeSetModel(result: unknown): unknown {
  const r = result as Record<string, unknown>;
  const rawModel = String(r['model'] ?? '');
  const providerName = String(r['providerName'] ?? extractProviderPrefix(rawModel));
  const model = rawModel.includes('/') ? rawModel.slice(rawModel.indexOf('/') + 1) : rawModel;
  return {
    provider: providerName || '<default>',
    model,
  };
}

function normalizeAgentConfig(config: unknown): unknown {
  const c = config as Record<string, unknown>;
  const modelAlias = String(c['modelAlias'] ?? '');
  const provider = c['provider'] as Record<string, unknown> | undefined;
  const providerId = extractProviderId(provider, modelAlias);
  const model = modelAlias.includes('/') ? modelAlias.slice(modelAlias.indexOf('/') + 1) : modelAlias;
  const capabilities = c['modelCapabilities'] as Record<string, unknown> | undefined;

  return {
    provider: providerId || '<default>',
    model,
    modelCapabilities: capabilities ?? null,
    thinkingLevel: c['thinkingLevel'],
  };
}

function extractProviderId(
  provider: Record<string, unknown> | undefined,
  modelAlias: string,
): string | undefined {
  if (provider !== undefined && typeof provider['id'] === 'string') {
    return provider['id'];
  }
  return extractProviderPrefix(modelAlias);
}

function extractProviderPrefix(modelAlias: string): string | undefined {
  const idx = modelAlias.indexOf('/');
  return idx > 0 ? modelAlias.slice(0, idx) : undefined;
}

function normalizeOdyConfig(config: unknown, activeProviderId: string): unknown {
  const c = config as Record<string, unknown>;
  const providers = normalizeProviders(c['providers']).filter((p) => {
    const provider = p as Record<string, unknown>;
    return provider['id'] === activeProviderId;
  });

  return {
    providers: providers.map((p) => {
      const provider = p as Record<string, unknown>;
      return {
        id: provider['id'],
        baseUrl: provider['baseUrl'],
      };
    }),
  };
}

function normalizeProviders(raw: unknown): unknown[] {
  if (Array.isArray(raw)) {
    return raw;
  }
  if (typeof raw === 'object' && raw !== null) {
    return Object.entries(raw as Record<string, unknown>).map(([id, value]) => {
      if (typeof value === 'object' && value !== null) {
        return { id, ...(value as Record<string, unknown>) };
      }
      return { id };
    });
  }
  return [];
}
