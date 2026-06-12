/**
 * normal-task-checkpoint.ts — opportunistic compaction at TodoList task boundaries.
 *
 * When the agent executes a plan in normal mode (via executing-plans or similar),
 * it iterates through tasks using TodoList, marking each in_progress → done. For
 * multi-task plans this is a single continuous turn, so context grows unbounded.
 * Each completed task is a SAFE place to compact: the todo state is captured in
 * storeData()['todo'], which persists across compaction, and the agent re-reads
 * it on the next step.
 *
 * On every `beforeStep` (between global compaction and injection), this detects
 * when the `done` count increases since the previous step — a task just finished —
 * and, if context usage crossed the configured ratio, runs a blocking compaction
 * before the next task begins.
 */

import type { Agent } from '..';
import type { TodoItem } from '../../tools/builtin/state/todo-list';

/** Fraction of the model context window at which a task boundary triggers compaction. */
export const DEFAULT_NORMAL_TASK_COMPACTION_RATIO = 0.5;

/** Store key for the todo list — matches todo-list.ts TODO_STORE_KEY. */
const TODO_STORE_KEY = 'todo';

export class NormalModeTaskCheckpoint {
  /** The `done` count last observed, or null when not tracking. */
  private lastDoneCount: number | null = null;

  constructor(private readonly agent: Agent) {}

  /** Forget the observed todo state (e.g. on context clear / mode transition). */
  reset(): void {
    this.lastDoneCount = null;
  }

  async beforeStep(signal: AbortSignal): Promise<void> {
    const ratio =
      this.agent.kimiConfig?.loopControl?.normalTaskCompactionRatio ??
      DEFAULT_NORMAL_TASK_COMPACTION_RATIO;

    // Only active in normal mode (sessionMode.isActive === false); plan/design mode
    // has its own SplitPlanCheckpoint.
    if (ratio <= 0 || this.agent.sessionMode.isActive) {
      this.lastDoneCount = null;
      return;
    }

    const todos = (this.agent.tools.storeData()[TODO_STORE_KEY] as readonly TodoItem[] | undefined) ?? [];
    const doneCount = todos.filter((t) => t.status === 'done').length;
    const hasWork = todos.some((t) => t.status === 'pending' || t.status === 'in_progress');

    const crossedBoundary = this.lastDoneCount !== null && doneCount > this.lastDoneCount;
    this.lastDoneCount = doneCount; // always re-sync to the live state
    // First observation only initializes; a boundary with no remaining work means
    // all tasks are done (final step before plan completion).
    if (!crossedBoundary || !hasWork) return;

    const maxContextTokens = this.agent.config.modelCapabilities.max_context_tokens;
    if (maxContextTokens <= 0) return;
    if (this.agent.context.tokenCountWithPending >= maxContextTokens * ratio) {
      await this.agent.fullCompaction.compactCheckpoint(signal);
    }
  }
}
