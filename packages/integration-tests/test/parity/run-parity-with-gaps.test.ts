import { describe, expect, it } from 'vitest';
import { runParityWithGaps } from '../../src/parity/run-parity';
import { StaleGapError } from '../../src/parity/known-gaps';
import type { ParityBackend, Scenario } from '../../src/parity/types';

function fakeBackend(homeDir: string, kind: 'ts' | 'rust'): ParityBackend {
  const listeners = new Set<(event: unknown) => void>();
  return {
    kind,
    homeDir,
    client: {
      onEvent(listener: (event: unknown) => void) {
        listeners.add(listener);
        return () => listeners.delete(listener);
      },
    } as any,
    close: async () => {},
  };
}

const passingScenario: Scenario = {
  name: 'passing',
  async run() {
    return { responses: ['ok'], events: [] };
  },
};

const failingScenario: Scenario = {
  name: 'failing',
  async run(backend) {
    return { responses: [backend.kind], events: [] };
  },
};

const errorScenario: Scenario = {
  name: 'error',
  async run() {
    throw new Error('backend failed');
  },
};

describe('runParityWithGaps', () => {
  it('passes when diff is null and no gap is registered', async () => {
    const result = await runParityWithGaps({
      scenario: passingScenario,
      mockLlm: {} as any,
      makeA: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
      makeB: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
      knownGaps: [],
    });
    expect(result.passed).toBe(true);
    expect(result.diff).toBeNull();
    expect(result.gapReason).toBeUndefined();
  });

  it('passes when diff exists but an L3 gap is registered', async () => {
    const result = await runParityWithGaps({
      scenario: failingScenario,
      mockLlm: {} as any,
      makeA: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
      makeB: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'rust')),
      knownGaps: [{ scenario: 'failing', layer: 'L3', reason: 'mock mismatch' }],
    });
    expect(result.passed).toBe(true);
    expect(result.diff).not.toBeNull();
    expect(result.gapReason).toBe('mock mismatch');
  });

  it('passes when diff exists but an L4 wildcard gap is registered', async () => {
    const result = await runParityWithGaps({
      scenario: failingScenario,
      mockLlm: {} as any,
      makeA: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
      makeB: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'rust')),
      knownGaps: [{ scenario: '*', layer: 'L4', reason: 'records not migrated' }],
    });
    expect(result.passed).toBe(true);
    expect(result.diff).not.toBeNull();
  });

  it('throws StaleGapError when diff is null but gap is registered', async () => {
    await expect(
      runParityWithGaps({
        scenario: passingScenario,
        mockLlm: {} as any,
        makeA: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
        makeB: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
        knownGaps: [{ scenario: 'passing', layer: 'L3', reason: 'mock mismatch' }],
      }),
    ).rejects.toBeInstanceOf(StaleGapError);
  });

  it('passes when runParity throws but a gap is registered', async () => {
    const result = await runParityWithGaps({
      scenario: errorScenario,
      mockLlm: {} as any,
      makeA: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
      makeB: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'rust')),
      knownGaps: [{ scenario: 'error', layer: 'L3', reason: 'backend unstable' }],
    });
    expect(result.passed).toBe(true);
    expect(result.diff).not.toBeNull();
    expect(result.gapReason).toBe('backend unstable');
  });

  it('re-throws when runParity throws and no gap is registered', async () => {
    await expect(
      runParityWithGaps({
        scenario: errorScenario,
        mockLlm: {} as any,
        makeA: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
        makeB: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'rust')),
        knownGaps: [],
      }),
    ).rejects.toThrow('backend failed');
  });
});
