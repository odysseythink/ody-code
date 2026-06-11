/**
 * split-checkpoint.ts — opportunistic compaction at split-plan/design part boundaries.
 *
 * A large plan/design is written as a split index with a "Parts" manifest (rows
 * `pending`/`done`) plus one file per part. When the model auto-continues through the
 * parts in a single turn, the context grows unbounded. Each completed part is a SAFE
 * place to compact: the durable state (manifest + already-written part files) lives on
 * disk and the split protocol re-reads it on the next turn, so a summary of the prior
 * context loses nothing the model cannot recover.
 *
 * On every `beforeStep` (run between the global compaction check and injection), this
 * detects when the manifest's `done` count grew since the previous step — a part just
 * finished — and, if context usage has crossed the configured ratio, runs a blocking
 * compaction before the next part begins. The post-compaction injection re-emits the
 * plan-mode split directive pointing at the next pending part.
 */

import type { Agent } from '..';
import { countManifestRows } from '../injection/parts-manifest';

/** Fraction of the model context window at which a part boundary triggers compaction. */
export const DEFAULT_SPLIT_PLAN_COMPACTION_RATIO = 0.5;

export class SplitPlanCheckpoint {
  /** The `done` row count last observed, or null when not tracking a manifest. */
  private lastDoneCount: number | null = null;
  /** The split-index file the baseline belongs to, so a different index (e.g. a
   * design→plan handoff or a new session) never inherits a stale `done` count. */
  private lastFilePath: string | null = null;

  constructor(private readonly agent: Agent) {}

  /** Forget the observed manifest state (e.g. on context clear / session reset). */
  reset(): void {
    this.lastDoneCount = null;
    this.lastFilePath = null;
  }

  async beforeStep(signal: AbortSignal): Promise<void> {
    const ratio =
      this.agent.kimiConfig?.loopControl?.splitPlanCompactionRatio ??
      DEFAULT_SPLIT_PLAN_COMPACTION_RATIO;
    const sessionMode = this.agent.sessionMode;
    if (ratio <= 0 || !sessionMode.isActive) {
      this.reset();
      return;
    }

    // A different split index starts its own baseline (its first observation only
    // initializes), so a higher `done` count carried over from another index/mode
    // cannot be mistaken for a boundary crossing here.
    const filePath = sessionMode.sessionModeFilePath ?? null;
    if (filePath !== this.lastFilePath) {
      this.lastDoneCount = null;
      this.lastFilePath = filePath;
    }

    let content: string;
    try {
      content = (await sessionMode.data())?.content ?? '';
    } catch {
      return;
    }

    const counts = countManifestRows(content);
    if (counts === null) {
      // Single-file plan/design (no manifest) — nothing to checkpoint.
      this.lastDoneCount = null;
      return;
    }

    const crossedBoundary = this.lastDoneCount !== null && counts.done > this.lastDoneCount;
    const morePending = counts.pending > 0;
    this.lastDoneCount = counts.done; // always re-sync to the on-disk truth
    // First observation only initializes; a boundary with no pending part left means the
    // split is finishing (the final review / ExitPlanMode handles that, not compaction).
    if (!crossedBoundary || !morePending) return;

    const maxContextTokens = this.agent.config.modelCapabilities.max_context_tokens;
    if (maxContextTokens <= 0) return;
    if (this.agent.context.tokenCountWithPending >= maxContextTokens * ratio) {
      await this.agent.fullCompaction.compactCheckpoint(signal);
    }
  }
}
