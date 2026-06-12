/**
 * Session resume integration for checkpoints.
 *
 * After the wire log has been replayed, this module compares the live agent
 * state with the most recent durable checkpoint. It restores any design-session
 * metadata that the wire records do not carry and emits a warning when the
 * checkpoint and the replayed state disagree.
 */

import { dirname, join } from 'pathe';

import type { Agent } from '#/agent';
import type { Logger } from '#/logging/types';
import type { Session } from '..';
import { CheckpointBackupStore } from './backup-store';
import { CheckpointIndex } from './checkpoint-index';
import { SessionCheckpoint } from './checkpoint';
import { findFallbackCheckpoint } from './recovery';
import { verifyCheckpointIntegrity } from './integrity';

export interface CheckpointResumeOptions {
  /** Path to the checkpoint JSON file. Defaults to `<homedir>/.ody-code/session-state/session.json`. */
  readonly checkpointPath?: string | undefined;
  /** Path to the checkpoint index. Defaults to `<homedir>/.ody-code/session-state/checkpoints.json`. */
  readonly indexPath?: string | undefined;
  /** Optional logger for diagnostics. */
  readonly logger?: Logger | undefined;
}

export interface CheckpointResumeResult {
  /** Human-readable warning when the checkpoint could not be verified or restored. */
  warning?: string | undefined;
  /** True when a checkpoint was found and passed integrity verification. */
  verified?: boolean | undefined;
}

const DEFAULT_STATE_DIR = '.ody-code/session-state';

/**
 * Verify and, if necessary, repair the resumed session state from the latest
 * checkpoint. This must run **after** the wire log has been replayed so that
 * the live agent state is the source of truth and the checkpoint is only used
 * to fill gaps (e.g. design-session metadata) and to detect drift.
 */
export async function verifyAndRestoreResumedSession(
  session: Session,
  options: CheckpointResumeOptions = {},
): Promise<CheckpointResumeResult> {
  const main = session.agents.get('main');
  if (main === undefined) {
    return { warning: 'No main agent found; skipping checkpoint resume verification.' };
  }

  const checkpointPath = options.checkpointPath ?? defaultCheckpointPath(session);
  const indexPath = options.indexPath ?? defaultIndexPath(session);
  const backupDir = join(dirname(indexPath), 'backups');

  const checkpoint = new SessionCheckpoint({ checkpointPath });
  const index = new CheckpointIndex({ indexPath });
  const backupStore =
    session.options.id === undefined
      ? undefined
      : new CheckpointBackupStore({ backupDir, sessionID: session.options.id });

  let indexData = await loadIndexSafe(index, options.logger);

  // If the index is missing but backups exist, rebuild the index from disk.
  if ((indexData === null || indexData.versions.length === 0) && backupStore !== undefined) {
    const backups = await backupStore.list();
    if (backups.length > 0) {
      await index.rebuildFromBackups(backupStore);
      indexData = await loadIndexSafe(index, options.logger);
    }
  }

  if (indexData === null || indexData.versions.length === 0) {
    // No checkpoint has been written yet; this is normal for new sessions.
    return { verified: false };
  }

  const recovered = await findFallbackCheckpoint({
    index: indexData,
    backupStore,
    fallbackCheckpoint: checkpoint,
    logger: options.logger,
  });

  if (recovered === null) {
    return { warning: 'No checkpoint could be loaded; recovery failed.' };
  }

  const { payload } = recovered;

  let integrity: ReturnType<typeof verifyCheckpointIntegrity>;
  try {
    integrity = verifyCheckpointIntegrity(payload, {
      expectedSessionID: session.options.id ?? undefined,
      expectedMessageCount: main.context.history.length,
    });
  } catch (error) {
    const warning = `Checkpoint integrity verification threw after replay: ${error instanceof Error ? error.message : String(error)}`;
    options.logger?.warn(warning, { error });
    return { warning, verified: false };
  }

  if (!integrity.valid) {
    const warning = `Checkpoint integrity failed after replay: ${integrity.errors.join('; ')}`;
    options.logger?.warn(warning, { errors: integrity.errors });
    return { warning, verified: false };
  }

  restoreDesignSessions(main, payload.designModeContext.sessions);

  const liveMode = main.sessionMode.isActive ? main.sessionMode.kind : 'normal';
  if (liveMode !== payload.currentMode) {
    const warning = `Mode mismatch after replay: live=${liveMode}, checkpoint=${payload.currentMode}`;
    options.logger?.warn(warning);
    return { warning, verified: true };
  }

  return { verified: true };
}

function defaultCheckpointPath(session: Session): string {
  return join(session.options.homedir, DEFAULT_STATE_DIR, 'session.json');
}

function defaultIndexPath(session: Session): string {
  return join(session.options.homedir, DEFAULT_STATE_DIR, 'checkpoints.json');
}

async function loadIndexSafe(
  index: CheckpointIndex,
  logger?: Logger,
): Promise<import('./checkpoint-index').CheckpointIndexData | null> {
  try {
    return await index.load();
  } catch (error) {
    logger?.warn('Failed to load checkpoint index during resume', { error });
    return null;
  }
}

function restoreDesignSessions(
  main: Agent,
  checkpointSessions: ReadonlyArray<import('./checkpoint').DesignSessionCheckpoint>,
): void {
  const live = main.sessionMode.designSessions;
  if (live.length > 0 || checkpointSessions.length === 0) {
    return;
  }
  main.sessionMode.restoreDesignSessions(checkpointSessions);
}
