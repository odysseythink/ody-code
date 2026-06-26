# Part A: 创建 `@odysseythink/agent-core-shared`

本 Part 将 `agent-core` 中被多包引用的基础类型与工具逐步下沉到新的 `@odysseythink/agent-core-shared`，并同步把 `agent-core` 内部调用点切到新包。完成后 `agent-core-shared` 是这些原语的唯一真实来源。

---

### Task A1: 创建 `agent-core-shared` 包骨架

**Depends on:** none  
**Files:**
- Create: `packages/agent-core-shared/package.json`
- Create: `packages/agent-core-shared/tsconfig.json`
- Create: `packages/agent-core-shared/vitest.config.ts`
- Create: `packages/agent-core-shared/src/index.ts`

- [ ] 编写 `packages/agent-core-shared/package.json`：

```json
{
  "name": "@odysseythink/agent-core-shared",
  "version": "0.1.0",
  "private": true,
  "description": "Shared primitives extracted from agent-core",
  "license": "MIT",
  "type": "module",
  "imports": {
    "#/*": ["./src/*.ts", "./src/*/index.ts"]
  },
  "exports": {
    ".": {
      "types": "./src/index.ts",
      "default": "./src/index.ts"
    }
  },
  "scripts": {
    "build": "tsdown",
    "test": "vitest run",
    "typecheck": "tsc -p tsconfig.json --noEmit",
    "clean": "rm -rf dist"
  },
  "dependencies": {
    "picomatch": "^4.0.4",
    "zod": "catalog:"
  },
  "devDependencies": {
    "@types/picomatch": "^4.0.3"
  }
}
```

- [ ] 编写 `packages/agent-core-shared/tsconfig.json`：

```json
{
  "extends": "../../tsconfig.json",
  "compilerOptions": {},
  "include": ["src", "test"]
}
```

- [ ] 编写 `packages/agent-core-shared/vitest.config.ts`：

```ts
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    name: 'agent-core-shared',
    include: ['test/**/*.test.ts'],
  },
});
```

- [ ] 编写占位 `packages/agent-core-shared/src/index.ts`：

```ts
export {};
```

- [ ] 运行包级 typecheck，确认骨架通过：

```bash
cd packages/agent-core-shared && pnpm typecheck
```

预期：无错误，`error code 0`。

- [ ] 提交：`git add packages/agent-core-shared && git commit -m "chore(shared): bootstrap agent-core-shared package"`


### Task A2: 迁移 `errors` 模块到 shared

**Depends on:** Task A1  
**Files:**
- Create: `packages/agent-core-shared/src/errors/codes.ts`
- Create: `packages/agent-core-shared/src/errors/classes.ts`
- Create: `packages/agent-core-shared/src/errors/serialize.ts`
- Create: `packages/agent-core-shared/src/errors/index.ts`
- Create: `packages/agent-core-shared/test/errors/codes.test.ts`
- Modify: `packages/agent-core-shared/src/index.ts`
- Modify: 26 files under `packages/agent-core/src/**` and `packages/agent-core/test/**` that import from `#/errors`
- Delete: `packages/agent-core/src/errors/`
- Delete: `packages/agent-core/test/errors/`

- [ ] 将 `packages/agent-core/src/errors/codes.ts` 原样复制到 `packages/agent-core-shared/src/errors/codes.ts`。

- [ ] 将 `packages/agent-core/src/errors/classes.ts` 原样复制到 `packages/agent-core-shared/src/errors/classes.ts`。

- [ ] 将 `packages/agent-core/src/errors/serialize.ts` 原样复制到 `packages/agent-core-shared/src/errors/serialize.ts`。

- [ ] 将 `packages/agent-core/src/errors/index.ts` 原样复制到 `packages/agent-core-shared/src/errors/index.ts`。

- [ ] 更新 `packages/agent-core-shared/src/index.ts`：

```ts
export {
  ErrorCodes,
  ODY_ERROR_INFO,
  type OdyErrorCode,
  type OdyErrorInfo,
  OdyError,
  type OdyErrorOptions,
  fromOdyErrorPayload,
  isOdyError,
  makeErrorPayload,
  toOdyErrorPayload,
  type OdyErrorPayload,
} from './errors';
```

- [ ] 将 `packages/agent-core/test/errors/codes.test.ts` 复制到 `packages/agent-core-shared/test/errors/codes.test.ts`，并将其中所有 `#/errors` 替换为 `#/errors`（仍在 shared 包内，路径不变）或相对路径 `#/errors`。测试内容保持不变。

- [ ] 在 `packages/agent-core-shared` 运行测试，确认通过：

