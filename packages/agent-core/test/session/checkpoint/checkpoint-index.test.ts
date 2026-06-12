import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { CheckpointIndex } from '../../../src/session/checkpoint/checkpoint-index';

describe('CheckpointIndex', () => {
  let workDir: string;

  beforeEach(async () => {
    workDir = await mkdtemp(join(tmpdir(), 'checkpoint-index-test-'));
  });

  afterEach(async () => {
    await rm(workDir, { recursive: true, force: true });
  });

  function version(n: number): {
    path: string;
    timestamp: string;
    messageCount: number;
    valid: boolean;
    lastValidParent: string | null;
  } {
    return {
      path: join(workDir, `v${n}.json`),
      timestamp: new Date(2026, 5, 12, 10, 0, n).toISOString(),
      messageCount: n * 10,
      valid: true,
      lastValidParent: null,
    };
  }

  it('records a new version and sets latest', async () => {
    const index = new CheckpointIndex({ indexPath: join(workDir, 'index.json') });
    const v1 = version(1);

    await index.update(v1);

    const data = await index.load();
    expect(data.latest).toBe(v1.path);
    expect(data.versions).toEqual([v1]);
  });

  it('prepends newer versions and keeps them ordered newest-first', async () => {
    const index = new CheckpointIndex({ indexPath: join(workDir, 'index.json') });
    const v1 = version(1);
    const v2 = version(2);

    await index.update(v1);
    await index.update(v2);

    const data = await index.load();
    expect(data.latest).toBe(v2.path);
    expect(data.versions.map((v) => v.path)).toEqual([v2.path, v1.path]);
  });

  it('rotates the version list to maxVersions', async () => {
    const index = new CheckpointIndex({ indexPath: join(workDir, 'index.json'), maxVersions: 3 });

    for (let i = 1; i <= 5; i += 1) {
      await index.update(version(i));
    }

    const data = await index.load();
    expect(data.versions).toHaveLength(3);
    expect(data.latest).toBe(version(5).path);
    expect(data.versions.map((v) => v.path)).toEqual([
      version(5).path,
      version(4).path,
      version(3).path,
    ]);
  });

  it('returns an empty index when the file does not exist', async () => {
    const index = new CheckpointIndex({ indexPath: join(workDir, 'missing.json') });

    await expect(index.load()).resolves.toEqual({ versions: [] });
  });

  it('persists pretty-printed JSON with a trailing newline', async () => {
    const indexPath = join(workDir, 'index.json');
    const index = new CheckpointIndex({ indexPath });
    await index.update(version(1));

    const text = await readFile(indexPath, 'utf8');
    expect(text).toMatch(/^\{[\s\S]*\}\n$/);
    const parsed = JSON.parse(text);
    expect(parsed.latest).toBe(version(1).path);
  });

  it('throws a typed error for corrupt index JSON', async () => {
    const indexPath = join(workDir, 'index.json');
    const { writeFile } = await import('node:fs/promises');
    await writeFile(indexPath, 'not json');

    const index = new CheckpointIndex({ indexPath });

    await expect(index.load()).rejects.toMatchObject({ code: 'session.state_invalid' });
  });
});
