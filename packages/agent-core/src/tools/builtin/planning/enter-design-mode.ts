/**
 * EnterDesignModeTool — design-mode entry tool.
 *
 * Design mode is the brainstorming / spec-exploration sibling of plan mode.
 * It reuses the same read-only-with-one-writable-file machinery as plan mode
 * (see {@link PlanMode}) but enters with `kind: 'design'`, which routes the
 * design document to the `designs/` directory and swaps the plan-mode prompt
 * for the brainstorming workflow. Entering design mode does not require
 * approval in any permission mode.
 */

import type { Agent } from '#/agent';
import { z } from 'zod';

import { designModeEntryMessage } from '../../../agent/injection/design-mode-contract';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './enter-design-mode.md';

// ── Input schema ─────────────────────────────────────────────────────

export const EnterDesignModeInputSchema = z.object({}).strict();
export type EnterDesignModeInput = z.infer<typeof EnterDesignModeInputSchema>;

export class EnterDesignModeTool implements BuiltinTool<EnterDesignModeInput> {
  readonly name = 'EnterDesignMode' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(EnterDesignModeInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(_args: EnterDesignModeInput): ToolExecution {
    return {
      description: 'Requesting to enter design mode',
      approvalRule: this.name,
      execute: async () => {
        // Guard: already in plan/design mode
        if (this.agent.planMode.isActive) {
          const active = this.agent.planMode.kind === 'design' ? 'Design' : 'Plan';
          return {
            isError: true,
            output: `${active} mode is already active. Use ExitDesignMode when the design is ready, or exit first.`,
          };
        }

        try {
          await this.agent.planMode.enter(undefined, undefined, undefined, 'design');
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to enter design mode.';
          return { isError: true, output: `Failed to enter design mode: ${message}` };
        }

        this.agent.telemetry.track('design_enter_resolved', { outcome: 'auto_approved' });
        return {
          output: designModeEntryMessage(
            this.agent.planMode.planFilePath,
            this.agent.rpc?.openExternal !== undefined,
          ),
        };
      },
    };
  }
}
