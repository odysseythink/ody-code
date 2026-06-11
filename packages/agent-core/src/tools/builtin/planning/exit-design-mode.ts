/**
 * ExitDesignModeTool — design-mode exit tool.
 *
 * The LLM calls this tool to surface a finalised design document to the user
 * and exit design mode. The design must already be written to the current
 * design file; this tool reads that file and flips design mode off. It mirrors
 * {@link ExitPlanModeTool} and reuses the same `plan_review` approval surface.
 */

import type { Agent } from '#/agent';
import type { SessionModeData } from '#/agent/session-mode';
import { z } from 'zod';

import type { BuiltinTool } from '../../../agent/tool';
import type { ExecutableToolResult, ToolExecution } from '../../../loop/types';
import type { ToolInputDisplay } from '../../display';
import { toInputJsonSchema } from '../../support/input-schema';
import { declaredOptionLabel, selectedApproachPrefix, selectedLabelOf } from './exit-mode-output';
import DESCRIPTION from './exit-design-mode.md';

// ── Input schema ─────────────────────────────────────────────────────

export interface ExitDesignModeOption {
  label: string;
  description: string;
}

export interface ExitDesignModeInput {
  options?: readonly ExitDesignModeOption[] | undefined;
}

const RESERVED_OPTION_LABELS = new Set(
  ['Approve', 'Reject', 'Reject and Exit', 'Revise'].map(normalizeOptionLabel),
);

const ExitDesignModeOptionSchema = z
  .object({
    label: z
      .string()
      .min(1)
      .max(80)
      .describe(
        'Short name for this approach (1-8 words). Append "(Recommended)" if you recommend it.',
      ),
    description: z
      .string()
      .default('')
      .describe('Brief summary of this approach and its trade-offs.'),
  })
  .strict();

export const ExitDesignModeInputSchema: z.ZodType<ExitDesignModeInput> = z
  .object({
    options: z
      .array(ExitDesignModeOptionSchema)
      .min(1)
      .max(3)
      .refine(hasUniqueOptionLabels, 'Option labels must be unique.')
      .refine(hasNoReservedOptionLabels, 'Option labels must not use reserved approval labels.')
      .optional()
      .describe(
        'When the design presents multiple alternative directions, list them here so the user can choose which one to pursue. Provide up to 3 options; 2-3 distinct approaches work best when the design offers a real choice. Passing a single option is allowed and is equivalent to a plain approval. Do not use "Reject", "Revise", "Approve", or "Reject and Exit" as labels.',
      ),
  })
  .strict();

interface ResolveDesignResult {
  ok: boolean;
  design?: string;
  path?: string | undefined;
  error?: ExecutableToolResult;
}

// ── Implementation ───────────────────────────────────────────────────

export class ExitDesignModeTool implements BuiltinTool<ExitDesignModeInput> {
  readonly name = 'ExitDesignMode' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(ExitDesignModeInputSchema);

  constructor(private readonly agent: Agent) {}

  async resolveExecution(args: ExitDesignModeInput): Promise<ToolExecution> {
    return {
      description: 'Presenting design and exiting design mode',
      display: await this.resolveDesignReviewDisplay(args),
      approvalRule: this.name,
      execute: (ctx) => this.execution(args, ctx.metadata),
    };
  }

  private async resolveDesignReviewDisplay(
    args: ExitDesignModeInput,
  ): Promise<ToolInputDisplay | undefined> {
    if (!this.agent.sessionMode.isActive) return undefined;
    let data: SessionModeData;
    try {
      data = await this.agent.sessionMode.data();
    } catch {
      return undefined;
    }
    if (data === null || data.content.trim().length === 0) return undefined;
    const display: ToolInputDisplay = {
      kind: 'plan_review',
      plan: data.content,
      path: data.path,
    };
    if (args.options !== undefined && args.options.length >= 2) {
      display.options = args.options;
    }
    return display;
  }

  private async execution(args: ExitDesignModeInput, metadata?: unknown): Promise<ExecutableToolResult> {
    if (!this.agent.sessionMode.isActive) {
      return {
        isError: true,
        output:
          'ExitDesignMode can only be called while design mode is active. Use EnterDesignMode (or /design) first.',
      };
    }

    const resolved = await this.resolveDesign();
    if (!resolved.ok) return resolved.error as ExecutableToolResult;

    const failed = await this.handoffToPlan();
    if (failed !== undefined) return failed;

    // Only surface the chosen approach when it is one of the declared options, so a
    // plain approval ("Approve") never prints "Selected approach: Approve".
    const optionLabel = declaredOptionLabel(args.options, selectedLabelOf(metadata));

    return {
      isError: false,
      output: `Exited design mode. ${formatDesignHandoffOutput(resolved.design ?? '', resolved.path, optionLabel)}`,
    };
  }

  private async handoffToPlan(): Promise<ExecutableToolResult | undefined> {
    try {
      await this.agent.sessionMode.handoffTo('plan');
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to hand off to plan mode.';
      return {
        isError: true,
        output: `Failed to exit design mode: ${message}`,
      };
    }
  }

  private async resolveDesign(): Promise<ResolveDesignResult> {
    let data: SessionModeData;
    try {
      data = await this.agent.sessionMode.data();
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to read design file.';
      return {
        ok: false,
        error: { isError: true, output: `Failed to read design file: ${message}` },
      };
    }

    if (data !== null && data.content.trim().length > 0) {
      return { ok: true, design: data.content, path: data.path };
    }

    const path = data?.path ?? this.agent.sessionMode.sessionModeFilePath;
    return {
      ok: false,
      error: {
        isError: true,
        output:
          path === null
            ? 'No design file found. Write the design to the current design file first, then call ExitDesignMode.'
            : `No design file found. Write your design to ${path} first, then call ExitDesignMode.`,
      },
    };
  }
}

function hasUniqueOptionLabels(options: readonly ExitDesignModeOption[]): boolean {
  const labels = new Set<string>();
  for (const option of options) {
    const label = normalizeOptionLabel(option.label);
    if (labels.has(label)) return false;
    labels.add(label);
  }
  return true;
}

function hasNoReservedOptionLabels(options: readonly ExitDesignModeOption[]): boolean {
  return options.every((option) => !RESERVED_OPTION_LABELS.has(normalizeOptionLabel(option.label)));
}

function normalizeOptionLabel(label: string): string {
  return label.trim().toLowerCase();
}

function formatDesignHandoffOutput(
  design: string,
  path: string | undefined,
  selectedLabel: string | undefined,
): string {
  const optionPrefix = selectedApproachPrefix(selectedLabel);
  const savedTo = path !== undefined ? `Design saved to: ${path}\n\n` : '';
  return `${optionPrefix}Design mode deactivated. Now in plan mode.\n${savedTo}## Approved Design:\n${design}\n\nYou are now in plan mode. Create a concrete, step-by-step implementation plan based on the approved design above.`;
}
