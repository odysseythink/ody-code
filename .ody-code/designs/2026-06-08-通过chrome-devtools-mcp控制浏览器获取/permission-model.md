# Part: Permission Model

## Purpose

将浏览器工具的权限控制从"每次调用都 ask"的粗粒度模式升级为"按 URL host session-level 授权"的细粒度模式，既保证安全又减少用户干扰。

## Scope

### In

- 解析浏览器工具参数中的 URL，提取 host。
- 静态 allowlist（配置文件中的 `browser.allowedHosts`）。
- Session-level 动态规则缓存（复用现有 `sessionApprovalRulePatterns`）。
- 敏感操作仍单独 ask（支付、密码、form submit 到非同域）。

### Out

- eTLD+1 通配符匹配（Phase 2） [C:DEFERRED]。
- 每页面/每路径授权 [C:DEFERRED]。
- 基于响应内容的动态策略 [C:DEFERRED]。

## Data Flow

```
Tool call BrowserBrowse(url='https://kimi.com/code/console')
            │
            ▼
    BrowserHostPermissionPolicy.evaluate(context)
            │
            ├──► host = 'kimi.com'
            ├──► check static allowlist ['localhost', '127.0.0.1']
            ├──► check sensitivePatterns ['*password*', '*payment*', '*checkout*']
            ├──► check sessionApprovalRulePatterns for 'Browser(kimi.com)'
            │
            ├──► if matched → approve
            ├──► if sensitive → ask
            └──► else → ask
                        │
                        ▼
                User dialog with option "Approve for this session"
                        │
                        ▼
                PermissionManager.recordApprovalResult()
                        └── adds 'Browser(kimi.com)' to sessionApprovalRulePatterns
```

## Typed Interfaces

```typescript
// packages/agent-core/src/agent/permission/policies/browser-host.ts

export interface BrowserHostPermissionPolicyOptions {
  readonly allowedHosts?: readonly string[];
  readonly sensitivePatterns?: readonly string[];   // glob or substring
}

export class BrowserHostPermissionPolicy implements PermissionPolicy {
  readonly name = 'browser-host';

  constructor(private readonly options: BrowserHostPermissionPolicyOptions) {}

  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
    // See algorithm below
  }
}
```

## Algorithm: `BrowserHostPermissionPolicy.evaluate()`

```text
INPUT: context (toolCall, args, execution, agent)
OUTPUT: PermissionPolicyResult | undefined

1. If toolCall.name does NOT match /^Browser/ → RETURN undefined

2. urlString ← EXTRACT from args:
     If args.url exists → urlString = args.url
     Else if args.instruction contains a URL → extract first URL
     Else → RETURN undefined

3. TRY:
     url ← new URL(urlString)
   CATCH:
     RETURN { kind: 'ask', reason: { invalid_url: urlString } }

4. host ← url.host

5. If host matches any sensitivePatterns →
     RETURN {
       kind: 'ask',
       reason: { host, sensitive: true, pattern: matchedPattern }
     }

6. If host is in options.allowedHosts →
     RETURN { kind: 'approve', reason: { host, allowlist: true } }

7. pattern ← 'Browser*(' + host + ')'
   If pattern in agent.permission.sessionApprovalRulePatterns →
     RETURN { kind: 'approve', reason: { host, session_rule: pattern } }
   # Relies on browser tools implementing execution.matchesRule(pattern)
   # so that SessionApprovalHistoryPermissionPolicy can match the host.

8. RETURN {
     kind: 'ask',
     reason: { host, tool: toolCall.name },
     resolveApproval: (response) => {
       If response.decision === 'approved' AND response.scope === 'session':
         RETURN { kind: 'approve', executionMetadata: { sessionApprovalRule: pattern } }
       If response.decision === 'approved':
         RETURN { kind: 'approve' }
       RETURN { kind: 'deny', message: 'User denied browser access to ' + host }
     }
   }
```

## Approval Rule Pattern Format

- Pattern: `Browser*(<host>)`
- Example: `Browser*(kimi.com)`, `Browser*(open.bigmodel.cn)`
- Tool-name matching: `Browser*` glob matches all native browser tools (`BrowserBrowse`, `BrowserExtract`, `BrowserAct`, atomic fallbacks) via picomatch.
- Host matching: each browser tool's `execution.matchesRule(argPattern)` compares `argPattern` to the URL host it intends to access; returns true on exact equality.
- Future: `Browser*(*.example.com)` for subdomain allow [C:DEFERRED].

## Integration with Existing Policy Chain

Policy chain order in `createPermissionDecisionPolicies()` (from `packages/agent-core/src/agent/permission/policies/index.ts`):

```text
1. plan-mode-guard-deny
2. plan-mode-tool-approve
3. auto-mode-ask-user-question-deny
4. yolo-mode-approve   // if mode is yolo, approves most things
5. auto-mode-approve   // if mode is auto, approves
6. user-configured-rules
7. session-approval-history
8. git-cwd-write-approve
9. file-access-ask
10. default-tool-approve
11. browser-tool-ask    // CURRENT
12. fallback-ask
```

New `BrowserHostPermissionPolicy` must be inserted **before** `yolo-mode-approve` and `auto-mode-approve`, so that browser access still requires host-level approval even in yolo/auto mode. Deny rules always fire first regardless.

```
6.5 browser-host          // NEW: host-level ask/approve
```

## Call-Sites

| Location | File | Lines | Action |
|---|---|---|---|
| Policy registration | `packages/agent-core/src/agent/permission/policies/index.ts` | Add `new BrowserHostPermissionPolicy({ allowedHosts: agent.kimiConfig?.browser?.allowedHosts ?? [] })` to policy list before yolo-mode-approve. |
| Approval rule | `packages/agent-core/src/agent/permission/index.ts:192-195` | Existing logic reads `context.execution.approvalRule`. Browser tools must set `approvalRule = 'Browser(' + host + ')'`. |
| Session rule caching | `packages/agent-core/src/agent/permission/index.ts:72-86` | `recordApprovalResult` already adds `sessionApprovalRule` to `localSessionApprovalRulePatterns` when scope='session'. Reused as-is. |

## Error / Degradation

| Scenario | Handling |
|---|---|
| URL 解析失败 | Ask user with reason `invalid_url`. |
| Host 为空（如 `file://`） | Treat as sensitive → ask. |
| User rejects approval | Return block reason; agent must not retry same URL. |
| User approves once but navigates到不同 host（redirect） | New host requires new approval; redirect chain should be monitored. |

## Test Assertions

1. Non-browser tool (`Read`) returns `undefined`.
2. Static allowlist host auto-approved.
3. Session rule `Browser*(kimi.com)` plus a browser tool `matchesRule('kimi.com') → true` causes subsequent `BrowserBrowse('https://kimi.com/...')` to auto-approve.
4. Different host (`bigmodel.cn`) still asks even when `kimi.com` is approved.
5. Sensitive pattern matches trigger ask even if host is in allowlist.
