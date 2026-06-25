import type { Agent } from '../../..';
import type { SessionModeKind } from '../types';

export interface ModeEnterContext {
  agent: Agent;
  id: string;
  restoreTargetAlias: string | undefined;
}

export interface ModeExitContext {
  agent: Agent;
  id?: string;
  sessionModeFilePath: string | null;
}

export interface SessionModeInjector {
  readonly injectionVariant: string;
  onContextClear(): void;
  onContextCompacted(compactedCount: number): void;
  onContextMessageRemoved(index: number): void;
  inject(): Promise<void>;
  getInjection(): string | Promise<string | undefined> | undefined;
}

export interface SessionModeInjectorOptions {
  fullRefreshTurns: number;
  dedupMinTurns: number;
}

export interface SessionModeBehavior<TKind extends SessionModeKind> {
  readonly kind: TKind;
  readonly outputSubdirectory: string;
  readonly modeModelKey: string;
  readonly injectorClass: new (agent: Agent) => SessionModeInjector;
  readonly handoffTarget?: 'plan' | 'normal';
  readonly supportsDesignSessions?: boolean;

  onEnter(ctx: ModeEnterContext): Promise<void> | void;
  onExit(ctx: ModeExitContext): Promise<void> | void;
  onCancel(ctx: ModeExitContext): Promise<void> | void;
}
