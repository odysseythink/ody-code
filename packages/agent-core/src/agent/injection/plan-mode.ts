import { basename } from 'pathe';

import type { SessionModeFilePath } from '../session-mode';
import type { SessionModeInjectorOptions } from '../session-mode/behaviors';
import { BaseSessionModeInjector } from './session-mode-injector';
import {
  type ManifestPart,
  parsePartsManifest,
  planModeFullReminder,
  planModeReentryReminder,
  planModeSparseReminder,
  splitContinuationDirective,
  splitFinalReviewDirective,
} from './plan-mode-contract';

export class PlanModeInjector extends BaseSessionModeInjector {
  readonly injectionVariant = 'plan_mode';
  readonly options: SessionModeInjectorOptions = {
    fullRefreshTurns: 5,
    dedupMinTurns: 2,
  };

  isModeActive(): boolean {
    return this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'plan';
  }

  // ── 5 abstract reminder methods ────────────────────────────────────

  /**
   * Called when mode becomes active with empty content.
   * Original code falls through to the variant logic which returns a full
   * reminder, so delegate to getFullReminder without split directive.
   */
  protected getEntryReminder(path: SessionModeFilePath): string {
    return planModeFullReminder(path);
  }

  protected getReentryReminder(path: SessionModeFilePath): string {
    return planModeReentryReminder(path);
  }

  protected getFullReminder(path: SessionModeFilePath): string {
    const directive = splitDirectiveFor(this.currentContent, path);
    return planModeFullReminder(path, directive);
  }

  protected getSparseReminder(path: SessionModeFilePath): string {
    const directive = splitDirectiveFor(this.currentContent, path);
    return planModeSparseReminder(path, directive);
  }

  protected getExitReminder(_path: SessionModeFilePath): string {
    return exitReminder();
  }

  // ── Overrides for handoff and skills ───────────────────────────────

  /**
   * On exit, check for a pending plan→normal handoff first.
   * Falls back to the plain exit reminder when no handoff is pending.
   */
  protected override getExitInjection(path: SessionModeFilePath): string | undefined {
    const handoff = this.agent.sessionMode.consumePendingHandoffForNormal();
    if (handoff !== null) {
      return planToNormalHandoffReminder(handoff.content, handoff.path, handoff.selectedLabel);
    }
    return super.getExitInjection(path);
  }

  /** Append the unavailable-skills reminder to every injection body. */
  protected override decorateReminder(body: string): string {
    const skillsReminder =
      this.agent.skills?.registry.getUnavailableSkillsReminder('plan') ?? '';
    if (skillsReminder.length === 0) return body;
    return `${body}\n\n${skillsReminder}`;
  }
}

// ── Helper functions (unchanged from original) ──────────────────────

function exitReminder(): string {
  return `Plan mode was cancelled — no plan was approved or handed off. The read-only and plan-file-only restrictions no longer apply. Continue with normal operation.`;
}

function planToNormalHandoffReminder(
  content: string,
  path: string,
  selectedLabel?: string,
): string {
  const savedTo = path ? `Plan saved to: ${path}\n\n` : '';
  const optionPrefix =
    selectedLabel !== undefined && selectedLabel.length > 0
      ? `Selected approach: ${selectedLabel}. Implement ONLY this approach; do not execute any unselected alternatives.\n\n`
      : '';
  return `Plan mode is no longer active. The approved plan has been handed off to this context.\n\n${optionPrefix}${savedTo}## Approved Plan\n\n${content}\n\nProceed with implementing the plan above using the normal tool and permission rules.`;
}

/**
 * When the current plan file is a split index, derive the directive that steers
 * the model to the next pending part (or the cross-file final review once every
 * part is done). Returns undefined for single-file plans (no manifest).
 */
function splitDirectiveFor(
  content: string,
  sessionModeFilePath: SessionModeFilePath,
): string | undefined {
  const manifest = parsePartsManifest(content);
  if (manifest === null) return undefined;
  if (manifest.next !== null) {
    const next: ManifestPart = manifest.next;
    return splitContinuationDirective(next, indexStemFor(sessionModeFilePath));
  }
  if (manifest.allDone) return splitFinalReviewDirective();
  return undefined;
}

/** The index file's stem (filename without the `.md`), used as the split subdirectory name. */
function indexStemFor(sessionModeFilePath: SessionModeFilePath): string {
  if (sessionModeFilePath === null || sessionModeFilePath.length === 0) return '';
  return basename(sessionModeFilePath).replace(/\.md$/, '');
}
