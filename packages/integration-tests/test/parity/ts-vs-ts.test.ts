import { describe, expect, it } from 'vitest';

import { makeTsBackend } from '../../src/parity/backends';
import { runParity } from '../../src/parity/run-parity';
import { scenarios } from '../../src/parity/scenarios';
import type { ParityBackend, Scenario } from '../../src/parity/types';

function fakeBackend(homeDir: string, kind: 'ts' | 'rust'): ParityBackend {
  return {
    kind,
    homeDir,
    client: {
      onEvent() {
        return () => {};
      },
    } as any,
    close: async () => {},
  };
}

describe('TS-vs-TS parity harness', () => {
  it.each(scenarios)(
    '$scenario.name produces identical normalized snapshots on two TS backends',
    async ({ scenario, mockLlm }) => {
      const diff = await runParity({
        scenario,
        mockLlm,
        makeA: (homeDir) => makeTsBackend({ homeDir, mockLlm }),
        makeB: (homeDir) => makeTsBackend({ homeDir, mockLlm }),
        timeoutMs: 30000,
      });
      expect(diff, `scenario "${scenario.name}" should produce no diff in TS-vs-TS`).toBeNull();
    },
    120000,
  );

  it('detects when two backends produce different snapshots', async () => {
    const scenario: Scenario = {
      name: 'intentionally-different',
      async run(backend) {
        return {
          responses: [{ kind: backend.kind }],
          events: [],
        };
      },
    };

    const diff = await runParity({
      scenario,
      mockLlm: {} as any,
      makeA: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'ts')),
      makeB: (homeDir) => Promise.resolve(fakeBackend(homeDir, 'rust')),
      timeoutMs: 1000,
    });

    expect(diff).not.toBeNull();
    expect(diff!.scenarioName).toBe('intentionally-different');
    expect(diff!.diffs.some((d) => d.path.includes('kind'))).toBe(true);
  });
});
