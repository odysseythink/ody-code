# Phase C: Permissions & Integration — 权限策略、MCP 兼容、生命周期清理

---

### Task 11: Create `browser-rule-match` support helper

**Depends on:** `tools.md` Task 10

**Files:**
- Create: `packages/agent-core/src/tools/support/browser-rule-match.ts`
- Modify: `packages/agent-core/src/tools/builtin/browser/_utils.ts`

**Steps:**

- [ ] Create `packages/agent-core/src/tools/support/browser-rule-match.ts`:

```typescript
const GLOB_LITERAL_SPECIAL = /[\\*?[\]{}()!+@|]/g;

export function browserHostApprovalRule(host: string): string {
  return `Browser*(${host.replace(GLOB_LITERAL_SPECIAL, '\\$&')})`;
}

export function matchesBrowserHostRule(ruleArgs: string, host: string): boolean {
  return ruleArgs === host;
}
```

- [ ] Update `packages/agent-core/src/tools/builtin/browser/_utils.ts` to re-export from the support module:

```typescript
export { browserHostApprovalRule, matchesBrowserHostRule } from '../../../tools/support/browser-rule-match';

export function truncateText(text: string, maxLength: number): string {
  if (text.length <= maxLength) return text;
  return text.slice(0, maxLength) + '\n[...truncated]';
}
```

- [ ] Run typecheck:

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core && pnpm typecheck
```

Expected: passes.

- [ ] Commit:

```bash
git add packages/agent-core/src/tools/support/browser-rule-match.ts packages/agent-core/src/tools/builtin/browser/_utils.ts && git commit -m "refactor(browser): move browser rule-match helper to support directory"
```

---

### Task 12: Implement `BrowserHostPermissionPolicy`

**Depends on:** Task 11

**Files:**
- Modify: `packages/agent-core/src/config/schema.ts` (add `allowedHosts`, `sensitivePatterns`)
- Create: `packages/agent-core/src/agent/permission/policies/browser-host.ts`
- Modify: `packages/agent-core/src/agent/permission/policies/index.ts`

**Steps:**

- [ ] First, update `BrowserConfigSchema` in `packages/agent-core/src/config/schema.ts:186-193` to add the two new fields:

```typescript
export const BrowserConfigSchema = z.object({
  enabled: z.boolean().optional(),
  chromePort: z.number().int().min(1).max(65535).optional(),
  traceEnabled: z.boolean().optional(),
  traceRetentionDays: z.number().int().min(1).optional(),
  autoLaunch: z.boolean().optional(),
  headless: z.boolean().optional(),
  executablePath: z.string().optional(),
  legacyMcpEnabled: z.boolean().optional(),
  allowedHosts: z.array(z.string()).optional(),
  sensitivePatterns: z.array(z.string()).optional(),
});
```

- [ ] Create `packages/agent-core/src/agent/permission/policies/browser-host.ts`:

```typescript
import type { Agent } from '../..';
import type { PermissionPolicy, PermissionPolicyContext, PermissionPolicyResult } from '../types';

export class BrowserHostPermissionPolicy implements PermissionPolicy {
  readonly name = 'browser-host';

  constructor(private readonly agent: Agent) {}

  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
    if (!context.toolCall.name.startsWith('Browser')) return;

    const url = this.extractUrl(context.args);
    if (!url) return;

    let host: string;
    try {
      host = new URL(url).host;
    } catch {
      return { kind: 'ask', reason: { invalid_url: url } };
    }

    const config = this.agent.kimiConfig?.browser;

    // Static allowlist
    if (config?.allowedHosts?.includes(host)) {
      return { kind: 'approve', reason: { host, allowlist: true } };
    }

    // Sensitive patterns (always ask)
    const sensitivePatterns = config?.sensitivePatterns ?? [];
    for (const pattern of sensitivePatterns) {
      try {
        if (new RegExp(pattern).test(url)) {
          return { kind: 'ask', reason: { host, sensitive: true } };
        }
      } catch {
        // Invalid regex, skip
      }
    }

