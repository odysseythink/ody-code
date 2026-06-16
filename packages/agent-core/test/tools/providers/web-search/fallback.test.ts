import { describe, expect, it, vi } from 'vitest';
import {
  FallbackWebSearchProvider,
  isRetryableError,
} from '../../../../src/tools/providers/web-search/fallback';
import type { WebSearchProvider, WebSearchResult } from '../../../../src/tools/providers/web-search/types';

function fakeProvider(
  name: string,
  behavior: (query: string) => Promise<WebSearchResult[]>,
): WebSearchProvider {
  return { name, search: vi.fn(behavior) };
}

describe('isRetryableError', () => {
  it('treats 401/403/unauthorized/auth as non-retryable', () => {
    expect(isRetryableError(new Error('HTTP 401'))).toBe(false);
    expect(isRetryableError(new Error('HTTP 403'))).toBe(false);
    expect(isRetryableError(new Error('unauthorized'))).toBe(false);
    expect(isRetryableError(new Error('authentication failed'))).toBe(false);
  });

  it('treats 429, 5xx, network, fetch and timeout as retryable', () => {
    expect(isRetryableError(new Error('HTTP 429'))).toBe(true);
    expect(isRetryableError(new Error('HTTP 503'))).toBe(true);
    expect(isRetryableError(new TypeError('fetch failed'))).toBe(true);
    expect(isRetryableError(new Error('network error'))).toBe(true);
    expect(isRetryableError(new Error('timed out'))).toBe(true);
  });

  it('treats AbortError as non-retryable', () => {
    const err = new Error('aborted');
    err.name = 'AbortError';
    expect(isRetryableError(err)).toBe(false);
  });
});

describe('FallbackWebSearchProvider', () => {
  it('returns primary results when primary succeeds', async () => {
    const primary = fakeProvider('primary', async () => [{ title: 'P', url: 'https://p.com', snippet: 'S' }]);
    const secondary = fakeProvider('secondary', async () => [{ title: 'S', url: 'https://s.com', snippet: 'S' }]);
    const fallback = new FallbackWebSearchProvider(primary, secondary, { debug: vi.fn() } as never);
    const results = await fallback.search('hello');
    expect(results).toHaveLength(1);
    expect(results[0]?.title).toBe('P');
    expect(secondary.search).not.toHaveBeenCalled();
  });

  it('falls back to secondary on retryable primary failure', async () => {
    const primary = fakeProvider('primary', async () => {
      throw new Error('HTTP 503');
    });
    const secondary = fakeProvider('secondary', async () => [{ title: 'S', url: 'https://s.com', snippet: 'S' }]);
    const fallback = new FallbackWebSearchProvider(primary, secondary, { debug: vi.fn() } as never);
    const results = await fallback.search('hello');
    expect(results[0]?.title).toBe('S');
  });

  it('throws primary error when secondary is undefined', async () => {
    const primary = fakeProvider('primary', async () => {
      throw new Error('HTTP 503 primary failed');
    });
    const fallback = new FallbackWebSearchProvider(primary, undefined, { debug: vi.fn() } as never);
    await expect(fallback.search('hello')).rejects.toThrow('primary failed');
  });

  it('does not fallback on auth error', async () => {
    const primary = fakeProvider('primary', async () => {
      throw new Error('HTTP 401');
    });
    const secondary = fakeProvider('secondary', async () => [{ title: 'S', url: 'https://s.com', snippet: 'S' }]);
    const fallback = new FallbackWebSearchProvider(primary, secondary, { debug: vi.fn() } as never);
    await expect(fallback.search('hello')).rejects.toThrow('HTTP 401');
    expect(secondary.search).not.toHaveBeenCalled();
  });

  it('throws combined secondary error when both fail', async () => {
    const primary = fakeProvider('primary', async () => {
      throw new Error('HTTP 503 primary failed');
    });
    const secondary = fakeProvider('secondary', async () => {
      throw new Error('HTTP 503 secondary failed');
    });
    const fallback = new FallbackWebSearchProvider(primary, secondary, { debug: vi.fn() } as never);
    await expect(fallback.search('hello')).rejects.toThrow('secondary failed');
  });
});
