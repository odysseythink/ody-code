import { basename } from 'pathe';

import type { SessionModeFilePath } from '../session-mode';
import type { SessionModeInjectorOptions } from '../session-mode/behaviors';
import { BaseSessionModeInjector } from './session-mode-injector';
import {
  designModeFullReminder,
  designModeReentryReminder,
  designModeSparseReminder,
  designSplitContinuationDirective,
  designSplitFinalReviewDirective,
} from './design-mode-contract';
import { type ManifestPart, parsePartsManifest } from './parts-manifest';

export class DesignModeInjector extends BaseSessionModeInjector {
  readonly injectionVariant = 'design_mode';
  readonly options: SessionModeInjectorOptions = {
    fullRefreshTurns: 5,
    dedupMinTurns: 2,
  };

  isModeActive(): boolean {
    return this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'design';
  }

  /** Whether ShowDesignMockup is both registered and enabled in the active profile. */
  private mockupAvailable(): boolean {
    return this.agent.tools.isToolActive('ShowDesignMockup');
  }

  // ── 5 abstract reminder methods ────────────────────────────────────

  /**
   * Called when mode becomes active with empty content.
   * Original code falls through to variant logic which returns a full reminder.
   */
  protected getEntryReminder(path: SessionModeFilePath): string {
    return designModeFullReminder(path, this.mockupAvailable());
  }

  protected getReentryReminder(path: SessionModeFilePath): string {
    const directive = splitDirectiveFor(this.currentContent, path);
    return designModeReentryReminder(path, this.mockupAvailable(), directive);
  }

  protected getFullReminder(path: SessionModeFilePath): string {
    const directive = splitDirectiveFor(this.currentContent, path);
    return designModeFullReminder(path, this.mockupAvailable(), directive);
  }

  protected getSparseReminder(path: SessionModeFilePath): string {
    const directive = splitDirectiveFor(this.currentContent, path);
    return designModeSparseReminder(path, this.mockupAvailable(), directive);
  }

  protected getExitReminder(_path: SessionModeFilePath): string {
    return exitReminder();
  }

  // ── Overrides for handoff and skills ───────────────────────────────

  /**
   * On exit, check for a pending design→plan handoff first.
   * Falls back to the plain exit reminder when no handoff is pending.
   */
  protected override getExitInjection(path: SessionModeFilePath): string | undefined {
    const handoff = this.agent.sessionMode.consumePendingHandoffForPlan();
    if (handoff !== null) {
      return designToPlanHandoffReminder(handoff.path, handoff.filename, handoff.selectedLabel);
    }
    return super.getExitInjection(path);
  }

  /** Append the unavailable-skills reminder to every injection body. */
  protected override decorateReminder(body: string): string {
    const skillsReminder =
      this.agent.skills?.registry.getUnavailableSkillsReminder('design') ?? '';
    if (skillsReminder.length === 0) return body;
    return `${body}\n\n${skillsReminder}`;
  }
}

// ── Helper functions (unchanged from original) ──────────────────────

function exitReminder(): string {
  return `Design mode was cancelled — no design was approved or handed off. Continue with normal operation.`;
}

function designToPlanHandoffReminder(
  path: string,
  filename: string,
  selectedLabel?: string,
): string {
  const savedTo = path ? `Design saved to: ${path}\n\n` : '';
  const selectedLabelPrefix =
    selectedLabel !== undefined && selectedLabel.length > 0
      ? `Selected approach: ${selectedLabel}. Execute ONLY the selected approach; do not execute any unselected alternatives.\n\n`
      : '';
  return `Design mode completed. The approved design has been handed off — you are now in plan mode.\n\n${savedTo}${selectedLabelPrefix}Create a concrete, step-by-step implementation plan based on the approved design in \`${filename}\`. Do not implement anything yet.`;
}

/**
 * When the current design file is a split index, derive the directive that steers
 * the model to the next pending part (or the cross-file final review once every
 * part is done). Returns undefined for single-file designs (no manifest).
 */
function splitDirectiveFor(
  content: string,
  sessionModeFilePath: SessionModeFilePath,
): string | undefined {
  const manifest = parsePartsManifest(content);
  if (manifest === null) return undefined;
  if (manifest.next !== null) {
    const next: ManifestPart = manifest.next;
    return designSplitContinuationDirective(next, indexStemFor(sessionModeFilePath));
  }
  if (manifest.allDone) return designSplitFinalReviewDirective();
  return undefined;
}

/** The index file's stem (filename without the `.md`), used as the split subdirectory name. */
function indexStemFor(sessionModeFilePath: SessionModeFilePath): string {
  if (sessionModeFilePath === null || sessionModeFilePath.length === 0) return '';
  return basename(sessionModeFilePath).replace(/\.md$/, '');
}
