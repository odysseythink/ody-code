import { describe, expect, it, vi } from 'vitest';
import type { Kaos, KaosProcess } from '@odysseythink/kaos';
import type { Readable, Writable } from 'node:stream';
import type { Agent } from '../../src/agent';
import { createFakeKaos } from '../tools/fixtures/fake-kaos';
import { ExitPlanModeTool } from '#/tools/builtin/planning/exit-plan-mode';

function makeKaosWithGit(files: string[]): Kaos {
  const base = {
    exec: vi.fn().mockResolvedValue({
      stdin: { end: vi.fn(), write: vi.fn() } as unknown as Writable,
      stdout: {
        on: (_ev: string, cb: (chunk: Buffer) => void) => {
          cb(Buffer.from(files.map(f => ` M ${f}\n`).join('')));
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
  };
  return createFakeKaos({
    ...base,
    withCwd: () => createFakeKaos(base),
    getcwd: () => '/workspace',
  });
}

describe('Plan enrichment end-to-end', () => {
  it('enriches plan with E2E task on exit-plan-mode via ExitPlanModeTool', async () => {
    const kaos = makeKaosWithGit([
      'packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts',
    ]);

    const planPath = '/workspace/.ody-code/plans/test-plan.md';
    const planContent = '# Plan\n\n### Task 1: Initial\n\n### Task 2: Final\n';

    // Build a minimal agent mock with sessionMode that reports plan mode
    let modeActive = true;
    const agent = {
      kaos,
      kimiConfig: { e2e: { enabled: true } as any },
      sessionMode: {
        get isActive() { return modeActive; },
        sessionModeFilePath: planPath,
        data: vi.fn(async () => ({
          id: 'test',
          content: planContent,
          path: planPath,
          kind: 'plan' as const,
        })),
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

    const tool = new ExitPlanModeTool(agent, kaos);
    const exec = await tool.resolveExecution({});
    if (!('execute' in exec)) {
      throw new Error('expected executable tool result');
    }
    await exec.execute({ signal: new AbortController().signal, turnId: '1', toolCallId: '1' });

    // Verify enrichment happened: writeText should have been called with enriched content
    const writeCalls = (kaos.writeText as any).mock.calls;
    const enrichmentCall = writeCalls.find((call: string[]) =>
      call[0] === planPath && typeof call[1] === 'string' && call[1].includes('Generate and run E2E tests'),
    );
    expect(enrichmentCall).toBeDefined();
    expect(enrichmentCall[1]).toContain('ExitPlanModeTool');
  });
});
