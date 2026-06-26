/**
 * Versioned checkpoint backup store.
 *
 * Every checkpoint save also writes an immutable backup copy under
 * `<backupDir>/<sessionID>-v<timestamp>.json`. Older backups are rotated out
 * once the store exceeds `maxBackups`, keeping the most recent copies.
 */

import { mkdir, readdir, stat, unlink } from 'node:fs/promises';
import { join } from 'pathe';

import { atomicWrite } from '#/utils/fs';
import { withFileLock } from '#/utils/file-lock';
import type { SessionCheckpointPayload } from './checkpoint';

export interface CheckpointBackupStoreOptions {
  /** Directory that will hold the backup files. */
  readonly backupDir: string;
  /** Session identifier used as the backup filename prefix. */
  readonly sessionID: string;
  /** Maximum number of backup files to retain (default 10). */
  readonly maxBackups?: number | undefined;
}

const DEFAULT_MAX_BACKUPS = 10;

export class CheckpointBackupStore {
  private counter = 0;
  private readonly options: Required<CheckpointBackupStoreOptions>;

  constructor(options: CheckpointBackupStoreOptions) {
    this.options = {
      maxBackups: DEFAULT_MAX_BACKUPS,
      ...options,
    };
  }

  get dir(): string {
    return this.options.backupDir;
  }

  /**
   * Persist an immutable backup copy of `payload` and rotate out older backups.
   *
   * Returns the absolute path of the newly written backup file.
   */
  async save(payload: SessionCheckpointPayload): Promise<string> {
    const path = this.nextBackupPath(payload.lastUpdatedAt);

    await mkdir(this.options.backupDir, { recursive: true });
    await withFileLock(this.lockFilePath(), async () => {
      await atomicWrite(path, `${JSON.stringify(payload, null, 2)}\n`);
      await this.rotateLocked();
    });

    return path;
  }

  /**
   * List backup paths from newest to oldest.
   */
  async list(): Promise<string[]> {
    await mkdir(this.options.backupDir, { recursive: true });
    return withFileLock(this.lockFilePath(), async () => this.listLocked());
  }

  /**
   * Remove the oldest `count` backups to free disk space. Returns the number
   * of files actually removed.
   */
  async freeOldest(count: number): Promise<number> {
    await mkdir(this.options.backupDir, { recursive: true });
    return withFileLock(this.lockFilePath(), async () => {
      const paths = await this.listLocked();
      const toRemove = paths.slice(-Math.max(0, count));
      let removed = 0;
      for (const path of toRemove) {
        try {
          await unlink(path);
          removed += 1;
        } catch {
          // Ignore errors; the next save will try again.
        }
      }
      return removed;
    });
  }

  private nextBackupPath(timestamp: string): string {
    this.counter += 1;
    const safeTimestamp = timestamp.replace(/[:.]/g, '-');
    const filename = `${this.options.sessionID}-v${safeTimestamp}-${this.counter}.json`;
    return join(this.options.backupDir, filename);
  }

  private lockFilePath(): string {
    return join(this.options.backupDir, '.rotation.lock');
  }

  private async listLocked(): Promise<string[]> {
    try {
      await mkdir(this.options.backupDir, { recursive: true });
    } catch {
      // mkdir may race; readdir will still fail below if the directory is truly missing.
    }

    let entries: string[];
    try {
      entries = await readdir(this.options.backupDir);
    } catch {
      return [];
    }

    const prefix = `${this.options.sessionID}-v`;
    const paths: { path: string; mtime: number }[] = [];

    for (const entry of entries) {
      if (!entry.startsWith(prefix) || !entry.endsWith('.json')) continue;
      const path = join(this.options.backupDir, entry);
      try {
        const st = await stat(path);
        paths.push({ path, mtime: st.mtimeMs });
      } catch {
        // File may have been removed between readdir and stat; skip it.
      }
    }

    paths.sort((a, b) => b.mtime - a.mtime);
    return paths.map((p) => p.path);
  }

  private async rotateLocked(): Promise<void> {
    const paths = await this.listLocked();
    const toRemove = paths.slice(this.options.maxBackups);
    for (const path of toRemove) {
      try {
        await unlink(path);
      } catch {
        // Ignore rotation errors; the next save will retry cleanup.
      }
    }
  }
}
