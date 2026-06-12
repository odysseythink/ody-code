import { mkdtemp, readFile, readdir, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { CheckpointBackupStore } from '../../../src/session/checkpoint/backup-store';
import type { SessionCheckpointPayload } from '../../../src/session/checkpoint/checkpoint';

function makePayload(timestamp: string): SessionCheckpointPayload {
  return {
    sessionID: 's1',
    createdAt: timestamp,
    lastUpdatedAt: timestamp,
    currentMode: 'normal',
    messages: [],
    designModeContext: { sessions: [] },
    toolCallIndex: { callIdToResult: {} },
  };
}

describe('CheckpointBackupStore', () => {
  let workDir: string;

  beforeEach(async () => {
    workDir = await mkdtemp(join(tmpdir(), 'backup-store-test-'));
  });

  afterEach(async () => {
    await rm(workDir, { recursive: true, force: true });
  });

  function makeStore(maxBackups = 3) {
    return new CheckpointBackupStore({
      backupDir: join(workDir, 'backups'),
      sessionID: 's1',
      maxBackups,
    });
  }

  it('writes a backup file and returns its path', async () => {
    const store = makeStore();
    const path = await store.save(makePayload('2026-06-12T10:00:00.000Z'));

    expect(path.startsWith(join(workDir, 'backups'))).toBe(true);
    const text = await readFile(path, 'utf8');
    const parsed = JSON.parse(text) as SessionCheckpointPayload;
    expect(parsed.sessionID).toBe('s1');
  });

  it('lists backups newest to oldest', async () => {
    const store = makeStore();
    const first = await store.save(makePayload('2026-06-12T10:00:00.000Z'));
    await new Promise((resolve) => setTimeout(resolve, 20));
    const second = await store.save(makePayload('2026-06-12T10:00:01.000Z'));

    const list = await store.list();
    expect(list).toEqual([second, first]);
  });

  it('rotates out older backups once maxBackups is exceeded', async () => {
    const store = makeStore(2);
    const p1 = await store.save(makePayload('2026-06-12T10:00:00.000Z'));
    await new Promise((resolve) => setTimeout(resolve, 20));
    const p2 = await store.save(makePayload('2026-06-12T10:00:01.000Z'));
    await new Promise((resolve) => setTimeout(resolve, 20));
    const p3 = await store.save(makePayload('2026-06-12T10:00:02.000Z'));

    const list = await store.list();
    expect(list).toHaveLength(2);
    expect(list).toContain(p2);
    expect(list).toContain(p3);
    expect(list).not.toContain(p1);

    const entries = await readdir(join(workDir, 'backups'));
    expect(entries.filter((e) => e.endsWith('.json'))).toHaveLength(2);
  });

  it('keeps the latest backup even when it is the oldest by mtime', async () => {
    const store = makeStore(2);
    const p1 = await store.save(makePayload('2026-06-12T10:00:02.000Z'));
    await new Promise((resolve) => setTimeout(resolve, 20));
    const p2 = await store.save(makePayload('2026-06-12T10:00:01.000Z'));
    await new Promise((resolve) => setTimeout(resolve, 20));
    const p3 = await store.save(makePayload('2026-06-12T10:00:00.000Z'));

    const list = await store.list();
    expect(list).toHaveLength(2);
    expect(list).toContain(p2);
    expect(list).toContain(p3);
  });
});
