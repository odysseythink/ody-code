import { describe, expect, it } from 'vitest';
import { normalizeAllowList } from '../allow-list';

describe('normalizeAllowList', () => {
  it('deduplicates and trims entries', () => {
    const list = normalizeAllowList([' EXAMPLE_KEY ', 'EXAMPLE_KEY', 'YOUR_API_KEY']);
    expect(list).toEqual(['EXAMPLE_KEY', 'YOUR_API_KEY']);
  });

  it('filters empty strings', () => {
    expect(normalizeAllowList(['', '  ', 'OK'])).toEqual(['OK']);
  });

  it('returns empty array for undefined', () => {
    expect(normalizeAllowList(undefined)).toEqual([]);
  });
});
