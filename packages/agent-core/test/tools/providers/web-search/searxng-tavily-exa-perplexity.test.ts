import { describe, expect, it, vi } from 'vitest';
import { SearXNGProvider } from '../../../../src/tools/providers/web-search/searxng';
import { TavilyProvider } from '../../../../src/tools/providers/web-search/tavily';
import { ExaProvider } from '../../../../src/tools/providers/web-search/exa';
import { PerplexityProvider } from '../../../../src/tools/providers/web-search/perplexity';

describe('SearXNGProvider', () => {
  it('maps results from a configured baseUrl', async () => {
    const data = {
      results: [
        { title: 'T', url: 'https://x.com', content: 'C', publishedDate: '2024-01-01' },
      ],
    };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new SearXNGProvider({ baseUrl: 'https://searx.example.com', timeoutMs: 1000 }, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(1);
    expect(results[0]?.date).toBe('2024-01-01');
    const url = fetchImpl.mock.calls[0]?.[0] as string;
    expect(url).toContain('https://searx.example.com');
    expect(url).toContain('format=json');
  });
});

describe('TavilyProvider', () => {
  it('posts api_key and search_depth', async () => {
    const data = { results: [{ title: 'T', url: 'https://x.com', snippet: 'S' }] };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new TavilyProvider('key', { searchDepth: 'advanced', timeoutMs: 1000 }, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(1);
    const init = fetchImpl.mock.calls[0]?.[1] as RequestInit;
    const body = JSON.parse(init.body as string);
    expect(body).toMatchObject({ api_key: 'key', query: 'hello', search_depth: 'advanced' });
  });
});

describe('ExaProvider', () => {
  it('posts query, type, numResults and contents', async () => {
    const data = {
      results: [{ title: 'T', url: 'https://x.com', text: 'X', publishedDate: '2024-01-01' }],
    };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new ExaProvider('key', { timeoutMs: 1000 }, fetchImpl);
    const results = await provider.search('hello', { limit: 3, includeContent: true });
    expect(results).toHaveLength(1);
    const init = fetchImpl.mock.calls[0]?.[1] as RequestInit;
    const body = JSON.parse(init.body as string);
    expect(body).toMatchObject({ query: 'hello', numResults: 3, contents: { text: true } });
  });
});

describe('PerplexityProvider', () => {
  it('posts query with max_results and max_tokens_per_page', async () => {
    const data = { results: [{ title: 'T', url: 'https://x.com', snippet: 'S' }] };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new PerplexityProvider('key', { timeoutMs: 1000, maxResults: 7 }, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(1);
    const init = fetchImpl.mock.calls[0]?.[1] as RequestInit;
    const body = JSON.parse(init.body as string);
    expect(body).toMatchObject({ query: 'hello', max_results: 7, max_tokens_per_page: 2048 });
  });
});
