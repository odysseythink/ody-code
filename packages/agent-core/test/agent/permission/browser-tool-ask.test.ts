import type { ToolCall } from '@odysseythink/kosong';
import { describe, expect, it } from 'vitest';

import type { PermissionPolicyContext } from '../../../src/agent/permission';
import { BrowserToolAskPermissionPolicy } from '../../../src/agent/permission/policies/browser-tool-ask';

const signal = new AbortController().signal;

function policyContext(toolName: string): PermissionPolicyContext {
  return {
    turnId: '0',
    stepNumber: 1,
    signal,
    llm: {},
    args: {},
    toolCall: {
      type: 'function',
      id: `call_${toolName}`,
      name: toolName,
      arguments: '{}',
    } satisfies ToolCall,
    execution: {
      accesses: {},
      approvalRule: toolName,
      execute: async () => ({ output: '' }),
    },
  } as unknown as PermissionPolicyContext;
}

describe('BrowserToolAskPermissionPolicy', () => {
  const policy = new BrowserToolAskPermissionPolicy();

  it('returns ask for chrome-devtools navigate tool', () => {
    const result = policy.evaluate(policyContext('mcp__chrome-devtools__navigate'));
    expect(result).toEqual({
      kind: 'ask',
      reason: { tool: 'mcp__chrome-devtools__navigate' },
    });
  });

  it('returns ask for chrome-devtools screenshot tool', () => {
    const result = policy.evaluate(policyContext('mcp__chrome-devtools__take_screenshot'));
    expect(result).toEqual({
      kind: 'ask',
      reason: { tool: 'mcp__chrome-devtools__take_screenshot' },
    });
  });

  it('returns undefined for non-browser MCP tools', () => {
    expect(policy.evaluate(policyContext('mcp__github__create_pr'))).toBeUndefined();
  });

  it('returns undefined for builtin tools', () => {
    expect(policy.evaluate(policyContext('Read'))).toBeUndefined();
  });

  it('returns undefined for Write tool', () => {
    expect(policy.evaluate(policyContext('Write'))).toBeUndefined();
  });

  it('returns ask for native BrowserBrowse tool', () => {
    const result = policy.evaluate(policyContext('BrowserBrowse'));
    expect(result).toEqual({
      kind: 'ask',
      reason: { tool: 'BrowserBrowse' },
    });
  });

  it('returns ask for native BrowserSnapshot tool', () => {
    const result = policy.evaluate(policyContext('BrowserSnapshot'));
    expect(result).toEqual({
      kind: 'ask',
      reason: { tool: 'BrowserSnapshot' },
    });
  });

  it('returns undefined for non-browser builtin tools', () => {
    expect(policy.evaluate(policyContext('Read'))).toBeUndefined();
    expect(policy.evaluate(policyContext('Write'))).toBeUndefined();
  });
});
