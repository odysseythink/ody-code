import { describe, expect, it } from 'vitest';
import {
  OdyConfigSchema,
  ServicesConfigSchema,
  WebSearchConfigSchema,
  WebSearchProviderConfigSchema,
  WebSearchProviderNameSchema,
} from '../../src/config/schema';
import { resolveWebSearchConfig } from '../../src/config/web-search';
import type { OdyConfig } from '../../src/config/schema';

describe('WebSearchConfigSchema', () => {
  it('accepts a minimal primary config', () => {
    const result = WebSearchConfigSchema.safeParse({ primary: { provider: 'duckduckgo' } });
    expect(result.success).toBe(true);
  });

  it('accepts primary and secondary slots', () => {
    const result = WebSearchConfigSchema.safeParse({
      primary: { provider: 'tavily', apiKey: 'sk-primary' },
      secondary: { provider: 'duckduckgo' },
    });
    expect(result.success).toBe(true);
  });

  it('rejects unknown provider names', () => {
    const result = WebSearchProviderConfigSchema.safeParse({ provider: 'unknown' });
    expect(result.success).toBe(false);
  });

  it('rejects timeoutMs below 1000', () => {
    const result = WebSearchProviderConfigSchema.safeParse({
      provider: 'duckduckgo',
      timeoutMs: 500,
    });
    expect(result.success).toBe(false);
  });

  it('rejects timeoutMs above 120000', () => {
    const result = WebSearchProviderConfigSchema.safeParse({
      provider: 'duckduckgo',
      timeoutMs: 200000,
    });
    expect(result.success).toBe(false);
  });

  it('includes webSearch in ServicesConfigSchema', () => {
    const result = ServicesConfigSchema.safeParse({
      moonshotSearch: { baseUrl: 'https://search.example/v1' },
      webSearch: { primary: { provider: 'exa' } },
    });
    expect(result.success).toBe(true);
  });

  it('includes webSearch in OdyConfigSchema through services', () => {
    const result = OdyConfigSchema.safeParse({
      services: { webSearch: { primary: { provider: 'perplexity' } } },
    });
    expect(result.success).toBe(true);
    if (!result.success) return;
    expect(result.data.services?.webSearch?.primary.provider).toBe('perplexity');
  });
});

describe('resolveWebSearchConfig', () => {
  it('returns undefined when neither webSearch nor moonshotSearch is configured', () => {
    const config: OdyConfig = { providers: {} };
    expect(resolveWebSearchConfig(config)).toBeUndefined();
  });

  it('aliases moonshotSearch to a moonshot primary provider', () => {
    const config: OdyConfig = {
      providers: {},
      services: {
        moonshotSearch: { baseUrl: 'https://search.example/v1', apiKey: 'sk-moonshot' },
      },
    };
    const resolved = resolveWebSearchConfig(config);
    expect(resolved).toBeDefined();
    expect(resolved?.primary.provider).toBe('moonshot');
    expect(resolved?.primary.apiKey).toBe('sk-moonshot');
    expect(resolved?.primary.timeoutMs).toBe(25000);
  });

  it('gives webSearch precedence over moonshotSearch', () => {
    const config: OdyConfig = {
      providers: {},
      services: {
        moonshotSearch: { baseUrl: 'https://search.example/v1' },
        webSearch: { primary: { provider: 'exa' } },
      },
    };
    const resolved = resolveWebSearchConfig(config);
    expect(resolved?.primary.provider).toBe('exa');
  });

  it('preserves secondary provider from webSearch', () => {
    const config: OdyConfig = {
      providers: {},
      services: {
        webSearch: {
          primary: { provider: 'tavily' },
          secondary: { provider: 'duckduckgo' },
        },
      },
    };
    const resolved = resolveWebSearchConfig(config);
    expect(resolved?.secondary?.provider).toBe('duckduckgo');
  });
});