    // Default: ask for unknown host
    return { kind: 'ask', reason: { host } };
  }

  private extractUrl(args: unknown): string | undefined {
    if (typeof args !== 'object' || args === null) return;
    const obj = args as Record<string, unknown>;
    if (typeof obj.url === 'string') return obj.url;
    return;
  }
}
```

- [ ] Modify `packages/agent-core/src/agent/permission/policies/index.ts` — insert `BrowserHostPermissionPolicy` after `UserConfiguredAllowPermissionPolicy` and before `BrowserToolAskPermissionPolicy`:

Current policy chain (lines 27-65):

```typescript
export function createPermissionDecisionPolicies(agent: Agent): readonly PermissionPolicy[] {
  return [
    new PreToolCallHookPermissionPolicy(agent),
    new AutoModeAskUserQuestionDenyPermissionPolicy(agent),
    new PlanModeGuardDenyPermissionPolicy(agent),
    new UserConfiguredDenyPermissionPolicy(agent),
    new AutoModeApprovePermissionPolicy(agent),
    new SessionApprovalHistoryPermissionPolicy(agent),
    new UserConfiguredAskPermissionPolicy(agent),
    new UserConfiguredAllowPermissionPolicy(agent),
    new BrowserToolAskPermissionPolicy(),
    new ExitPlanModeReviewAskPermissionPolicy(agent),
    new PlanModeToolApprovePermissionPolicy(agent),
    new SensitiveFileAccessAskPermissionPolicy(agent),
    new GitControlPathAccessAskPermissionPolicy(agent),
    new CwdOutsideFileWriteAskPermissionPolicy(agent),
    new YoloModeApprovePermissionPolicy(agent),
    new DefaultToolApprovePermissionPolicy(),
    new GitCwdWriteApprovePermissionPolicy(agent),
    new FallbackAskPermissionPolicy(),
  ];
}
```

Add `new BrowserHostPermissionPolicy(agent),` after `new UserConfiguredAllowPermissionPolicy(agent),`:

```typescript
    new UserConfiguredAllowPermissionPolicy(agent),
    new BrowserHostPermissionPolicy(agent),
    new BrowserToolAskPermissionPolicy(),
```

- [ ] Run typecheck:

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core && pnpm typecheck
```

Expected: passes.

- [ ] Commit:

```bash
git add packages/agent-core/src/config/schema.ts packages/agent-core/src/agent/permission/policies/browser-host.ts packages/agent-core/src/agent/permission/policies/index.ts && git commit -m "feat(permission): add BrowserHostPermissionPolicy with URL host authorization"
```

---

### Task 13: Update `BrowserToolAskPermissionPolicy`

**Depends on:** Task 12

**Files:**
- Modify: `packages/agent-core/src/agent/permission/policies/browser-tool-ask.ts`
- Modify: `packages/agent-core/test/agent/permission/browser-tool-ask.test.ts`

**Steps:**

- [ ] Update `packages/agent-core/src/agent/permission/policies/browser-tool-ask.ts`:

```typescript
import type { PermissionPolicy, PermissionPolicyContext, PermissionPolicyResult } from '../types';

export class BrowserToolAskPermissionPolicy implements PermissionPolicy {
  readonly name = 'browser-tool-ask';

  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
    const name = context.toolCall.name;
    if (!name.startsWith('mcp__chrome-devtools__') && !name.startsWith('Browser')) return;
    return {
      kind: 'ask',
      reason: { tool: name },
    };
  }
}
```

- [ ] Update `packages/agent-core/test/agent/permission/browser-tool-ask.test.ts` — append after the existing tests:

```typescript
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
```

- [ ] Run tests:

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core && pnpm test -- test/agent/permission/browser-tool-ask.test.ts
```

Expected: all tests pass (existing + new).

- [ ] Commit:

```bash
git add packages/agent-core/src/agent/permission/policies/browser-tool-ask.ts packages/agent-core/test/agent/permission/browser-tool-ask.test.ts && git commit -m "feat(permission): extend BrowserToolAskPermissionPolicy to cover native Browser* tools"
```

---

### Task 14: Update `BuiltInMcpRegistry.isDisabled` for legacy MCP

**Depends on:** Task 13

**Files:**
- Modify: `packages/agent-core/src/mcp/built-in/registry.ts:44-51`

**Steps:**

- [ ] Update `isDisabled` in `packages/agent-core/src/mcp/built-in/registry.ts`:

```typescript
  isDisabled(name: string, config: KimiConfig): boolean {
    const def = this.definitions.get(name);
    if (def === undefined) return true;
    if (name === 'chrome-devtools') {
      // Legacy MCP: disabled by default; only enabled when legacyMcpEnabled === true
      return config.browser?.legacyMcpEnabled !== true;
    }
    return !def.enabledByDefault;
  }
