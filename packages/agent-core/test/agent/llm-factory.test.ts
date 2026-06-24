import { describe, expect, it, vi } from 'vitest';

import { Agent, type LLM, type LLMFactoryConfig } from '../../src/agent';
import { getDefaultConfig } from '../../src/config';
import { SingleModelProvider } from '../../src/session/provider-manager';
import { createFakeKaos } from '../tools/fixtures/fake-kaos';

const zeroUsage = { inputOther: 0, output: 0, inputCacheRead: 0, inputCacheCreation: 0 };

describe('llmFactory injection', () => {
  it('uses the injected factory and re-creates after refresh', () => {
    const mockLlm: LLM = {
      systemPrompt: 'factory-sp',
      modelName: 'factory-model',
      chat: vi.fn(async () => ({ toolCalls: [], usage: zeroUsage })),
    };
    const factory = vi.fn((_rpc, config: LLMFactoryConfig) => {
      expect(config.modelName).toBe('mock-model');
      expect(config.systemPrompt).toBe('<system-prompt>');
      return mockLlm;
    });

    const agent = new Agent({
      kaos: createFakeKaos(),
      config: getDefaultConfig(),
      llmFactory: factory,
    });

    agent.config.update({ modelAlias: 'mock-model', systemPrompt: '<system-prompt>' });

    expect(agent.llm).toBe(mockLlm);
    expect(factory).toHaveBeenCalledTimes(1);

    agent.refreshLlm();
    expect(agent.llm).toBe(mockLlm);
    expect(factory).toHaveBeenCalledTimes(2);
  });

  it('falls back to default KosongLLM when llmFactory is not provided', () => {
    const agent = new Agent({
      kaos: createFakeKaos(),
      config: getDefaultConfig(),
      modelProvider: new SingleModelProvider(
        { type: 'kimi', apiKey: 'test-key', model: 'mock-model' },
        { max_context_tokens: 1_000_000 },
      ),
    });

    agent.config.update({ modelAlias: 'mock-model', systemPrompt: '<system-prompt>' });

    const llm = agent.llm;
    expect(llm).toBeDefined();
    expect(llm.systemPrompt).toBe('<system-prompt>');
    expect(llm.modelName).toBe('mock-model');
  });
});
