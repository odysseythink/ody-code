import type { Agent } from '../..';
import type { ApprovalResponse, PermissionPolicy, PermissionPolicyContext, PermissionPolicyResult } from '../types';

export class ExitPlanModeReviewAskPermissionPolicy implements PermissionPolicy {
  readonly name = 'exit-plan-mode-review-ask';

  constructor(private readonly agent: Agent) {}

  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
    const toolName = context.toolCall.name;
    const isExitPlan = toolName === 'ExitPlanMode';
    const isExitDesign = toolName === 'ExitDesignMode';
    if (!isExitPlan && !isExitDesign) return;
    if (this.agent.permission.mode === 'auto') return;
    if (!this.agent.sessionMode.isActive) return;
    const isDesign = this.agent.sessionMode.kind === 'design';
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
      resolveApproval: (result) => this.exitModeApprovalResult(result, isDesign),
    };
  }

  private exitModeApprovalResult(result: ApprovalResponse, isDesign: boolean) {
    if (result.decision !== 'approved') {
      return this.rejectedExitModeApprovalResult(result, isDesign);
    }

    // Approved (both design and plan): let the tool's execute() run handoffTo(), which
    // exits the current mode and stores the artifact so the injection system carries it
    // into the target partition (design→plan, plan→normal) on the next turn. Pass
    // selectedLabel (and viaApproval for plan) via executionMetadata so the tool can
    // include the chosen option in its output.
    if (isDesign) {
      // Design resolution is tracked here (the design tool does not track it).
      if (result.selectedLabel !== undefined && result.selectedLabel.length > 0) {
        this.agent.telemetry.track('design_resolved', {
          outcome: 'approved',
          chosen_option: result.selectedLabel,
        });
      } else {
        this.agent.telemetry.track('design_resolved', { outcome: 'approved' });
      }
      return {
        kind: 'approve' as const,
        executionMetadata: { selectedLabel: result.selectedLabel },
      };
    }

    // Plan resolution telemetry is fired by the tool AFTER a successful handoff, so a
    // failed exit() does not emit a spurious approved event (see the telemetry tests).
    return {
      kind: 'approve' as const,
      executionMetadata: { selectedLabel: result.selectedLabel, viaApproval: true },
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
    const isDesign = this.agent.sessionMode.kind === 'design';
    const modeLabel = isDesign ? 'design' : 'plan';
    try {
      this.agent.sessionMode.exit();
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
