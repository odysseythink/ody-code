import type { Agent } from '#agent';
import { z } from 'zod';
import { gameDesignEntryReminder } from '#agent/injection/game-design-contract';
import { t } from '../../../i18n';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './enter-game-design.md';

export const EnterGameDesignModeInputSchema = z.object({}).strict();
export type EnterGameDesignModeInput = z.infer<typeof EnterGameDesignModeInputSchema>;

export class EnterGameDesignModeTool implements BuiltinTool<EnterGameDesignModeInput> {
  readonly name = 'EnterGameDesignMode' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(EnterGameDesignModeInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(_args: EnterGameDesignModeInput): ToolExecution {
    return {
      description: 'Requesting to enter game-design mode',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (this.agent.sessionMode.isActive) {
          if (this.agent.sessionMode.kind === 'game-design') {
            return { isError: true, output: t('gameDesign.alreadyActive', lang) };
          }
          return { isError: true, output: t('gameDesign.anotherModeActive', lang) };
        }
        try {
          await this.agent.sessionMode.enter(undefined, undefined, undefined, 'game-design');
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to enter game-design mode.';
          return {
            isError: true,
            output: t('gameDesign.failedToEnter', lang).replace('{message}', message),
          };
        }
        return {
          output: gameDesignEntryReminder(this.agent.sessionMode.sessionModeFilePath),
        };
      },
    };
  }
}
