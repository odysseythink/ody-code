/**
 * EnterDesignModeTool — design-mode entry tool.
 *
 * Design mode is the brainstorming / spec-exploration sibling of plan mode.
 * It reuses the same read-only-with-one-writable-file machinery as plan mode
 * (see {@link SessionMode}) but enters with `kind: 'design'`, which routes the
 * design document to the `designs/` directory and swaps the plan-mode prompt
 * for the brainstorming workflow. Entering design mode does not require
 * approval in any permission mode.
 */

import type { Agent } from '#agent';
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
        // Guard: already in a session mode. Name the ACTUAL active mode and its
        // exit tool — a blanket "Plan mode is already active" lie sends the model
        // down the wrong recovery path (looking for a plan file, calling
        // ExitPlanMode) when it is really in product/game-design.
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
          await this.agent.sessionMode.enter(undefined, undefined, undefined, 'design');
        } catch (error) {
          const message = error instanceof Error ? error.message : 'Failed to enter design mode.';
          return { isError: true, output: `Failed to enter design mode: ${message}` };
        }

        this.agent.telemetry.track('design_enter_resolved', { outcome: 'auto_approved' });
        return {
          output: designModeEntryMessage(
            this.agent.sessionMode.sessionModeFilePath,
            this.agent.rpc?.openExternal !== undefined,
          ),
        };
      },
    };
  }
}
