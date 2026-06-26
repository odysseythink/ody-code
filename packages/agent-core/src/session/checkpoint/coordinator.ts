/**
 * Event-driven checkpoint coordinator.
 *
 * The coordinator subscribes to an agent's records, appends every message to
 * the Markdown transcript, and asynchronously saves a JSON checkpoint on key
 * lifecycle boundaries:
 *
 *   - session_mode.exit (design approval, plan completion, etc.)
 *   - step.end inside a context.append_loop_event (turn boundary)
 *   - manual /checkpoint tool invocation
 *
 * Saves are serialized through a promise chain so concurrent triggers cannot
 * interleave writes. Failures are logged but never thrown back into the turn.
 */

import type { Message } from '@odysseythink/kosong';
import { join } from 'pathe';

import type { Agent } from '#agent';
import type { AgentRecord } from '#agent/records';
import type { Logger } from '@odysseythink/agent-core-shared';
import type { Session } from '..';
import type { SessionMarkdownExport } from '../export/markdown-export';
import { CheckpointBackupStore } from './backup-store';
import { CheckpointIndex } from './checkpoint-index';
import { SessionCheckpoint, type SessionCheckpointPayload } from './checkpoint';
import { verifyCheckpointIntegrity } from './integrity';
import { withCheckpointSaveRetry } from './save-retry';

export interface CheckpointCoordinatorOptions {
  readonly session: Session;
  readonly checkpoint: SessionCheckpoint;
  readonly index: CheckpointIndex;
  readonly backupStore?: CheckpointBackupStore | undefined;
  readonly markdownExport?: SessionMarkdownExport | undefined;
  readonly logger?: Logger | undefined;
}

export class CheckpointCoordinator {
  private unsubscribe?: () => void;
  private saveChain = Promise.resolve();
  private saving = false;
  private readonly backupStore?: CheckpointBackupStore;

  constructor(private readonly options: CheckpointCoordinatorOptions) {
    this.backupStore =
      options.backupStore ??
      (options.session.options.id === undefined
        ? undefined
        : new CheckpointBackupStore({
            backupDir: join(options.session.options.homedir, '.ody-code', 'session-state', 'backups'),
            sessionID: options.session.options.id,
          }));
  }

  get isSaving(): boolean {
    return this.saving;
  }

  /** Attach to an agent's record stream. Call with the main agent. */
  attachAgent(agent: Agent): void {
    this.unsubscribe?.();
    this.unsubscribe = agent.records.subscribe((record) => {
      this.onRecord(record);
    });
  }

  /** Detach from the record stream and stop processing triggers. */
  detach(): void {
    this.unsubscribe?.();
    this.unsubscribe = undefined;
  }

  /**
   * Save a checkpoint immediately. Used by the manual /checkpoint tool and
   * by internal triggers.
   */
  checkpointNow(): Promise<void> {
    return this.enqueueSave('manual');
  }

  private onRecord(record: AgentRecord): void {
    if (record.type === 'context.append_message' && this.options.markdownExport !== undefined) {
      void this.options.markdownExport.append(record);
    }

    if (shouldTriggerCheckpoint(record)) {
      void this.enqueueSave(record.type);
    }
  }

  private enqueueSave(trigger: string): Promise<void> {
    const promise = this.saveChain.then(async () => {
      this.saving = true;
      try {
        await this.saveCheckpoint(trigger);
      } catch (error) {
        this.options.logger?.error('Checkpoint save failed', error);
      } finally {
        this.saving = false;
      }
    });
    this.saveChain = promise.catch(() => {
      // Errors are handled above; keep the chain alive.
    });
    return promise;
  }

  private async saveCheckpoint(trigger: string): Promise<void> {
    const main = this.options.session.agents.get('main');
    if (main === undefined) {
      this.options.logger?.warn('Checkpoint skipped: no main agent', { trigger });
      return;
    }

    const payload = buildCheckpointPayload(this.options.session, main);

    const integrity = verifyCheckpointIntegrity(payload, {
      expectedMessageCount: main.context.history.length,
      expectedSessionID: this.options.session.options.id ?? undefined,
    });
    if (!integrity.valid) {
      this.options.logger?.warn('Checkpoint integrity check failed', {
        trigger,
        errors: integrity.errors,
      });
    }

    await withCheckpointSaveRetry(
      () => this.options.checkpoint.save(payload),
      async () => {
        if (this.backupStore !== undefined) {
          await this.backupStore.freeOldest(1);
        }
      },
      { logger: this.options.logger },
    );

    let versionPath = this.options.checkpoint.path;
    if (this.backupStore !== undefined) {
      versionPath = await withCheckpointSaveRetry(
        () => this.backupStore!.save(payload),
        () => this.backupStore!.freeOldest(1).then(() => {}),
        { logger: this.options.logger },
      );
    }

    const indexData = await this.options.index.load();
    const lastValidParent =
      indexData.versions.find((v) => v.valid)?.path ?? null;

    await this.options.index.update({
      path: versionPath,
      timestamp: payload.lastUpdatedAt,
      messageCount: payload.messages.length,
      valid: integrity.valid,
      lastValidParent,
    });
  }
}

function shouldTriggerCheckpoint(record: AgentRecord): boolean {
  if (record.type === 'session_mode.exit') {
    return true;
  }
  if (record.type === 'context.append_loop_event') {
    return record.event.type === 'step.end';
  }
  return false;
}

function buildCheckpointPayload(session: Session, main: Agent): SessionCheckpointPayload {
  const kind = main.sessionMode.isActive ? main.sessionMode.kind : 'normal';
  const messages = main.context.history.slice() as Message[];
  return {
    sessionID: session.options.id ?? 'unknown',
    createdAt: session.metadata.createdAt,
    lastUpdatedAt: new Date().toISOString(),
    currentMode: kind,
    messages,
    designModeContext: {
      sessions: main.sessionMode.designSessions.slice(),
    },
    toolCallIndex: buildToolCallIndex(messages),
  };
}

function buildToolCallIndex(messages: Message[]): SessionCheckpointPayload['toolCallIndex'] {
  const callIdToResult: Record<string, true> = {};
  for (const message of messages) {
    if (message.role === 'tool' && typeof message.toolCallId === 'string' && message.toolCallId.length > 0) {
      callIdToResult[message.toolCallId] = true;
    }
  }
  return { callIdToResult };
}
