import { describe, expect, it, vi } from 'vitest';
import { resolveWebSearchRuntime } from '../../../../src/tools/providers/web-search/runtime';
import type { OdyConfig } from '@odysseythink/agent-core-shared';

describe('resolveWebSearchRuntime', () => {
  it('returns undefined when no search config exists', () => {
    const config: OdyConfig = { providers: {} };
    expect(resolveWebSearchRuntime(config, { fetchImpl: vi.fn() })).toBeUndefined();
  });

  it('returns a fallback provider for webSearch.primary', () => {
    const config: OdyConfig = {
      providers: {},
      services: {
        webSearch: { primary: { provider: 'duckduckgo' } },
      },
    };
    const runtime = resolveWebSearchRuntime(config, { fetchImpl: vi.fn() });
    expect(runtime).toBeDefined();
    expect(runtime?.name).toBe('fallback');
  });

  it('composes primary and secondary providers', () => {
    const config: OdyConfig = {
      providers: {},
      services: {
        webSearch: {
          primary: { provider: 'tavily', apiKey: 'sk-tavily' },
          secondary: { provider: 'duckduckgo' },
        },
      },
    };
    const runtime = resolveWebSearchRuntime(config, { fetchImpl: vi.fn() });
    expect(runtime).toBeDefined();
  });

  it('aliases moonshotSearch to a moonshot provider', () => {
    const config: OdyConfig = {
      providers: {},
      services: {
        moonshotSearch: { baseUrl: 'https://search.example/v1' },
      },
    };
    const runtime = resolveWebSearchRuntime(config, { fetchImpl: vi.fn() });
    expect(runtime).toBeDefined();
    expect(runtime?.name).toBe('fallback');
  });
});
