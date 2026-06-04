import type { Agent } from '../..';
import type { PermissionPolicy, PermissionPolicyContext, PermissionPolicyResult } from '../types';
import { writeFileAccesses } from './file-access-ask';

export class PlanModeGuardDenyPermissionPolicy implements PermissionPolicy {
  readonly name = 'plan-mode-guard-deny';

  constructor(private readonly agent: Agent) {}

  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
    if (!this.agent.planMode.isActive) return;

    const isDesign = this.agent.planMode.kind === 'design';
    const modeLabel = isDesign ? 'design' : 'plan';
    const exitTool = isDesign ? 'ExitDesignMode' : 'ExitPlanMode';
    const toolName = context.toolCall.name;

    if (toolName === 'Write' || toolName === 'Edit') {
      const planFilePath = this.agent.planMode.planFilePath;
      if (planFilePath === null) {
        return {
          kind: 'deny',
          message: modeWriteDeniedMessage(modeLabel, planFilePath),
        };
      }
      if (writesOnlyPlanFileset(context, this.agent)) {
        return;
      }
      return {
        kind: 'deny',
        message: modeWriteDeniedMessage(modeLabel, planFilePath),
      };
    }

    if (toolName === 'TaskStop') {
      return {
        kind: 'deny',
        message: `TaskStop is not available in ${modeLabel} mode. Call ${exitTool} to exit ${modeLabel} mode before stopping a background task.`,
      };
    }

    if (toolName === 'CronCreate' || toolName === 'CronDelete') {
      return {
        kind: 'deny',
        message: `${toolName} is not available in ${modeLabel} mode because it would mutate scheduled work that runs after ${modeLabel} exit. Call ${exitTool} first.`,
      };
    }

    return;
  }
}

function writesOnlyPlanFileset(context: PermissionPolicyContext, agent: Agent): boolean {
  const writeAccesses = writeFileAccesses(context);
  if (writeAccesses.length === 0) return false;
  // The plan's writable set is the main plan file plus its split siblings
  // (`<id>-*.md` in the same directory); everything else stays denied.
  return writeAccesses.every((access) => agent.planMode.isWritablePlanPath(access.path));
}

function modeWriteDeniedMessage(modeLabel: string, planFilePath: string | null): string {
  const Mode = modeLabel.charAt(0).toUpperCase() + modeLabel.slice(1);
  const exitTool = modeLabel === 'design' ? 'ExitDesignMode' : 'ExitPlanMode';
  return (
    `${Mode} mode is active. You may only write to the current ${modeLabel} file: ${planFilePath ?? `(no ${modeLabel} file selected yet)`}. ` +
    `Call ${exitTool} to exit ${modeLabel} mode before editing other files.`
  );
}
