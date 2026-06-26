import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'pathe';
import { describe, expect, it } from 'vitest';
import { makeTsBackend, makeRustBackend } from '../../src/parity/backends';
import { parseKnownGaps } from '../../src/parity/known-gaps';
import { runParityWithGaps } from '../../src/parity/run-parity';
import {
  helloWorldScenario,
  helloWorldMockLlm,
  fileEditScenario,
  fileEditMockLlm,
  multiTurnToolScenario,
  multiTurnToolMockLlm,
} from '../../src/parity/scenarios';
import { resolveRustBinaryPath } from '../../src/parity/rust-binary';

const knownGapsSource = readFileSync(
  join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'src', 'parity', 'known-gaps.md'),
  'utf8',
);
const knownGaps = parseKnownGaps(knownGapsSource);

const binaryPath = (() => {
  try {
    return resolveRustBinaryPath();
  } catch {
    return null;
  }
})();

const cases = [
  { name: helloWorldScenario.name, scenario: helloWorldScenario, mockLlm: helloWorldMockLlm },
  { name: fileEditScenario.name, scenario: fileEditScenario, mockLlm: fileEditMockLlm },
  { name: multiTurnToolScenario.name, scenario: multiTurnToolScenario, mockLlm: multiTurnToolMockLlm },
];

describe.skipIf(binaryPath === null)('TS-vs-Rust parity', () => {
  it.each(cases)(
    '$name passes or is covered by a known gap',
    async ({ scenario, mockLlm }) => {
      const result = await runParityWithGaps({
        scenario,
        mockLlm,
        makeA: (homeDir) => makeTsBackend({ homeDir, mockLlm }),
        makeB: (homeDir) =>
          makeRustBackend({
            homeDir,
            binaryPath: binaryPath!,
            transport: 'stdio',
            extraArgs: ['--mock-provider'],
          }),
        knownGaps,
        timeoutMs: 60000,
      });
      expect(result.passed).toBe(true);
    },
    120000,
  );
});
