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
      return exitReminder();
    }
    const content = await this.currentPlanContent();
    if (!this.wasActive) {
      this.injectedAt = null;
      this.wasActive = true;
      if (content.trim().length > 0) {
        return planModeReentryReminder(sessionModeFilePath);
      }
    }
    const variant = this.getVariant();
    if (variant === null) return undefined;
    if (variant === 'reentry') return planModeReentryReminder(sessionModeFilePath);

    const directive = splitDirectiveFor(content);
    return variant === 'full'
      ? planModeFullReminder(sessionModeFilePath, directive)
      : planModeSparseReminder(sessionModeFilePath, directive);
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
function splitDirectiveFor(content: string): string | undefined {
  const manifest = parsePartsManifest(content);
  if (manifest === null) return undefined;
  if (manifest.next !== null) {
    const next: ManifestPart = manifest.next;
    return splitContinuationDirective(next);
  }
  if (manifest.allDone) return splitFinalReviewDirective();
  return undefined;
}

function exitReminder(): string {
  return `Plan mode is no longer active. The read-only and plan-file-only restrictions from plan mode no longer apply. Continue with the approved plan using the normal tool and permission rules.`;
}
