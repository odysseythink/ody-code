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
      return exitReminder();
    }
    const content = await this.currentDesignContent();
    if (!this.wasActive) {
      this.injectedAt = null;
      this.wasActive = true;
      if (content.trim().length > 0) {
        const directive = splitDirectiveFor(content, sessionModeFilePath);
        return designModeReentryReminder(sessionModeFilePath, mockupAvailable, directive);
      }
    }
    const variant = this.getVariant();
    if (variant === null) return undefined;
    if (variant === 'reentry') return designModeReentryReminder(sessionModeFilePath, mockupAvailable);

    const directive = splitDirectiveFor(content, sessionModeFilePath);
    return variant === 'full'
      ? designModeFullReminder(sessionModeFilePath, mockupAvailable, directive)
      : designModeSparseReminder(sessionModeFilePath, mockupAvailable, directive);
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
  return `Design mode is no longer active. The design has been approved. STOP — do NOT begin implementing, writing, or editing code now. Your ONLY next action is to recommend the user run /plan to turn the approved design into a concrete implementation plan, then wait for them. Implementation happens after a plan is approved, not here.`;
}
