/**
 * Session checkpoint persistence.
 *
 * A checkpoint is a durable JSON snapshot of session state. It is written
 * atomically (temp-file + rename) and protected by a file lock so concurrent
 * saves from the same process cannot corrupt it.
 */

import { mkdir, readFile } from 'node:fs/promises';
import { dirname } from 'pathe';

import { ErrorCodes, KimiError } from '#/errors';
import { atomicWrite } from '#/utils/fs';
import { withFileLock } from '#/utils/file-lock';
import type { ModeKey } from '../../agent';

export interface DesignSessionCheckpoint {
  designSessionID: string;
  startedAtMsg: number;
  exitedAtMsg?: number | undefined;
  approvedPath?: string | undefined;
}

export interface SessionCheckpointPayload {
  sessionID: string;
  createdAt: string;
  lastUpdatedAt: string;
  currentMode: ModeKey;
  messages: unknown[];
  designModeContext: {
    sessions: DesignSessionCheckpoint[];
  };
  toolCallIndex: {
    callIdToResult: Record<string, unknown>;
  };
}

export interface SessionCheckpointOptions {
  /** Absolute path to the checkpoint JSON file. */
  readonly checkpointPath: string;
}

/**
 * Durable checkpoint storage for a single session.
 */
export class SessionCheckpoint {
  constructor(private readonly options: SessionCheckpointOptions) {}

  get path(): string {
    return this.options.checkpointPath;
  }

  /**
   * Persist `payload` to disk atomically.
   *
   * The payload is serialized with a trailing newline, written to a temp
   * file in the same directory, then renamed over the target. A file lock
   * prevents concurrent saves from interleaving.
   */
  async save(payload: SessionCheckpointPayload): Promise<void> {
    const payloadWithTimestamp: SessionCheckpointPayload = {
      ...payload,
      lastUpdatedAt: new Date().toISOString(),
    };
    const text = `${JSON.stringify(payloadWithTimestamp, null, 2)}\n`;

    await withFileLock(this.path, async () => {
      await mkdir(dirname(this.path), { recursive: true });
      await atomicWrite(this.path, text);
    });
  }

  /**
   * Load the checkpoint from disk.
   *
   * Returns `null` when the file does not exist. Throws a typed error when
   * the file exists but cannot be parsed, so callers can decide to fall
   * back to an older checkpoint.
   */
  async load(): Promise<SessionCheckpointPayload | null> {
    let text: string;
    try {
      text = await readFile(this.path, 'utf8');
    } catch (error) {
      const code = (error as NodeJS.ErrnoException).code;
      if (code === 'ENOENT') return null;
      throw new KimiError(
        ErrorCodes.SESSION_STATE_INVALID,
        `Failed to read checkpoint ${this.path}: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }

    try {
      return JSON.parse(text) as SessionCheckpointPayload;
    } catch (error) {
      throw new KimiError(
        ErrorCodes.SESSION_STATE_INVALID,
        `Checkpoint ${this.path} is not valid JSON: ${error instanceof Error ? error.message : String(error)}`,
        { cause: error },
      );
    }
  }
}