```bash
cd packages/agent-core-shared && pnpm test
```

预期：`errors/codes.test.ts` 通过。

- [ ] 搜索并替换 agent-core 中所有 `#/errors` import：

```bash
cd packages/agent-core
rg "from '#\/errors'" src test -l
```

预期列出约 25 个文件（如 `src/agent/index.ts`、`src/config/schema.ts`、...）。然后执行：

```bash
# 注意：先确认 rg 列出的文件再运行
find src test -name '*.ts' -exec sed -i "" "s|from '#/errors'|from '@odysseythink/agent-core-shared'|g" {} +
find src test -name '*.ts' -exec sed -i "" "s|from '#/errors/|from '@odysseythink/agent-core-shared/|g" {} +
```

- [ ] 删除 agent-core 中旧的 errors 模块：

```bash
rm -rf packages/agent-core/src/errors
rm -rf packages/agent-core/test/errors
```

- [ ] 运行 agent-core typecheck：

```bash
cd packages/agent-core && pnpm typecheck
```

预期：无 `#/errors` 相关错误，仅可能因尚未迁移的其他模块（如 logging/abort 等）报错；若只有这些后续迁移项的报错，可继续。

- [ ] 提交：`git add packages/agent-core-shared packages/agent-core && git commit -m "refactor(shared): move errors to agent-core-shared"`


### Task A3: 迁移日志接口类型到 shared

**Depends on:** Task A2  
**Files:**
- Create: `packages/agent-core-shared/src/logging.ts`
- Modify: `packages/agent-core-shared/src/index.ts`
- Modify: `packages/agent-core/src/logging/types.ts`
- Modify: 所有 `import type { Logger, LogLevel, LogContext, LogPayload } from '#/logging/types'` 的文件

- [ ] 将 `packages/agent-core/src/logging/types.ts` 的全部内容复制到 `packages/agent-core-shared/src/logging.ts`。

- [ ] 在 `packages/agent-core-shared/src/index.ts` 追加：

```ts
export type {
  Logger,
  LogLevel,
  LogContext,
  LogPayload,
  LogEntry,
  LoggingConfig,
  SessionLogHandle,
  SessionAttachInput,
  RootLogger,
} from './logging';
```

- [ ] 将 `packages/agent-core/src/logging/types.ts` 替换为 re-export：

```ts
export type {
  Logger,
  LogLevel,
  LogContext,
  LogPayload,
  LogEntry,
  LoggingConfig,
  SessionLogHandle,
  SessionAttachInput,
  RootLogger,
} from '@odysseythink/agent-core-shared';
```

- [ ] 搜索需要改 import 的文件：

```bash
cd packages/agent-core
rg "from '#\/logging/types'" src test -l
```

将这些文件中的 `from '#/logging/types'` 替换为 `from '@odysseythink/agent-core-shared'`。

- [ ] 运行 `packages/agent-core-shared` typecheck：

```bash
cd packages/agent-core-shared && pnpm typecheck
```

- [ ] 运行 `packages/agent-core` typecheck，确认 logging 路径错误消失：

```bash
cd packages/agent-core && pnpm typecheck
```

- [ ] 提交。

---

### Task A4: 迁移 `utils/abort.ts` 到 shared

**Depends on:** Task A3  
**Files:**
- Create: `packages/agent-core-shared/src/abort.ts`
- Create: `packages/agent-core-shared/test/abort.test.ts`
- Modify: `packages/agent-core-shared/src/index.ts`
- Modify: `packages/agent-core/src/utils/abort.ts`
- Modify: 所有 `from '#/utils/abort'` import

- [ ] 将 `packages/agent-core/src/utils/abort.ts` 原样复制到 `packages/agent-core-shared/src/abort.ts`。

- [ ] 将 `packages/agent-core/test/utils/abort.test.ts` 复制到 `packages/agent-core-shared/test/abort.test.ts`。

- [ ] 更新 `packages/agent-core-shared/src/index.ts` 追加：

```ts
export {
  abortError,
  abortable,
  UserCancellationError,
  isUserCancellation,
  createDeadlineAbortSignal,
  type DeadlineAbortSignal,
  linkAbortSignal,
} from './abort';
```

- [ ] 将 `packages/agent-core/src/utils/abort.ts` 替换为 re-export：

```ts
export {
  abortError,
  abortable,
  UserCancellationError,
  isUserCancellation,
  createDeadlineAbortSignal,
  type DeadlineAbortSignal,
  linkAbortSignal,
} from '@odysseythink/agent-core-shared';
```

- [ ] 搜索并替换其他 `#/utils/abort` import：

```bash
cd packages/agent-core
rg "from '#\/utils/abort'" src test -l
```

