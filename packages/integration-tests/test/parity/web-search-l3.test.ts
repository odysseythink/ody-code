import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'pathe';
import { webSearchMockLlm, webSearchScenario } from '../../src/parity/scenarios/web-search';
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
  try { return resolveRustBinaryPath(); } catch { return null; }
})();

const mockSearcher = {
  name: 'mock',
  async search(query: string) {
    return [{ title: `Mock result for ${query}`, url: 'https://example.test/1', snippet: 'mock snippet' }];
  },
};

describe.skipIf(binaryPath === null)('WebSearch L3 parity', () => {
  it('TS and Rust emit the same tool call/result shapes for WebSearch', async () => {
    const result = await runParityWithGaps({
      scenario: webSearchScenario,
      mockLlm: webSearchMockLlm,
      makeA: (homeDir) => makeTsBackend({ homeDir, mockLlm: webSearchMockLlm, runtime: { webSearcher: mockSearcher } }),
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
