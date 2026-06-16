import { describe, expect, it, vi } from 'vitest';
import { SerpApiProvider } from '../../../../src/tools/providers/web-search/serpapi';
import { SearchApiProvider } from '../../../../src/tools/providers/web-search/searchapi';
import { SerperProvider } from '../../../../src/tools/providers/web-search/serper';

describe('SerpApiProvider', () => {
  it('maps Google organic results plus knowledge_graph and answer_box', async () => {
    const data = {
      knowledge_graph: { title: 'KG', link: 'https://kg.com', snippet: 'kg snippet' },
      answer_box: { title: 'AB', link: 'https://ab.com', snippet: 'ab snippet' },
      organic_results: [{ title: 'Organic', link: 'https://organic.com', snippet: 'organic snippet' }],
    };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new SerpApiProvider('key', { engine: 'google', timeoutMs: 1000 }, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(3);
    expect(results.map((r) => r.title)).toEqual(['KG', 'AB', 'Organic']);
  });
});

describe('SearchApiProvider', () => {
  it('maps results with Authorization header and source tag', async () => {
    const data = {
      knowledge_graph: { description: { title: 'KG', link: 'https://kg.com', snippet: 'kg' } },
      answer_box: { answer: { title: 'AB', link: 'https://ab.com', snippet: 'ab' } },
      organic_results: [{ title: 'O', link: 'https://o.com', snippet: 'o' }],
    };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new SearchApiProvider('key', { engine: 'google', timeoutMs: 1000 }, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(3);
    const init = fetchImpl.mock.calls[0]?.[1] as RequestInit;
    expect(init.headers).toMatchObject({ Authorization: 'Bearer key', 'X-SearchApi-Source': 'ody-code' });
  });
});

describe('SerperProvider', () => {
  it('maps knowledgeGraph and organic arrays', async () => {
    const data = {
      knowledgeGraph: { title: 'KG', link: 'https://kg.com', snippet: 'kg' },
      organic: [{ title: 'O', link: 'https://o.com', snippet: 'o' }],
    };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new SerperProvider('key', { timeoutMs: 1000 }, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(2);
    const init = fetchImpl.mock.calls[0]?.[1] as RequestInit;
    expect(init.headers).toMatchObject({ 'X-API-KEY': 'key' });
  });
});
