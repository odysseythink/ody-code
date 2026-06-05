import { describe, expect, it, vi } from 'vitest';
import {
  applyProviderLoginConfig,
  fetchProviderModels,
  getProviderLoginDefinition,
  isSupportedProviderLoginType,
  removeProviderConfig,
  SUPPORTED_PROVIDER_LOGINS,
  validateProviderName,
  type ProviderModelInfo,
} from '../src/provider-login';
import { OpenPlatformApiError } from '../src/open-platform';
import type { ManagedKimiConfigShape } from '../src/managed-kimi-code';

describe('SUPPORTED_PROVIDER_LOGINS', () => {
  it('contains deepseek with correct metadata', () => {
    const def = getProviderLoginDefinition('deepseek');
    expect(def).toBeDefined();
    expect(def!.displayName).toBe('DeepSeek');
    expect(def!.defaultBaseUrl).toBe('https://api.deepseek.com/v1');
    expect(def!.modelListStrategy).toBe('openai-compatible');
  });

  it('rejects unsupported types', () => {
    expect(isSupportedProviderLoginType('google-genai')).toBe(false);
    expect(isSupportedProviderLoginType('vertexai')).toBe(false);
  });
});

describe('fetchProviderModels (openai-compatible)', () => {
  it('parses models on 200', async () => {
    const fetchMock = vi.fn(async () =>
      new Response(
        JSON.stringify({
          data: [
            { id: 'deepseek-chat', context_length: 64000, display_name: 'DeepSeek Chat' },
            { id: 'deepseek-reasoner', context_length: 64000 },
          ],
        }),
        { status: 200, headers: { 'Content-Type': 'application/json' } },
      ),
    );
    const def = getProviderLoginDefinition('deepseek')!;
    const models = await fetchProviderModels(def, 'sk-test', fetchMock as unknown as typeof fetch);
    expect(models).toHaveLength(2);
    expect(models[0]).toMatchObject({
      id: 'deepseek-chat',
      contextLength: 64000,
      supportsToolUse: true,
      supportsReasoning: false,
    });
  });

  it('throws OpenPlatformApiError on 401', async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify({ error: { message: 'invalid key' } }), { status: 401 }),
    );
    const def = getProviderLoginDefinition('deepseek')!;
    const error = await fetchProviderModels(def, 'sk-bad', fetchMock as unknown as typeof fetch).catch(
      (e: unknown) => e,
    );
    expect(error).toBeInstanceOf(OpenPlatformApiError);
    expect((error as OpenPlatformApiError).status).toBe(401);
  });
});

describe('fetchProviderModels (glm overrides)', () => {
  it('overrides glm-5 series to 200K context, 128K output, thinking=true', async () => {
    const fetchMock = vi.fn(async () =>
      new Response(
        JSON.stringify({
          data: [
            { id: 'glm-5' },
            { id: 'glm-5-turbo' },
            { id: 'glm-5.1' },
          ],
        }),
        { status: 200, headers: { 'Content-Type': 'application/json' } },
      ),
    );
    const def = getProviderLoginDefinition('glm')!;
    const models = await fetchProviderModels(def, 'sk-test', fetchMock as unknown as typeof fetch);
    expect(models).toHaveLength(3);
    for (const model of models) {
      expect(model.contextLength).toBe(200_000);
      expect(model.maxOutputSize).toBe(128_000);
      expect(model.supportsReasoning).toBe(true);
      expect(model.supportsToolUse).toBe(true);
    }
  });

  it('overrides glm-4.5/4.5-air/4.6/4.7 to 128K context, 96K output, thinking=false', async () => {
    const fetchMock = vi.fn(async () =>
      new Response(
        JSON.stringify({
          data: [
            { id: 'glm-4.5' },
            { id: 'glm-4.5-air' },
            { id: 'glm-4.6' },
            { id: 'glm-4.7' },
          ],
        }),
        { status: 200, headers: { 'Content-Type': 'application/json' } },
      ),
    );
    const def = getProviderLoginDefinition('glm')!;
    const models = await fetchProviderModels(def, 'sk-test', fetchMock as unknown as typeof fetch);
    expect(models).toHaveLength(4);
    for (const model of models) {
      expect(model.contextLength).toBe(128_000);
      expect(model.maxOutputSize).toBe(96_000);
      expect(model.supportsReasoning).toBe(false);
      expect(model.supportsToolUse).toBe(true);
    }
  });

  it('leaves unknown glm models untouched', async () => {
    const fetchMock = vi.fn(async () =>
      new Response(
        JSON.stringify({
          data: [{ id: 'glm-unknown', context_length: 32000 }],
        }),
        { status: 200, headers: { 'Content-Type': 'application/json' } },
      ),
    );
    const def = getProviderLoginDefinition('glm')!;
    const models = await fetchProviderModels(def, 'sk-test', fetchMock as unknown as typeof fetch);
    expect(models).toHaveLength(1);
    expect(models[0]).toMatchObject({
      id: 'glm-unknown',
      contextLength: 32000,
      supportsToolUse: true,
      supportsReasoning: false,
    });
    expect(models[0].maxOutputSize).toBeUndefined();
  });
});

