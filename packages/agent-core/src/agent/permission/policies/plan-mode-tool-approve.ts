import type { Agent } from '../..';
import type { PermissionPolicy, PermissionPolicyContext, PermissionPolicyResult } from '../types';
import { writeFileAccesses } from './file-access-ask';

export class PlanModeToolApprovePermissionPolicy implements PermissionPolicy {
  readonly name = 'plan-mode-tool-approve';

  constructor(private readonly agent: Agent) {}

  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
    const toolName = context.toolCall.name;

    if (toolName === 'EnterPlanMode' || toolName === 'EnterDesignMode') {
      return { kind: 'approve' };
    }

    if (
      (toolName === 'Write' || toolName === 'Edit') &&
      this.agent.sessionMode.isActive &&
      writesOnlyPlanFile(context, this.agent.sessionMode.sessionModeFilePath)
    ) {
      return { kind: 'approve' };
    }

    if (toolName === 'ExitPlanMode' || toolName === 'ExitDesignMode') {
      if (!this.agent.sessionMode.isActive) {
        return { kind: 'approve' };
      }
      if (context.execution.display?.kind !== 'plan_review') {
        return { kind: 'approve' };
      }
      if (context.execution.display.plan.trim().length > 0) return;
      return { kind: 'approve' };
    }
  }
}

function writesOnlyPlanFile(
  context: PermissionPolicyContext,
  sessionModeFilePath: string | null,
): boolean {
  if (sessionModeFilePath === null) return false;
  const writeAccesses = writeFileAccesses(context);
  return writeAccesses.every((access) => access.path === sessionModeFilePath);
}
