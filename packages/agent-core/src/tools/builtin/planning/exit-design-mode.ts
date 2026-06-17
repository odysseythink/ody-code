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
import type { ExecutableToolErrorResult, ExecutableToolResult, ToolExecution } from '../../../loop/types';
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
  path?: string | undefined;
  error?: ExecutableToolResult;
}

// ── Implementation ───────────────────────────────────────────────────

export function findMissingDesignSections(content: string): string[] {
  const missing: string[] = [];
  const trimmed = content.trim();

  if (trimmed.length < 300) {
    missing.push('sufficient content (design appears incomplete or empty)');
  }

  const headingCount = (trimmed.match(/^## /gm) ?? []).length;
  if (headingCount < 3) {
    missing.push(`at least 3 design sections (found ${headingCount})`);
  }

  const scopePattern = /^#{1,3}\s+(scope|in\/out|范围|scope\s+in)/im;
  if (!scopePattern.test(trimmed)) {
    missing.push('Scope or Scope In/Out section');
  }

  const archPattern = /^#{1,3}\s+(architecture|design|approach|overview|架构|设计方案)/im;
  if (!archPattern.test(trimmed)) {
    missing.push('Architecture or Design section');
  }

  // C3: Data Models
  const dataModelsPattern = /^#{1,3}\s+(data\s*models?|数据模型|models?|data\s+&?\s*state)/im;
  if (!dataModelsPattern.test(trimmed)) {
    missing.push('Data Models section');
  }

  // C4: Algorithms
  const algorithmsPattern = /^#{1,3}\s+(algorithms?|算法|pseudocode|implementation\s+notes?)/im;
  if (!algorithmsPattern.test(trimmed)) {
    missing.push('Algorithms section');
  }

  // C5: Error Handling
  const errorHandlingPattern = /^#{1,3}\s+(error\s*handling|错误处理|errors?|degradation|failure\s+scenarios?)/im;
  if (!errorHandlingPattern.test(trimmed)) {
    missing.push('Error Handling section');
  }

  // C6: Self-Review
  const selfReviewPattern = /^#{1,3}\s+(self[- ]?review|自检|review|audit)/im;
  if (!selfReviewPattern.test(trimmed)) {
    missing.push('Self-Review section');
  }

  // C7: User Final Approval
  const userApprovalPattern = /^#{1,3}\s+(user\s+(final\s+)?approval|用户批准|批准状态|approved?)/im;
  if (!userApprovalPattern.test(trimmed)) {
    missing.push('User Approval');
  }

  // C8: Reuse Analysis
  const reuseAnalysisPattern = /^#{1,3}\s+(?:reuse\s+analysis|复用分析|component\s+reuse|existing\s+components?)/im;
  if (!reuseAnalysisPattern.test(trimmed)) {
    missing.push('Reuse Analysis section');
  }

  return missing;
}

export class ExitDesignModeTool implements BuiltinTool<ExitDesignModeInput> {
  readonly name = 'ExitDesignMode' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(ExitDesignModeInputSchema);

  constructor(private readonly agent: Agent) {}

  async resolveExecution(args: ExitDesignModeInput): Promise<ToolExecution> {
    const incomplete = await this.checkDesignCompleteness();
    if (incomplete !== null) return incomplete;

    return {
      description: 'Presenting design and exiting design mode',
      display: await this.resolveDesignReviewDisplay(args),
      approvalRule: this.name,
      execute: (ctx) => this.execution(args, ctx.metadata),
    };
  }

  private async checkDesignCompleteness(): Promise<ExecutableToolErrorResult | null> {
    if (!this.agent.sessionMode.isActive) return null;

    let data: SessionModeData;
    try {
      data = await this.agent.sessionMode.data();
    } catch {
      return null;
    }

    if (data === null) return null;

    const missing = findMissingDesignSections(data.content);
    if (missing.length === 0) return null;

    const list = missing.map((item) => `- ${item}`).join('\n');
    return {
      isError: true,
      output: `Design is incomplete. Missing:\n${list}\n\nPlease add the missing sections to the design file, then call ExitDesignMode again.`,
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

    const optionLabel = declaredOptionLabel(args.options, selectedLabelOf(metadata));

    const failed = await this.handoffToPlan(optionLabel);
    if (failed !== undefined) return failed;

    return {
      isError: false,
      output: formatDesignHandoffOutput(resolved.path, optionLabel),
    };
  }

  private async handoffToPlan(selectedLabel?: string): Promise<ExecutableToolResult | undefined> {
    try {
      await this.agent.sessionMode.handoffTo('plan', { selectedLabel });
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

    if (data !== null && data.path.length > 0) {
      return { ok: true, path: data.path };
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
  path: string | undefined,
  selectedLabel: string | undefined,
): string {
  const optionPrefix = selectedApproachPrefix(selectedLabel);
  const savedTo = path !== undefined ? `Design saved to: ${path}\n\n` : '';
  return `${optionPrefix}Design mode deactivated. Now in plan mode.\n\n${savedTo}Create a concrete, step-by-step implementation plan based on the approved design document.`;
}
