import { tmpdir } from 'node:os';

import type { ChatProvider } from '@odysseythink/kosong';

import { assertParity } from './assert-parity';
import { createTempHome, cleanupHome, makeTsBackend, makeRustBackend } from './backends';
import { ParityDriver } from './driver';
import { normalize } from './normalize';
import { resolveRustBinaryPath } from './rust-binary';
import type {
  NormalizedSnapshot,
  ParityBackend,
  ParityDiff,
  Scenario,
  ScenarioSnapshot,
} from './types';

async function runOnce(
  scenario: Scenario,
  makeBackend: (homeDir: string) => Promise<ParityBackend>,
  timeoutMs: number,
): Promise<{ readonly snapshot: ScenarioSnapshot; readonly homeDir: string }> {
  const homeDir = await createTempHome(`parity-l2-${scenario.name}-`);
  const backend = await makeBackend(homeDir);
  try {
    const driver = new ParityDriver({ timeoutMs });
    const snapshot = await driver.runScenario(backend, scenario);
    return { snapshot, homeDir };
  } finally {
    await backend.close();
  }
}

export async function runL2Parity(
  scenario: Scenario,
  mockLlm: ChatProvider,
): Promise<ParityDiff | null> {
  const binaryPath = resolveRustBinaryPath();
  const timeoutMs = 30000;

  const { snapshot: tsSnapshot, homeDir: tsHome } = await runOnce(
    scenario,
    (homeDir) => makeTsBackend({ homeDir, mockLlm }),
    timeoutMs,
  );
  const { snapshot: rustSnapshot, homeDir: rustHome } = await runOnce(
    scenario,
    (homeDir) =>
      makeRustBackend({
        homeDir,
        binaryPath,
        transport: 'stdio',
        extraArgs: ['--mock-provider'],
      }),
    timeoutMs,
  );

  let first: NormalizedSnapshot;
  let second: NormalizedSnapshot;
  try {
    first = normalize(tsSnapshot, {
      homeDir: tsHome,
      tmpDir: tmpdir(),
    });
    second = normalize(rustSnapshot, {
      homeDir: rustHome,
      tmpDir: tmpdir(),
    });
  } finally {
    await cleanupHome(tsHome);
    await cleanupHome(rustHome);
  }

  // L2 parity: only compare responses (not events, records, or fsTree).
  // Replace events with empty array before diffing since L2 only cares about response shapes.
  return assertParity(
    scenario.name,
    { ...first, events: [], records: undefined, fsTree: undefined },
    { ...second, events: [], records: undefined, fsTree: undefined },
  );
}
