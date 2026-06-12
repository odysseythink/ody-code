/**
 * CheckpointTool — manual `/checkpoint` trigger.
 *
 * Allows the model (or user) to force an immediate durable checkpoint save.
 * The actual save is delegated to the agent's checkpoint coordinator so the
 * same payload construction, integrity checks, and backup rotation are reused.
 */

import { z } from 'zod';

import type { Agent } from '#/agent';
import type { BuiltinTool } from '#/agent/tool';
import type { ToolExecution } from '#/loop/types';
import { toInputJsonSchema } from '../../support/input-schema';
import DESCRIPTION from './checkpoint.md';

export const CHECKPOINT_TOOL_NAME = 'Checkpoint' as const;

export interface CheckpointInput {
  /** Optional reason for the checkpoint. */
  reason?: string | undefined;
}

export const CheckpointInputSchema: z.ZodType<CheckpointInput> = z.object({
  reason: z
    .string()
    .optional()
    .describe('Short reason for taking the checkpoint.'),
});

export class CheckpointTool implements BuiltinTool<CheckpointInput> {
  readonly name = CHECKPOINT_TOOL_NAME;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(CheckpointInputSchema);

  constructor(private readonly agent: Agent) {}

  resolveExecution(args: CheckpointInput): ToolExecution {
    return {
      description: `Taking manual checkpoint${args.reason ? `: ${args.reason}` : ''}`,
      approvalRule: this.name,
      execute: async () => {
        const coordinator = this.agent.checkpointCoordinator;
        if (coordinator === undefined) {
          return {
            isError: true,
            output: 'Checkpoint coordinator is not enabled for this agent.',
          };
        }

        await coordinator.checkpointNow();
        return {
          isError: false,
          output: 'Checkpoint saved.',
        };
      },
    };
  }
}
