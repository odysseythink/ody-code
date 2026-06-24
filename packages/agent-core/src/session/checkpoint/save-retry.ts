/**
 * Retry logic for checkpoint persistence failures.
 *
 * When a save fails because the disk is full (ENOSPC), the caller can attempt
 * to free space by removing older backups and then retry. Other errors are
 * surfaced immediately.
 */

import type { Logger } from '@odysseythink/agent-core-shared';

export interface CheckpointSaveRetryOptions {
  /** Number of cleanup/retry attempts after a disk-full error (default 1). */
  readonly maxRetries?: number | undefined;
  /** Optional logger for diagnostics. */
  readonly logger?: Logger | undefined;
}

function isNoSpaceError(error: unknown): boolean {
  return (
    error !== null &&
    typeof error === 'object' &&
    'code' in error &&
    (error as NodeJS.ErrnoException).code === 'ENOSPC'
  );
}

/**
 * Execute `save`. If it fails with ENOSPC, invoke `cleanup` up to `maxRetries`
 * times and retry after each cleanup. Returns the result of the successful
 * save; throws the last error if all retries fail.
 */
export async function withCheckpointSaveRetry<T>(
  save: () => Promise<T>,
  cleanup: () => Promise<void>,
  options: CheckpointSaveRetryOptions = {},
): Promise<T> {
  const maxRetries = Math.max(0, options.maxRetries ?? 1);
  let lastError: unknown;

  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      return await save();
    } catch (error) {
      lastError = error;
      if (!isNoSpaceError(error)) {
        throw error;
      }

      if (attempt < maxRetries) {
        options.logger?.warn('Checkpoint save failed with ENOSPC; freeing space and retrying', {
          attempt: attempt + 1,
        });
        try {
          await cleanup();
        } catch (cleanupError) {
          options.logger?.error('Checkpoint cleanup failed', cleanupError);
        }
      }
    }
  }

  throw lastError;
}
