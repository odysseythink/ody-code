import type { Agent } from '../..';
import type { PermissionPolicy, PermissionPolicyContext, PermissionPolicyResult } from '../types';
import { writeFileAccesses } from './file-access-ask';

export class PlanModeGuardDenyPermissionPolicy implements PermissionPolicy {
  readonly name = 'plan-mode-guard-deny';

  constructor(private readonly agent: Agent) {}

  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
    if (!this.agent.sessionMode.isActive) return;

    const isDesign = this.agent.sessionMode.kind === 'design';
    const modeLabel = isDesign ? 'design' : 'plan';
    const exitTool = isDesign ? 'ExitDesignMode' : 'ExitPlanMode';
    const toolName = context.toolCall.name;

    if (toolName === 'Write' || toolName === 'Edit') {
      const sessionModeFilePath = this.agent.sessionMode.sessionModeFilePath;
      if (sessionModeFilePath === null) {
        return {
          kind: 'deny',
          message: modeWriteDeniedMessage(modeLabel, sessionModeFilePath),
        };
      }
      if (writesOnlyPlanFileset(context, this.agent)) {
        return;
      }
      return {
        kind: 'deny',
        message: modeWriteDeniedMessage(modeLabel, sessionModeFilePath),
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
  // The plan's writable set is the main plan/design file plus its split parts —
  // `.md` files inside a subdirectory named after the index file's stem
  // (`<id>/*.md`); everything else stays denied.
  return writeAccesses.every((access) => agent.sessionMode.isWritableSessionModePath(access.path));
}

function modeWriteDeniedMessage(modeLabel: string, sessionModeFilePath: string | null): string {
  const Mode = modeLabel.charAt(0).toUpperCase() + modeLabel.slice(1);
  const exitTool = modeLabel === 'design' ? 'ExitDesignMode' : 'ExitPlanMode';
  return (
    `${Mode} mode is active. You may only write to the current ${modeLabel} file: ${sessionModeFilePath ?? `(no ${modeLabel} file selected yet)`}. ` +
    `Call ${exitTool} to exit ${modeLabel} mode before editing other files.`
  );
}
