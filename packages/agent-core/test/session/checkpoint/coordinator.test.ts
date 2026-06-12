import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../../src/agent';
import { AgentRecords, InMemoryAgentRecordPersistence } from '../../../src/agent/records';
import type { Session } from '../../../src/session';
import { CheckpointIndex } from '../../../src/session/checkpoint/checkpoint-index';
import { SessionCheckpoint } from '../../../src/session/checkpoint/checkpoint';
import { CheckpointCoordinator } from '../../../src/session/checkpoint/coordinator';
import { SessionMarkdownExport } from '../../../src/session/export/markdown-export';

describe('CheckpointCoordinator', () => {
  let workDir: string;

  beforeEach(async () => {
    workDir = await mkdtemp(join(tmpdir(), 'coordinator-test-'));
  });

  afterEach(async () => {
    await rm(workDir, { recursive: true, force: true });
  });

  function makeFixture() {
    const checkpointPath = join(workDir, 'session.json');
    const indexPath = join(workDir, 'checkpoints.json');
    const markdownPath = join(workDir, 'session.md');

    const sessionMode = {
      isActive: false,
      kind: 'normal' as const,
      designSessions: [],
    };

    const context = {
      history: [{ role: 'user', content: [{ type: 'text', text: 'hi' }] }],
    };

    const persistence = new InMemoryAgentRecordPersistence();
    const records = new AgentRecords(
      {
        appVersion: 'test',
        log: { error: vi.fn(), info: vi.fn(), warn: vi.fn(), debug: vi.fn() },
      } as unknown as Agent,
      persistence,
    );

    const agent = {
      records,
      sessionMode,
      context,
    } as unknown as Agent;

    const agents = new Map<string, Agent>();
    agents.set('main', agent);

    const session = {
      options: { id: 's1' },
      metadata: { createdAt: '2026-06-12T10:00:00.000Z' },
      agents,
    } as unknown as Session;

    const checkpoint = new SessionCheckpoint({ checkpointPath });
    const index = new CheckpointIndex({ indexPath });
    const markdownExport = new SessionMarkdownExport({ filePath: markdownPath });

    const coordinator = new CheckpointCoordinator({
      session,
      checkpoint,
      index,
      markdownExport,
    });

    return { coordinator, agent, session, checkpoint, index, markdownPath };
  }

  it('appends messages to the markdown export', async () => {
    const { coordinator, agent, markdownPath } = makeFixture();
    coordinator.attachAgent(agent);

    agent.records.logRecord({
      type: 'context.append_message',
      message: { role: 'assistant', content: [{ type: 'text', text: 'hello' }] },
    });

    // Give the async append a chance to run.
    await new Promise((resolve) => setTimeout(resolve, 50));

    const content = await readFile(markdownPath, 'utf8');
    expect(content).toContain('role: assistant');
    expect(content).toContain('hello');
  });

  it('saves a checkpoint on session_mode.exit', async () => {
    const { coordinator, agent, checkpoint, index } = makeFixture();
    coordinator.attachAgent(agent);

    agent.records.logRecord({ type: 'session_mode.exit', id: 'd1' });
    await new Promise((resolve) => setTimeout(resolve, 50));

    const payload = await checkpoint.load();
    expect(payload).not.toBeNull();
    expect(payload!.currentMode).toBe('normal');
    expect(payload!.messages).toHaveLength(1);

    const data = await index.load();
    expect(data.latest).toBe(checkpoint.path);
    expect(data.versions).toHaveLength(1);
  });

  it('saves a checkpoint on step.end wrapped in context.append_loop_event', async () => {
    const { coordinator, agent, checkpoint } = makeFixture();
    coordinator.attachAgent(agent);

    agent.records.logRecord({
      type: 'context.append_loop_event',
      event: {
        type: 'step.end',
        uuid: 'step-1',
        turnId: 'turn-1',
        step: 1,
      },
    });
    await new Promise((resolve) => setTimeout(resolve, 50));

    const payload = await checkpoint.load();
    expect(payload).not.toBeNull();
  });

  it('saves a checkpoint on manual checkpointNow()', async () => {
    const { coordinator, agent, checkpoint } = makeFixture();
    coordinator.attachAgent(agent);

    await coordinator.checkpointNow();

    const payload = await checkpoint.load();
    expect(payload).not.toBeNull();
    expect(payload!.sessionID).toBe('s1');
  });

  it('serializes concurrent saves without corrupting the index', async () => {
    const { coordinator, agent, index } = makeFixture();
    coordinator.attachAgent(agent);

    await Promise.all([
      coordinator.checkpointNow(),
      coordinator.checkpointNow(),
      coordinator.checkpointNow(),
    ]);

    const data = await index.load();
    // Each manual call saves a version; the three calls executed serially.
    expect(data.versions).toHaveLength(3);
    expect(data.latest).toBeDefined();
  });

  it('does not break when there is no main agent', async () => {
    const { coordinator, agent } = makeFixture();
    coordinator.attachAgent(agent);

    const session = {
      options: { id: 's1' },
      metadata: { createdAt: new Date().toISOString() },
      agents: new Map<string, Agent>(),
    } as unknown as Session;

    const noMain = new CheckpointCoordinator({
      session,
      checkpoint: new SessionCheckpoint({ checkpointPath: join(workDir, 'x.json') }),
      index: new CheckpointIndex({ indexPath: join(workDir, 'x-index.json') }),
    });

    await expect(noMain.checkpointNow()).resolves.toBeUndefined();
  });
});
