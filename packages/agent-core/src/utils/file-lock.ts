/**
 * Cross-platform file locking via `proper-lockfile`.
 *
 * The implementation relies on mkdir-based atomic lock directories, which
 * works on local filesystems on both POSIX and Windows. Locks are identified
 * by the target path; the library creates a companion `<target>.lock`
 * directory. Stale locks (left behind by crashed processes) are automatically
 * reaped based on mtime.
 */

import { lock, check } from 'proper-lockfile';
import type { OperationOptions } from 'retry';

export interface FileLockOptions {
  /** Maximum time in ms to wait for the lock before throwing. */
  retries?: number | undefined;
  /** Interval between retries in ms (used as both min and max timeout). */
  intervalMs?: number | undefined;
  /** Lock is considered stale if its mtime is older than this many ms. */
  staleMs?: number | undefined;
}

const DEFAULT_RETRIES = 60;
const DEFAULT_INTERVAL_MS = 100;
const DEFAULT_STALE_MS = 5000;

function resolveLockOptions(
  options: FileLockOptions | undefined,
): { retries: OperationOptions; stale: number } {
  const retriesCount = Math.max(0, options?.retries ?? DEFAULT_RETRIES);
  const intervalMs = Math.max(1, options?.intervalMs ?? DEFAULT_INTERVAL_MS);
  return {
    retries: {
      retries: retriesCount,
      minTimeout: intervalMs,
      maxTimeout: intervalMs,
      factor: 1,
    },
    stale: Math.max(0, options?.staleMs ?? DEFAULT_STALE_MS),
  };
}

/**
 * Acquire a lock on `filePath`, run `task`, and release the lock afterwards.
 * The target file does not need to exist; the lock is identified by the path.
 */
export async function withFileLock<T>(
  filePath: string,
  task: () => Promise<T>,
  options?: FileLockOptions,
): Promise<T> {
  const lockOptions = resolveLockOptions(options);
  const release = await lock(filePath, {
    ...lockOptions,
    realpath: false,
    onCompromised: (err) => {
      // Surface lock compromise as a rejected task so callers can log/abort.
      throw err;
    },
  });
  try {
    return await task();
  } finally {
    await release().catch(() => {
      // Best-effort release; a compromised or already-reaped lock may fail.
    });
  }
}

/**
 * Check whether `filePath` is currently locked by any process.
 */
export async function isFileLocked(
  filePath: string,
  options?: Pick<FileLockOptions, 'staleMs'>,
): Promise<boolean> {
  return check(filePath, {
    stale: Math.max(0, options?.staleMs ?? DEFAULT_STALE_MS),
    realpath: false,
  });
}
