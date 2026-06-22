import { describe, expect, it } from 'vitest';

import { IdeaToolDirectoryApprovePermissionPolicy } from '../../../src/agent/permission/policies/idea-tool-directory';
import type { PermissionPolicyContext } from '../../../src/agent/permission/types';
import type { Agent } from '../../../src/agent';
import { createFakeKaos } from '../../tools/fixtures/fake-kaos';

function mockContext(toolName: string, paths: string[]): PermissionPolicyContext {
  return {
    toolCall: { name: toolName, id: 'call_1', arguments: {} },
    execution: {
      accesses: paths.map((path) => ({ kind: 'file' as const, operation: 'write' as const, path })),
      approvalRule: toolName,
    },
  } as unknown as PermissionPolicyContext;
}

function mockAgent(cwd: string): Agent {
  return {
    config: { cwd } as unknown as Agent['config'],
    kaos: createFakeKaos(),
  } as unknown as Agent;
}

describe('IdeaToolDirectoryApprovePermissionPolicy', () => {
  it('approves writes directly under .ody-code/ideas/', () => {
    const agent = mockAgent('/workspace');
    const policy = new IdeaToolDirectoryApprovePermissionPolicy(agent);
    const result = policy.evaluate(mockContext('SaveIdeaReport', [
      '/workspace/.ody-code/ideas/2026-06-22-foo.md',
    ]));
    expect(result).toEqual({ kind: 'approve' });
  });

  it('approves writes in nested subdirectories of .ody-code/ideas/', () => {
    const agent = mockAgent('/workspace');
    const policy = new IdeaToolDirectoryApprovePermissionPolicy(agent);
    const result = policy.evaluate(mockContext('SaveIdeaReport', [
      '/workspace/.ody-code/ideas/archive/2026-06-22-foo.md',
    ]));
    expect(result).toEqual({ kind: 'approve' });
  });

  it('does not approve writes to .ody-code/plans/', () => {
    const agent = mockAgent('/workspace');
    const policy = new IdeaToolDirectoryApprovePermissionPolicy(agent);
    const result = policy.evaluate(mockContext('SaveIdeaReport', [
      '/workspace/.ody-code/plans/2026-06-22-foo.md',
    ]));
    expect(result).toBeUndefined();
  });

  it('does not approve writes that escape ideas via traversal', () => {
    const agent = mockAgent('/workspace');
    const policy = new IdeaToolDirectoryApprovePermissionPolicy(agent);
    const result = policy.evaluate(mockContext('SaveIdeaReport', [
      '/workspace/.ody-code/ideas/../plans/foo.md',
    ]));
    expect(result).toBeUndefined();
  });

  it('does not approve non-file accesses under .ody-code/ideas/', () => {
    const agent = mockAgent('/workspace');
    const policy = new IdeaToolDirectoryApprovePermissionPolicy(agent);
    const context = {
      toolCall: { name: 'SaveIdeaReport', id: 'call_1', arguments: {} },
      execution: {
        accesses: [{ kind: 'network', operation: 'write', path: '/workspace/.ody-code/ideas/foo.md' }],
        approvalRule: 'SaveIdeaReport',
      },
    } as unknown as PermissionPolicyContext;
    const result = policy.evaluate(context);
    expect(result).toBeUndefined();
  });

  it('returns undefined for empty accesses', () => {
    const agent = mockAgent('/workspace');
    const policy = new IdeaToolDirectoryApprovePermissionPolicy(agent);
    const context = {
      toolCall: { name: 'SaveIdeaReport', id: 'call_1', arguments: {} },
      execution: {
        accesses: [],
        approvalRule: 'SaveIdeaReport',
      },
    } as unknown as PermissionPolicyContext;
    const result = policy.evaluate(context);
    expect(result).toBeUndefined();
  });

  it('does not approve reads under .ody-code/ideas/', () => {
    const agent = mockAgent('/workspace');
    const policy = new IdeaToolDirectoryApprovePermissionPolicy(agent);
    const context = {
      toolCall: { name: 'Read', id: 'call_1', arguments: {} },
      execution: {
        accesses: [{ kind: 'file', operation: 'read', path: '/workspace/.ody-code/ideas/foo.md' }],
        approvalRule: 'Read',
      },
    } as unknown as PermissionPolicyContext;
    const result = policy.evaluate(context);
    expect(result).toBeUndefined();
  });
});
