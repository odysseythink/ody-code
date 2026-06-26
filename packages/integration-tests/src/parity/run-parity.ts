import { tmpdir } from 'node:os';

import type { ChatProvider } from '@odysseythink/kosong';

import { assertParity } from './assert-parity';
import { createTempHome, cleanupHome, makeTsBackend } from './backends';
import type { AgentEvent } from '@odysseythink/agent-core';
import { ParityDriver } from './driver';
import { normalize } from './normalize';
import { scenarios } from './scenarios';
import type { ParityBackend, ParityDiff, Scenario, ScenarioSnapshot } from './types';

const IGNORE_EVENT_TYPES = new Set<AgentEvent['type']>([
  'mcp.server.status',
  'tool.list.updated',
  'session.meta.updated',
]);

export interface RunParityOptions {
  readonly scenario: Scenario;
  readonly mockLlm: ChatProvider;
  readonly makeA: (homeDir: string) => Promise<ParityBackend>;
  readonly makeB: (homeDir: string) => Promise<ParityBackend>;
  readonly timeoutMs?: number;
}

export interface RunParityResult {
  readonly scenarioName: string;
  readonly equal: boolean;
}

async function runOnce(
  scenario: Scenario,
  makeBackend: (homeDir: string) => Promise<ParityBackend>,
  timeoutMs: number,
): Promise<{ readonly snapshot: ScenarioSnapshot; readonly homeDir: string }> {
  const homeDir = await createTempHome(`parity-${scenario.name}-`);
  const backend = await makeBackend(homeDir);
  try {
    const driver = new ParityDriver({ timeoutMs });
    const snapshot = await driver.runScenario(backend, scenario);
    return { snapshot, homeDir };
  } finally {
    await backend.close();
  }
}

export async function runParity(options: RunParityOptions): Promise<ParityDiff | null> {
  const { scenario, makeA, makeB, timeoutMs = 30000 } = options;
  const { snapshot: firstSnapshot, homeDir: firstHomeDir } = await runOnce(scenario, makeA, timeoutMs);
  const { snapshot: secondSnapshot, homeDir: secondHomeDir } = await runOnce(scenario, makeB, timeoutMs);
  try {
    const first = normalize(firstSnapshot, {
      homeDir: firstHomeDir,
      tmpDir: tmpdir(),
      ignoreEventTypes: IGNORE_EVENT_TYPES,
    });
    const second = normalize(secondSnapshot, {
      homeDir: secondHomeDir,
      tmpDir: tmpdir(),
      ignoreEventTypes: IGNORE_EVENT_TYPES,
    });
    return assertParity(scenario.name, first, second);
  } finally {
    await cleanupHome(firstHomeDir);
    await cleanupHome(secondHomeDir);
  }
}

export async function runTsVsTs(options: { readonly timeoutMs?: number } = {}): Promise<RunParityResult[]> {
  const timeoutMs = options.timeoutMs ?? 30000;
  const results: RunParityResult[] = [];
  for (const { scenario, mockLlm } of scenarios) {
    const diff = await runParity({
      scenario,
      mockLlm,
      makeA: (homeDir) => makeTsBackend({ homeDir, mockLlm }),
      makeB: (homeDir) => makeTsBackend({ homeDir, mockLlm }),
      timeoutMs,
    });
    results.push({ scenarioName: scenario.name, equal: diff === null });
  }
  return results;
}
