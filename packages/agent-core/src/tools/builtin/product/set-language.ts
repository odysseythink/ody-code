import type { Agent } from '#agent';
import { t, isSupportedLanguage, type SupportedLanguage } from '../../../i18n';
import { z } from 'zod';

import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './set-language.md';

export const SetProductLanguageInputSchema = z.object({
  language: z.string().refine(isSupportedLanguage, {
    message: 'Language must be "en" or "zh"',
  }),
}).strict();
export type SetProductLanguageInput = z.infer<typeof SetProductLanguageInputSchema>;

export class SetProductLanguageTool implements BuiltinTool<SetProductLanguageInput> {
  readonly name = 'SetProductLanguage' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(SetProductLanguageInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(args: SetProductLanguageInput): ToolExecution {
    return {
      description: 'Setting office hours user language',
      approvalRule: this.name,
      execute: async () => {
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'product') {
          return {
            isError: true,
            output: t('product.modeNotActive', this.agent.userLanguage),
          };
        }

        if (!isSupportedLanguage(args.language)) {
          return {
            isError: true,
            output: `Unsupported language: ${args.language}`,
          };
        }

        this.agent.setUserLanguage(args.language as SupportedLanguage);
        return {
          output: t('product.languageSet', args.language as SupportedLanguage)
            .replace('{language}', args.language),
        };
      },
    };
  }
}