```

- [ ] Verify by reading the existing registry tests or creating a quick check:

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core && pnpm typecheck
```

Expected: passes.

- [ ] Commit:

```bash
git add packages/agent-core/src/mcp/built-in/registry.ts && git commit -m "feat(mcp): disable chrome-devtools MCP by default, enable only with legacyMcpEnabled"
```

---

### Task 15: Add browser connection cleanup to Session lifecycle

**Depends on:** Task 14

**Files:**
- Modify: `packages/agent-core/src/session/index.ts:214-229`

**Steps:**

- [ ] Modify `Session.close()` in `packages/agent-core/src/session/index.ts`:

Current code (lines 214-229):

```typescript
  async close(): Promise<void> {
    try {
      await Promise.allSettled(
        Array.from(this.agents.values(), async (agent) => agent.cron?.stop()),
      );
      await this.stopBackgroundTasksOnExit();
      await this.flushMetadata();
      await this.triggerSessionEnd('exit');
    } finally {
      try {
        await this.mcp.shutdown();
      } finally {
        await this.logHandle?.close();
      }
    }
  }
```

Add browser connection cleanup alongside cron cleanup:

```typescript
  async close(): Promise<void> {
    try {
      await Promise.allSettled(
        Array.from(this.agents.values(), async (agent) => {
          await agent.cron?.stop();
          await agent.browserConnection?.closeAll();
        }),
      );
      await this.stopBackgroundTasksOnExit();
      await this.flushMetadata();
      await this.triggerSessionEnd('exit');
    } finally {
      try {
        await this.mcp.shutdown();
      } finally {
        await this.logHandle?.close();
      }
    }
  }
```

- [ ] Run typecheck:

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core && pnpm typecheck
```

Expected: passes (`browserConnection` is optional on `Agent`).

- [ ] Commit:

```bash
git add packages/agent-core/src/session/index.ts && git commit -m "feat(session): clean up browser connections on Session.close()"
```

---

### Task 16: Tests for `BrowserHostPermissionPolicy`

**Depends on:** Task 15

**Files:**
- Create: `packages/agent-core/test/agent/permission/browser-host.test.ts`

**Steps:**

- [ ] Write the test file:

```typescript
import { describe, expect, it, vi } from 'vitest';
import { BrowserHostPermissionPolicy } from '../../../src/agent/permission/policies/browser-host';
import type { PermissionPolicyContext } from '../../../src/agent/permission';
import type { Agent } from '../../../src/agent';

const signal = new AbortController().signal;

function mockAgent(options: {
  allowedHosts?: string[];
  sensitivePatterns?: string[];
} = {}): Agent {
  return {
    kimiConfig: {
      browser: {
        allowedHosts: options.allowedHosts,
        sensitivePatterns: options.sensitivePatterns,
      },
    },
    permission: {
      sessionApprovalRulePatterns: [],
    },
  } as unknown as Agent;
}

function policyContext(toolName: string, args: Record<string, unknown> = {}): PermissionPolicyContext {
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
      arguments: '{}',
    },
    execution: {
      accesses: {},
      approvalRule: toolName,
      execute: async () => ({ output: '' }),
    },
  } as unknown as PermissionPolicyContext;
}

