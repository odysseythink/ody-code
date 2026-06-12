import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { CheckpointBackupStore } from '../../../src/session/checkpoint/backup-store';
import { CheckpointIndex } from '../../../src/session/checkpoint/checkpoint-index';
import { SessionCheckpoint } from '../../../src/session/checkpoint/checkpoint';
import { findFallbackCheckpoint } from '../../../src/session/checkpoint/recovery';
import type { SessionCheckpointPayload } from '../../../src/session/checkpoint/checkpoint';

function makePayload(timestamp: string): SessionCheckpointPayload {
  return {
    sessionID: 's1',
    createdAt: '2026-06-12T10:00:00.000Z',
    lastUpdatedAt: timestamp,
    currentMode: 'normal',
    messages: [],
    designModeContext: { sessions: [] },
    toolCallIndex: { callIdToResult: {} },
  };
}

describe('findFallbackCheckpoint', () => {
  let workDir: string;

  beforeEach(async () => {
    workDir = await mkdtemp(join(tmpdir(), 'recovery-test-'));
  });

  afterEach(async () => {
    await rm(workDir, { recursive: true, force: true });
  });

  function makeStore() {
    return new CheckpointBackupStore({ backupDir: join(workDir, 'backups'), sessionID: 's1' });
  }

  it('prefers the newest valid version from the index', async () => {
    const main = new SessionCheckpoint({ checkpointPath: join(workDir, 'session.json') });
    await main.save(makePayload('2026-06-12T10:00:00.000Z'));

    const result = await findFallbackCheckpoint({
      index: {
        latest: main.path,
        versions: [
          { path: '/missing.json', timestamp: '2026-06-12T10:00:02.000Z', messageCount: 0, valid: true, lastValidParent: null },
          { path: main.path, timestamp: '2026-06-12T10:00:01.000Z', messageCount: 0, valid: true, lastValidParent: null },
        ],
      },
      fallbackCheckpoint: main,
    });

    expect(result).not.toBeNull();
    expect(result!.source).toBe('index');
    expect(result!.path).toBe(main.path);
  });

  it('skips an invalid newest index version and uses the next valid one', async () => {
    const older = new SessionCheckpoint({ checkpointPath: join(workDir, 'older.json') });
    await older.save(makePayload('2026-06-12T10:00:00.000Z'));

    const result = await findFallbackCheckpoint({
      index: {
        latest: '/missing.json',
        versions: [
          { path: '/missing.json', timestamp: '2026-06-12T10:00:02.000Z', messageCount: 0, valid: false, lastValidParent: null },
          { path: older.path, timestamp: '2026-06-12T10:00:01.000Z', messageCount: 0, valid: true, lastValidParent: null },
        ],
      },
      fallbackCheckpoint: older,
    });

    expect(result).not.toBeNull();
    expect(result!.source).toBe('index');
    expect(result!.path).toBe(older.path);
  });

  it('skips index versions marked invalid and falls back to the main checkpoint', async () => {
    const main = new SessionCheckpoint({ checkpointPath: join(workDir, 'session.json') });
    await main.save(makePayload('2026-06-12T10:00:00.000Z'));

    const result = await findFallbackCheckpoint({
      index: {
        versions: [{ path: main.path, timestamp: '2026-06-12T10:00:00.000Z', messageCount: 0, valid: false, lastValidParent: null }],
      },
      fallbackCheckpoint: main,
    });

    expect(result).not.toBeNull();
    expect(result!.source).toBe('main');
  });

  it('falls back to backups when the index versions are unreadable', async () => {
    const main = new SessionCheckpoint({ checkpointPath: join(workDir, 'session.json') });
    await main.save(makePayload('2026-06-12T10:00:00.000Z'));

    const backupStore = makeStore();
    const backupPath = await backupStore.save(makePayload('2026-06-12T10:00:01.000Z'));

    const result = await findFallbackCheckpoint({
      index: { versions: [{ path: '/missing.json', timestamp: '2026-06-12T10:00:00.000Z', messageCount: 0, valid: true, lastValidParent: null }] },
      backupStore,
      fallbackCheckpoint: main,
    });

    expect(result).not.toBeNull();
    expect(result!.source).toBe('backup');
    expect(result!.path).toBe(backupPath);
  });

  it('falls back to the main checkpoint when nothing else is available', async () => {
    const main = new SessionCheckpoint({ checkpointPath: join(workDir, 'session.json') });
    await main.save(makePayload('2026-06-12T10:00:00.000Z'));

    const result = await findFallbackCheckpoint({
      index: { versions: [] },
      fallbackCheckpoint: main,
    });

    expect(result).not.toBeNull();
    expect(result!.source).toBe('main');
    expect(result!.path).toBe(main.path);
  });

  it('returns null when every source is unreadable', async () => {
    const main = new SessionCheckpoint({ checkpointPath: join(workDir, 'session.json') });

    const result = await findFallbackCheckpoint({
      index: { versions: [{ path: '/missing.json', timestamp: '2026-06-12T10:00:00.000Z', messageCount: 0, valid: true, lastValidParent: null }] },
      fallbackCheckpoint: main,
    });

    expect(result).toBeNull();
  });
});

describe('CheckpointIndex rebuildFromBackups', () => {
  let workDir: string;

  beforeEach(async () => {
    workDir = await mkdtemp(join(tmpdir(), 'rebuild-test-'));
  });

  afterEach(async () => {
    await rm(workDir, { recursive: true, force: true });
  });

  it('rebuilds the index from backup files', async () => {
    const backupStore = new CheckpointBackupStore({
      backupDir: join(workDir, 'backups'),
      sessionID: 's1',
    });
    const first = await backupStore.save(makePayload('2026-06-12T10:00:00.000Z'));
    await new Promise((resolve) => setTimeout(resolve, 20));
    const second = await backupStore.save(makePayload('2026-06-12T10:00:01.000Z'));

    const index = new CheckpointIndex({ indexPath: join(workDir, 'checkpoints.json') });
    await index.rebuildFromBackups(backupStore);

    const data = await index.load();
    expect(data.latest).toBe(second);
    expect(data.versions).toHaveLength(2);
    expect(data.versions[0]!.path).toBe(second);
    expect(data.versions[1]!.path).toBe(first);
    expect(data.versions[0]!.valid).toBe(false);
  });
});
