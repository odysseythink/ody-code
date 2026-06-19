import { afterAll, describe, expect, it, vi } from 'vitest';
import type { Kaos, KaosProcess } from '@odysseythink/kaos';
import type { Readable, Writable } from 'node:stream';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import type { Agent } from '../../src/agent';
import { createFakeKaos } from '../tools/fixtures/fake-kaos';
import { ExitPlanModeTool } from '#/tools/builtin/planning/exit-plan-mode';

const tempRoots: string[] = [];

afterAll(() => {
  for (const root of tempRoots) rmSync(root, { recursive: true, force: true });
});

/** Create a real project dir on disk so generator detection (node:fs) resolves. */
function makeProjectDir(files: Record<string, string>): string {
  const root = mkdtempSync(join(tmpdir(), 'plan-enrich-'));
  tempRoots.push(root);
  for (const [rel, content] of Object.entries(files)) {
    writeFileSync(join(root, rel), content);
  }
  return root;
}

function makeKaosWithGit(changedFiles: string[], cwd: string): Kaos {
  const base = {
    exec: vi.fn().mockResolvedValue({
      stdin: { end: vi.fn(), write: vi.fn() } as unknown as Writable,
      stdout: {
        on: (_ev: string, cb: (chunk: Buffer) => void) => {
          cb(Buffer.from(changedFiles.map(f => ` M ${f}\n`).join('')));
        },
      } as unknown as Readable,
      stderr: { on: vi.fn() } as unknown as Readable,
      pid: 1, exitCode: 0,
      wait: vi.fn().mockResolvedValue(0),
      kill: vi.fn().mockResolvedValue(undefined),
    } as KaosProcess),
    writeText: vi.fn(async (_p: string, c: string) => c.length),
    mkdir: vi.fn().mockResolvedValue(undefined),
    readText: vi.fn(async () => ''),
    getcwd: () => cwd,
  };
  return createFakeKaos({
    ...base,
    withCwd: () => createFakeKaos(base),
  });
}

function makeAgent(kaos: Kaos, planPath: string, planContent: string): Agent {
  let modeActive = true;
  return {
    kaos,
    // Recursive analysis off → impact is computed from the changed files
    // directly, keeping the test deterministic and off the real filesystem.
    kimiConfig: { e2e: { enabled: true, recursiveAnalysisEnabled: false } as any },
    sessionMode: {
      get isActive() { return modeActive; },
      sessionModeFilePath: planPath,
      data: vi.fn(async () => ({ id: 'test', content: planContent, path: planPath, kind: 'plan' as const })),
      createSessionModeId: () => 'test',
      enter: vi.fn(),
      handoffTo: vi.fn(async () => { modeActive = false; }),
      finalizeFileName: vi.fn().mockResolvedValue(null),
    },
    config: { cwd: '/workspace' },
    context: { appendSystemReminder: vi.fn() },
    tools: { storeData: vi.fn() },
    records: { logRecord: vi.fn() },
    telemetry: { track: vi.fn() },
    emit: vi.fn(),
    rpc: {},
    emitStatusUpdated: vi.fn(),
    fullCompaction: { compactCheckpoint: vi.fn() },
  } as unknown as Agent;
}

async function runExitPlanMode(agent: Agent, kaos: Kaos): Promise<void> {
  const tool = new ExitPlanModeTool(agent, kaos);
  const exec = await tool.resolveExecution({});
  if (!('execute' in exec)) throw new Error('expected executable tool result');
  await exec.execute({ signal: new AbortController().signal, turnId: '1', toolCallId: '1' });
}

function enrichmentWrite(kaos: Kaos, planPath: string): string | undefined {
  const calls = (kaos.writeText as any).mock.calls as Array<[string, string]>;
  const hit = calls.find(c => c[0] === planPath && typeof c[1] === 'string' && c[1].includes('Generate and run E2E tests'));
  return hit?.[1];
}

describe('Plan enrichment end-to-end', () => {
  it('enriches a TypeScript/Vitest project (ody-code self-test) via ExitPlanModeTool', async () => {
    const root = makeProjectDir({ 'package.json': JSON.stringify({ devDependencies: { vitest: '^4.0.0' } }) });
    const kaos = makeKaosWithGit(
      ['packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts'],
      root,
    );
    const planPath = join(root, '.ody-code/plans/test-plan.md');
    const agent = makeAgent(kaos, planPath, '# Plan\n\n### Task 1: Initial\n\n### Task 2: Final\n');

    await runExitPlanMode(agent, kaos);

    const enriched = enrichmentWrite(kaos, planPath);
    expect(enriched).toBeDefined();
    expect(enriched!).toContain('ExitPlanModeTool');
  });

  it('enriches a USER TypeScript/Vitest project by changed package (not ody tool map)', async () => {
    const root = makeProjectDir({ 'package.json': JSON.stringify({ devDependencies: { vitest: '^4.0.0' } }) });
    const kaos = makeKaosWithGit(['src/api/handler.ts'], root);
    const planPath = join(root, '.ody-code/plans/test-plan.md');
    const agent = makeAgent(kaos, planPath, '# Plan\n\n### Task 1: Add API handler\n');

    await runExitPlanMode(agent, kaos);

    const enriched = enrichmentWrite(kaos, planPath);
    expect(enriched).toBeDefined();
    expect(enriched!).toContain('src/api');
    // It must NOT fall back to ody-code's own builtin tool names.
    expect(enriched!).not.toContain('ExitPlanModeTool');
  });

  it('enriches a Go project by changed package (the Phase 2 fix)', async () => {
    const root = makeProjectDir({ 'go.mod': 'module demo\n\ngo 1.22\n' });
    const kaos = makeKaosWithGit(['internal/search/foo.go'], root);
    const planPath = join(root, '.ody-code/plans/test-plan.md');
    const agent = makeAgent(kaos, planPath, '# Plan\n\n### Task 1: Implement search\n');

    await runExitPlanMode(agent, kaos);

    const enriched = enrichmentWrite(kaos, planPath);
    expect(enriched).toBeDefined();
    expect(enriched!).toContain('internal/search');
  });

  it('does not enrich a project with no matching generator', async () => {
    const root = makeProjectDir({ 'README.md': '# just docs\n' });
    const kaos = makeKaosWithGit(['README.md'], root);
    const planPath = join(root, '.ody-code/plans/test-plan.md');
    const agent = makeAgent(kaos, planPath, '# Plan\n\n### Task 1: Docs\n');

    await runExitPlanMode(agent, kaos);

    expect(enrichmentWrite(kaos, planPath)).toBeUndefined();
  });
});
