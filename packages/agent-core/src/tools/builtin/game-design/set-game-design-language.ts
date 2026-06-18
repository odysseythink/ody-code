import type { Agent } from '#/agent';
import { t } from '../../../i18n';
import { z } from 'zod';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './set-game-design-language.md';

export const SetGameDesignLanguageInputSchema = z.object({
  language: z.enum(['en', 'zh']),
}).strict();
export type SetGameDesignLanguageInput = z.infer<typeof SetGameDesignLanguageInputSchema>;

export class SetGameDesignLanguageTool implements BuiltinTool<SetGameDesignLanguageInput> {
  readonly name = 'SetGameDesignLanguage' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(SetGameDesignLanguageInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(args: SetGameDesignLanguageInput): ToolExecution {
    return {
      description: 'Setting game-design language',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'game-design') {
          return { isError: true, output: t('gameDesign.modeNotActive', lang) };
        }
        this.agent.setUserLanguage?.(args.language);
        return { output: t('gameDesign.languageSet', lang).replace('{language}', args.language) };
      },
    };
  }
}