将这些文件中的 `from '#/utils/abort'` 替换为 `from '@odysseythink/agent-core-shared'`。

- [ ] 在 shared 运行 abort 测试：

```bash
cd packages/agent-core-shared && pnpm test
```

- [ ] 在 agent-core 运行 typecheck。

- [ ] 提交。

---

### Task A5: 迁移 `version.ts` 到 shared

**Depends on:** Task A4  
**Files:**
- Create: `packages/agent-core-shared/src/version.ts`
- Create: `packages/agent-core-shared/test/version.test.ts`
- Modify: `packages/agent-core-shared/src/index.ts`
- Modify: `packages/agent-core/src/version.ts`

- [ ] 将 `packages/agent-core/src/version.ts` 原样复制到 `packages/agent-core-shared/src/version.ts`（注意它读取的是 `../package.json`，在 shared 中会读取 shared 自身版本）。

- [ ] 在 `packages/agent-core-shared/test/version.test.ts` 写最小测试：

```ts
import { describe, expect, it } from 'vitest';
import { getCoreVersion } from '../src/version';

describe('getCoreVersion', () => {
  it('returns a non-empty semver string', () => {
    const version = getCoreVersion();
    expect(version).toMatch(/^\d+\.\d+\.\d+/);
  });
});
```

- [ ] 更新 `packages/agent-core-shared/src/index.ts` 追加：

```ts
export { getCoreVersion } from './version';
```

- [ ] 将 `packages/agent-core/src/version.ts` 改为 agent-core 自身版本封装：

```ts
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

export function getCoreVersion(): string {
  try {
    const pkgUrl = new URL('../package.json', import.meta.url);
    const raw = readFileSync(fileURLToPath(pkgUrl), 'utf-8');
    const pkg = JSON.parse(raw) as { version?: unknown };
    return typeof pkg.version === 'string' ? pkg.version : '0.0.0';
  } catch {
    return '0.0.0';
  }
}
```

- [ ] 更新所有 `from '#/version'` import：

```bash
cd packages/agent-core
rg "from '#\/version'" src test -l
```

保留 `packages/agent-core/src/version.ts` 自身的内部使用；其余文件改为 `from '@odysseythink/agent-core-shared'`。注意 `src/mcp/client-shared.ts` 会在后续 mcp-host 迁移中处理；此时先改为从 shared 导入，因为它将随 mcp 迁出。

- [ ] 运行 shared 与 agent-core typecheck。

- [ ] 提交。


### Task A6: 迁移 `loop/types.ts` 中的工具执行类型到 shared

**Depends on:** Task A5  
**Files:**
- Create: `packages/agent-core-shared/src/tool-execution.ts`
- Modify: `packages/agent-core-shared/src/index.ts`
- Modify: `packages/agent-core/src/loop/types.ts`

- [ ] 创建 `packages/agent-core-shared/src/tool-execution.ts`，内容取自 `packages/agent-core/src/loop/types.ts` 中 `ExecutableToolResult`、`ExecutableToolSuccessResult`、`ExecutableToolErrorResult`、`ExecutableToolContext`、`ExecutableTool`、`ToolExecution`、`RunnableToolExecution`、`ToolUpdate` 的定义，并导入 `@odysseythink/kosong` 的 `ContentPart`、`Tool`、`ToolCall`。完整代码：

```ts
import type { ContentPart, Tool, ToolCall } from '@odysseythink/kosong';

export type { ToolCall };

export type ExecutableToolOutput = string | ContentPart[];

export interface ExecutableToolSuccessResult {
  readonly output: ExecutableToolOutput;
  readonly isError?: false | undefined;
  readonly stopTurn?: boolean | undefined;
  readonly message?: string | undefined;
}

export interface ExecutableToolErrorResult {
  readonly output: ExecutableToolOutput;
  readonly isError: true;
  readonly message?: string | undefined;
  readonly stopTurn?: boolean | undefined;
}

export type ExecutableToolResult = ExecutableToolSuccessResult | ExecutableToolErrorResult;

export interface ToolUpdate {
  kind: 'stdout' | 'stderr' | 'progress' | 'status' | 'custom';
  text?: string | undefined;
  percent?: number | undefined;
  customKind?: string | undefined;
  customData?: unknown;
}

export interface ExecutableToolContext {
  readonly turnId: string;
  readonly toolCallId: string;
  readonly metadata?: unknown;
  readonly signal: AbortSignal;
  readonly onUpdate?: ((update: ToolUpdate) => void) | undefined;
}

export interface RunnableToolExecution {
  readonly isError?: false | undefined;
  readonly description?: string;
  readonly stopBatchAfterThis?: boolean | undefined;
  readonly approvalRule: string;
  readonly matchesRule?: ((ruleArgs: string) => boolean) | undefined;
  readonly execute: (ctx: ExecutableToolContext) => Promise<ExecutableToolResult>;
}

export type ToolExecution = RunnableToolExecution | ExecutableToolErrorResult;

export interface ExecutableTool<Input = unknown> extends Tool {
  resolveExecution(input: Input): ToolExecution | Promise<ToolExecution>;
}
```

