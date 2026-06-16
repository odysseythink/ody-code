import { describe, expect, it, vi } from 'vitest';
import type { Agent } from '../../src/agent';
import type { Kaos, KaosProcess } from '@odysseythink/kaos';
import type { Readable, Writable } from 'node:stream';
import { createFakeKaos } from '../tools/fixtures/fake-kaos';
import { E2EPlanEnricher } from '#/e2e-testing/plan-enricher';
import { ImpactAnalyzer } from '#/e2e-testing/impact-analyzer';
import { E2EConfigResolver } from '#/e2e-testing/config';
import type { KimiConfig } from '#/config/schema';

function fakeKaosWithGit(files: string[]): Kaos {
  return createFakeKaos({
    exec: vi.fn().mockResolvedValue({
      stdin: { end: vi.fn(), write: vi.fn() } as unknown as Writable,
      stdout: {
        on: (_ev: string, cb: (chunk: Buffer) => void) => {
          const output = files.map(f => ` M ${f}\n`).join('');
          cb(Buffer.from(output));
        },
      } as unknown as Readable,
      stderr: {
        on: (_ev: string, _cb: (chunk: Buffer) => void) => {},
      } as unknown as Readable,
      pid: 1,
      exitCode: 0,
      wait: vi.fn().mockResolvedValue(0),
      kill: vi.fn().mockResolvedValue(undefined),
    } as KaosProcess),
    writeText: vi.fn().mockResolvedValue(42),
    mkdir: vi.fn().mockResolvedValue(undefined),
  });
}

const baseConfig = E2EConfigResolver.resolve({} as KimiConfig);
const planContent = '# Plan\n\n### Task 1: Do stuff\n\n### Task 2: More stuff\n';

describe('E2EPlanEnricher', () => {
  it('returns null when e2e is disabled', async () => {
    const config = { ...baseConfig, enabled: false };
    const enricher = new E2EPlanEnricher(createFakeKaos({}), config, ImpactAnalyzer);
    const result = await enricher.enrich('/plan.md', planContent, '/app');
    expect(result).toBeNull();
  });

  it('appends E2E task when git status returns matching file', async () => {
    const kaos = fakeKaosWithGit([
      'packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts',
    ]);
    const enricher = new E2EPlanEnricher(kaos, baseConfig, ImpactAnalyzer);
    const result = await enricher.enrich('/plan.md', planContent, '/app');
    expect(result).not.toBeNull();
    expect(result!).toContain('### Task 3: Generate and run E2E tests');
    expect(result!).toContain('ExitPlanModeTool');
  });

  it('returns null for smart strategy with no matched files', async () => {
    const kaos = fakeKaosWithGit(['unrelated.ts']);
    const enricher = new E2EPlanEnricher(kaos, baseConfig, ImpactAnalyzer);
    const result = await enricher.enrich('/plan.md', planContent, '/app');
    expect(result).toBeNull();
  });

  it('returns null for critical-only with no critical tools affected', async () => {
    const config = { ...baseConfig, strategy: 'critical-only' as const };
    const kaos = fakeKaosWithGit(['packages/agent-core/src/tools/builtin/planning/enter-plan-mode.ts']);
    const enricher = new E2EPlanEnricher(kaos, config, ImpactAnalyzer);
    const result = await enricher.enrich('/plan.md', planContent, '/app');
    expect(result).toBeNull();
  });

  it('is idempotent and does not append E2E task twice', async () => {
    const kaos = fakeKaosWithGit([
      'packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts',
    ]);
    const enricher = new E2EPlanEnricher(kaos, baseConfig, ImpactAnalyzer);
    const first = await enricher.enrich('/plan.md', planContent, '/app');
    expect(first).not.toBeNull();
    const second = await enricher.enrich('/plan.md', first!, '/app');
    expect(second).toBeNull();
  });
});

import { RunE2ETestsTool, derivePackageRoot } from '#/tools/builtin/e2e/run-e2e-tests';

