/**
 * ExitPlanModeTool — plan-mode exit tool.
 *
 * The LLM calls this tool to surface a finalised plan to the user and
 * exit plan mode. The plan must already be written to the current plan
 * file; this tool reads that file and flips plan mode off.
 */

import type { Agent } from '#agent';
import type { OdyConfig } from '@odysseythink/agent-core-shared';
import type { SessionModeData } from '#agent/session-mode';
import type { Kaos } from '@odysseythink/kaos';
import { z } from 'zod';

import type { BuiltinTool } from '../../../agent/tool';
import type { ExecutableToolResult, ToolExecution } from '../../../loop/types';
import type { ToolInputDisplay } from '../../display';
import { toInputJsonSchema } from '../../support/input-schema';
import {
  E2EPlanEnricher,
  E2EConfigResolver,
  registry,
} from '@odysseythink/e2e-testing';
import {
  declaredOptionLabel,
  isViaApproval,
  selectedApproachPrefix,
  selectedLabelOf,
} from './exit-mode-output';
import DESCRIPTION from './exit-plan-mode.md';

// ── Input schema ─────────────────────────────────────────────────────

/**
 * User-selectable option surfaced at plan approval time. The LLM supplies
 * up to 3 of these when the plan contains multiple approaches; the host's
 * ApprovalRuntime presents them to the user and returns the chosen `label`
 * (or `{kind:'revise', feedback}` when the user asks for revisions).
 */
export interface ExitPlanModeOption {
  label: string;
  description: string;
}

export interface ExitPlanModeInput {
  options?: readonly ExitPlanModeOption[] | undefined;
}

const RESERVED_OPTION_LABELS = new Set(
  ['Approve', 'Reject', 'Reject and Exit', 'Revise'].map(normalizeOptionLabel),
);

const ExitPlanModeOptionSchema = z
  .object({
    label: z
      .string()
      .min(1)
      .max(80)
      .describe(
        'Short name for this option (1-8 words). Append "(Recommended)" if you recommend this option.',
      ),
    description: z
      .string()
      .default('')
      .describe('Brief summary of this approach and its trade-offs.'),
  })
  .strict();

export const ExitPlanModeInputSchema: z.ZodType<ExitPlanModeInput> = z
  .object({
    options: z
      .array(ExitPlanModeOptionSchema)
      .min(1)
      .max(3)
      .refine(hasUniqueOptionLabels, 'Option labels must be unique.')
      .refine(hasNoReservedOptionLabels, 'Option labels must not use reserved approval labels.')
      .optional()
      .describe(
        'When the plan contains multiple alternative approaches, list them here so the user can choose which one to execute. Provide up to 3 options; 2-3 distinct approaches work best when the plan offers a real choice. Passing a single option is allowed and is equivalent to a plain plan approval. Each option represents a distinct approach from the plan. Do not use "Reject", "Revise", "Approve", or "Reject and Exit" as labels.',
      ),
  })
  .strict();

export interface ExitPlanModePlanSource {
  plan: string;
  path?: string | undefined;
}

type ResolvePlanResult =
  | { readonly ok: true; readonly plan: string; readonly path?: string | undefined }
  | { readonly ok: false; readonly error: ExecutableToolResult };

// ── Implementation ───────────────────────────────────────────────────

export class ExitPlanModeTool implements BuiltinTool<ExitPlanModeInput> {
  readonly name = 'ExitPlanMode' as const;
  readonly description: string = DESCRIPTION;
  readonly parameters: Record<string, unknown> = toInputJsonSchema(ExitPlanModeInputSchema);

  constructor(private readonly agent: Agent, private readonly kaos: Kaos) {}

  async resolveExecution(args: ExitPlanModeInput): Promise<ToolExecution> {
    return {
      description: 'Presenting plan and exiting plan mode',
      display: await this.resolvePlanReviewDisplay(args),
      approvalRule: this.name,
      execute: (ctx) => this.execution(args, ctx.metadata),
    };
  }

  private async maybeEnrichPlanForE2E(): Promise<void> {
    try {
      const e2eConfig = E2EConfigResolver.resolve(this.agent.kimiConfig ?? ({} as OdyConfig));
      if (!e2eConfig.enabled) return;

      const modeData = await this.agent.sessionMode.data();
      if (modeData === null || modeData.kind !== 'plan') return;
      if (modeData.content.trim().length === 0 || modeData.path.length === 0) return;

      // Use the project's own generator (Go, Python, TS/Vitest, …) to decide
      // which features are affected, so user projects — not just ody-code's
      // builtin tools — get an E2E task injected. No matching generator means
      // we cannot generate tests for this project, so skip enrichment.
      const projectRoot = this.kaos.getcwd();
      let generator;
      try {
        generator = await registry.detectAndGet(projectRoot);
      } catch {
        return;
      }

      const enricher = new E2EPlanEnricher(this.kaos, e2eConfig, {
        analyze: (files, config) => generator.analyzeImpact(files, config, projectRoot),
      });
      const enriched = await enricher.enrich(modeData.path, modeData.content, projectRoot);
      if (enriched !== null) {
        await this.kaos.writeText(modeData.path, enriched);
      }
    } catch {
      // Enrichment is best-effort; failures should not block plan exit.
    }
  }

