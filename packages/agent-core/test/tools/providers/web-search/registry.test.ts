import { describe, expect, it, vi } from 'vitest';
import { createDefaultRegistry } from '../../../../src/tools/providers/web-search/registry';

describe('createDefaultRegistry', () => {
  it('creates a DuckDuckGo provider', () => {
    const registry = createDefaultRegistry();
    const provider = registry.create({ provider: 'duckduckgo', timeoutMs: 1000 }, { fetchImpl: vi.fn() });
    expect(provider.name).toBe('duckduckgo');
  });

  it('creates a Tavily provider with an apiKey', () => {
    const registry = createDefaultRegistry();
    const provider = registry.create(
      { provider: 'tavily', apiKey: 'sk-tavily', timeoutMs: 1000 },
      { fetchImpl: vi.fn() },
    );
    expect(provider.name).toBe('tavily');
  });

  it('rejects unknown provider names', () => {
    const registry = createDefaultRegistry();
    expect(() =>
      registry.create({ provider: 'unknown' as never, timeoutMs: 1000 }, { fetchImpl: vi.fn() }),
    ).toThrow(/Unknown web search provider: unknown/);
  });

  it('creates a Moonshot provider from the existing adapter', () => {
    const registry = createDefaultRegistry();
    const provider = registry.create(
      { provider: 'moonshot', timeoutMs: 1000 },
      {
        fetchImpl: vi.fn(),
        moonshotServiceConfig: { baseUrl: 'https://search.example/v1', apiKey: 'sk-moon' },
      },
    );
    expect(provider.name).toBe('moonshot');
  });

  it('throws when moonshot is requested without moonshotServiceConfig', () => {
    const registry = createDefaultRegistry();
    expect(() =>
      registry.create({ provider: 'moonshot', timeoutMs: 1000 }, { fetchImpl: vi.fn() }),
    ).toThrow(/moonshotSearch/);
  });
});
