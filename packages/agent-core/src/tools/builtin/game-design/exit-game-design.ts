import type { Agent } from '#agent';
import { z } from 'zod';
import { t } from '../../../i18n';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './exit-game-design.md';

export const ExitGameDesignModeInputSchema = z.object({}).strict();
export type ExitGameDesignModeInput = z.infer<typeof ExitGameDesignModeInputSchema>;

export class ExitGameDesignModeTool implements BuiltinTool<ExitGameDesignModeInput> {
  readonly name = 'ExitGameDesignMode' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(ExitGameDesignModeInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(_args: ExitGameDesignModeInput): ToolExecution {
    return {
      description: 'Requesting to exit game-design mode',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'game-design') {
          return { isError: true, output: t('gameDesign.modeNotActive', lang) };
        }
        const path = this.agent.sessionMode.sessionModeFilePath;
        this.agent.sessionMode.exit();
        const parts = [t('gameDesign.sessionComplete', lang)];
        if (path) {
          parts.push(t('gameDesign.designDocSaved', lang).replace('{path}', path));
        }
        parts.push(t('gameDesign.appWillExit', lang));
        return { output: parts.join('\n') };
      },
    };
  }
}
