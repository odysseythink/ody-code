import { describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../src/agent';
import { SessionMode } from '../../src/agent/session-mode';

const CWD = '/tmp/kimi-writing-plan-test';

/**
 * Build a SessionMode over a fake Agent whose `kaos.stat` reports `existingPaths`
 * as present and everything else as missing. `stat` backs both the source-file
 * existence check and the dedup probe in findUniqueStemInDir (reject == unique).
 */
function makeSessionMode(
  existingPaths: readonly string[],
  directoryPaths: readonly string[] = [],
): {
  sessionMode: SessionMode;
  emitStatusUpdated: ReturnType<typeof vi.fn>;
} {
  const existing = new Set([...existingPaths, ...directoryPaths]);
  const dirs = new Set(directoryPaths);
  const emitStatusUpdated = vi.fn();
  const agent = {
    homedir: CWD,
    config: { cwd: CWD },
    emitStatusUpdated,
    kaos: {
      mkdir: vi.fn().mockResolvedValue(undefined),
      stat: vi.fn(async (p: string) => {
        if (!existing.has(p)) throw new Error('ENOENT');
        // 0o040000 = S_IFDIR, 0o100000 = S_IFREG
        return { stMode: dirs.has(p) ? 0o040755 : 0o100644 };
      }),
    },
  } as unknown as Agent;
  const sessionMode = new SessionMode(agent);
  Object.assign(agent, { sessionMode });
  return { sessionMode, emitStatusUpdated };
}

describe('SessionMode.setWritingPlanSource', () => {
  it('names the plan file after the source file (basename, verbatim)', async () => {
    const source = `${CWD}/.ody-code/designs/2026-06-09-sub-project-2-phaseD.md`;
    const { sessionMode, emitStatusUpdated } = makeSessionMode([source]);

    await sessionMode.setWritingPlanSource(source);

    expect(sessionMode.sessionModeFilePath).toBe(
      `${CWD}/.ody-code/plans/2026-06-09-sub-project-2-phaseD.md`,
    );
    expect(emitStatusUpdated).toHaveBeenCalled();
  });

  it('throws when the source file does not exist', async () => {
    const { sessionMode } = makeSessionMode([]); // nothing exists
    await expect(
      sessionMode.setWritingPlanSource(`${CWD}/.ody-code/designs/missing.md`),
    ).rejects.toThrow('文件不存在');
    expect(sessionMode.sessionModeFilePath).toBeNull();
  });

  it('clears the design→plan filename handoff so it cannot override', async () => {
    const source = `${CWD}/.ody-code/designs/spec.md`;
    const { sessionMode } = makeSessionMode([source]);
    // Simulate a prior design exit having cached a stem.
    (sessionMode as unknown as { _lastCompletedDesignFilePath: string | null })._lastCompletedDesignFilePath =
      `${CWD}/.ody-code/designs/2026-06-09-continue.md`;

    await sessionMode.setWritingPlanSource(source);

    expect(sessionMode.sessionModeFilePath).toBe(`${CWD}/.ody-code/plans/spec.md`);
    expect(
      (sessionMode as unknown as { _lastCompletedDesignFilePath: string | null })
        ._lastCompletedDesignFilePath,
    ).toBeNull();
  });

  it('throws when the source path is a directory', async () => {
    const dir = `${CWD}/.ody-code/designs`;
    const { sessionMode } = makeSessionMode([], [dir]);
    await expect(sessionMode.setWritingPlanSource(dir)).rejects.toThrow('不是文件');
    expect(sessionMode.sessionModeFilePath).toBeNull();
  });

  it('deduplicates against an existing same-named plan', async () => {
    const source = `${CWD}/.ody-code/designs/spec.md`;
    const existingPlan = `${CWD}/.ody-code/plans/spec.md`;
    const { sessionMode } = makeSessionMode([source, existingPlan]);

    await sessionMode.setWritingPlanSource(source);

    expect(sessionMode.sessionModeFilePath).toBe(`${CWD}/.ody-code/plans/spec-1.md`);
  });
});
