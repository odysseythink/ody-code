import type { Agent } from '../../..';
import { DesignModeInjector } from '../../injection/design-mode';
import { BaseSessionModeBehavior } from './base';
import type { ModeEnterContext, ModeExitContext, SessionModeInjector } from './types';

export class DesignModeBehavior extends BaseSessionModeBehavior<'design'> {
  readonly kind = 'design' as const;
  readonly outputSubdirectory = 'designs';
  readonly modeModelKey = 'design';
  readonly injectorClass = DesignModeInjector as unknown as new (agent: Agent) => SessionModeInjector;
  override readonly handoffTarget = 'plan' as const;
  override readonly supportsDesignSessions = true;

  override async onEnter(ctx: ModeEnterContext): Promise<void> {
    await super.onEnter(ctx);
    ctx.agent.sessionMode.startDesignSession(ctx.id);
  }

  /**
   * Called from SessionMode.exit() which is synchronous.
   * Design session state changes must happen inline, not deferred via await.
   */
  override onExit(ctx: ModeExitContext): void {
    // Call super without await so the continuation below runs synchronously
    // eslint-disable-next-line @typescript-eslint/no-floating-promises
    super.onExit(ctx);
    ctx.agent.sessionMode.closeCurrentDesignSession(ctx.sessionModeFilePath ?? undefined);
    if (ctx.sessionModeFilePath !== null) {
      ctx.agent.sessionMode.setLastCompletedDesignFilePath(ctx.sessionModeFilePath);
    }
  }

  /**
   * Called from SessionMode.cancel() which is synchronous.
   * Design session state changes must happen inline, not deferred via await.
   */
  override onCancel(ctx: ModeExitContext): void {
    // Call super without await so the continuation below runs synchronously
    // eslint-disable-next-line @typescript-eslint/no-floating-promises
    super.onCancel(ctx);
    ctx.agent.sessionMode.closeCurrentDesignSession();
  }
}
