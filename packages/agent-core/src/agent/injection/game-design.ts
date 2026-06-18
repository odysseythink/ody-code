import { DynamicInjector } from './injector';
import {
  gameDesignEntryReminder,
  gameDesignExitReminder,
  gameDesignFullReminder,
  gameDesignReentryReminder,
  gameDesignSparseReminder,
} from './game-design-contract';

const GAME_DESIGN_DEDUP_MIN_TURNS = 2;
const GAME_DESIGN_FULL_REFRESH_TURNS = 5;

export class GameDesignInjector extends DynamicInjector {
  protected override readonly injectionVariant = 'game_design';
  private wasActive = false;

  override onContextClear(): void {
    super.onContextClear();
    this.wasActive =
      this.agent.sessionMode.isActive &&
      this.agent.sessionMode.kind === 'game-design';
  }

  override async getInjection(): Promise<string | undefined> {
    const isActive =
      this.agent.sessionMode.isActive &&
      this.agent.sessionMode.kind === 'game-design';
    const { sessionModeFilePath } = this.agent.sessionMode;

    if (!isActive) {
      if (!this.wasActive) return undefined;
      this.wasActive = false;
      this.injectedAt = null;
      return gameDesignExitReminder(sessionModeFilePath);
    }

    if (!this.wasActive) {
      this.injectedAt = null;
      this.wasActive = true;
      const content = await this.currentGameDesignContent();
      if (content.trim().length > 0) {
        return gameDesignReentryReminder(sessionModeFilePath);
      }
      return gameDesignEntryReminder(sessionModeFilePath);
    }

    const variant = this.getVariant();
    if (variant === null) return undefined;
    return variant === 'full'
      ? gameDesignFullReminder(sessionModeFilePath)
      : gameDesignSparseReminder(sessionModeFilePath);
  }

  protected getVariant(): 'full' | 'sparse' | null {
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
    if (assistantTurnsSince >= GAME_DESIGN_FULL_REFRESH_TURNS) return 'full';
    if (assistantTurnsSince >= GAME_DESIGN_DEDUP_MIN_TURNS) return 'sparse';
    return null;
  }

  private async currentGameDesignContent(): Promise<string> {
    try {
      const data = await this.agent.sessionMode.data();
      return data?.content ?? '';
    } catch {
      return '';
    }
  }
}
