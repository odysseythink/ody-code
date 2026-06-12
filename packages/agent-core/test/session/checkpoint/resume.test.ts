import { mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../../src/agent';
import type { Session } from '../../../src/session';
import type { Logger } from '../../../src/logging/types';
import { CheckpointIndex } from '../../../src/session/checkpoint/checkpoint-index';
import { SessionCheckpoint } from '../../../src/session/checkpoint/checkpoint';
import { verifyAndRestoreResumedSession } from '../../../src/session/checkpoint/resume';

function makeMainAgent(overrides: Partial<Agent> = {}) {
  const designSessions: { designSessionID: string; startedAtMsg: number; exitedAtMsg?: number }[] = [];
  return {
    context: {
      history: [{ role: 'user', content: [{ type: 'text', text: 'hi' }], toolCalls: [] }],
    },
    sessionMode: {
      isActive: false,
      kind: 'normal',
      designSessions,
      restoreDesignSessions: (sessions: typeof designSessions) => {
        designSessions.length = 0;
        designSessions.push(...sessions);
      },
    },
    ...overrides,
  } as unknown as Agent;
}

function makeLogger(): Logger {
  return {
    error: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    debug: vi.fn(),
    createChild: vi.fn(() => makeLogger()),
  } as unknown as Logger;
}

function makeSession(agents: Map<string, Agent>, workDir: string) {
  return {
    options: { id: 's1', homedir: workDir },
    agents,
  } as unknown as Session;
}

describe('verifyAndRestoreResumedSession', () => {
  let workDir: string;

  beforeEach(async () => {
    workDir = await mkdtemp(join(tmpdir(), 'resume-test-'));
  });

  afterEach(async () => {
    const { rm } = await import('node:fs/promises');
    await rm(workDir, { recursive: true, force: true });
  });

  function makePaths() {
    return {
      checkpointPath: join(workDir, 'session.json'),
      indexPath: join(workDir, 'checkpoints.json'),
    };
  }

  async function writeCheckpoint(
    payload: import('../../../src/session/checkpoint/checkpoint').SessionCheckpointPayload,
  ) {
    const { checkpointPath } = makePaths();
    await new SessionCheckpoint({ checkpointPath }).save(payload);
  }

  async function writeIndex(valid: boolean) {
    const { checkpointPath, indexPath } = makePaths();
    const index = new CheckpointIndex({ indexPath });
    await index.update({
      path: checkpointPath,
      timestamp: new Date().toISOString(),
      messageCount: 1,
      valid,
      lastValidParent: null,
    });
  }

  function samplePayload(): import('../../../src/session/checkpoint/checkpoint').SessionCheckpointPayload {
    return {
      sessionID: 's1',
      createdAt: '2026-06-12T10:00:00.000Z',
      lastUpdatedAt: '2026-06-12T10:00:00.000Z',
      currentMode: 'normal',
      messages: [{ role: 'user', content: [{ type: 'text', text: 'hi' }], toolCalls: [] }],
      designModeContext: { sessions: [] },
      toolCallIndex: { callIdToResult: {} },
    };
  }

  it('warns when there is no main agent', async () => {
    const session = makeSession(new Map<string, Agent>(), workDir);
    const result = await verifyAndRestoreResumedSession(session, makePaths());
    expect(result.warning).toContain('No main agent');
    expect(result.verified).toBeUndefined();
  });

  it('returns verified=false when the checkpoint index is missing', async () => {
    const main = makeMainAgent();
    const session = makeSession(new Map<string, Agent>([['main', main]]), workDir);
    const result = await verifyAndRestoreResumedSession(session, makePaths());
    expect(result.warning).toBeUndefined();
    expect(result.verified).toBe(false);
  });

  it('returns verified=false when the index has no versions', async () => {
    const main = makeMainAgent();
    const session = makeSession(new Map<string, Agent>([['main', main]]), workDir);
    const { indexPath } = makePaths();
    await writeFile(indexPath, JSON.stringify({ versions: [] }));
    const result = await verifyAndRestoreResumedSession(session, makePaths());
    expect(result.warning).toBeUndefined();
    expect(result.verified).toBe(false);
  });

  it('verifies a valid checkpoint and restores missing design sessions', async () => {
    const main = makeMainAgent();
    const session = makeSession(new Map<string, Agent>([['main', main]]), workDir);

    await writeCheckpoint({
      ...samplePayload(),
      designModeContext: {
        sessions: [{ designSessionID: 'd1', startedAtMsg: 0, exitedAtMsg: 0 }],
      },
    });
    await writeIndex(true);

    const result = await verifyAndRestoreResumedSession(session, makePaths());
    expect(result.verified).toBe(true);
    expect(result.warning).toBeUndefined();
    expect(main.sessionMode.designSessions).toHaveLength(1);
  });

  it('does not overwrite live design sessions', async () => {
    const main = makeMainAgent();
    main.sessionMode.restoreDesignSessions([{ designSessionID: 'live', startedAtMsg: 0 }]);
    const session = makeSession(new Map<string, Agent>([['main', main]]), workDir);

    await writeCheckpoint({
      ...samplePayload(),
      designModeContext: {
        sessions: [{ designSessionID: 'd1', startedAtMsg: 0, exitedAtMsg: 0 }],
      },
    });
    await writeIndex(true);

    await verifyAndRestoreResumedSession(session, makePaths());
    expect(main.sessionMode.designSessions[0]!.designSessionID).toBe('live');
  });

  it('reports a warning when integrity checks fail', async () => {
    const main = makeMainAgent();
    const session = makeSession(new Map<string, Agent>([['main', main]]), workDir);

    await writeCheckpoint({
      ...samplePayload(),
      messages: [
        { role: 'user', content: [{ type: 'text', text: 'hi' }], toolCalls: [] },
        { role: 'assistant', content: [{ type: 'text', text: 'extra' }], toolCalls: [] },
      ],
    });
    await writeIndex(true);

    const result = await verifyAndRestoreResumedSession(session, makePaths());
    expect(result.verified).toBe(false);
    expect(result.warning).toContain('integrity failed');
  });

  it('reports a warning when the resumed mode differs from the checkpoint', async () => {
    const main = makeMainAgent({
      sessionMode: {
        isActive: true,
        kind: 'design',
        designSessions: [],
        restoreDesignSessions: () => {},
      } as any,
    });
    const session = makeSession(new Map<string, Agent>([['main', main]]), workDir);

    await writeCheckpoint(samplePayload());
    await writeIndex(true);

    const result = await verifyAndRestoreResumedSession(session, makePaths());
    expect(result.verified).toBe(true);
    expect(result.warning).toContain('Mode mismatch');
  });

  it('falls back to an invalid version when no valid version exists', async () => {
    const main = makeMainAgent();
    const session = makeSession(new Map<string, Agent>([['main', main]]), workDir);

    await writeCheckpoint(samplePayload());
    await writeIndex(false);

    const logger = makeLogger();
    const result = await verifyAndRestoreResumedSession(session, { ...makePaths(), logger });
    expect(result.verified).toBe(true);
    expect(logger.warn).not.toHaveBeenCalledWith(
      expect.stringContaining('No checkpoint versions'),
      expect.anything(),
    );
  });
});
