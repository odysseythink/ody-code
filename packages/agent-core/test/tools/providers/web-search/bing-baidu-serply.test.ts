import { describe, expect, it, vi } from 'vitest';
import { BingProvider } from '../../../../src/tools/providers/web-search/bing';
import { BaiduProvider } from '../../../../src/tools/providers/web-search/baidu';
import { SerplyProvider } from '../../../../src/tools/providers/web-search/serply';

describe('BingProvider', () => {
  it('maps webPages.value to normalized results', async () => {
    const data = {
      webPages: {
        value: [
          { name: 'N1', url: 'https://one.com', snippet: 'S1' },
          { name: 'N2', url: 'https://two.com', snippet: 'S2' },
        ],
      },
    };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new BingProvider('key', { timeoutMs: 1000 }, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(2);
    expect(results[0]?.url).toBe('https://one.com');
    const init = fetchImpl.mock.calls[0]?.[1] as RequestInit;
    expect(init.headers).toMatchObject({ 'Ocp-Apim-Subscription-Key': 'key' });
  });
});

describe('BaiduProvider', () => {
  it('maps references and deduplicates by URL', async () => {
    const data = {
      references: [
        { type: 'web', title: 'T', url: 'https://x.com', snippet: 'S' },
        { type: 'web', title: 'T2', url: 'https://x.com', snippet: 'S2' },
        { type: 'image', title: 'I', url: 'https://img.com', snippet: 'S' },
      ],
    };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new BaiduProvider('key', { timeoutMs: 1000 }, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(1);
    expect(results[0]?.url).toBe('https://x.com');
    const init = fetchImpl.mock.calls[0]?.[1] as RequestInit;
    expect(init.body).toContain('top_k');
  });

  it('throws on upstream error payload', async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(
      new Response(JSON.stringify({ code: 'E1', message: 'bad' }), { status: 200 }),
    );
    const provider = new BaiduProvider('key', { timeoutMs: 1000 }, fetchImpl);
    await expect(provider.search('hello')).rejects.toThrow(/bad/);
  });
});

describe('SerplyProvider', () => {
  it('maps results and sets location/device headers', async () => {
    const data = { results: [{ title: 'T', link: 'https://x.com', snippet: 'S' }] };
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response(JSON.stringify(data), { status: 200 }));
    const provider = new SerplyProvider('key', { timeoutMs: 1000, gl: 'us', device: 'desktop' }, fetchImpl);
    const results = await provider.search('hello');
    expect(results).toHaveLength(1);
    const url = fetchImpl.mock.calls[0]?.[0] as string;
    expect(url).toContain('hl=us');
    expect(url).toContain('gl=US');
    const init = fetchImpl.mock.calls[0]?.[1] as RequestInit;
    expect(init.headers).toMatchObject({ 'X-API-KEY': 'key', 'X-User-Agent': 'desktop' });
  });

  it('throws on Unauthorized payload', async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(
      new Response(JSON.stringify({ message: 'Unauthorized' }), { status: 200 }),
    );
    const provider = new SerplyProvider('key', { timeoutMs: 1000 }, fetchImpl);
    await expect(provider.search('hello')).rejects.toThrow(/authentication/i);
  });
});
