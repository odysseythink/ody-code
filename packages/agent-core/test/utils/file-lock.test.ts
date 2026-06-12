import { mkdir, mkdtemp, rm, utimes, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';

import { isFileLocked, withFileLock } from '../../src/utils/file-lock';

describe('withFileLock', () => {
  let workDir: string;

  beforeEach(async () => {
    workDir = await mkdtemp(join(tmpdir(), 'file-lock-test-'));
  });

  afterEach(async () => {
    await rm(workDir, { recursive: true, force: true });
  });

  it('serializes concurrent writers on the same target', async () => {
    const target = join(workDir, 'data.json');
    const order: string[] = [];

    const a = withFileLock(target, async () => {
      order.push('a-start');
      await new Promise((resolve) => setTimeout(resolve, 50));
      order.push('a-end');
    });

    const b = withFileLock(target, async () => {
      order.push('b-start');
      order.push('b-end');
    });

    await Promise.all([a, b]);

    // Ensure the critical sections never overlapped.
    expect(order).toHaveLength(4);
    const firstEnd = order.findIndex((x) => x.endsWith('-end'));
    const secondStart = order.findIndex((x, i) => i > firstEnd && x.endsWith('-start'));
    expect(secondStart).toBeGreaterThan(firstEnd);
  });

  it('allows independent locks on different targets', async () => {
    const targetA = join(workDir, 'a.json');
    const targetB = join(workDir, 'b.json');
    let running = 0;
    let maxRunning = 0;

    const task = async () => {
      running += 1;
      maxRunning = Math.max(maxRunning, running);
      await new Promise((resolve) => setTimeout(resolve, 50));
      running -= 1;
    };

    await Promise.all([withFileLock(targetA, task), withFileLock(targetB, task)]);

    expect(maxRunning).toBe(2);
  });

  it('releases the lock even if the task throws', async () => {
    const target = join(workDir, 'throw.json');

    await expect(
      withFileLock(target, async () => {
        throw new Error('boom');
      }),
    ).rejects.toThrow('boom');

    expect(await isFileLocked(target)).toBe(false);
  });

  it('reaps a stale lock and lets a new caller acquire it', async () => {
    const target = join(workDir, 'stale.json');
    const lockDir = `${target}.lock`;

    // Create a stale lock directory manually and backdate its mtime.
    // proper-lockfile leaves empty lock directories; contents are not expected.
    await mkdir(lockDir);
    const oldMtime = new Date(Date.now() - 10000);
    await utimes(lockDir, oldMtime, oldMtime);

    await withFileLock(
      target,
      async () => {
        expect(await isFileLocked(target)).toBe(true);
      },
      { retries: 5, intervalMs: 20 },
    );

    expect(await isFileLocked(target)).toBe(false);
  });

  it('throws when the lock cannot be acquired within the retry budget', async () => {
    const target = join(workDir, 'timeout.json');
    const lockDir = `${target}.lock`;

    // Hold the lock with a fresh mtime so it is not considered stale.
    await mkdir(lockDir);
    await writeFile(join(lockDir, 'pid'), String(process.pid), 'utf8');

    await expect(
      withFileLock(
        target,
        async () => {
          // never reached
        },
        { retries: 2, intervalMs: 10, staleMs: 5000 },
      ),
    ).rejects.toThrow();
  });

  it('locks a target that does not exist yet', async () => {
    const target = join(workDir, 'not-yet-created.json');

    await withFileLock(target, async () => {
      expect(await isFileLocked(target)).toBe(true);
    });

    expect(await isFileLocked(target)).toBe(false);
  });
});
