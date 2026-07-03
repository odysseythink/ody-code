import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'pathe';
import {
  bashToolCallMockLlm,
  bashToolCallScenario,
} from '../../src/parity/scenarios/bash-tool-call';
import { makeTsBackend, makeRustBackend } from '../../src/parity/backends';
import { parseKnownGaps } from '../../src/parity/known-gaps';
import { runParityWithGaps } from '../../src/parity/run-parity';
import { resolveRustBinaryPath } from '../../src/parity/rust-binary';

const knownGaps = parseKnownGaps(
  readFileSync(
    join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'src', 'parity', 'known-gaps.md'),
    'utf8',
  ),
);

const binaryPath = (() => {
  try {
    return resolveRustBinaryPath();
  } catch {
    return null;
  }
})();

describe.skipIf(binaryPath === null)('Bash tool call parity', () => {
  it('TS and Rust tool call/result shapes are covered by known gaps where they differ', async () => {
    const result = await runParityWithGaps({
      scenario: bashToolCallScenario,
      mockLlm: bashToolCallMockLlm,
      makeA: (homeDir) => makeTsBackend({ homeDir, mockLlm: bashToolCallMockLlm }),
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
  }, 120000);
});
