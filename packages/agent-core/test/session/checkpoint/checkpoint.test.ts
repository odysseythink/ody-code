import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { SessionCheckpoint } from '../../../src/session/checkpoint/checkpoint';

describe('SessionCheckpoint', () => {
  let workDir: string;

  beforeEach(async () => {
    workDir = await mkdtemp(join(tmpdir(), 'session-checkpoint-test-'));
  });

  afterEach(async () => {
    await rm(workDir, { recursive: true, force: true });
  });

  function makeCheckpoint(): SessionCheckpoint {
    return new SessionCheckpoint({ checkpointPath: join(workDir, 'session.json') });
  }

  function samplePayload() {
    return {
      sessionID: 's1',
      createdAt: '2026-06-12T10:00:00.000Z',
      lastUpdatedAt: '2026-06-12T10:00:00.000Z',
      currentMode: 'design' as const,
      messages: [{ role: 'user', content: 'hello' }],
      designModeContext: {
        sessions: [
          {
            designSessionID: 'd1',
            startedAtMsg: 0,
            exitedAtMsg: 2,
            approvedPath: '/design.md',
          },
        ],
      },
      toolCallIndex: {
        callIdToResult: {},
      },
    };
  }

  it('saves a checkpoint and updates lastUpdatedAt', async () => {
    const checkpoint = makeCheckpoint();
    const before = Date.now();

    await checkpoint.save(samplePayload());

    const text = await readFile(checkpoint.path, 'utf8');
    const parsed = JSON.parse(text) as ReturnType<typeof samplePayload>;
    expect(parsed.sessionID).toBe('s1');
    expect(parsed.currentMode).toBe('design');
    expect(new Date(parsed.lastUpdatedAt).getTime()).toBeGreaterThanOrEqual(before);
  });

  it('loads a previously saved checkpoint', async () => {
    const checkpoint = makeCheckpoint();
    const payload = samplePayload();
    await checkpoint.save(payload);

    const loaded = await checkpoint.load();

    expect(loaded).not.toBeNull();
    expect(loaded!.sessionID).toBe(payload.sessionID);
    expect(loaded!.messages).toEqual(payload.messages);
    expect(loaded!.designModeContext.sessions).toHaveLength(1);
  });

  it('returns null when the checkpoint file does not exist', async () => {
    const checkpoint = new SessionCheckpoint({ checkpointPath: join(workDir, 'missing.json') });

    await expect(checkpoint.load()).resolves.toBeNull();
  });

  it('throws a typed error when the checkpoint file is not valid JSON', async () => {
    const checkpoint = makeCheckpoint();
    await checkpoint.save(samplePayload());
    await rm(checkpoint.path);

    const bad = new SessionCheckpoint({
      checkpointPath: join(workDir, 'session.json'),
    });
    // Atomic write uses temp+rename, so recreate the target path with garbage.
    await writeFile(bad.path, 'not json');

    await expect(bad.load()).rejects.toMatchObject({ code: 'session.state_invalid' });
  });

  it('overwrites an existing checkpoint with newer state', async () => {
    const checkpoint = makeCheckpoint();
    await checkpoint.save(samplePayload());
    const first = await checkpoint.load();

    const updated = {
      ...samplePayload(),
      messages: [...samplePayload().messages, { role: 'assistant', content: 'ok' }],
    };
    await checkpoint.save(updated);
    const second = await checkpoint.load();

    expect(second!.messages).toHaveLength(2);
    expect(new Date(second!.lastUpdatedAt).getTime()).toBeGreaterThanOrEqual(
      new Date(first!.lastUpdatedAt).getTime(),
    );
  });
});
