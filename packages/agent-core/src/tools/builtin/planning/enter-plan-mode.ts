/**
 * EnterPlanModeTool — plan-mode entry tool.
 *
 * The LLM calls this tool to enter plan mode directly. Entering plan mode
 * does not require approval in any permission mode.
 */

import type { Agent } from '#/agent';
import { z } from 'zod';

import { planModeEntryMessage } from '../../../agent/injection/plan-mode-contract';
import { cleanupTopic } from '../../../agent/plan/topic-generator';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './enter-plan-mode.md';

// ── Input schema ─────────────────────────────────────────────────────

export const EnterPlanModeInputSchema = z
  .object({
    topic: z.string().max(100).optional(),
  })
  .strict();
export type EnterPlanModeInput = z.infer<typeof EnterPlanModeInputSchema>;

export class EnterPlanModeTool implements BuiltinTool<EnterPlanModeInput> {
  readonly name = 'EnterPlanMode' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(EnterPlanModeInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(_args: EnterPlanModeInput): ToolExecution {
    return {
      description: 'Requesting to enter plan mode',
      approvalRule: this.name,
      execute: async () => {
        // Guard: already in plan mode
        if (this.agent.planMode.isActive) {
          return {
            isError: true,
            output: 'Plan mode is already active. Use ExitPlanMode when the plan is ready.',
          };
        }

        let fileStem: string | undefined;
        if (_args.topic !== undefined) {
          const cleaned = cleanupTopic(_args.topic);
          if (cleaned !== null) {
            fileStem = cleaned;
          }
        }

        try {
          await this.agent.planMode.enter(undefined, undefined, undefined, 'plan', fileStem);
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to enter plan mode.';
          return { isError: true, output: `Failed to enter plan mode: ${message}` };
        }

        this.agent.telemetry.track('plan_enter_resolved', { outcome: 'auto_approved' });
        return { output: planModeEntryMessage(this.agent.planMode.planFilePath) };
      },
    };
  }
}