describe('applyProviderLoginConfig', () => {
  it('writes provider, models, and sets default when none exists', () => {
    const config: ManagedKimiConfigShape = { providers: {} };
    const def = getProviderLoginDefinition('deepseek')!;
    const models: ProviderModelInfo[] = [
      { id: 'deepseek-chat', contextLength: 64000, supportsToolUse: true, supportsReasoning: false, supportsImageIn: false, supportsVideoIn: false },
    ];

    applyProviderLoginConfig(config, {
      providerName: 'deepseek_main',
      definition: def,
      baseUrl: 'https://api.deepseek.com/v1',
      apiKey: 'sk-test',
      models,
      selectedModel: models[0]!,
      thinking: false,
    });

    expect(config.providers['deepseek_main']).toMatchObject({
      type: 'deepseek',
      baseUrl: 'https://api.deepseek.com/v1',
      apiKey: 'sk-test',
    });
    expect(config.models?.['deepseek_main/deepseek-chat']).toMatchObject({
      provider: 'deepseek_main',
      model: 'deepseek-chat',
      maxContextSize: 64000,
    });
    expect(config.defaultModel).toBe('deepseek_main/deepseek-chat');
  });

  it('does not overwrite existing defaultModel', () => {
    const config: ManagedKimiConfigShape = {
      providers: {},
      defaultModel: 'other/model',
    };
    const def = getProviderLoginDefinition('deepseek')!;
    const models: ProviderModelInfo[] = [
      { id: 'deepseek-chat', contextLength: 64000, supportsToolUse: true, supportsReasoning: false, supportsImageIn: false, supportsVideoIn: false },
    ];

    applyProviderLoginConfig(config, {
      providerName: 'deepseek_main',
      definition: def,
      baseUrl: 'https://api.deepseek.com/v1',
      apiKey: 'sk-test',
      models,
      selectedModel: models[0]!,
      thinking: false,
    });

    expect(config.defaultModel).toBe('other/model');
  });

  it('uses model.maxOutputSize when present, otherwise falls back to 8192', () => {
    const config: ManagedKimiConfigShape = { providers: {} };
    const def = getProviderLoginDefinition('deepseek')!;
    const models: ProviderModelInfo[] = [
      { id: 'glm-5', contextLength: 200_000, maxOutputSize: 128_000, supportsToolUse: true, supportsReasoning: true, supportsImageIn: false, supportsVideoIn: false },
      { id: 'glm-4.5', contextLength: 128_000, maxOutputSize: 96_000, supportsToolUse: true, supportsReasoning: false, supportsImageIn: false, supportsVideoIn: false },
      { id: 'other', contextLength: 64000, supportsToolUse: true, supportsReasoning: false, supportsImageIn: false, supportsVideoIn: false },
    ];

    applyProviderLoginConfig(config, {
      providerName: 'glm_1',
      definition: def,
      baseUrl: 'https://api.z.ai/api/paas/v4',
      apiKey: 'sk-test',
      models,
      selectedModel: models[0]!,
      thinking: false,
    });

    expect(config.models?.['glm_1/glm-5']?.maxOutputSize).toBe(128_000);
    expect(config.models?.['glm_1/glm-4.5']?.maxOutputSize).toBe(96_000);
    expect(config.models?.['glm_1/other']?.maxOutputSize).toBe(8192);
  });
});

describe('removeProviderConfig', () => {
  it('cascade-deletes models and clears default when matched', () => {
    const config: ManagedKimiConfigShape = {
      providers: { deepseek_main: { type: 'deepseek', apiKey: 'sk-test' } },
      models: { 'deepseek_main/chat': { provider: 'deepseek_main', model: 'chat', maxContextSize: 64000 } },
      defaultModel: 'deepseek_main/chat',
    };

    removeProviderConfig(config, 'deepseek_main');

    expect(config.providers['deepseek_main']).toBeUndefined();
    expect(config.models?.['deepseek_main/chat']).toBeUndefined();
    expect(config.defaultModel).toBeUndefined();
  });
});

describe('validateProviderName', () => {
  it('accepts valid names', () => {
    expect(validateProviderName('deepseek_main', {})).toBeUndefined();
    expect(validateProviderName('DeepSeek1', {})).toBeUndefined();
  });

  it('rejects names starting with digit', () => {
    expect(validateProviderName('1deepseek', {})).toContain('start with a letter');
  });

  it('rejects names with spaces', () => {
    expect(validateProviderName('deep seek', {})).toContain('only letters');
  });

  it('rejects names with special chars', () => {
    expect(validateProviderName('deepseek!', {})).toContain('only letters');
  });

  it('rejects reserved names', () => {
    expect(validateProviderName('managed:ody-code', {})).toContain('reserved');
  });

  it('rejects duplicate names', () => {
    expect(validateProviderName('existing', { existing: {} })).toContain('already exists');
  });
});