describe('BrowserHostPermissionPolicy', () => {
  it('returns undefined for non-browser tools', () => {
    const policy = new BrowserHostPermissionPolicy(mockAgent());
    expect(policy.evaluate(policyContext('Read'))).toBeUndefined();
    expect(policy.evaluate(policyContext('Write'))).toBeUndefined();
    expect(policy.evaluate(policyContext('Bash'))).toBeUndefined();
  });

  it('returns ask for BrowserBrowse with unknown host', () => {
    const policy = new BrowserHostPermissionPolicy(mockAgent());
    const result = policy.evaluate(policyContext('BrowserBrowse', { url: 'https://evil.test/path' }));
    expect(result).toEqual({ kind: 'ask', reason: { host: 'evil.test' } });
  });

  it('returns approve for host in allowedHosts', () => {
    const policy = new BrowserHostPermissionPolicy(mockAgent({ allowedHosts: ['kimi.com'] }));
    const result = policy.evaluate(policyContext('BrowserBrowse', { url: 'https://kimi.com/code' }));
    expect(result).toEqual({ kind: 'approve', reason: { host: 'kimi.com', allowlist: true } });
  });

  it('returns ask for sensitive URL matching sensitivePatterns', () => {
    const policy = new BrowserHostPermissionPolicy(mockAgent({ sensitivePatterns: ['/pay', 'checkout'] }));
    const result = policy.evaluate(policyContext('BrowserBrowse', { url: 'https://shop.test/checkout' }));
    expect(result).toEqual({ kind: 'ask', reason: { host: 'shop.test', sensitive: true } });
  });

  it('returns ask for invalid URL', () => {
    const policy = new BrowserHostPermissionPolicy(mockAgent());
    const result = policy.evaluate(policyContext('BrowserBrowse', { url: 'not-a-url' }));
    expect(result).toEqual({ kind: 'ask', reason: { invalid_url: 'not-a-url' } });
  });

  it('returns undefined for browser tool without URL', () => {
    const policy = new BrowserHostPermissionPolicy(mockAgent());
    expect(policy.evaluate(policyContext('BrowserClick', { selector: '#btn' }))).toBeUndefined();
  });

  it('returns ask for BrowserNavigate with unknown host', () => {
    const policy = new BrowserHostPermissionPolicy(mockAgent());
    const result = policy.evaluate(policyContext('BrowserNavigate', { url: 'https://example.com' }));
    expect(result).toEqual({ kind: 'ask', reason: { host: 'example.com' } });
  });

  it('subdomain does not match parent domain in allowedHosts', () => {
    const policy = new BrowserHostPermissionPolicy(mockAgent({ allowedHosts: ['kimi.com'] }));
    const result = policy.evaluate(policyContext('BrowserBrowse', { url: 'https://evil.kimi.com' }));
    // evil.kimi.com is NOT in allowedHosts, so it should ask
    expect(result).toEqual({ kind: 'ask', reason: { host: 'evil.kimi.com' } });
  });
});
```

- [ ] Run tests:

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core && pnpm test -- test/agent/permission/browser-host.test.ts
```

Expected: all 8 tests pass.

- [ ] Commit:

```bash
git add packages/agent-core/test/agent/permission/browser-host.test.ts && git commit -m "test(permission): add BrowserHostPermissionPolicy unit tests"
```

---

## Local Self-Review

- [ ] **1. Spec-coverage table:** Phase C covers all permission layer requirements — `BrowserHostPermissionPolicy` (Task 12), `BrowserToolAskPermissionPolicy` update (Task 13), MCP legacy registry (Task 14), Session cleanup (Task 15), permission tests (Task 16).
- [ ] **2. Placeholder scan:** No TODO/TBD in any code block. All implementations are complete.
- [ ] **3. No phantom tasks:** Every task produces verifiable changes — support helper refactor, permission policy + config schema update, policy chain insertion, ask policy update, registry logic change, session cleanup, test file.
- [ ] **4. Dependency soundness:** Task 11 (tools.md: Task 10) → Task 12 (Task 11) → Task 13 (Task 12) → Task 14 (Task 13) → Task 15 (Task 14) → Task 16 (Task 15). Correct chain.
- [ ] **5. Caller & build soundness:** Task 12 changes `BrowserConfigSchema` (adding `allowedHosts` and `sensitivePatterns`). This schema is referenced by `KimiConfigSchema` and `KimiConfigPatchSchema` in the same file, both automatically pick up the new fields. No external callers need updating. Task 12 also inserts `BrowserHostPermissionPolicy` into the policy chain — no signature change, just adding an element to an array. Task 15 modifies `Session.close()` which is an internal method. All tasks end with typecheck.
- [ ] **6. Test-the-risk:**
  - Static allowlist approval tested (Task 16, "returns approve for host in allowedHosts").
  - Sensitive pattern matching tested (Task 16, "returns ask for sensitive URL").
  - Subdomain isolation tested (Task 16, "subdomain does not match parent domain").
  - Invalid URL handling tested (Task 16, "returns ask for invalid URL").
  - Native browser tool catch-all tested (Task 13, browser-tool-ask.test.ts updates).
- [ ] **7. Type consistency:** `BrowserHostPermissionPolicy` uses `agent.kimiConfig?.browser?.allowedHosts` and `agent.kimiConfig?.browser?.sensitivePatterns`, which match the fields added to `BrowserConfigSchema` in Task 12. `Session.close()` calls `agent.browserConnection?.closeAll()`, matching the optional field added in `tools.md` Task 9.