- [ ] 更新 `packages/agent-core-shared/src/index.ts` 追加：

```ts
export type {
  ExecutableTool,
  ExecutableToolContext,
  ExecutableToolResult,
  ExecutableToolSuccessResult,
  ExecutableToolErrorResult,
  RunnableToolExecution,
  ToolExecution,
  ToolUpdate,
} from './tool-execution';
```

- [ ] 修改 `packages/agent-core/src/loop/types.ts`：保留 loop 特有的类型（LoopStepStopReason、TurnResult、LoopHooks 等），删除已迁移到 shared 的 ExecutableTool* / ToolUpdate 定义，改为从 shared 导入：

```ts
import type {
  ExecutableTool,
  ExecutableToolContext,
  ExecutableToolResult,
  ExecutableToolSuccessResult,
  ExecutableToolErrorResult,
  ToolExecution,
  ToolUpdate,
} from '@odysseythink/agent-core-shared';

export type {
  ExecutableTool,
  ExecutableToolContext,
  ExecutableToolResult,
  ExecutableToolSuccessResult,
  ExecutableToolErrorResult,
  ToolExecution,
  ToolUpdate,
} from '@odysseythink/agent-core-shared';
```

- [ ] 搜索 `#/loop/types` 在 agent-core 中的 import，确认 `ExecutableTool*` 仍可通过 `#/loop/types` 获得（因为 loop/types re-exports）。无需修改这些调用点。

- [ ] 运行 `packages/agent-core` typecheck。

- [ ] 提交。

---

### Task A7: 迁移 `tools/support/input-schema.ts` 到 shared

**Depends on:** Task A6  
**Files:**
- Create: `packages/agent-core-shared/src/input-schema.ts`
- Create: `packages/agent-core-shared/test/input-schema.test.ts`
- Modify: `packages/agent-core-shared/src/index.ts`
- Modify: `packages/agent-core/src/tools/support/input-schema.ts`
- Modify: 所有 `from '#/tools/support/input-schema'` import

- [ ] 将 `packages/agent-core/src/tools/support/input-schema.ts` 复制到 `packages/agent-core-shared/src/input-schema.ts`。

- [ ] 将 `packages/agent-core/test/tools/input-schema-io.test.ts` 复制到 `packages/agent-core-shared/test/input-schema.test.ts`，并删除其中对 agent-core 内部其他模块的依赖（若有）。通常该测试只测 `toInputJsonSchema`，可直接运行。

- [ ] 更新 `packages/agent-core-shared/src/index.ts` 追加：

```ts
export { toInputJsonSchema } from './input-schema';
```

- [ ] 将 `packages/agent-core/src/tools/support/input-schema.ts` 替换为 re-export：

```ts
export { toInputJsonSchema } from '@odysseythink/agent-core-shared';
```

- [ ] 搜索并替换 agent-core 中的 import：

```bash
cd packages/agent-core
rg "from '#\/tools/support/input-schema'" src test -l
```

替换为 `from '@odysseythink/agent-core-shared'`。

- [ ] 在 shared 运行测试，在 agent-core 运行 typecheck。

- [ ] 提交。

---

### Task A8: 迁移 MCP OAuth 事件常量到 shared

**Depends on:** Task A7  
**Files:**
- Create: `packages/agent-core-shared/src/mcp-events.ts`
- Modify: `packages/agent-core-shared/src/index.ts`
- Modify: `packages/agent-core/src/rpc/events.ts`

- [ ] 创建 `packages/agent-core-shared/src/mcp-events.ts`：

```ts
export const MCP_OAUTH_AUTHORIZATION_URL_TOOL_UPDATE = 'mcp.oauth.authorization_url';

export interface McpOAuthAuthorizationUrlUpdateData {
  readonly serverName: string;
  readonly authorizationUrl: string;
}
```

- [ ] 更新 `packages/agent-core-shared/src/index.ts` 追加：

```ts
export { MCP_OAUTH_AUTHORIZATION_URL_TOOL_UPDATE } from './mcp-events';
export type { McpOAuthAuthorizationUrlUpdateData } from './mcp-events';
```

