import { describe, expect, it } from 'vitest';
import { runParityWithGaps } from '../../src/parity/run-parity';
import { StaleGapError } from '../../src/parity/known-gaps';
import type { ParityBackend, Scenario } from '../../src/parity/types';

function fakeBackend(homeDir: string, kind: 'ts' | 'rust'): ParityBackend & { emitEvent(event: unknown): void } {
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
    emitEvent(event: unknown) {
      for (const listener of listeners) listener(event);
    },
  };
}

const passingScenario: Scenario = {
  name: 'passing',
  async run() {
    return { responses: ['ok'], events: [] };
  },
};

const l3FailingScenario: Scenario = {
  name: 'l3-failing',
  async run(backend) {
    (backend as ReturnType<typeof fakeBackend>).emitEvent({ type: 'turn.ended', turnId: 1, reason: (backend as { kind: string }).kind });
    return { responses: [], events: [] };
  },
};

const l4FailingScenario: Scenario = {
  name: 'l4-failing',
  async run(backend) {
    return { responses: [], events: [], records: [`${backend.kind}-record`] };
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
      scenario: l3FailingScenario,
      mockLlm: {} as any,
      makeA: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
      makeB: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'rust')),
      knownGaps: [{ scenario: 'l3-failing', layer: 'L3', reason: 'mock mismatch' }],
    });
    expect(result.passed).toBe(true);
    expect(result.diff).not.toBeNull();
    expect(result.gapReason).toBe('mock mismatch');
  });

  it('does not let L4 wildcard cover an L3 diff', async () => {
    const result = await runParityWithGaps({
      scenario: l3FailingScenario,
      mockLlm: {} as any,
      makeA: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
      makeB: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'rust')),
      knownGaps: [{ scenario: '*', layer: 'L4', reason: 'records not migrated' }],
    });
    expect(result.passed).toBe(false);
    expect(result.diff).not.toBeNull();
    expect(result.gapReason).toBeUndefined();
  });

  it('does not let an L3 gap cover an L4 diff', async () => {
    const result = await runParityWithGaps({
      scenario: l4FailingScenario,
      mockLlm: {} as any,
      makeA: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
      makeB: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'rust')),
      knownGaps: [{ scenario: 'l4-failing', layer: 'L3', reason: 'mock mismatch' }],
    });
    expect(result.passed).toBe(false);
    expect(result.diff).not.toBeNull();
    expect(result.gapReason).toBeUndefined();
  });

  it('passes when diff exists but an L4 gap is registered for an L4 diff', async () => {
    const result = await runParityWithGaps({
      scenario: l4FailingScenario,
      mockLlm: {} as any,
      makeA: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
      makeB: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'rust')),
      knownGaps: [{ scenario: 'l4-failing', layer: 'L4', reason: 'records not migrated' }],
    });
    expect(result.passed).toBe(true);
    expect(result.diff).not.toBeNull();
    expect(result.gapReason).toBe('records not migrated');
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

  it('throws StaleGapError at L2 when diff is null and an L2 gap is registered', async () => {
    await expect(
      runParityWithGaps({
        scenario: passingScenario,
        mockLlm: {} as any,
        makeA: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
        makeB: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
        knownGaps: [{ scenario: 'passing', layer: 'L2', reason: 'session id prefix' }],
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
