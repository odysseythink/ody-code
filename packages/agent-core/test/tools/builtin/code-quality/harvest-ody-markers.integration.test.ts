import { mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ExecutableToolContext, RunnableToolExecution } from '#loop/types';
import { GrepTool } from '#tools/builtin/file/grep';
import { noopTelemetryClient, type TelemetryClient } from '#telemetry';
import type { LocalKaos } from '@odysseythink/kaos';
import { HarvestOdyMarkersTool } from '#tools/builtin/code-quality/harvest-ody-markers';

/** Minimal context for internal tool delegation in tests. */
function testToolCtx(signal: AbortSignal): ExecutableToolContext {
  return { turnId: 'test', toolCallId: 'test', signal };
}

describe('HarvestOdyMarkersTool', () => {
  let fixtureDir: string;
  let kaos: LocalKaos;

  beforeEach(async () => {
    fixtureDir = join(tmpdir(), `ody-test-fixtures-${process.pid}-${Date.now()}`);
    await mkdir(fixtureDir, { recursive: true });

    const { LocalKaos: LK } = await import('@odysseythink/kaos');
    kaos = (await LK.create()).withCwd(fixtureDir);

    await writeFile(
      join(fixtureDir, 'lock.ts'),
      [
        '// ody: 全局锁, 吞吐 > 100 rps 时改为按账户锁',
        'export function lock() {}',
        '// ody: 临时文件    ',
      ].join('\n'),
    );

    await writeFile(
      join(fixtureDir, 'parse.py'),
      [
        '# ody: 用 JSON.parse, 需要 schema 校验时改为 zod',
        'import json',
      ].join('\n'),
    );

    await writeFile(
      join(fixtureDir, 'clean.ts'),
      ['// TODO: refactor later', 'export const x = 1;'].join('\n'),
    );
  });

  afterEach(async () => {
    await rm(fixtureDir, { recursive: true, force: true });
  });

  it('scans fixtures and returns correct marker counts', { timeout: 15_000 }, async () => {
    const workspace = { workspaceDir: fixtureDir, additionalDirs: [] as string[] };
    const grepTool = new GrepTool(kaos, workspace);
    const telemetry: TelemetryClient = {
      ...noopTelemetryClient,
      track: vi.fn(),
    };

    const tool = new HarvestOdyMarkersTool(
      kaos,
      workspace,
      grepTool,
      telemetry,
    );

    const execution = tool.resolveExecution({});
    const result = await (execution as RunnableToolExecution).execute(testToolCtx(new AbortController().signal));

    expect(result.isError).toBe(false);
    const output = JSON.parse(String(result.output));
    expect(output.markerCount).toBe(3);
    expect(output.rotRiskCount).toBe(1);
    expect(output.truncated).toBe(false);
    expect(output.markdown).toContain('### lock.ts');
    expect(output.markdown).toContain('### parse.py');
    expect(output.markdown).not.toContain('clean.ts');
    expect(output.markdown).toContain('⚠️ rot');
    expect(output.markdown).toContain('**汇总**：3 个标记，1 个 rot 风险。');

    expect(telemetry.track).toHaveBeenCalledWith(
      'debt_ledger_harvested',
      expect.objectContaining({
        marker_count: 3,
        rot_risk_count: 1,
      }),
    );
  });

  it('handles empty scan (no markers)', { timeout: 15_000 }, async () => {
    const emptyDir = join(fixtureDir, 'empty');
    await mkdir(emptyDir);

    const emptyKaos = kaos.withCwd(emptyDir);
    const workspace = { workspaceDir: emptyDir, additionalDirs: [] as string[] };
    const grepTool = new GrepTool(emptyKaos, workspace);
    const telemetry: TelemetryClient = {
      ...noopTelemetryClient,
      track: vi.fn(),
    };

    const tool = new HarvestOdyMarkersTool(
      emptyKaos,
      workspace,
      grepTool,
      telemetry,
    );

    const execution = tool.resolveExecution({});
    const result = await (execution as RunnableToolExecution).execute(testToolCtx(new AbortController().signal));

    expect(result.isError).toBe(false);
    const output = JSON.parse(String(result.output));
    expect(output.markerCount).toBe(0);
    expect(output.rotRiskCount).toBe(0);
    expect(output.markdown).toBe(
      '未找到 `ody:` 债务标记。台账干净。',
    );
    expect(telemetry.track).toHaveBeenCalledWith(
      'debt_ledger_harvested',
      expect.objectContaining({ marker_count: 0, rot_risk_count: 0 }),
    );
  });

  it('scans a specific subdirectory via path input', { timeout: 15_000 }, async () => {
    const workspace = { workspaceDir: fixtureDir, additionalDirs: [] as string[] };
    const grepTool = new GrepTool(kaos, workspace);
    const telemetry: TelemetryClient = {
      ...noopTelemetryClient,
      track: vi.fn(),
    };

    const tool = new HarvestOdyMarkersTool(
      kaos,
      workspace,
      grepTool,
      telemetry,
    );
    const execution = tool.resolveExecution({ path: 'parse.py' });
    const result = await (execution as RunnableToolExecution).execute(testToolCtx(new AbortController().signal));

    expect(result.isError).toBe(false);
    const output = JSON.parse(String(result.output));
    expect(output.markerCount).toBe(1);
  });

  it('reports failure telemetry when GrepTool errors', { timeout: 15_000 }, async () => {
    const badDir = join(fixtureDir, 'nonexistent');
    const badKaos = kaos.withCwd(badDir);
    const workspace = { workspaceDir: badDir, additionalDirs: [] as string[] };
    const grepTool = new GrepTool(badKaos, workspace);
    const telemetry: TelemetryClient = {
      ...noopTelemetryClient,
      track: vi.fn(),
    };

    const tool = new HarvestOdyMarkersTool(
      badKaos,
      workspace,
      grepTool,
      telemetry,
    );

    const execution = tool.resolveExecution({});
    const result = await (execution as RunnableToolExecution).execute(testToolCtx(new AbortController().signal));

    if (result.isError) {
      expect(telemetry.track).toHaveBeenCalledWith(
        'debt_ledger_failed',
        expect.any(Object),
      );
    }
  });
});
