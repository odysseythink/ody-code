import { describe, it, expect } from 'vitest';
import { createPermissionDecisionPolicies } from '../../../src/agent/permission/policies';
import { IdeaToolDirectoryApprovePermissionPolicy } from '../../../src/agent/permission/policies/idea-tool-directory';

describe('builtin policy registration', () => {
  it('includes the idea-tool-directory auto-approve policy', () => {
    const agent = {
      config: { cwd: '/workspace' },
      kaos: {} as any,
    } as any;
    const policies = createPermissionDecisionPolicies(agent);
    const found = policies.some(p => p instanceof IdeaToolDirectoryApprovePermissionPolicy);
    expect(found).toBe(true);
  });
});
