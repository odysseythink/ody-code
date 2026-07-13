/**
 * EnterPlanModeTool — plan-mode entry tool.
 *
 * The LLM calls this tool to enter plan mode directly. Entering plan mode
 * does not require approval in any permission mode.
 */

import type { Agent } from '#agent';
import { z } from 'zod';

import { planModeEntryMessage } from '../../../agent/injection/plan-mode-contract';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './enter-plan-mode.md';

// ── Input schema ─────────────────────────────────────────────────────

export const EnterPlanModeInputSchema = z.object({}).strict();
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
        // Guard: already in a session mode. Name the ACTUAL active mode and its
        // exit tool so the model doesn't think it is in plan mode when it is
        // really in product/game-design/design.
        if (this.agent.sessionMode.isActive) {
          const kind = this.agent.sessionMode.kind;
          const active =
            kind === 'design' ? 'Design' :
            kind === 'product' ? 'Product' :
            kind === 'game-design' ? 'Game-design' :
            'Plan';
          const exitTool =
            kind === 'design' ? 'ExitDesignMode' :
            kind === 'product' ? 'ExitProductMode' :
            kind === 'game-design' ? 'ExitGameDesignMode' :
            'ExitPlanMode';
          return {
            isError: true,
            output: `${active} mode is already active. Use ${exitTool} when you are ready to exit ${active.toLowerCase()} mode; do not try to enter another mode on top of it.`,
          };
        }

        try {
          await this.agent.sessionMode.enter(undefined, undefined, undefined, 'plan');
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to enter plan mode.';
          return { isError: true, output: `Failed to enter plan mode: ${message}` };
        }

        this.agent.telemetry.track('plan_enter_resolved', { outcome: 'auto_approved' });
        return { output: planModeEntryMessage(this.agent.sessionMode.sessionModeFilePath) };
      },
    };
  }
}
