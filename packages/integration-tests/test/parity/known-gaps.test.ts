import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'pathe';
import { affectedLayers, checkGapState, findGap, findGapForLayers, parseKnownGaps, StaleGapError } from '../../src/parity/known-gaps';

const knownGapsPath = join(
  dirname(fileURLToPath(import.meta.url)),
  '..',
  '..',
  'src',
  'parity',
  'known-gaps.md',
);

describe('parseKnownGaps', () => {
  it('parses the real known-gaps.md', () => {
    const source = readFileSync(knownGapsPath, 'utf8');
    const gaps = parseKnownGaps(source);
    expect(gaps).toContainEqual({
      scenario: 'hello-world',
      layer: 'L3',
      reason: 'Rust 后端 mock provider 未 emit `turn.ended`，scenario 等待超时',
    });
    expect(gaps.some((g) => g.scenario === '*' && g.layer === 'L4')).toBe(true);
  });

  it('ignores header and separator rows', () => {
    const source = `| Scenario | Layer | Reason |\n|---|---|---|\n| x | L2 | r |`;
    expect(parseKnownGaps(source)).toEqual([{ scenario: 'x', layer: 'L2', reason: 'r' }]);
  });

  it('skips rows with invalid layer', () => {
    const source = `| x | L9 | r |`;
    expect(parseKnownGaps(source)).toEqual([]);
  });
});

describe('findGap', () => {
  const gaps = [
    { scenario: 'mock prompt', layer: 'L3' as const, reason: 'r1' },
    { scenario: '*', layer: 'L4' as const, reason: 'r2' },
  ];

  it('matches exact scenario', () => {
    expect(findGap(gaps, 'mock prompt', 'L3')).toBe('r1');
  });

  it('matches wildcard', () => {
    expect(findGap(gaps, 'session lifecycle', 'L4')).toBe('r2');
  });

  it('prefers exact match over wildcard for the same layer', () => {
    const mixed = [
      { scenario: '*', layer: 'L3' as const, reason: 'wildcard' },
      { scenario: 'exact', layer: 'L3' as const, reason: 'exact-reason' },
    ];
    expect(findGap(mixed, 'exact', 'L3')).toBe('exact-reason');
  });

  it('returns undefined when no match', () => {
    expect(findGap(gaps, 'session lifecycle', 'L3')).toBeUndefined();
  });

  it('returns undefined for exact scenario matched at wrong layer', () => {
    const layerSpecific = [{ scenario: 'mock prompt', layer: 'L3' as const, reason: 'r1' }];
    expect(findGap(layerSpecific, 'mock prompt', 'L4')).toBeUndefined();
  });

  it('returns undefined for wildcard matched at wrong layer', () => {
    expect(findGap(gaps, 'session lifecycle', 'L3')).toBeUndefined();
  });
});

describe('checkGapState', () => {
  const gaps = [{ scenario: 'mock prompt', layer: 'L3' as const, reason: 'r1' }];

  it('throws StaleGapError when gap is registered but scenario passes', () => {
    expect(() => checkGapState(gaps, 'mock prompt', 'L3', true)).toThrow(StaleGapError);
  });

  it('does nothing when gap is registered and scenario fails', () => {
    expect(() => checkGapState(gaps, 'mock prompt', 'L3', false)).not.toThrow();
  });

  it('does nothing when no gap is registered', () => {
    expect(() => checkGapState(gaps, 'setModel', 'L3', true)).not.toThrow();
  });

  it('does nothing when gap is registered at a different layer', () => {
    expect(() => checkGapState(gaps, 'mock prompt', 'L4', true)).not.toThrow();
  });
});

describe('affectedLayers', () => {
  it('maps response paths to L2', () => {
    expect(affectedLayers(['$.responses[0].id'])).toEqual(['L2']);
  });

  it('maps event paths to L3', () => {
    expect(affectedLayers(['$.events[0].type'])).toEqual(['L3']);
  });

  it('maps record and fsTree paths to L4', () => {
    expect(affectedLayers(['$.records[0]', '$.fsTree.foo'])).toEqual(['L4']);
  });

  it('maps mixed paths to all affected layers', () => {
    expect(affectedLayers(['$.responses[0]', '$.events[0].type'])).toEqual(['L2', 'L3']);
  });

  it('defaults unknown paths to L3', () => {
    expect(affectedLayers(['$.error.runParity'])).toEqual(['L3']);
  });

  it('keeps unknown paths together with recognized layers', () => {
    expect(affectedLayers(['$.responses[0]', '$.error.runParity'])).toEqual(['L2', 'L3']);
  });
});

describe('findGapForLayers', () => {
  const gaps = [
    { scenario: 'x', layer: 'L2' as const, reason: 'l2-reason' },
    { scenario: 'x', layer: 'L3' as const, reason: 'l3-reason' },
    { scenario: '*', layer: 'L4' as const, reason: 'l4-wildcard' },
  ];

  it('returns the first matching layer', () => {
    expect(findGapForLayers(gaps, 'x', ['L2', 'L3'])).toEqual({ layer: 'L2', reason: 'l2-reason' });
  });

  it('respects the order of the layers argument', () => {
    expect(findGapForLayers(gaps, 'x', ['L3', 'L2'])).toEqual({ layer: 'L3', reason: 'l3-reason' });
  });

  it('falls back to wildcard at a matching layer', () => {
    expect(findGapForLayers(gaps, 'other', ['L4'])).toEqual({ layer: 'L4', reason: 'l4-wildcard' });
  });

  it('returns undefined when no layer matches', () => {
    const noWildcard = [
      { scenario: 'x', layer: 'L2' as const, reason: 'l2-reason' },
      { scenario: 'x', layer: 'L3' as const, reason: 'l3-reason' },
    ];
    expect(findGapForLayers(noWildcard, 'x', ['L4'])).toBeUndefined();
  });
});
