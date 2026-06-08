import type { ToolCall } from '@odysseythink/kosong';
import { describe, expect, it } from 'vitest';

import type { PermissionPolicyContext } from '../../../src/agent/permission';
import { BrowserHostPermissionPolicy } from '../../../src/agent/permission/policies/browser-host';
import type { Agent } from '../../../src/agent';
import type { KimiConfig } from '../../../src/config';

const signal = new AbortController().signal;

function makePolicy(config?: KimiConfig['browser']): BrowserHostPermissionPolicy {
  const agent = { kimiConfig: { browser: config } } as unknown as Agent;
  return new BrowserHostPermissionPolicy(agent);
}

function context(
  toolName: string,
  args: Record<string, unknown> = {},
): PermissionPolicyContext {
  return {
    turnId: '0',
    stepNumber: 1,
    signal,
    llm: {},
    args,
    toolCall: {
      type: 'function',
      id: `call_${toolName}`,
      name: toolName,
      arguments: JSON.stringify(args),
    } satisfies ToolCall,
    execution: {
      accesses: {},
      approvalRule: toolName,
      execute: async () => ({ output: '' }),
    },
  } as unknown as PermissionPolicyContext;
}

describe('BrowserHostPermissionPolicy', () => {
  it('returns undefined for non-browser tools', () => {
    const policy = makePolicy();
    expect(policy.evaluate(context('Read'))).toBeUndefined();
    expect(policy.evaluate(context('Write'))).toBeUndefined();
  });

  it('returns undefined when no URL argument is present', () => {
    const policy = makePolicy();
    expect(policy.evaluate(context('BrowserBrowse', {}))).toBeUndefined();
    expect(policy.evaluate(context('BrowserNavigate', { selector: '#btn' }))).toBeUndefined();
  });

  it('returns ask for invalid URL', () => {
    const policy = makePolicy();
    const result = policy.evaluate(context('BrowserBrowse', { url: 'not-a-url' }));
    expect(result).toEqual({ kind: 'ask', reason: { invalid_url: 'not-a-url' } });
  });

  it('approves allowed hosts', () => {
    const policy = makePolicy({ allowedHosts: ['example.com', 'docs.example.com'] });
    const result = policy.evaluate(context('BrowserBrowse', { url: 'https://example.com/page' }));
    expect(result).toEqual({ kind: 'approve', reason: { host: 'example.com', allowlist: true } });
  });

  it('approves sub-domain in allowedHosts', () => {
    const policy = makePolicy({ allowedHosts: ['docs.example.com'] });
    const result = policy.evaluate(context('BrowserBrowse', { url: 'https://docs.example.com/page' }));
    expect(result).toEqual({ kind: 'approve', reason: { host: 'docs.example.com', allowlist: true } });
  });

  it('asks for non-allowed host', () => {
    const policy = makePolicy({ allowedHosts: ['example.com'] });
    const result = policy.evaluate(context('BrowserBrowse', { url: 'https://evil.com/page' }));
    expect(result).toEqual({ kind: 'ask', reason: { host: 'evil.com' } });
  });

  it('asks when URL matches sensitive pattern on non-allowed host', () => {
    const policy = makePolicy({
      sensitivePatterns: ['\\/admin', '\\/settings'],
    });
    const result = policy.evaluate(context('BrowserBrowse', { url: 'https://example.com/admin' }));
    expect(result).toEqual({ kind: 'ask', reason: { host: 'example.com', sensitive: true } });
  });

  it('asks for unknown host by default', () => {
    const policy = makePolicy();
    const result = policy.evaluate(context('BrowserBrowse', { url: 'https://unknown.test/page' }));
    expect(result).toEqual({ kind: 'ask', reason: { host: 'unknown.test' } });
  });

  it('skips invalid regex in sensitivePatterns', () => {
    const policy = makePolicy({
      sensitivePatterns: ['[invalid', '\\/safe'],
    });
    // The invalid regex should not throw; /safe should not match this URL.
    const result = policy.evaluate(context('BrowserBrowse', { url: 'https://example.com/page' }));
    expect(result).toEqual({ kind: 'ask', reason: { host: 'example.com' } });
  });

  it('allowlist takes precedence over sensitive patterns', () => {
    const policy = makePolicy({
      allowedHosts: ['example.com'],
      sensitivePatterns: ['.*'],
    });
    const result = policy.evaluate(context('BrowserBrowse', { url: 'https://example.com/anything' }));
    expect(result).toEqual({ kind: 'approve', reason: { host: 'example.com', allowlist: true } });
  });
});
