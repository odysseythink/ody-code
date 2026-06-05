import { DynamicInjector } from './injector';
import {
  designModeFullReminder,
  designModeReentryReminder,
  designModeSparseReminder,
} from './design-mode-contract';

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
    if (!this.wasActive) {
      this.injectedAt = null;
      this.wasActive = true;
      if (await this.hasCurrentDesignContent()) {
        return designModeReentryReminder(sessionModeFilePath, mockupAvailable);
      }
    }
    const variant = this.getVariant();
    if (variant === null) return undefined;
    return variant === 'full'
      ? designModeFullReminder(sessionModeFilePath, mockupAvailable)
      : variant === 'sparse'
        ? designModeSparseReminder(sessionModeFilePath, mockupAvailable)
        : designModeReentryReminder(sessionModeFilePath, mockupAvailable);
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

  private async hasCurrentDesignContent(): Promise<boolean> {
    try {
      const data = await this.agent.sessionMode.data();
      return data !== null && data.content.trim().length > 0;
    } catch {
      return false;
    }
  }
}

function exitReminder(): string {
  return `Design mode is no longer active. The design has been approved. STOP — do NOT begin implementing, writing, or editing code now. Your ONLY next action is to recommend the user run /plan to turn the approved design into a concrete implementation plan, then wait for them. Implementation happens after a plan is approved, not here.`;
}
