import type { PermissionPolicy, PermissionPolicyContext, PermissionPolicyResult } from '../types';

const DEFAULT_APPROVE_TOOLS = new Set([
  'Read',
  'Grep',
  'Glob',
  'ReadMediaFile',
  'SetTodoList',
  'TodoList',
  'TaskList',
  'TaskOutput',
  'CronList',
  'WebSearch',
  'FetchURL',
  'Agent',
  'AskUserQuestion',
  'Skill',
  // Goal control tools have no side effects on the world: GetGoal reads, and
  // mutation tools only record the goal's own runtime state.
  'GetGoal',
  'SetGoalBudget',
  'UpdateGoal',
  // product profile persistence only appends to the application's own
  // data directory (~/.ody-code/product) and is required for the mode's
  // tiered handoff to work.
  'AppendBuilderProfile',
]);

export class DefaultToolApprovePermissionPolicy implements PermissionPolicy {
  readonly name = 'default-tool-approve';

  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
    if (!DEFAULT_APPROVE_TOOLS.has(context.toolCall.name)) return;
    return {
      kind: 'approve',
    };
  }
}
