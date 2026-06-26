/**
 * Checkpoint recovery fallback chain.
 *
 * When the newest checkpoint version cannot be loaded or fails integrity checks,
 * walk backwards through:
 *
 *   1. Older valid versions from the index
 *   2. Backup files not currently referenced by the index
 *   3. The main/latest checkpoint file
 *
 * The first loadable payload is returned so resume can continue with the best
 * available state.
 */

import type { Logger } from '@odysseythink/agent-core-shared';
import { CheckpointBackupStore } from './backup-store';
import type { CheckpointIndexData } from './checkpoint-index';
import { SessionCheckpoint } from './checkpoint';
import type { SessionCheckpointPayload } from './checkpoint';

export interface CheckpointRecoveryOptions {
  readonly index: CheckpointIndexData;
  readonly backupStore?: CheckpointBackupStore | undefined;
  readonly fallbackCheckpoint: SessionCheckpoint;
  readonly logger?: Logger | undefined;
}

export interface CheckpointRecoveryResult {
  payload: SessionCheckpointPayload;
  source: 'index' | 'backup' | 'main';
  path: string;
}

/**
 * Find the best available checkpoint payload using the fallback chain.
 */
export async function findFallbackCheckpoint(
  options: CheckpointRecoveryOptions,
): Promise<CheckpointRecoveryResult | null> {
  const fromIndex = await findInIndex(options);
  if (fromIndex !== null) return fromIndex;

  const fromBackups = await findInBackups(options);
  if (fromBackups !== null) return fromBackups;

  return findInMain(options);
}

async function findInIndex(
  options: CheckpointRecoveryOptions,
): Promise<CheckpointRecoveryResult | null> {
  for (const version of options.index.versions) {
    if (!version.valid) continue;
    const payload = await loadPayload(version.path, options.logger);
    if (payload !== null) {
      return { payload, source: 'index', path: version.path };
    }
  }
  return null;
}

async function findInBackups(
  options: CheckpointRecoveryOptions,
): Promise<CheckpointRecoveryResult | null> {
  if (options.backupStore === undefined) return null;

  const paths = await options.backupStore.list();
  for (const path of paths) {
    if (options.index.versions.some((v) => v.path === path)) continue;
    const payload = await loadPayload(path, options.logger);
    if (payload !== null) {
      return { payload, source: 'backup', path };
    }
  }
  return null;
}

async function findInMain(
  options: CheckpointRecoveryOptions,
): Promise<CheckpointRecoveryResult | null> {
  const payload = await loadPayload(options.fallbackCheckpoint.path, options.logger);
  if (payload === null) return null;
  return { payload, source: 'main', path: options.fallbackCheckpoint.path };
}

async function loadPayload(
  path: string,
  logger?: Logger,
): Promise<SessionCheckpointPayload | null> {
  const checkpoint = new SessionCheckpoint({ checkpointPath: path });
  try {
    return await checkpoint.load();
  } catch (error) {
    logger?.warn('Failed to load fallback checkpoint', { path, error });
    return null;
  }
}
