# Phase B: Core 集成与权限

---

### Task 4: CoreImpl — 合并内置 MCP 配置与注册表初始化

**Depends on:** Task 2, Task 3

**Files:**
- Modify: `packages/agent-core/src/rpc/core-impl.ts`

**步骤:**

- [ ] 在 `packages/agent-core/src/rpc/core-impl.ts` 顶部添加 import（在现有 `import` 块中）：

```typescript
import { getRootLogger } from '#/logging/logger';
import { BuiltInMcpRegistry } from '../mcp/built-in';
import { createChromeDevToolsServerDefinition } from '../mcp/built-in/chrome-devtools';
import { BuiltInRootNotFoundError } from '../mcp/built-in/resolve-root';
```

- [ ] 在 `KimiCore` 类中 `private readonly appVersion: string | undefined;` 之后添加字段（约 L139）：

```typescript
  private builtInMcpRegistry: BuiltInMcpRegistry;
```

- [ ] 在 `KimiCore` 构造函数中 `this.plugins = new PluginManager(...)` 之后添加初始化（约 L166）：

```typescript
    this.builtInMcpRegistry = new BuiltInMcpRegistry();
    try {
      this.builtInMcpRegistry.register(createChromeDevToolsServerDefinition());
    } catch (error) {
      if (error instanceof BuiltInRootNotFoundError) {
        getRootLogger().warn('Built-in MCP server not found', { server: error.serverName });
      } else {
        throw error;
      }
    }
```

- [ ] 在 `createSession` 中（约 L200），将：

```typescript
    const mcpConfig = this.mergePluginMcpConfig(baseMcpConfig);
```

替换为：

```typescript
    let mcpConfig = this.mergePluginMcpConfig(baseMcpConfig);
    mcpConfig = this.mergeBuiltInMcpConfig(mcpConfig, { sessionId: id, config });
```

- [ ] 在 `resumeSession` 中（约 L290），将：

```typescript
    const mcpConfig = this.mergePluginMcpConfig(baseMcpConfig);
```

替换为：

```typescript
    let mcpConfig = this.mergePluginMcpConfig(baseMcpConfig);
    mcpConfig = this.mergeBuiltInMcpConfig(mcpConfig, { sessionId: summary.id, config });
```

- [ ] 在 `mergePluginMcpConfig` 方法（约 L736）之后添加新方法：

```typescript
  private mergeBuiltInMcpConfig(
    base: SessionMcpConfig | undefined,
    ctx: { sessionId: string; config: KimiConfig },
  ): SessionMcpConfig | undefined {
    const builtInServers = this.builtInMcpRegistry.getEnabledConfigs(
      {
        kimiHomeDir: this.homeDir,
        sessionId: ctx.sessionId,
        chromePort: ctx.config.browser?.chromePort,
      },
      ctx.config,
    );
    if (Object.keys(builtInServers).length === 0) return base;
    return {
      servers: {
        ...base?.servers,
        ...builtInServers,
      },
    };
  }
```

- [ ] 运行全树 typecheck（共享签名/结构变更后必须）：

```bash
pnpm -r typecheck
```

期望：无类型错误。

- [ ] 运行现有 MCP 和 Session 相关测试，确认无回归：

```bash
cd packages/agent-core && pnpm vitest run test/mcp/connection-manager.test.ts test/session/cron-stop-on-close.test.ts
```

期望：全部通过。

- [ ] Commit：

```bash
git add packages/agent-core/src/rpc/core-impl.ts
git commit -m "feat(core): wire BuiltInMcpRegistry into CoreImpl session creation"
```

---

### Task 5: 浏览器工具权限策略

**Depends on:** Task 4

**Files:**
- Create: `packages/agent-core/src/agent/permission/policies/browser-tool-ask.ts`
- Modify: `packages/agent-core/src/agent/permission/policies/index.ts`
- Create: `packages/agent-core/test/agent/permission/browser-tool-ask.test.ts`

