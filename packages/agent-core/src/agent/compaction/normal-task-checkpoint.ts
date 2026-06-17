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

/**
 * Titles that look like a test-writing task, used to suggest an independent
 * adversarial review of the tests the model just wrote (judge ≠ athlete).
 * Matches English (`test`/`tests`/`spec`) and Chinese (`测试`).
 */
const TEST_TASK_TITLE_RE = /\btests?\b|\bspec\b|测试/i;

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

    // E2E auto-trigger: when a completed todo explicitly asks for E2E tests,
    // inject a reminder. Runs before the `hasWork` guard so it fires even on the
    // final (all-done) step. Only active when the user has enabled E2E testing.
    if (crossedBoundary) {
      const e2eConfig = this.agent.kimiConfig?.e2e;
      const e2eEnabled = e2eConfig !== undefined && e2eConfig.enabled === true;
      const anyE2eDone = e2eEnabled && todos.some(t => {
        if (t.status !== 'done') return false;
        return t.title.toLowerCase().includes('e2e');
      });
      if (anyE2eDone) {
        try {
          this.agent.context.appendSystemReminder(
            'The E2E task is complete. Call RunE2ETests to validate your changes.',
            { kind: 'system_trigger', name: 'e2e_reminder' },
          );
        } catch {
          // best-effort
        }
      }

      // Test-review auto-trigger: the same model wrote both the tests and the
      // code under test, so its blind spots are correlated — a tautological or
      // happy-path-only test passes while proving nothing. When a test-writing
      // task completes and the feature is enabled, suggest an independent
      // adversarial audit of those tests. Opt-in, non-blocking.
      // Default-on: fire unless the user explicitly set `enabled = false`.
      const testReviewEnabled = this.agent.kimiConfig?.testReview?.enabled !== false;
      const anyTestTaskDone = testReviewEnabled && todos.some(t => {
        if (t.status !== 'done') return false;
        return TEST_TASK_TITLE_RE.test(t.title);
      });
      if (anyTestTaskDone) {
        try {
          this.agent.context.appendSystemReminder(
            'A task that looks test-related just completed. If it changed tests, call ReviewTests to have ' +
              'an independent model adversarially audit them, then run the mutation probes it returns to ' +
              'prove the tests actually catch regressions. (ReviewTests no-ops if no test files changed.)',
            { kind: 'system_trigger', name: 'test_review_reminder' },
          );
        } catch {
          // best-effort
        }
      }
    }

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