- [ ] 修改 `packages/agent-core/src/rpc/events.ts`：删除 `MCP_OAUTH_AUTHORIZATION_URL_TOOL_UPDATE` 与 `McpOAuthAuthorizationUrlUpdateData` 的定义，改为从 shared 导入：

```ts
import {
  MCP_OAUTH_AUTHORIZATION_URL_TOOL_UPDATE,
  type McpOAuthAuthorizationUrlUpdateData,
} from '@odysseythink/agent-core-shared';

export {
  MCP_OAUTH_AUTHORIZATION_URL_TOOL_UPDATE,
  type McpOAuthAuthorizationUrlUpdateData,
} from '@odysseythink/agent-core-shared';
```

- [ ] 运行 agent-core typecheck。

- [ ] 提交。


### Task A9: 抽取 permission pattern 与 hook event types 到 shared

**Depends on:** Task A8  
**Files:**
- Create: `packages/agent-core-shared/src/permission-pattern.ts`
- Create: `packages/agent-core-shared/src/hook-events.ts`
- Create: `packages/agent-core-shared/test/permission-pattern.test.ts`
- Modify: `packages/agent-core-shared/src/index.ts`
- Modify: `packages/agent-core/src/agent/permission/matches-rule.ts`
- Modify: `packages/agent-core/src/session/hooks/types.ts`

- [ ] 创建 `packages/agent-core-shared/src/permission-pattern.ts`，包含 `parsePattern` 与相关类型。从 `packages/agent-core/src/agent/permission/matches-rule.ts` 复制 `ParsedPattern` 接口与 `parsePattern` 函数实现（注意它只依赖字符串操作，不依赖 picomatch）：

```ts
export interface ParsedPattern {
  readonly toolName: string;
  readonly argPattern?: string;
}

export function parsePattern(pattern: string): ParsedPattern {
  const trimmed = pattern.trim();
  if (trimmed.length === 0) {
    throw new Error('permission pattern: empty string');
  }

  const openIdx = trimmed.indexOf('(');
  if (openIdx === -1) {
    return { toolName: trimmed };
  }

  if (!trimmed.endsWith(')')) {
    throw new Error(`permission pattern: missing closing paren in "${pattern}"`);
  }

  const toolName = trimmed.slice(0, openIdx);
  const argPattern = trimmed.slice(openIdx + 1, -1);
  if (toolName.length === 0) {
    throw new Error(`permission pattern: empty tool name in "${pattern}"`);
  }
  if (argPattern.length === 0) {
    return { toolName };
  }
  return { toolName, argPattern };
}

export function isValidPermissionPattern(pattern: string): boolean {
  try {
    parsePattern(pattern);
    return true;
  } catch {
    return false;
  }
}
```

- [ ] 创建 `packages/agent-core-shared/src/hook-events.ts`：

```ts
export const HOOK_EVENT_TYPES = [
  'PreToolUse',
  'PostToolUse',
  'PostToolUseFailure',
  'PermissionRequest',
  'PermissionResult',
  'UserPromptSubmit',
  'Stop',
  'StopFailure',
  'SessionStart',
  'SessionEnd',
  'SubagentStart',
  'SubagentStop',
  'PreCompact',
  'PostCompact',
  'Notification',
] as const;

export type HookEventType = (typeof HOOK_EVENT_TYPES)[number];
```

- [ ] 创建 `packages/agent-core-shared/test/permission-pattern.test.ts`：

```ts
import { describe, expect, it } from 'vitest';
import { isValidPermissionPattern, parsePattern } from '../src/permission-pattern';

describe('permission pattern parser', () => {
  it('parses tool-name-only patterns', () => {
    expect(parsePattern('Write')).toEqual({ toolName: 'Write' });
  });

  it('parses arg patterns', () => {
    expect(parsePattern('Read(/etc/**)')).toEqual({ toolName: 'Read', argPattern: '/etc/**' });
  });

  it('validates well-formed patterns', () => {
    expect(isValidPermissionPattern('Bash(!rm *)')).toBe(true);
  });

  it('rejects malformed patterns', () => {
    expect(isValidPermissionPattern('')).toBe(false);
    expect(isValidPermissionPattern('Read(/etc')).toBe(false);
  });
});
```

- [ ] 更新 `packages/agent-core-shared/src/index.ts` 追加：

```ts
export {
  parsePattern,
  isValidPermissionPattern,
  type ParsedPattern,
} from './permission-pattern';
export { HOOK_EVENT_TYPES, type HookEventType } from './hook-events';
```

