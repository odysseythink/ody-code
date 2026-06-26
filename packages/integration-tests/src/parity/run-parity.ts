import { tmpdir } from 'node:os';

import { createTempHome, cleanupHome, makeTsBackend } from './backends';
import type { AgentEvent } from '@odysseythink/agent-core';
import { ParityDriver } from './driver';
import { normalize } from './normalize';
import { scenarios } from './scenarios';
import type { NormalizedSnapshot, ScenarioSnapshot } from './types';

const IGNORE_EVENT_TYPES = new Set<AgentEvent['type']>([
  'mcp.server.status',
  'tool.list.updated',
  'session.meta.updated',
]);

export interface RunParityOptions {
  readonly timeoutMs?: number;
}

export interface RunParityResult {
  readonly scenarioName: string;
  readonly first: NormalizedSnapshot;
  readonly second: NormalizedSnapshot;
  readonly equal: boolean;
}

async function runOnce(
  scenarioName: string,
  timeoutMs: number,
): Promise<{ readonly snapshot: ScenarioSnapshot; readonly homeDir: string }> {
  const entry = scenarios.find((s) => s.scenario.name === scenarioName);
  if (entry === undefined) {
    throw new Error(`Unknown scenario: ${scenarioName}`);
  }
  const homeDir = await createTempHome(`parity-${scenarioName}-`);
  const backend = await makeTsBackend({ homeDir, mockLlm: entry.mockLlm });
  try {
    const driver = new ParityDriver({ timeoutMs });
    const snapshot = await driver.runScenario(backend, entry.scenario);
    return { snapshot, homeDir };
  } finally {
    await backend.close();
  }
}

export async function runTsVsTs(options: RunParityOptions = {}): Promise<RunParityResult[]> {
  const timeoutMs = options.timeoutMs ?? 30000;
  const results: RunParityResult[] = [];
  for (const { scenario } of scenarios) {
    const { snapshot: firstSnapshot, homeDir: firstHomeDir } = await runOnce(scenario.name, timeoutMs);
    const { snapshot: secondSnapshot, homeDir: secondHomeDir } = await runOnce(
      scenario.name,
      timeoutMs,
    );
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
      const equal = JSON.stringify(first) === JSON.stringify(second);
      results.push({ scenarioName: scenario.name, first, second, equal });
    } finally {
      await cleanupHome(firstHomeDir);
      await cleanupHome(secondHomeDir);
    }
  }
  return results;
}


