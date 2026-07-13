import type { SessionModeFilePath } from '../session-mode';
import type { SessionModeInjectorOptions } from '../session-mode/behaviors';
import { BaseSessionModeInjector } from './session-mode-injector';
import {
  productEntryReminder,
  productExitReminder,
  productFullReminder,
  productReentryReminder,
  productSparseReminder,
} from './product-contract';

export class ProductInjector extends BaseSessionModeInjector {
  readonly injectionVariant = 'office_hours';
  readonly options: SessionModeInjectorOptions = {
    fullRefreshTurns: 5,
    dedupMinTurns: 2,
  };

  isModeActive(): boolean {
    return this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'product';
  }

  protected getEntryReminder(path: SessionModeFilePath): string {
    return productEntryReminder(path);
  }

  protected getReentryReminder(path: SessionModeFilePath): string {
    return productReentryReminder(path);
  }

  protected getFullReminder(path: SessionModeFilePath): string {
    return productFullReminder(path);
  }

  protected getSparseReminder(path: SessionModeFilePath): string {
    return productSparseReminder(path);
  }

  protected getExitReminder(path: SessionModeFilePath): string {
    return productExitReminder(path);
  }
}