describe('derivePackageRoot', () => {
  it('returns packages/<name> for packages/agent-core path', () => {
    expect(derivePackageRoot(['packages/agent-core/src/index.ts'])).toBe('packages/agent-core');
  });

  it('returns apps/<name> for apps/ody-code path', () => {
    expect(derivePackageRoot(['apps/ody-code/src/main.ts'])).toBe('apps/ody-code');
  });

  it('returns first matching package root', () => {
    expect(derivePackageRoot(['unrelated.txt', 'packages/kaos/src/index.ts'])).toBe('packages/kaos');
  });

  it('returns undefined when no monorepo path is present', () => {
    expect(derivePackageRoot(['src/index.ts'])).toBeUndefined();
  });

  it('handles backslash paths', () => {
    expect(derivePackageRoot(['packages\\agent-core\\src\\index.ts'])).toBe('packages/agent-core');
  });
});

describe('RunE2ETestsTool', () => {
  it('has name RunE2ETests', () => {
    const kaos = createFakeKaos({});
    const tool = new RunE2ETestsTool(kaos, { kimiConfig: {} } as any);
    expect(tool.name).toBe('RunE2ETests');
  });

  it('resolveExecution returns approval rule', () => {
    const kaos = createFakeKaos({});
    const tool = new RunE2ETestsTool(kaos, { kimiConfig: {} } as any);
    const exec = tool.resolveExecution({ toolId: 'ExitPlanModeTool' });
    expect(exec).toHaveProperty('approvalRule');
    expect('approvalRule' in exec && typeof exec.approvalRule === 'string').toBe(true);
  });

  it('returns info when e2e is disabled', async () => {
    const kaos = createFakeKaos({});
    const agent = {
      kimiConfig: { e2e: { enabled: false } },
      config: { cwd: '/tmp' },
      kaos: createFakeKaos({}),
    };
    const tool = new RunE2ETestsTool(kaos, agent as any);
    const exec = tool.resolveExecution({});
    if ('execute' in exec) {
      const result = await exec.execute({ signal: new AbortController().signal, turnId: '1', toolCallId: '1' });
      const output = typeof result.output === 'string' ? result.output : JSON.stringify(result.output);
      expect(output).toContain('disabled');
    }
  });
});

import { NormalModeTaskCheckpoint } from '../../src/agent/compaction/normal-task-checkpoint';

describe('NormalModeTaskCheckpoint E2E hook', () => {
  it('injects reminder when e2e-related todo is completed', async () => {
    const appended: string[] = [];
    const agent = {
      kimiConfig: { e2e: { enabled: true } as any },
      sessionMode: { isActive: false },
      config: { modelCapabilities: { max_context_tokens: 100000 } },
      context: {
        tokenCountWithPending: 1000,
        appendSystemReminder: ((content: string) => { appended.push(content); }) as any,
      },
      tools: {
        storeData: () => ({
          todo: [
            { title: 'Generate and run E2E tests', status: 'done' } as any,
            { title: 'Other task', status: 'done' } as any,
          ],
        }),
      },
      fullCompaction: { compactCheckpoint: vi.fn() },
    } as unknown as Agent;

    const checkpoint = new NormalModeTaskCheckpoint(agent);
    (checkpoint as any).lastDoneCount = 0;
    await checkpoint.beforeStep(new AbortController().signal);
    expect(appended.length).toBeGreaterThanOrEqual(1);
    expect(appended[0]).toContain('RunE2ETests');
  });
});

describe('final integration assertions', () => {
  it('ExitPlanModeTool enrichment is skipped when e2e is disabled', async () => {
    const kaos = fakeKaosWithGit(['exit-plan-mode.ts']);
    const config = { ...baseConfig, enabled: false };
    const enricher = new E2EPlanEnricher(kaos, config, ImpactAnalyzer);
    const result = await enricher.enrich('/plan.md', planContent, '/app');
    expect(result).toBeNull();
  });
});
