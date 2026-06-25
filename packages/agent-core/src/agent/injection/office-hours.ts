import type { SessionModeFilePath } from '../session-mode';
import type { SessionModeInjectorOptions } from '../session-mode/behaviors';
import { BaseSessionModeInjector } from './session-mode-injector';
import {
  officeHoursEntryReminder,
  officeHoursExitReminder,
  officeHoursFullReminder,
  officeHoursReentryReminder,
  officeHoursSparseReminder,
} from './office-hours-contract';

export class OfficeHoursInjector extends BaseSessionModeInjector {
  readonly injectionVariant = 'office_hours';
  readonly options: SessionModeInjectorOptions = {
    fullRefreshTurns: 5,
    dedupMinTurns: 2,
  };

  isModeActive(): boolean {
    return this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'office-hours';
  }

  protected getEntryReminder(path: SessionModeFilePath): string {
    return officeHoursEntryReminder(path);
  }

  protected getReentryReminder(path: SessionModeFilePath): string {
    return officeHoursReentryReminder(path);
  }

  protected getFullReminder(path: SessionModeFilePath): string {
    return officeHoursFullReminder(path);
  }

  protected getSparseReminder(path: SessionModeFilePath): string {
    return officeHoursSparseReminder(path);
  }

  protected getExitReminder(path: SessionModeFilePath): string {
    return officeHoursExitReminder(path);
  }
}
