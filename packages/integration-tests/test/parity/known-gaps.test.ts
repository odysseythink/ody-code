import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'pathe';
import { checkGapState, findGap, parseKnownGaps, StaleGapError } from '../../src/parity/known-gaps';

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
      scenario: 'mock prompt',
      layer: 'L3',
      reason: 'Rust mock provider 事件 payload 未实现对齐',
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

  it('returns undefined when no match', () => {
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
});