- [ ] 修改 `packages/agent-core/src/agent/permission/matches-rule.ts`：删除本地的 `parsePattern` 与 `ParsedPattern`，改为从 shared 导入；保留 `matchPermissionRule` 与 picomatch 相关逻辑。

```ts
import {
  parsePattern,
  type ParsedPattern,
} from '@odysseythink/agent-core-shared';
```

- [ ] 修改 `packages/agent-core/src/session/hooks/types.ts`：删除本地 `HOOK_EVENT_TYPES`，改为从 shared 导入：

```ts
import { HOOK_EVENT_TYPES, type HookEventType } from '@odysseythink/agent-core-shared';
export { HOOK_EVENT_TYPES, type HookEventType } from '@odysseythink/agent-core-shared';
```

- [ ] 运行 shared 测试与 agent-core typecheck。

- [ ] 提交。

---

### Task A10: 迁移 `config/schema.ts` 到 shared

**Depends on:** Task A9  
**Files:**
- Create: `packages/agent-core-shared/src/config.ts`
- Modify: `packages/agent-core-shared/src/index.ts`
- Modify: `packages/agent-core/src/config/index.ts`
- Delete: `packages/agent-core/src/config/schema.ts`
- Modify: 所有 `from '#/config/schema'` import

- [ ] 将 `packages/agent-core/src/config/schema.ts` 复制到 `packages/agent-core-shared/src/config.ts`。

- [ ] 修改 copy 后的 `packages/agent-core-shared/src/config.ts`：
  - 将 `import { HOOK_EVENT_TYPES } from '../session/hooks/types';` 改为 `import { HOOK_EVENT_TYPES } from './hook-events';`
  - 将 `import { parsePattern } from '#/agent/permission/matches-rule';` 改为 `import { parsePattern } from './permission-pattern';`
  - 将 `import { ErrorCodes, OdyError } from '#/errors';` 改为 `import { ErrorCodes, OdyError } from './errors';`
  - 保留 `zod` import。

- [ ] 更新 `packages/agent-core-shared/src/index.ts` 追加：

```ts
export {
  ProviderTypeSchema,
  type ProviderType,
  OAuthRefSchema,
  type OAuthRef,
  ProviderConfigSchema,
  type ProviderConfig,
  ModelAliasSchema,
  type ModelAlias,
  ThinkingConfigSchema,
  type ThinkingConfig,
  PermissionModeSchema,
  PermissionRuleDecisionSchema,
  PermissionRuleScopeSchema,
  PermissionRuleSchema,
  PermissionConfigSchema,
  type PermissionConfig,
  LoopControlSchema,
  type LoopControl,
  BackgroundConfigSchema,
  type BackgroundConfig,
  HookDefSchema,
  type HookDefConfig,
  MoonshotServiceConfigSchema,
  type MoonshotServiceConfig,
  WebSearchProviderNameSchema,
  type WebSearchProviderName,
  WebSearchProviderConfigSchema,
  type WebSearchProviderConfig,
  WebSearchConfigSchema,
  type WebSearchConfig,
  ServicesConfigSchema,
  type ServicesConfig,
  McpServerStdioConfigSchema,
  type McpServerStdioConfig,
  McpServerHttpConfigSchema,
  type McpServerHttpConfig,
  McpServerConfigSchema,
  type McpServerConfig,
  BrowserConfigSchema,
  type BrowserConfig,
  E2EConfigSchema,
  type E2EConfig,
  TestReviewConfigSchema,
  type TestReviewConfig,
  MicroagentBudgetConfigSchema,
  type MicroagentBudgetConfig,
  OdyConfigSchema,
  type OdyConfig,
  OdyConfigPatchSchema,
  type OdyConfigPatch,
  getDefaultConfig,
  validateConfig,
  formatConfigValidationError,
} from './config';
```

- [ ] 删除 `packages/agent-core/src/config/schema.ts`。

- [ ] 修改 `packages/agent-core/src/config/index.ts`：保留本地 `merge`、`path`、`resolve`、`toml`、`env-model`、`web-search` 的导出，并追加从 shared 重导出 schema：

```ts
export * from '@odysseythink/agent-core-shared';
export * from './merge';
export * from './path';
export * from './resolve';
export * from './toml';
export * from './env-model';
export { resolveWebSearchConfig } from './web-search';
```

注意：这会重导出 shared 的所有内容；如需精确只重导出 config 相关，可改为 `export * from '@odysseythink/agent-core-shared/config'`，但 shared 当前没有子路径导出，因此用顶层重导出。

- [ ] 搜索并替换 agent-core 中 `from '#/config/schema'` import：

```bash
cd packages/agent-core
rg "from '#\/config/schema'" src test -l
```

