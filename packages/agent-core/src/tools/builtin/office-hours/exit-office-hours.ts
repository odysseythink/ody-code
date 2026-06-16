import type { Agent } from '#/agent';
import { z } from 'zod';

import { t } from '../../../i18n';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './exit-office-hours.md';

export const ExitOfficeHoursModeInputSchema = z.object({}).strict();
export type ExitOfficeHoursModeInput = z.infer<typeof ExitOfficeHoursModeInputSchema>;

export class ExitOfficeHoursModeTool implements BuiltinTool<ExitOfficeHoursModeInput> {
  readonly name = 'ExitOfficeHoursMode' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(ExitOfficeHoursModeInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(_args: ExitOfficeHoursModeInput): ToolExecution {
    return {
      description: 'Requesting to exit office hours mode',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'office-hours') {
          return {
            isError: true,
            output: t('officeHours.modeNotActive', lang),
          };
        }

        const path = this.agent.sessionMode.sessionModeFilePath;
        this.agent.sessionMode.exit();

        const parts = [
          t('officeHours.sessionComplete', lang),
        ];
        if (path) {
          parts.push(t('officeHours.designDocSaved', lang).replace('{path}', path));
        }
        parts.push(t('officeHours.appWillExit', lang));

        return {
          output: parts.join('\n'),
        };
      },
    };
  }
}
