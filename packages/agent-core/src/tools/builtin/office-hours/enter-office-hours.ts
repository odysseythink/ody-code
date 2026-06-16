import type { Agent } from '#/agent';
import { z } from 'zod';

import { officeHoursEntryReminder } from '../../../agent/injection/office-hours-contract';
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
        if (this.agent.sessionMode.isActive) {
          if (this.agent.sessionMode.kind === 'office-hours') {
            return {
              isError: true,
              output: 'Office hours mode is already active. Use ExitOfficeHoursMode when the session is complete.',
            };
          }
          return {
            isError: true,
            output: 'Another session mode is already active. Exit it first before entering office hours mode.',
          };
        }

        try {
          await this.agent.sessionMode.enter(undefined, undefined, undefined, 'office-hours');
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to enter office hours mode.';
          return { isError: true, output: `Failed to enter office hours mode: ${message}` };
        }

        return {
          output: officeHoursEntryReminder(this.agent.sessionMode.sessionModeFilePath),
        };
      },
    };
  }
}
