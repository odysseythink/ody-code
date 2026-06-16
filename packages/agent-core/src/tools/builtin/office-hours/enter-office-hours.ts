import type { Agent } from '#/agent';
import { z } from 'zod';

import { officeHoursEntryReminder } from '#/agent/injection/office-hours-contract';
import { t } from '../../../i18n';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './enter-office-hours.md';

export const EnterOfficeHoursModeInputSchema = z.object({}).strict();
export type EnterOfficeHoursModeInput = z.infer<typeof EnterOfficeHoursModeInputSchema>;

export class EnterOfficeHoursModeTool implements BuiltinTool<EnterOfficeHoursModeInput> {
  readonly name = 'EnterOfficeHoursMode' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(EnterOfficeHoursModeInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(_args: EnterOfficeHoursModeInput): ToolExecution {
    return {
      description: 'Requesting to enter office hours mode',
      approvalRule: this.name,
      execute: async () => {
        const lang = this.agent.userLanguage;
        if (this.agent.sessionMode.isActive) {
          if (this.agent.sessionMode.kind === 'office-hours') {
            return {
              isError: true,
              output: t('officeHours.alreadyActive', lang),
            };
          }
          return {
            isError: true,
            output: t('officeHours.anotherModeActive', lang),
          };
        }

        try {
          await this.agent.sessionMode.enter(undefined, undefined, undefined, 'office-hours');
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to enter office hours mode.';
          return {
            isError: true,
            output: t('officeHours.failedToEnter', lang).replace('{message}', message),
          };
        }

        return {
          output: officeHoursEntryReminder(this.agent.sessionMode.sessionModeFilePath),
        };
      },
    };
  }
}
