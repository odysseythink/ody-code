/**
 * Checkpoint index: a durable registry of saved checkpoint versions.
 *
 * The index lives next to the checkpoint files (e.g. `checkpoints.json`)
 * and records, for each version, the path to the checkpoint file, when it
 * was written, how many messages it contains, whether it passed integrity
 * validation, and a pointer to the previous valid version. Older entries
 * are rotated out once the registry exceeds `maxVersions`.
 */

import { mkdir, readFile } from 'node:fs/promises';
import { dirname } from 'pathe';

import { ErrorCodes, OdyError } from '#/errors';
import { atomicWrite } from '#/utils/fs';
import { withFileLock } from '#/utils/file-lock';
import { CheckpointBackupStore } from './backup-store';
import { SessionCheckpoint } from './checkpoint';

export interface CheckpointVersion {
  /** Path to the checkpoint file for this version. */
  path: string;
  /** ISO timestamp when the checkpoint was written. */
  timestamp: string;
  /** Number of messages captured in the checkpoint. */
  messageCount: number;
  /** Whether integrity verification passed for this checkpoint. */
  valid: boolean;
  /** Path to the previous valid checkpoint version, or null if none. */
  lastValidParent: string | null;
}

export interface CheckpointIndexData {
  /** The most recently recorded checkpoint path. */
  latest?: string | undefined;
  /** Versions from newest to oldest. */
  versions: CheckpointVersion[];
}

export interface CheckpointIndexOptions {
  /** Absolute path to the index JSON file. */
  readonly indexPath: string;
  /** Maximum number of versions to keep (default 10). */
  readonly maxVersions?: number | undefined;
}

const DEFAULT_MAX_VERSIONS = 10;

/**
 * Durable registry of checkpoint versions.
 */
export class CheckpointIndex {
  private readonly options: Required<CheckpointIndexOptions>;

  constructor(options: CheckpointIndexOptions) {
    this.options = {
      maxVersions: DEFAULT_MAX_VERSIONS,
      ...options,
    };
  }

  get path(): string {
    return this.options.indexPath;
  }

  /**
   * Add a new version to the index and persist it atomically.
   *
   * The new version is inserted at the front of the list, `latest` is set
   * to its path, and the tail is truncated to `maxVersions`.
   */
  async update(version: CheckpointVersion): Promise<void> {
    await withFileLock(this.path, async () => {
      const data = await this.loadSafe();
      const versions = [version, ...data.versions];
      if (versions.length > this.options.maxVersions) {
        versions.length = this.options.maxVersions;
      }

      const next: CheckpointIndexData = {
        latest: version.path,
        versions,
      };
      const text = `${JSON.stringify(next, null, 2)}\n`;

      await mkdir(dirname(this.path), { recursive: true });
      await atomicWrite(this.path, text);
    });
  }

  /**
   * Read the index from disk.
   *
   * Returns a default empty object when the file does not exist. Throws a
   * typed error when the file exists but is not valid JSON.
   */
  async load(): Promise<CheckpointIndexData> {
    return this.loadSafe();
  }

  /**
   * Rebuild the index by scanning backup files.
   *
   * Loads each backup to extract its timestamp and message count, then writes
   * a fresh index with the newest backups first. This handles the case where
   * the index file is lost or corrupted (error E4).
   */
  async rebuildFromBackups(backupStore: CheckpointBackupStore): Promise<void> {
    const paths = await backupStore.list();
    const versions: CheckpointVersion[] = [];

    for (const path of paths) {
      const checkpoint = new SessionCheckpoint({ checkpointPath: path });
      try {
        const payload = await checkpoint.load();
        if (payload === null) continue;
        versions.push({
          path,
          timestamp: payload.lastUpdatedAt,
          messageCount: payload.messages.length,
          valid: false,
          lastValidParent: null,
        });
      } catch {
        // Ignore unreadable backups; they will be skipped during recovery.
      }
    }

    const next: CheckpointIndexData = {
      latest: versions[0]?.path,
      versions,
    };
    const text = `${JSON.stringify(next, null, 2)}\n`;

    await withFileLock(this.path, async () => {
      await mkdir(dirname(this.path), { recursive: true });
      await atomicWrite(this.path, text);
    });
  }

  private async loadSafe(): Promise<CheckpointIndexData> {
    let text: string;
    try {
      text = await readFile(this.path, 'utf8');
    } catch (error) {
      const code = (error as NodeJS.ErrnoException).code;
      if (code === 'ENOENT') {
        return { versions: [] };
      }
      throw new OdyError(
        ErrorCodes.SESSION_STATE_INVALID,
        `Failed to read checkpoint index ${this.path}: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }

    try {
      return JSON.parse(text) as CheckpointIndexData;
    } catch (error) {
      throw new OdyError(
        ErrorCodes.SESSION_STATE_INVALID,
        `Checkpoint index ${this.path} is not valid JSON: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }
}
