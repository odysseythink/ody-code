import type { Agent } from '#/agent';
import { t, isSupportedLanguage, type SupportedLanguage } from '../../../i18n';
import { z } from 'zod';

import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './set-language.md';

export const SetOfficeHoursLanguageInputSchema = z.object({
  language: z.string().refine(isSupportedLanguage, {
    message: 'Language must be "en" or "zh"',
  }),
}).strict();
export type SetOfficeHoursLanguageInput = z.infer<typeof SetOfficeHoursLanguageInputSchema>;

export class SetOfficeHoursLanguageTool implements BuiltinTool<SetOfficeHoursLanguageInput> {
  readonly name = 'SetOfficeHoursLanguage' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(SetOfficeHoursLanguageInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(args: SetOfficeHoursLanguageInput): ToolExecution {
    return {
      description: 'Setting office hours user language',
      approvalRule: this.name,
      execute: async () => {
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'office-hours') {
          return {
            isError: true,
            output: t('officeHours.modeNotActive', this.agent.userLanguage),
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
          output: t('officeHours.languageSet', args.language as SupportedLanguage)
            .replace('{language}', args.language),
        };
      },
    };
  }
}
