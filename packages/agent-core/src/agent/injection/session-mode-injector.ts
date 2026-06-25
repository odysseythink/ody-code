import type { SessionModeFilePath } from '../session-mode';
import type { SessionModeInjectorOptions } from '../session-mode/behaviors';
import { DynamicInjector } from './injector';

export abstract class BaseSessionModeInjector extends DynamicInjector {
  protected wasActive = false;
  protected currentContent = '';

  abstract override readonly injectionVariant: string;
  abstract readonly options: SessionModeInjectorOptions;

  abstract isModeActive(): boolean;

  override onContextClear(): void {
    super.onContextClear();
    this.wasActive = this.isModeActive();
  }

  override async getInjection(): Promise<string | undefined> {
    const active = this.isModeActive();
    const path = this.agent.sessionMode.sessionModeFilePath as SessionModeFilePath;

    if (!active) {
      if (!this.wasActive) {
        return undefined;
      }
      this.wasActive = false;
      this.injectedAt = null;
      const exitReminder = this.getExitInjection(path);
      if (exitReminder === undefined) return undefined;
      return this.decorateReminder(exitReminder);
    }

    this.currentContent = await this.readModeContent();

    if (!this.wasActive) {
      this.injectedAt = null;
      this.wasActive = true;
      const entryReminder = this.getEntryInjection(this.currentContent, path);
      if (entryReminder === undefined) return undefined;
      return this.decorateReminder(entryReminder);
    }

    const variant = this.computeVariant(
      this.injectedAt,
      this.agent.context.history,
      this.options,
    );
    if (variant === null) {
      return undefined;
    }
    if (variant === 'full') {
      return this.decorateReminder(this.getFullReminder(path));
    }
    return this.decorateReminder(this.getSparseReminder(path));
  }

  protected readModeContent(): Promise<string> {
    try {
      return this.agent.sessionMode
        .data()
        .then((data: { content?: string } | null) => data?.content ?? '');
    } catch {
      return Promise.resolve('');
    }
  }

  protected computeVariant(
    injectedAt: number | null,
    history: readonly { role: string }[],
    options: SessionModeInjectorOptions,
  ): 'full' | 'sparse' | null {
    if (injectedAt === null) {
      return 'full';
    }
    let assistantTurnsSince = 0;
    for (let i = injectedAt + 1; i < history.length; i++) {
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
    if (assistantTurnsSince >= options.fullRefreshTurns) {
      return 'full';
    }
    if (assistantTurnsSince >= options.dedupMinTurns) {
      return 'sparse';
    }
    return null;
  }

  protected getEntryInjection(
    content: string,
    path: SessionModeFilePath,
  ): string | undefined {
    if (content.trim().length > 0) {
      return this.getReentryReminder(path);
    }
    return this.getEntryReminder(path);
  }

  protected getExitInjection(path: SessionModeFilePath): string | undefined {
    return this.getExitReminder(path);
  }

  protected decorateReminder(body: string): string {
    return body;
  }

  protected abstract getEntryReminder(path: SessionModeFilePath): string;
  protected abstract getReentryReminder(path: SessionModeFilePath): string;
  protected abstract getFullReminder(path: SessionModeFilePath): string;
  protected abstract getSparseReminder(path: SessionModeFilePath): string;
  protected abstract getExitReminder(path: SessionModeFilePath): string;
}
