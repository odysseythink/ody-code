import type {
  PermissionPolicy,
  PermissionPolicyContext,
  PermissionPolicyResult,
} from '../types';

export class BrowserToolAskPermissionPolicy implements PermissionPolicy {
  readonly name = 'browser-tool-ask';

  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
    if (!context.toolCall.name.startsWith('mcp__chrome-devtools__')) return;
    return {
      kind: 'ask',
      reason: { tool: context.toolCall.name },
    };
  }
}