**步骤:**

- [ ] 创建 `packages/agent-core/src/agent/permission/policies/browser-tool-ask.ts`：

```typescript
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
```

- [ ] 修改 `packages/agent-core/src/agent/permission/policies/index.ts`，在 `UserConfiguredAllowPermissionPolicy(agent)` 之后、`ExitPlanModeReviewAskPermissionPolicy(agent)` 之前插入：

```typescript
import { BrowserToolAskPermissionPolicy } from './browser-tool-ask';
// ...
    new UserConfiguredAllowPermissionPolicy(agent),
    new BrowserToolAskPermissionPolicy(),
    new ExitPlanModeReviewAskPermissionPolicy(agent),
```

- [ ] 编写测试 `packages/agent-core/test/agent/permission/browser-tool-ask.test.ts`：

```typescript
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
});
```

- [ ] 运行新测试：

```bash
cd packages/agent-core && pnpm vitest run test/agent/permission/browser-tool-ask.test.ts
```

期望：全部通过。

- [ ] 运行全树 typecheck：

```bash
pnpm -r typecheck
```

期望：无类型错误。

- [ ] Commit：

```bash
git add packages/agent-core/src/agent/permission/policies/browser-tool-ask.ts packages/agent-core/src/agent/permission/policies/index.ts packages/agent-core/test/agent/permission/browser-tool-ask.test.ts
git commit -m "feat(permission): add BrowserToolAskPermissionPolicy for first-use confirmation"
```

---

## Local Self-Review (Phase B)

- [ ] 1. Spec-coverage table: Task 4 覆盖 CoreImpl mergeBuiltInMcpConfig 与注册表初始化（Call-Site 1 & 2）；Task 5 覆盖浏览器工具权限确认（Security 权限模型）。全部 covered。
- [ ] 2. Placeholder scan: 无 TODO/TBD。`mergeBuiltInMcpConfig` 完整实现；`BrowserToolAskPermissionPolicy` 完整实现。
- [ ] 3. No phantom tasks: Task 4 修改了 `core-impl.ts` 并运行 typecheck + 回归测试；Task 5 创建了新策略文件、修改了策略索引、编写了测试。
- [ ] 4. Dependency soundness: Task 4 依赖 Task 2（`BuiltInMcpRegistry`）和 Task 3（`createChromeDevToolsServerDefinition`）；Task 5 依赖 Task 4（确保 `mcp__chrome-devtools__` 工具名已被注册到系统）。
- [ ] 5. Caller & build soundness: Task 4 修改了 `KimiCore` 类结构（新增字段和方法），运行了 `pnpm -r typecheck`；Task 5 修改了 `policies/index.ts` 的策略列表（运行时消费），运行了全树 typecheck。`core-impl.ts` 中 `createSession` 和 `resumeSession` 的 `mcpConfig` 变量从 `const` 改为 `let`，下游 `Session` 构造函数签名未变，类型兼容。
- [ ] 6. Test-the-risk: `BrowserToolAskPermissionPolicy` 的测试覆盖了 must-survive 输入（`mcp__github__create_pr`、`Read`、`Write` 不应被拦截）和 must-ask 输入（`mcp__chrome-devtools__navigate`、`mcp__chrome-devtools__take_screenshot`）。`core-impl.ts` 的回归测试通过 `connection-manager.test.ts` 和 `session/cron-stop-on-close.test.ts` 验证 Session 构造未破坏。
- [ ] 7. Type consistency: `mergeBuiltInMcpConfig` 使用的 `SessionMcpConfig` 类型与现有 `mergePluginMcpConfig` 一致；`BrowserToolAskPermissionPolicy.evaluate` 返回的 `PermissionPolicyResult` 与现有策略一致；`BuiltInContext.chromePort` 为 `number | undefined`，与 `BrowserConfigSchema` 一致。
