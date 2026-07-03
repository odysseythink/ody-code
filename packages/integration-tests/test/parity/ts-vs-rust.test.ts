import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'pathe';
import { describe, expect, it } from 'vitest';
import { makeTsBackend, makeRustBackend } from '../../src/parity/backends';
import { parseKnownGaps } from '../../src/parity/known-gaps';
import { runParityWithGaps } from '../../src/parity/run-parity';
import {
  sessionLifecycleScenario,
  sessionLifecycleMockLlm,
  setModelScenario,
  setModelMockLlm,
  mockPromptScenario,
  mockPromptMockLlm,
  helloWorldScenario,
  helloWorldMockLlm,
  fileEditScenario,
  fileEditMockLlm,
  multiTurnToolScenario,
  multiTurnToolMockLlm,
  hostConfigScenario,
  hostConfigMockLlm,
  sessionModeHandoffScenario,
  sessionModeHandoffMockLlm,
  backgroundCronScenario,
  backgroundCronMockLlm,
  webSearchScenario,
  webSearchMockLlm,
  bashToolCallScenario,
  bashToolCallMockLlm,
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

const rustTransportEnv = process.env['ODY_HOST_TRANSPORT'] ?? 'stdio';
const rustTransport = (homeDir: string): 'stdio' | { socketPath: string } => {
  if (rustTransportEnv === 'uds') {
    return { socketPath: join(homeDir, 'ody-host.sock') };
  }
  return 'stdio';
};

const mockSearcher = {
  name: 'mock',
  async search(query: string) {
    return [{ title: `Mock result for ${query}`, url: 'https://example.test/1', snippet: 'mock snippet' }];
  },
};

const cases = [
  { name: sessionLifecycleScenario.name, scenario: sessionLifecycleScenario, mockLlm: sessionLifecycleMockLlm },
  { name: setModelScenario.name, scenario: setModelScenario, mockLlm: setModelMockLlm },
  { name: mockPromptScenario.name, scenario: mockPromptScenario, mockLlm: mockPromptMockLlm },
  { name: helloWorldScenario.name, scenario: helloWorldScenario, mockLlm: helloWorldMockLlm },
  { name: fileEditScenario.name, scenario: fileEditScenario, mockLlm: fileEditMockLlm },
  { name: multiTurnToolScenario.name, scenario: multiTurnToolScenario, mockLlm: multiTurnToolMockLlm },
  { name: hostConfigScenario.name, scenario: hostConfigScenario, mockLlm: hostConfigMockLlm },
  { name: sessionModeHandoffScenario.name, scenario: sessionModeHandoffScenario, mockLlm: sessionModeHandoffMockLlm },
  { name: backgroundCronScenario.name, scenario: backgroundCronScenario, mockLlm: backgroundCronMockLlm },
  { name: webSearchScenario.name, scenario: webSearchScenario, mockLlm: webSearchMockLlm },
  { name: bashToolCallScenario.name, scenario: bashToolCallScenario, mockLlm: bashToolCallMockLlm },
];

describe.skipIf(binaryPath === null)('TS-vs-Rust parity', () => {
  it.each(cases)(
    '$name passes or is covered by a known gap',
    async ({ scenario, mockLlm }) => {
      const result = await runParityWithGaps({
        scenario,
        mockLlm,
        makeA: (homeDir) =>
          makeTsBackend({
            homeDir,
            mockLlm,
            runtime: scenario.name === 'web-search' ? { webSearcher: mockSearcher } : undefined,
          }),
        makeB: (homeDir) =>
          makeRustBackend({
            homeDir,
            binaryPath: binaryPath!,
            transport: rustTransport(homeDir),
            extraArgs: ['--mock-provider'],
          }),
        knownGaps,
        timeoutMs: 60000,
      });
      expect(result.passed).toBe(true);
      if (result.gapReason === undefined) {
        expect(result.diff, `scenario "${scenario.name}" has no known gap, so diff must be null`).toBeNull();
      }
    },
    120000,
  );
});
