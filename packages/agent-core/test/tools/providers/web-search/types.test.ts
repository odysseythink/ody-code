import { describe, expect, it } from 'vitest';
import {
  normalizeResult,
  normalizeResults,
} from '../../../../src/tools/providers/web-search/types';

describe('normalizeResult', () => {
  it('extracts title, url and snippet from common upstream shapes', () => {
    const r = normalizeResult({ title: 'T', link: 'https://example.com', snippet: 'S' }, 'test');
    expect(r.title).toBe('T');
    expect(r.url).toBe('https://example.com');
    expect(r.snippet).toBe('S');
    expect(r.raw).toEqual({ title: 'T', link: 'https://example.com', snippet: 'S' });
  });

  it('falls back through url/link/uri and snippet/description/content/text', () => {
    const r = normalizeResult({ name: 'N', uri: 'https://x.com', content: 'C' }, 'test');
    expect(r.title).toBe('N');
    expect(r.url).toBe('https://x.com');
    expect(r.snippet).toBe('C');
  });

  it('truncates oversized fields defensively', () => {
    const r = normalizeResult(
      { title: 'x'.repeat(600), url: 'https://x.com/' + 'y'.repeat(3000), snippet: 'z'.repeat(5000) },
      'test',
    );
    expect(r.title.length).toBe(500);
    expect(r.url.length).toBe(2048);
    expect(r.snippet.length).toBe(4000);
  });
});

describe('normalizeResults', () => {
  it('drops results with empty title or url', () => {
    const out = normalizeResults(
      [
        { title: 'T', url: 'https://x.com', snippet: 'S' },
        { title: '', url: 'https://x.com', snippet: 'S' },
        { title: 'T', url: '', snippet: 'S' },
      ],
      'test',
    );
    expect(out).toHaveLength(1);
  });

  it('returns an empty array for non-array input', () => {
    expect(normalizeResults(null as unknown as unknown[], 'test')).toEqual([]);
  });
});