这些文件改为 `from '#/config'`（因为 config/index 已重导出 schema）或 `from '@odysseythink/agent-core-shared'`。

- [ ] 运行 agent-core typecheck。

- [ ] 提交。


### Task A11: 迁移 `flags` 系统到 shared

**Depends on:** Task A10  
**Files:**
- Create: `packages/agent-core-shared/src/flags/registry.ts`
- Create: `packages/agent-core-shared/src/flags/types.ts`
- Create: `packages/agent-core-shared/src/flags/resolver.ts`
- Create: `packages/agent-core-shared/src/flags/index.ts`
- Modify: `packages/agent-core-shared/src/index.ts`
- Modify: `packages/agent-core/src/flags/registry.ts`
- Modify: `packages/agent-core/src/flags/types.ts`
- Modify: `packages/agent-core/src/flags/resolver.ts`
- Modify: `packages/agent-core/src/config/resolve.ts`

- [ ] 将 `packages/agent-core/src/flags/registry.ts` 复制到 `packages/agent-core-shared/src/flags/registry.ts`。

- [ ] 将 `packages/agent-core/src/flags/types.ts` 复制到 `packages/agent-core-shared/src/flags/types.ts`。

- [ ] 将 `packages/agent-core/src/flags/resolver.ts` 复制到 `packages/agent-core-shared/src/flags/resolver.ts`，并修改 `parseBooleanEnv` import：
  - 删除 `import { parseBooleanEnv } from '#/config/resolve';`
  - 在文件内内联 `parseBooleanEnv`（从 `config/resolve.ts` 复制该函数实现）。

- [ ] 创建 `packages/agent-core-shared/src/flags/index.ts`：

```ts
export { FLAG_DEFINITIONS, type FlagId } from './registry';
export { FlagResolver, flags, MASTER_ENV } from './resolver';
export type {
  FlagSurface,
  FlagDefinitionInput,
  FlagDefinition,
  ExperimentalFlagMap,
} from './types';
```

- [ ] 更新 `packages/agent-core-shared/src/index.ts` 追加：

```ts
export {
  FLAG_DEFINITIONS,
  flags,
  FlagResolver,
  MASTER_ENV,
  type FlagId,
  type FlagSurface,
  type FlagDefinitionInput,
  type FlagDefinition,
  type ExperimentalFlagMap,
} from './flags';
```

- [ ] 将 agent-core 的 flags 文件改为 re-export：
  - `packages/agent-core/src/flags/registry.ts`：

```ts
export {
  FLAG_DEFINITIONS,
  type FlagId,
} from '@odysseythink/agent-core-shared';
```

  - `packages/agent-core/src/flags/types.ts`：

```ts
export type {
  FlagSurface,
  FlagDefinitionInput,
  FlagDefinition,
  ExperimentalFlagMap,
} from '@odysseythink/agent-core-shared';
```

  - `packages/agent-core/src/flags/resolver.ts`：

```ts
export {
  FlagResolver,
  flags,
  MASTER_ENV,
} from '@odysseythink/agent-core-shared';
```

  - `packages/agent-core/src/flags/index.ts`（若存在，保持重导出 shared flags）。

- [ ] 运行 agent-core typecheck。

- [ ] 提交。

---

### Task A12: 迁移 `wasm-loader.ts` 与 `wasm-string.ts` 到 shared

**Depends on:** Task A11  
**Files:**
- Create: `packages/agent-core-shared/src/wasm-loader.ts`
- Create: `packages/agent-core-shared/src/wasm-string.ts`
- Create: `packages/agent-core-shared/test/wasm-loader.test.ts`
- Create: `packages/agent-core-shared/test/wasm-string.test.ts`
- Modify: `packages/agent-core-shared/src/index.ts`
- Modify: `packages/agent-core/src/utils/wasm-loader.ts`
- Modify: `packages/agent-core/src/utils/wasm-string.ts`
- Modify: 所有 `from '#/utils/wasm-loader'` / `from '#/utils/wasm-string'` import

- [ ] 将 `packages/agent-core/src/utils/wasm-loader.ts` 复制到 `packages/agent-core-shared/src/wasm-loader.ts`，并修改 flags import：
  - `import { FlagResolver } from '../flags/resolver';`
  - `import type { FlagId } from '../flags/registry';`

- [ ] 将 `packages/agent-core/src/utils/wasm-string.ts` 复制到 `packages/agent-core-shared/src/wasm-string.ts`，并修改 wasm-loader import 为相对路径 `#/wasm-loader`（在 shared 包内）或 `./wasm-loader`。

- [ ] 将相关测试复制到 `packages/agent-core-shared/test/`。