  private async resolvePlanReviewDisplay(
    args: ExitPlanModeInput,
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

  private async execution(
    args: ExitPlanModeInput,
    metadata?: unknown,
  ): Promise<ExecutableToolResult> {
    if (!this.agent.sessionMode.isActive) {
      return {
        isError: true,
        output:
          'ExitPlanMode can only be called while plan mode is active. Use EnterPlanMode (or /plan) first.',
      };
    }

    // E2E enrichment: analyze changes and append E2E task after approval but
    // before the plan is handed off, so the file is only modified when the user
    // actually approves the plan.
    await this.maybeEnrichPlanForE2E();

    const resolvedPlan = await this.resolvePlan();
    if (!resolvedPlan.ok) return resolvedPlan.error;

    // When the plan was approved through the review surface, the approval policy
    // already tracked `plan_submitted` (in evaluate()) and forwarded the chosen
    // option via executionMetadata. In auto mode the policy is skipped, so the
    // tool is the only place these events fire.
    const viaApproval = isViaApproval(metadata);
    // The raw approval label (used for telemetry, even reserved labels like "Approve").
    const rawLabel = selectedLabelOf(metadata);
    // The approach prefix is only surfaced for a label that is one of the declared
    // options, so a plain approval ("Approve") never prints "Selected approach: Approve".
    const optionLabel = declaredOptionLabel(args.options, rawLabel);

    if (!viaApproval) {
      this.agent.telemetry.track('plan_submitted', {
        has_options: args.options !== undefined && args.options.length >= 2,
      });
    }

    const failed = await this.handoffToNormal(optionLabel);
    if (failed !== undefined) return failed;

    // Track resolution only AFTER a successful handoff so a failed exit() does not
    // emit a spurious approved/auto_approved event (see exit-plan-mode-telemetry.test.ts).
    if (viaApproval) {
      this.agent.telemetry.track(
        'plan_resolved',
        rawLabel !== undefined && rawLabel.length > 0
          ? { outcome: 'approved', chosen_option: rawLabel }
          : { outcome: 'approved' },
      );
    } else {
      this.agent.telemetry.track('plan_resolved', { outcome: 'auto_approved' });
    }

    return {
      isError: false,
      stopTurn: true,
      output: `Exited plan mode. ${formatPlanHandoffOutput(resolvedPlan.plan, resolvedPlan.path, optionLabel)}`,
    };
  }

  private async handoffToNormal(
    selectedLabel: string | undefined,
  ): Promise<ExecutableToolResult | undefined> {
    try {
      await this.agent.sessionMode.handoffTo('normal', { selectedLabel });
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to exit plan mode.';
      return {
        isError: true,
        output: `Failed to exit plan mode: ${message}`,
      };
    }
  }

  private async resolvePlan(): Promise<ResolvePlanResult> {
    let source: ExitPlanModePlanSource | null;
    try {
      const data = await this.agent.sessionMode.data();
      source = data === null ? null : { plan: data.content, path: data.path };
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Failed to read plan file.';
      return {
        ok: false,
        error: { isError: true, output: `Failed to read plan file: ${message}` },
      };
    }

    if (source !== null && source.plan.trim().length > 0) {
      return {
        ok: true,
        plan: source.plan,
        path: source.path,
      };
    }

    const path = source?.path ?? this.agent.sessionMode.sessionModeFilePath;
    return {
      ok: false,
      error: {
        isError: true,
        output:
          path === null
            ? 'No plan file found. Write the plan to the current plan file first, then call ExitPlanMode.'
            : `No plan file found. Write your plan to ${path} first, then call ExitPlanMode.`,
      },
    };
  }
}

function hasUniqueOptionLabels(options: readonly ExitPlanModeOption[]): boolean {
  const labels = new Set<string>();
  for (const option of options) {
    const label = normalizeOptionLabel(option.label);
    if (labels.has(label)) return false;
    labels.add(label);
  }
  return true;
}

function hasNoReservedOptionLabels(options: readonly ExitPlanModeOption[]): boolean {
  return options.every((option) => !RESERVED_OPTION_LABELS.has(normalizeOptionLabel(option.label)));
}

function normalizeOptionLabel(label: string): string {
  return label.trim().toLowerCase();
}

function formatPlanHandoffOutput(
  plan: string,
  path: string | undefined,
  selectedLabel: string | undefined,
): string {
  const optionPrefix = selectedApproachPrefix(selectedLabel);
  const savedTo = path !== undefined ? `Plan saved to: ${path}\n\n` : '';
  return `${optionPrefix}Plan mode deactivated. The approved plan has been handed off to the main conversation context.\n${savedTo}## Approved Plan:\n${plan}\n\nSTOP — do NOT begin executing now. This turn ends here. The user will start implementation themselves — the plan is now available in their main conversation context.`;
}
