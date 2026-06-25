import type { SessionModeFilePath } from '../session-mode';
import type { SessionModeInjectorOptions } from '../session-mode/behaviors';
import { BaseSessionModeInjector } from './session-mode-injector';
import {
  gameDesignEntryReminder,
  gameDesignExitReminder,
  gameDesignFullReminder,
  gameDesignReentryReminder,
  gameDesignSparseReminder,
} from './game-design-contract';

export class GameDesignInjector extends BaseSessionModeInjector {
  readonly injectionVariant = 'game_design';
  readonly options: SessionModeInjectorOptions = {
    fullRefreshTurns: 5,
    dedupMinTurns: 2,
  };

  isModeActive(): boolean {
    return (
      this.agent.sessionMode.isActive &&
      this.agent.sessionMode.kind === 'game-design'
    );
  }

  protected getEntryReminder(path: SessionModeFilePath): string {
    return gameDesignEntryReminder(path);
  }

  protected getReentryReminder(path: SessionModeFilePath): string {
    return gameDesignReentryReminder(path);
  }

  protected getFullReminder(path: SessionModeFilePath): string {
    return gameDesignFullReminder(path);
  }

  protected getSparseReminder(path: SessionModeFilePath): string {
    return gameDesignSparseReminder(path);
  }

  protected getExitReminder(path: SessionModeFilePath): string {
    return gameDesignExitReminder(path);
  }
}
