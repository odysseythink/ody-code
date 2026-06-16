import type { Agent } from '#/agent';
import { z } from 'zod';

import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './exit-office-hours.md';

export const ExitOfficeHoursModeInputSchema = z.object({
  approved: z.boolean().describe('Whether the design document has been approved.'),
}).strict();
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
        if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'office-hours') {
          return {
            isError: true,
            output: 'Office hours mode is not active.',
          };
        }

        const path = this.agent.sessionMode.sessionModeFilePath;
        this.agent.sessionMode.exit();

        return {
          output: [
            'Office hours session complete.',
            path ? `Design document saved to: ${path}` : '',
            'The application will now exit.',
          ].filter(Boolean).join('\n'),
        };
      },
    };
  }
}