- [ ] 更新 `packages/agent-core-shared/src/index.ts` 追加：

```ts
export {
  loadWasmModule,
  wrapWithFallback,
  type WasmFlagId,
  type WasmExports,
  type WasmModuleConfig,
  type LoadContext,
} from './wasm-loader';
export {
  writeString,
  readCString,
  callWasmStringFunction,
  callWasmU32Function,
  type WasmExports,
  type StringAllocation,
} from './wasm-string';
```

- [ ] 将 agent-core 的 `packages/agent-core/src/utils/wasm-loader.ts` 与 `wasm-string.ts` 替换为 re-export：

```ts
// wasm-loader.ts
export {
  loadWasmModule,
  wrapWithFallback,
  type WasmFlagId,
  type WasmExports,
  type WasmModuleConfig,
  type LoadContext,
} from '@odysseythink/agent-core-shared';

// wasm-string.ts
export {
  writeString,
  readCString,
  callWasmStringFunction,
  callWasmU32Function,
  type WasmExports,
  type StringAllocation,
} from '@odysseythink/agent-core-shared';
```

- [ ] 搜索并替换 agent-core 中的 import：

```bash
cd packages/agent-core
rg "from '#\/utils/wasm-loader'|from '#\/utils/wasm-string'" src test -l
```

替换为 `from '@odysseythink/agent-core-shared'`。

- [ ] 运行 shared 测试与 agent-core typecheck。

- [ ] 提交。


### Task A13: 清理 agent-core 内部遗留 import 并做全树 typecheck

**Depends on:** Task A12  
**Files:**
- Modify: 所有仍引用旧 `#/errors`、`#/logging/types`、`#/utils/abort`、`#/version`、`#/tools/support/input-schema`、`#/rpc/events` 中已迁移定义、`#/config/schema`、`#/utils/wasm-loader`、`#/utils/wasm-string` 的测试或源码文件
- Modify: `pnpm-workspace.yaml`（临时加入 shared 以便 workspace 识别）

- [ ] 在 `packages/agent-core` 中搜索任何仍指向已删除内部路径的 import：

```bash
cd packages/agent-core
rg "from '#\/errors'|from '#\/logging/types'|from '#\/utils/abort'|from '#\/version'|from '#\/tools/support/input-schema'|from '#\/config/schema'|from '#\/utils/wasm-loader'|from '#\/utils/wasm-string'" src test -l
```

预期为空。若仍有残留，改为 `@odysseythink/agent-core-shared` 或 `#/config`。

- [ ] 临时更新 `pnpm-workspace.yaml`，在 `packages/*` 通配已覆盖的前提下无需手动添加；若本地 pnpm 未识别新包，运行：

```bash
pnpm install
```

- [ ] 运行全 workspace typecheck：

```bash
pnpm -r --filter './packages/*' run typecheck
```

预期：agent-core-shared、agent-core 等包均无错误。

- [ ] 运行 agent-core-shared 与 agent-core 的测试：

```bash
pnpm --filter @odysseythink/agent-core-shared test
pnpm --filter @odysseythink/agent-core test
```

预期：均通过（agent-core 的测试可能因源码目录仍存在 `code-review`、`e2e-testing`、`mcp` 而大量通过；本阶段只验证 shared 迁移未破坏 agent-core）。

- [ ] 提交。

---

## Local Self-Review (Part A)

- [ ] **Spec-coverage**: 本 Part 覆盖设计中的 errors / logging / abort / version / tool-execution / input-schema / mcp-events / permission-pattern / hook-events / config / flags / wasm-loader / wasm-string 下沉需求。
- [ ] **Placeholder scan**: 无 TODO/TBD；每个模块都给出完整文件内容与 import 替换命令。
- [ ] **No phantom tasks**: 每个 Task 都产生可验证的代码/测试/typecheck 变更。
- [ ] **Dependency soundness**: Task A1 创建骨架；A2-A12 按依赖链逐个迁移；A13 收尾；无后续 Part 符号。
- [ ] **Caller & build soundness**: 每次迁移都列出 `rg` 搜索与 `sed` 替换命令，更新所有调用者（含测试），并以 agent-core/shared typecheck 结束；共享签名（如 `OdyError`、`Logger`、`getCoreVersion`）在同一 Task 内完成移动与所有调用点更新。
- [ ] **Test-the-risk**: `errors/codes`、`abort`、`version`、`permission-pattern`、`wasm-loader/string` 均有行为测试；config schema 的 risk 通过 typecheck 验证。
- [ ] **Type consistency**: shared 中导出的类型名与 agent-core 原有类型名保持一致，agent-core 通过 re-export 或 import 替换保持接口不变。
