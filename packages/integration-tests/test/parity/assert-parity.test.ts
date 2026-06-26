import { describe, expect, it } from 'vitest';
import { assertParity } from '../../src/parity/assert-parity';
import type { NormalizedSnapshot } from '../../src/parity/types';

function snap(overrides: Partial<NormalizedSnapshot> = {}): NormalizedSnapshot {
  return { responses: [], events: [], ...overrides };
}

describe('assertParity', () => {
  it('returns null for identical snapshots', () => {
    const s = snap({ responses: ['a'], events: [{ type: 'turn.started', turnId: 1 } as any] });
    expect(assertParity('same', s, structuredClone(s))).toBeNull();
  });

  it('reports primitive diff', () => {
    const diff = assertParity('primitive', snap({ responses: [1] }), snap({ responses: [2] }));
    expect(diff).not.toBeNull();
    expect(diff!.diffs).toEqual([{ path: '$.responses[0]', tsValue: 1, rustValue: 2 }]);
  });

  it('reports array length diff', () => {
    const diff = assertParity('length', snap({ responses: [[1]] }), snap({ responses: [[1, 2]] }));
    expect(diff!.diffs).toEqual([{ path: '$.responses[0]', tsValue: 1, rustValue: 2 }]);
  });

  it('reports missing key diff', () => {
    const diff = assertParity(
      'missing',
      snap({ responses: [{ a: 1 }] }),
      snap({ responses: [{ a: 1, b: 2 }] }),
    );
    expect(diff!.diffs).toEqual([{ path: '$.responses[0].b', tsValue: undefined, rustValue: 2 }]);
  });

  it('includes scenario name and both snapshots', () => {
    const ts = snap({ responses: ['x'] });
    const rust = snap({ responses: ['y'] });
    const diff = assertParity('named', ts, rust);
    expect(diff!.scenarioName).toBe('named');
    expect(diff!.ts).toBe(ts);
    expect(diff!.rust).toBe(rust);
  });
});
