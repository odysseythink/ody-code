import { describe, expect, it, vi } from 'vitest';
import { DuckDuckGoProvider } from '../../../../src/tools/providers/web-search/duckduckgo';

const SAMPLE_HTML = `
<div class="result results_links">
  <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fone">Title One</a>
  <a class="result__snippet"><b>Snippet</b> one</a>
</div>
<div class="result results_links">
  <a class="result__a" href="https://example.com/two">Title Two</a>
  <a class="result__snippet">Snippet two</a>
</div>
`;

describe('DuckDuckGoProvider', () => {
  it('parses captured HTML into normalized results', async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(SAMPLE_HTML, { status: 200 }));
    const provider = new DuckDuckGoProvider({ timeoutMs: 1000 }, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(2);
    expect(results[0]).toMatchObject({
      title: 'Title One',
      url: 'https://example.com/one',
      snippet: 'Snippet one',
    });
    expect(results[1]).toMatchObject({
      title: 'Title Two',
      url: 'https://example.com/two',
      snippet: 'Snippet two',
    });
  });

  it('throws on non-ok HTTP status', async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response('err', { status: 503 }));
    const provider = new DuckDuckGoProvider({ timeoutMs: 1000 }, fetchImpl);
    await expect(provider.search('hello')).rejects.toThrow(/HTTP 503/);
  });

  it('respects proxyUrl when configured', async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(SAMPLE_HTML, { status: 200 }));
    const provider = new DuckDuckGoProvider({ proxyUrl: 'http://proxy.example.com', timeoutMs: 1000 }, fetchImpl);
    await provider.search('hello');
    expect(fetchImpl).toHaveBeenCalledWith(
      'http://proxy.example.com',
      expect.objectContaining({
        method: 'GET',
        headers: expect.objectContaining({ 'X-Proxy-Url': 'https://html.duckduckgo.com/html?q=hello' }),
      }),
    );
  });
});
