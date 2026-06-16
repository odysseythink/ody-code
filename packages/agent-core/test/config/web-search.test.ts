import { describe, expect, it } from 'vitest';
import {
  KimiConfigSchema,
  ServicesConfigSchema,
  WebSearchConfigSchema,
  WebSearchProviderConfigSchema,
  WebSearchProviderNameSchema,
} from '../../src/config/schema';

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

  it('includes webSearch in KimiConfigSchema through services', () => {
    const result = KimiConfigSchema.safeParse({
      services: { webSearch: { primary: { provider: 'perplexity' } } },
    });
    expect(result.success).toBe(true);
    if (!result.success) return;
    expect(result.data.services?.webSearch?.primary.provider).toBe('perplexity');
  });
});
