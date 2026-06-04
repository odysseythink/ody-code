import type { Agent } from '../..';
import type { ApprovalResponse, PermissionPolicy, PermissionPolicyContext, PermissionPolicyResult } from '../types';

interface ExitPlanModeOption {
  readonly label: string;
  readonly description: string;
}

interface PlanReviewDisplay {
  readonly plan: string;
  readonly path?: string | undefined;
  readonly options?: readonly ExitPlanModeOption[] | undefined;
}

export class ExitPlanModeReviewAskPermissionPolicy implements PermissionPolicy {
  readonly name = 'exit-plan-mode-review-ask';

  constructor(private readonly agent: Agent) {}

  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
    const toolName = context.toolCall.name;
    const isExitPlan = toolName === 'ExitPlanMode';
    const isExitDesign = toolName === 'ExitDesignMode';
    if (!isExitPlan && !isExitDesign) return;
    if (this.agent.permission.mode === 'auto') return;
    if (!this.agent.planMode.isActive) return;
    const isDesign = this.agent.planMode.kind === 'design';
    // Guard: ExitPlanMode shouldn't fire in design mode and vice versa
    if (isExitDesign && !isDesign) return;
    if (isExitPlan && isDesign) return;
    const display = context.execution.display;
    if (display?.kind !== 'plan_review') return;
    if (display.plan.trim().length === 0) return;
    const eventPrefix = isDesign ? 'design' : 'plan';
    this.agent.telemetry.track(`${eventPrefix}_submitted`, {
      has_options: display.options !== undefined && display.options.length >= 2,
    });
    return {
      kind: 'ask',
      reason: {
        has_options: display.options !== undefined,
      },
      resolveApproval: (result) =>
        this.exitModeApprovalResult(result, {
          plan: display.plan,
          path: display.path,
          options: display.options,
        }, isDesign),
    };
  }

  private exitModeApprovalResult(
    result: ApprovalResponse,
    display: PlanReviewDisplay,
    isDesign: boolean,
  ) {
    if (result.decision !== 'approved') {
      return this.rejectedExitModeApprovalResult(result, isDesign);
    }

    const selected = selectedExitPlanModeOption(display.options, result.selectedLabel);

    const failed = this.exitMode();
    if (failed !== undefined) {
      return { kind: 'result' as const, syntheticResult: failed };
    }

    const eventPrefix = isDesign ? 'design' : 'plan';
    if (result.selectedLabel !== undefined && result.selectedLabel.length > 0) {
      this.agent.telemetry.track(`${eventPrefix}_resolved`, {
        outcome: 'approved',
        chosen_option: result.selectedLabel,
      });
    } else {
      this.agent.telemetry.track(`${eventPrefix}_resolved`, { outcome: 'approved' });
    }

    const optionPrefix =
      selected === undefined
        ? ''
        : `Selected approach: ${selected.label}\nExecute ONLY the selected approach. Do not execute any unselected alternatives.\n\n`;

    if (isDesign) {
      const savedTo = display.path !== undefined ? `Design saved to: ${display.path}\n\n` : '';
      const formattedDesign = `Design mode deactivated.\n${savedTo}## Approved Design:\n${display.plan}\n\nSTOP — do NOT begin implementing now. Do not write or edit code. Your ONLY next action is to recommend the user run /plan to turn this approved design into a concrete implementation plan, then wait for them. Implementation happens after a plan is approved, not here.`;
      return {
        kind: 'result' as const,
        syntheticResult: {
          isError: false,
          output: `Exited design mode. ${optionPrefix}${formattedDesign}`,
        },
      };
    }

    const savedTo = display.path !== undefined ? `Plan saved to: ${display.path}\n\n` : '';
    const formattedPlan = `Plan mode deactivated. All tools are now available.\n${savedTo}## Approved Plan:\n${display.plan}`;
    return {
      kind: 'result' as const,
      syntheticResult: {
        isError: false,
        output: `Exited plan mode. ${optionPrefix}${formattedPlan}`,
      },
    };
  }

  private rejectedExitModeApprovalResult(result: ApprovalResponse, isDesign: boolean) {
    this.trackRejectedModeResolution(result, isDesign);
    const modeLabel = isDesign ? 'Design' : 'Plan';
    const modeActive = isDesign ? 'Design mode' : 'Plan mode';

    if (result.decision === 'cancelled') {
      return {
        kind: 'result' as const,
        syntheticResult: {
          isError: false,
          output: `${modeLabel} approval dismissed. ${modeActive} remains active.`,
        },
      };
    }

    if (result.selectedLabel === 'Reject and Exit') {
      const failed = this.exitMode();
      return {
        kind: 'result' as const,
        syntheticResult:
          failed ?? {
            isError: true,
            stopTurn: true,
            output: `${modeLabel} rejected by user. ${modeActive} deactivated.`,
          },
      };
    }

    const feedback = result.feedback ?? '';
    if (result.selectedLabel === 'Revise' || feedback.length > 0) {
      return {
        kind: 'result' as const,
        syntheticResult: {
          isError: false,
          output:
            feedback.length > 0
              ? `User rejected the ${modeLabel.toLowerCase()}. Feedback:\n\n${feedback}`
              : `User requested revisions. ${modeActive} remains active.`,
        },
      };
    }

    return {
      kind: 'result' as const,
      syntheticResult: {
        isError: true,
        stopTurn: true,
        output: `${modeLabel} rejected by user. ${modeActive} remains active.`,
      },
    };
  }

  private exitMode(): { isError: true; output: string } | undefined {
    const isDesign = this.agent.planMode.kind === 'design';
    const modeLabel = isDesign ? 'design' : 'plan';
    try {
      this.agent.planMode.exit();
    } catch (error) {
      const message = error instanceof Error ? error.message : `Unknown error.`;
      return {
        isError: true,
        output: `Failed to exit ${modeLabel} mode: ${message}`,
      };
    }
  }

  private trackRejectedModeResolution(result: ApprovalResponse, isDesign: boolean): void {
    const eventPrefix = isDesign ? 'design' : 'plan';
    if (result.decision === 'cancelled') {
      this.agent.telemetry.track(`${eventPrefix}_resolved`, { outcome: 'dismissed' });
      return;
    }
    if (result.selectedLabel === 'Reject and Exit') {
      this.agent.telemetry.track(`${eventPrefix}_resolved`, { outcome: 'rejected_and_exited' });
      return;
    }
    const feedback = result.feedback ?? '';
    if (result.selectedLabel === 'Revise' || feedback.length > 0) {
      this.agent.telemetry.track(`${eventPrefix}_resolved`, {
        outcome: 'revise',
        has_feedback: feedback.length > 0,
      });
      return;
    }
    this.agent.telemetry.track(`${eventPrefix}_resolved`, { outcome: 'rejected' });
  }
}

function selectedExitPlanModeOption(
  options: readonly ExitPlanModeOption[] | undefined,
  label: string | undefined,
): ExitPlanModeOption | undefined {
  if (options === undefined || label === undefined) return;
  return options.find((option) => option.label === label);
}
