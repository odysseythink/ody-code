import { basename } from 'pathe';

import type { SessionModeFilePath } from '../session-mode';
import { DynamicInjector } from './injector';
import {
  type ManifestPart,
  parsePartsManifest,
  planModeFullReminder,
  planModeReentryReminder,
  planModeSparseReminder,
  splitContinuationDirective,
  splitFinalReviewDirective,
} from './plan-mode-contract';

const PLAN_MODE_DEDUP_MIN_TURNS = 2;
const PLAN_MODE_FULL_REFRESH_TURNS = 5;

/**
 * Plan-mode reminder variants.
 *
 * `reentry` is used once when a restored planning session already has plan
 * content. `full` is used for the first reminder and periodic refreshes.
 * `sparse` keeps the read-only invariant visible between full reminders.
 */
export type PlanModeVariant = 'full' | 'sparse' | 'reentry';

export class PlanModeInjector extends DynamicInjector {
  protected override readonly injectionVariant = 'plan_mode';
  private wasActive = false;

  override onContextClear(): void {
    super.onContextClear();
    this.wasActive = this.agent.sessionMode.isActive && this.agent.sessionMode.kind !== 'design';
  }

  override async getInjection(): Promise<string | undefined> {
    const isPlanActive = this.agent.sessionMode.isActive && this.agent.sessionMode.kind !== 'design';
    const { sessionModeFilePath } = this.agent.sessionMode;
    if (!isPlanActive) {
      if (!this.wasActive) {
        return undefined;
      }
      this.wasActive = false;
      this.injectedAt = null;
      const handoff = this.agent.sessionMode.consumePendingHandoffForNormal();
      if (handoff !== null) {
        return planToNormalHandoffReminder(handoff.content, handoff.path);
      }
      return exitReminder();
    }
    const skillsReminder = this.agent.skills?.registry.getUnavailableSkillsReminder('plan') ?? '';
    const content = await this.currentPlanContent();
    if (!this.wasActive) {
      this.injectedAt = null;
      this.wasActive = true;
      if (content.trim().length > 0) {
        return appendSkillsReminder(planModeReentryReminder(sessionModeFilePath), skillsReminder);
      }
    }
    const variant = this.getVariant();
    if (variant === null) return undefined;
    if (variant === 'reentry') return appendSkillsReminder(planModeReentryReminder(sessionModeFilePath), skillsReminder);

    const directive = splitDirectiveFor(content, sessionModeFilePath);
    const body = variant === 'full'
      ? planModeFullReminder(sessionModeFilePath, directive)
      : planModeSparseReminder(sessionModeFilePath, directive);
    return appendSkillsReminder(body, skillsReminder);
  }

  protected getVariant(): PlanModeVariant | null {
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
      if (msg.role === 'user') {
        return 'full';
      }
    }
    if (assistantTurnsSince >= PLAN_MODE_FULL_REFRESH_TURNS) return 'full';
    if (assistantTurnsSince >= PLAN_MODE_DEDUP_MIN_TURNS) return 'sparse';
    return null;
  }

  private async currentPlanContent(): Promise<string> {
    try {
      const data = await this.agent.sessionMode.data();
      return data?.content ?? '';
    } catch {
      return '';
    }
  }
}

/**
 * When the current plan file is a split index, derive the directive that steers
 * the model to the next pending part (or the cross-file final review once every
 * part is done). Returns undefined for single-file plans (no manifest).
 */
function splitDirectiveFor(content: string, sessionModeFilePath: SessionModeFilePath): string | undefined {
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

function exitReminder(): string {
  return `Plan mode is no longer active. The read-only and plan-file-only restrictions from plan mode no longer apply. Continue with the approved plan using the normal tool and permission rules.`;
}

function planToNormalHandoffReminder(content: string, path: string): string {
  const savedTo = path ? `Plan saved to: ${path}\n\n` : '';
  return `Plan mode is no longer active. The approved plan has been handed off to this context.\n\n${savedTo}## Approved Plan\n\n${content}\n\nProceed with implementing the plan above using the normal tool and permission rules.`;
}

function appendSkillsReminder(body: string, reminder: string): string {
  return reminder.length > 0 ? `${body}\n\n${reminder}` : body;
}
