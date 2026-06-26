import { tmpdir } from 'node:os';

import type { ChatProvider } from '@odysseythink/kosong';

import { assertParity } from './assert-parity';
import { writeParityArtifacts, writeParityErrorArtifacts } from './artifacts';
import { createTempHome, cleanupHome, makeTsBackend } from './backends';
import type { AgentEvent } from '@odysseythink/agent-core';
import { ParityDriver } from './driver';
import { affectedLayers, checkGapState, findGapForLayers, type KnownGap } from './known-gaps';
import { normalize } from './normalize';
import { scenarios } from './scenarios';
import type { NormalizedSnapshot, ParityBackend, ParityDiff, Scenario, ScenarioSnapshot } from './types';

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
  let first: NormalizedSnapshot;
  let second: NormalizedSnapshot;
  try {
    first = normalize(firstSnapshot, {
      homeDir: firstHomeDir,
      tmpDir: tmpdir(),
      ignoreEventTypes: IGNORE_EVENT_TYPES,
    });
    second = normalize(secondSnapshot, {
      homeDir: secondHomeDir,
      tmpDir: tmpdir(),
      ignoreEventTypes: IGNORE_EVENT_TYPES,
    });
  } finally {
    await cleanupHome(firstHomeDir);
    await cleanupHome(secondHomeDir);
  }

  const diff = assertParity(scenario.name, first, second);
  if (diff !== null) {
    await writeParityArtifacts(
      scenario.name,
      { snapshot: firstSnapshot, normalized: first },
      { snapshot: secondSnapshot, normalized: second },
      diff,
    );
  }
  return diff;
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

export interface RunParityWithGapsResult {
  readonly diff: ParityDiff | null;
  readonly gapReason: string | undefined;
  readonly passed: boolean;
}

export async function runParityWithGaps(
  options: RunParityOptions & { readonly knownGaps: readonly KnownGap[] },
): Promise<RunParityWithGapsResult> {
  const { knownGaps, scenario } = options;

  let diff: ParityDiff | null;
  try {
    diff = await runParity(options);
  } catch (error) {
    const layers = affectedLayers(['$.error.runParity']);
    const match = findGapForLayers(knownGaps, scenario.name, layers);
    await writeParityErrorArtifacts(scenario.name, 'runParity', error);
    if (match !== undefined) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      const errorStack = error instanceof Error ? error.stack : undefined;
      return {
        diff: {
          scenarioName: scenario.name,
          ts: { responses: [], events: [] },
          rust: { responses: [], events: [] },
          diffs: [
            {
              path: '$.error.runParity',
              tsValue: null,
              rustValue: { message: errorMessage, stack: errorStack },
            },
          ],
        },
        gapReason: match.reason,
        passed: true,
      };
    }
    throw error;
  }

  const layers = diff === null ? ['L2', 'L3', 'L4'] as const : affectedLayers(diff.diffs.map((d) => d.path));
  const match = findGapForLayers(knownGaps, scenario.name, layers);

  if (diff === null) {
    for (const layer of layers) {
      checkGapState(knownGaps, scenario.name, layer, true);
    }
  }

  return {
    diff,
    gapReason: match?.reason,
    passed: diff === null || match !== undefined,
  };
}
