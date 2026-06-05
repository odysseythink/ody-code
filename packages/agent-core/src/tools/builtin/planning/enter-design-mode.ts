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

import type { Agent } from '#/agent';
import { z } from 'zod';

import { designModeEntryMessage } from '../../../agent/injection/design-mode-contract';
import { cleanupTopic } from '../../../agent/session-mode/topic-generator';
import type { BuiltinTool } from '../../../agent/tool';
import type { ToolExecution } from '../../../loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './enter-design-mode.md';

// ── Input schema ─────────────────────────────────────────────────────

export const EnterDesignModeInputSchema = z
  .object({
    topic: z.string().max(100).optional(),
  })
  .strict();
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
        if (this.agent.sessionMode.isActive) {
          const active = this.agent.sessionMode.kind === 'design' ? 'Design' : 'Plan';
          return {
            isError: true,
            output: `${active} mode is already active. Use ExitDesignMode when the design is ready, or exit first.`,
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
          await this.agent.sessionMode.enter(undefined, undefined, undefined, 'design', fileStem);
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
