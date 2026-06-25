import { ensureGitignore } from '../../../utils/gitignore';
import type { Agent } from '../../..';
import type { SessionModeKind } from '../types';
import { resolveSessionModeDirectory } from '../directory';
import { modelAliasHasUsableAuth } from '../model-auth';
import type { ModeEnterContext, ModeExitContext, SessionModeBehavior, SessionModeInjector } from './types';

export abstract class BaseSessionModeBehavior<TKind extends SessionModeKind>
  implements SessionModeBehavior<TKind> {
  abstract readonly kind: TKind;
  abstract readonly outputSubdirectory: string;
  abstract readonly modeModelKey: string;
  abstract readonly injectorClass: new (agent: Agent) => SessionModeInjector;
  readonly handoffTarget?: 'plan' | 'normal';
  readonly supportsDesignSessions?: boolean;

  onEnter(ctx: ModeEnterContext): Promise<void> | void {
    return this.doEnter(ctx);
  }

  onExit(_ctx: ModeExitContext): Promise<void> | void {}
  onCancel(_ctx: ModeExitContext): Promise<void> | void {}

  private async doEnter(ctx: ModeEnterContext): Promise<void> {
    const { isProjectScoped } = await resolveSessionModeDirectory(ctx.agent, this.kind);
    if (isProjectScoped) {
      try {
        await ensureGitignore(ctx.agent.config.cwd, ctx.agent.kaos);
      } catch (error) {
        ctx.agent.log?.warn('Failed to update .gitignore', { error });
      }
    }

    const modeModelAlias = ctx.agent.kimiConfig?.modeModels?.[this.modeModelKey as keyof typeof ctx.agent.kimiConfig.modeModels];
    if (modeModelAlias !== undefined) {
      let resolved;
      let usable = false;
      try {
        resolved = ctx.agent.modelProvider?.resolveProviderConfig(modeModelAlias);
        usable = resolved === undefined || modelAliasHasUsableAuth(ctx.agent, modeModelAlias, resolved);
      } catch {
        ctx.agent.log?.warn(`modeModels.${this.modeModelKey} "${modeModelAlias}" not found, keeping current model`);
      }
      if (usable && modeModelAlias !== ctx.agent.config.modelAlias) {
        ctx.agent.config.update({ modelAlias: modeModelAlias });
        ctx.agent.refreshLlm();
      }
    }
  }
}
