import { describe, expect, it, vi } from 'vitest';
import {
  authHeaderForProvider,
  buildUrl,
  getJson,
  httpError,
  postJson,
} from '../../../../src/tools/providers/web-search/http';

describe('buildUrl', () => {
  it('omits undefined params and stringifies values', () => {
    expect(buildUrl('https://api.example.com/search', { q: 'hello', limit: 10, skip: undefined })).toBe(
      'https://api.example.com/search?q=hello&limit=10',
    );
  });
});

describe('authHeaderForProvider', () => {
  it('returns Bearer for searchapi, baidu, perplexity', () => {
    expect(authHeaderForProvider('searchapi', 'k')).toEqual({ Authorization: 'Bearer k' });
    expect(authHeaderForProvider('baidu', 'k')).toEqual({
      Authorization: 'Bearer k',
      'X-Appbuilder-Authorization': 'Bearer k',
    });
    expect(authHeaderForProvider('perplexity', 'k')).toEqual({ Authorization: 'Bearer k' });
  });

  it('returns X-API-KEY for serper and serply', () => {
    expect(authHeaderForProvider('serper', 'k')).toEqual({ 'X-API-KEY': 'k' });
    expect(authHeaderForProvider('serply', 'k')).toEqual({ 'X-API-KEY': 'k' });
  });

  it('returns Ocp-Apim-Subscription-Key for bing', () => {
    expect(authHeaderForProvider('bing', 'k')).toEqual({ 'Ocp-Apim-Subscription-Key': 'k' });
  });

  it('returns lowercase x-api-key for exa', () => {
    expect(authHeaderForProvider('exa', 'k')).toEqual({ 'x-api-key': 'k' });
  });

  it('returns an empty object for providers that pass keys in query/body', () => {
    expect(authHeaderForProvider('serpapi', 'k')).toEqual({});
    expect(authHeaderForProvider('searxng', 'k')).toEqual({});
    expect(authHeaderForProvider('tavily', 'k')).toEqual({});
  });
});

describe('postJson', () => {
  it('times out after timeoutMs', async () => {
    const fetchImpl = vi.fn<typeof fetch>((_url, init) => {
      // Create a promise that rejects when the signal fires
      return new Promise<Response>((_resolve, reject) => {
        const signal = (init as RequestInit)?.signal as AbortSignal | undefined;
        if (signal?.aborted) {
          reject(new DOMException('aborted', 'AbortError'));
          return;
        }
        signal?.addEventListener('abort', () => {
          reject(new DOMException('aborted', 'AbortError'));
        }, { once: true });
      });
    });
    const promise = postJson('https://api.example.com', { q: 'x' }, {
      fetchImpl,
      timeoutMs: 5,
      provider: 'tavily',
    });
    await expect(promise).rejects.toThrow(/timed out|aborted/i);
  }, 10000);

  it('sends JSON body and default headers', async () => {
    const fetchImpl = vi.fn<typeof fetch>().mockResolvedValue(new Response('{}'));
    await postJson('https://api.example.com', { q: 'x' }, {
      fetchImpl,
      timeoutMs: 1000,
      apiKey: 'ak',
      provider: 'perplexity',
      toolCallId: 'tc1',
    });
    const init = fetchImpl.mock.calls[0]?.[1] as RequestInit;
    expect(init.method).toBe('POST');
    expect(init.body).toBe('{"q":"x"}');
    expect(init.headers).toMatchObject({
      'Content-Type': 'application/json',
      Authorization: 'Bearer ak',
      'X-Msh-Tool-Call-Id': 'tc1',
    });
  });
});

describe('httpError', () => {
  it('includes status and a JSON body detail', async () => {
    const response = new Response(JSON.stringify({ error: 'bad' }), { status: 500, statusText: 'Oops' });
    const err = await httpError(response, 'test');
    expect(err.message).toContain('HTTP 500');
    expect(err.message).toContain('bad');
  });
});
