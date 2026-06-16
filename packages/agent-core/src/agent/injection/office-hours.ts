import { DynamicInjector } from './injector';
import {
  officeHoursEntryReminder,
  officeHoursExitReminder,
  officeHoursFullReminder,
  officeHoursReentryReminder,
  officeHoursSparseReminder,
} from './office-hours-contract';

const OFFICE_HOURS_DEDUP_MIN_TURNS = 2;
const OFFICE_HOURS_FULL_REFRESH_TURNS = 5;

export class OfficeHoursInjector extends DynamicInjector {
  protected override readonly injectionVariant = 'office_hours';
  private wasActive = false;

  override onContextClear(): void {
    super.onContextClear();
    this.wasActive =
      this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'office-hours';
  }

  override async getInjection(): Promise<string | undefined> {
    const isActive =
      this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'office-hours';
    const { sessionModeFilePath } = this.agent.sessionMode;

    if (!isActive) {
      if (!this.wasActive) return undefined;
      this.wasActive = false;
      this.injectedAt = null;
      return officeHoursExitReminder(sessionModeFilePath);
    }

    if (!this.wasActive) {
      this.injectedAt = null;
      this.wasActive = true;
      return officeHoursEntryReminder(sessionModeFilePath);
    }

    const variant = this.getVariant();
    if (variant === null) return undefined;
    if (variant === 'reentry') return officeHoursReentryReminder(sessionModeFilePath);
    return variant === 'full'
      ? officeHoursFullReminder(sessionModeFilePath)
      : officeHoursSparseReminder(sessionModeFilePath);
  }

  protected getVariant(): 'full' | 'sparse' | 'reentry' | null {
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
    if (assistantTurnsSince >= OFFICE_HOURS_FULL_REFRESH_TURNS) return 'full';
    if (assistantTurnsSince >= OFFICE_HOURS_DEDUP_MIN_TURNS) return 'sparse';
    return null;
  }
}
