import { basename } from 'pathe';

import type { SessionModeFilePath } from '../session-mode';
import { DynamicInjector } from './injector';
import {
  designModeFullReminder,
  designModeReentryReminder,
  designModeSparseReminder,
  designSplitContinuationDirective,
  designSplitFinalReviewDirective,
} from './design-mode-contract';
import { type ManifestPart, parsePartsManifest } from './parts-manifest';

const DESIGN_MODE_DEDUP_MIN_TURNS = 2;
const DESIGN_MODE_FULL_REFRESH_TURNS = 5;

export type DesignModeVariant = 'full' | 'sparse' | 'reentry';

export class DesignModeInjector extends DynamicInjector {
  protected override readonly injectionVariant = 'design_mode';
  private wasActive = false;

  override onContextClear(): void {
    super.onContextClear();
    this.wasActive = this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'design';
  }

  override async getInjection(): Promise<string | undefined> {
    const isDesignActive = this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'design';
    const { sessionModeFilePath } = this.agent.sessionMode;
    // Machine signal: ShowDesignMockup is usable only when it is BOTH registered
    // (host advertises openExternal, see ToolManager.initializeBuiltinTools) AND
    // enabled by the active profile — otherwise it never reaches the model's tool
    // list. Checking actual tool visibility (not just openExternal) keeps the
    // prompt from advertising a tool the model can't call.
    const mockupAvailable = this.agent.tools.isToolActive('ShowDesignMockup');

    if (!isDesignActive) {
      if (!this.wasActive) return undefined;
      this.wasActive = false;
      this.injectedAt = null;
      const handoff = this.agent.sessionMode.consumePendingHandoffForPlan();
      if (handoff !== null) {
        return designToPlanHandoffReminder(handoff.content, handoff.path);
      }
      return exitReminder();
    }
    const skillsReminder = this.agent.skills?.registry.getUnavailableSkillsReminder('design') ?? '';
    const content = await this.currentDesignContent();
    if (!this.wasActive) {
      this.injectedAt = null;
      this.wasActive = true;
      if (content.trim().length > 0) {
        const directive = splitDirectiveFor(content, sessionModeFilePath);
        return appendSkillsReminder(designModeReentryReminder(sessionModeFilePath, mockupAvailable, directive), skillsReminder);
      }
    }
    const variant = this.getVariant();
    if (variant === null) return undefined;
    if (variant === 'reentry') return appendSkillsReminder(designModeReentryReminder(sessionModeFilePath, mockupAvailable), skillsReminder);

    const directive = splitDirectiveFor(content, sessionModeFilePath);
    const body = variant === 'full'
      ? designModeFullReminder(sessionModeFilePath, mockupAvailable, directive)
      : designModeSparseReminder(sessionModeFilePath, mockupAvailable, directive);
    return appendSkillsReminder(body, skillsReminder);
  }

  protected getVariant(): DesignModeVariant | null {
    if (this.injectedAt === null) return 'full';
    const history = this.agent.context.history;
    let assistantTurnsSince = 0;
    for (let i = this.injectedAt + 1; i < history.length; i++) {
      const msg = history[i];
      if (msg === undefined) continue;
      if (msg.role === 'assistant') {
        assistantTurnsSince += 1;
        continue;
      }
      if (msg.role === 'user') return 'full';
    }
    if (assistantTurnsSince >= DESIGN_MODE_FULL_REFRESH_TURNS) return 'full';
    if (assistantTurnsSince >= DESIGN_MODE_DEDUP_MIN_TURNS) return 'sparse';
    return null;
  }

  private async currentDesignContent(): Promise<string> {
    try {
      const data = await this.agent.sessionMode.data();
      return data?.content ?? '';
    } catch {
      return '';
    }
  }
}

/**
 * When the current design file is a split index, derive the directive that steers
 * the model to the next pending part (or the cross-file final review once every
 * part is done). Returns undefined for single-file designs (no manifest).
 */
function splitDirectiveFor(content: string, sessionModeFilePath: SessionModeFilePath): string | undefined {
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

function exitReminder(): string {
  return `Design mode was cancelled — no design was approved or handed off. Continue with normal operation.`;
}

function designToPlanHandoffReminder(content: string, path: string): string {
  const savedTo = path ? `Design saved to: ${path}\n\n` : '';
  return `Design mode completed. The approved design has been handed off — you are now in plan mode.\n\n${savedTo}## Approved Design\n\n${content}\n\nCreate a concrete, step-by-step implementation plan based on the approved design above. Do not implement anything yet.`;
}

function appendSkillsReminder(body: string, reminder: string): string {
  return reminder.length > 0 ? `${body}\n\n${reminder}` : body;
}
