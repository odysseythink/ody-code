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

import type { Agent } from '#/agent';
import type { AgentRecord } from '#/agent/records';
import type { Logger } from '#/logging/types';
import type { Session } from '..';
import type { SessionMarkdownExport } from '../export/markdown-export';
import { CheckpointIndex } from './checkpoint-index';
import { SessionCheckpoint, type SessionCheckpointPayload } from './checkpoint';

export interface CheckpointCoordinatorOptions {
  readonly session: Session;
  readonly checkpoint: SessionCheckpoint;
  readonly index: CheckpointIndex;
  readonly markdownExport?: SessionMarkdownExport | undefined;
  readonly logger?: Logger | undefined;
}

export class CheckpointCoordinator {
  private unsubscribe?: () => void;
  private saveChain = Promise.resolve();
  private saving = false;

  constructor(private readonly options: CheckpointCoordinatorOptions) {}

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
    await this.options.checkpoint.save(payload);

    const indexData = await this.options.index.load();
    const lastValidParent =
      indexData.versions.find((v) => v.valid)?.path ?? null;

    await this.options.index.update({
      path: this.options.checkpoint.path,
      timestamp: payload.lastUpdatedAt,
      messageCount: payload.messages.length,
      valid: true,
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
  return {
    sessionID: session.options.id ?? 'unknown',
    createdAt: session.metadata.createdAt,
    lastUpdatedAt: new Date().toISOString(),
    currentMode: kind,
    messages: main.context.history.slice(),
    designModeContext: {
      sessions: main.sessionMode.designSessions.slice(),
    },
    toolCallIndex: {
      callIdToResult: {},
    },
  };
}
