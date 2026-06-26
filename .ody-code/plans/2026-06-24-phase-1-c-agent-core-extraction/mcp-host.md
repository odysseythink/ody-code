# Part B3: 创建 `@odysseythink/mcp-host`

本 Part 将 `packages/agent-core/src/mcp/` 整体迁移为独立包 `@odysseythink/mcp-host`。
新包只依赖 `@odysseythink/agent-core-shared` 与 `@odysseythink/kaos`，不反向依赖 `agent-core`。
迁移后 `agent-core` 中的 `ToolManager`、`Session`、`KimiCore`、`Agent` 改为从新包导入 MCP 能力，
`packages/agent-core/src/mcp` 目录删除。

**依赖上游：**
- `shared.md` Task A2（`errors` 已迁移到 shared）
- `shared.md` Task A3（日志接口类型已迁移到 shared）
- `shared.md` Task A4（`abortable` 已迁移到 shared）
- `shared.md` Task A5（`version` 已迁移到 shared）
- `shared.md` Task A7（`input-schema` 已迁移到 shared）
- `shared.md` Task A8（MCP OAuth 事件常量已迁移到 shared）
- `shared.md` Task A10（`config/schema` 已迁移到 shared）
- `shared.md` Task A13（shared 包完成并可通过 workspace 引用）

**Phase B3 任务图：**

```
B3-1 包骨架
   ↓
B3-2 迁移源码 + 本地 helper + 重建导出
   ↓
B3-3 迁移单元测试到新包
   ↓
B3-4 更新 agent-core 调用方并清理旧目录
```

**风险与开放问题：**

| # | 风险 | 缓解措施 |
|---|---|---|
| R1 | `connection-manager.ts` 原使用 agent-core 全局 `log`；迁出后无默认 logger。 | 在 `mcp-host` 内提供最小 fallback logger；生产代码由 `Session` 传入 session logger，保持原有行为。 |
| R2 | `config-loader.ts` / `sea-builtins.ts` 使用 `#/config/path` 的 `resolveOdyHome`；该 helper 未下沉到 shared。 | 在 `mcp-host` 内实现同语义本地 `resolveOdyHome`，避免反向依赖 agent-core。 |
| R3 | `client-shared.ts` 的 `KIMI_MCP_CLIENT_VERSION` 原读取 agent-core 版本；迁出后若读取 shared 版本会改变 MCP `initialize` 报出的版本号。 | 给 `McpConnectionManagerOptions` 增加 `clientVersion`，由 `Session` 传入 `options.appVersion ?? getCoreVersion()`；未传时回退到 shared 版本，仅影响无 appVersion 的测试。 |
| R4 | `auth-tool.test.ts` 原依赖 agent-core 的 `executeTool` fixture。 | 在新包内提供最小 `executeTool` helper，不反向依赖 agent-core。 |
| R5 | `connection-manager.test.ts`、`tool-manager-mcp.test.ts`、`built-in-integration.test.ts` 依赖 agent-core 的 `Session`/`Agent`/`ToolManager`，无法迁出。 | 这些测试留在 agent-core 作为集成测试，仅更新 import。 |

---

### Task B3-1: 创建 mcp-host 包骨架

**Depends on:** `shared.md` Task A13
**Files:**
- Create: `packages/mcp-host/package.json`
- Create: `packages/mcp-host/tsconfig.json`
- Create: `packages/mcp-host/vitest.config.ts`
- Create: `packages/mcp-host/src/index.ts`

- [ ] 编写 `packages/mcp-host/package.json`：

```json
{
  "name": "@odysseythink/mcp-host",
  "version": "0.1.0",
  "private": true,
  "description": "MCP host subsystem extracted from agent-core",
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
    "@odysseythink/agent-core-shared": "workspace:^",
    "@odysseythink/kaos": "workspace:^",
    "@modelcontextprotocol/sdk": "^1.29.0",
    "pathe": "^2.0.3",
    "zod": "catalog:"
  }
}
```

- [ ] 编写 `packages/mcp-host/tsconfig.json`：

```json
{
  "extends": "../../tsconfig.json",
  "compilerOptions": {},
  "include": ["src", "test"]
}
```

- [ ] 编写 `packages/mcp-host/vitest.config.ts`：

```ts
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    name: 'mcp-host',
    include: ['test/**/*.test.ts'],
  },
});
```

- [ ] 编写占位 `packages/mcp-host/src/index.ts`：

```ts
export {};
```

- [ ] 安装依赖并验证骨架通过：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm install
cd packages/mcp-host
pnpm typecheck
```

预期：无错误，`error code 0`。

- [ ] 提交：`git add packages/mcp-host && git commit -m "chore(mcp-host): bootstrap package"`

---

### Task B3-2: 迁移 MCP 源码并重建导出

**Depends on:** Task B3-1
**Files:**
- Create: `packages/mcp-host/src/auth-tool.ts`
- Create: `packages/mcp-host/src/client-http.ts`
- Create: `packages/mcp-host/src/client-shared.ts`
- Create: `packages/mcp-host/src/client-stdio.ts`
- Create: `packages/mcp-host/src/config-loader.ts`
- Create: `packages/mcp-host/src/connection-manager.ts`
- Create: `packages/mcp-host/src/output.ts`
- Create: `packages/mcp-host/src/session-config.ts`
- Create: `packages/mcp-host/src/tool-naming.ts`
- Create: `packages/mcp-host/src/trace-recorder.ts`
- Create: `packages/mcp-host/src/types.ts`
- Create: `packages/mcp-host/src/logger.ts`
- Create: `packages/mcp-host/src/paths.ts`
- Create: `packages/mcp-host/src/built-in/chrome-devtools.ts`
- Create: `packages/mcp-host/src/built-in/index.ts`
- Create: `packages/mcp-host/src/built-in/registry.ts`
- Create: `packages/mcp-host/src/built-in/resolve-root.ts`
- Create: `packages/mcp-host/src/built-in/sea-builtins.ts`
- Create: `packages/mcp-host/src/oauth/callback-server.ts`
- Create: `packages/mcp-host/src/oauth/index.ts`
- Create: `packages/mcp-host/src/oauth/provider.ts`
- Create: `packages/mcp-host/src/oauth/service.ts`
- Create: `packages/mcp-host/src/oauth/store.ts`
- Modify: `packages/mcp-host/src/index.ts`

- [ ] 创建本地 helper `packages/mcp-host/src/paths.ts`：

```ts
import { homedir } from 'node:os';
import { join } from 'pathe';

export function resolveOdyHome(homeDir?: string | undefined): string {
  return homeDir ?? process.env['ODY_CODE_HOME'] ?? join(homedir(), '.ody-code');
}
```

- [ ] 创建本地 fallback logger `packages/mcp-host/src/logger.ts`：

```ts
import type { Logger, LogContext, LogPayload } from '@odysseythink/agent-core-shared';

class FallbackLogger implements Logger {
  error(message: string, payload?: LogPayload): void {
    console.error(message, payload);
  }

  warn(message: string, payload?: LogPayload): void {
    console.warn(message, payload);
  }

  info(message: string, payload?: LogPayload): void {
    console.info(message, payload);
  }

  debug(message: string, payload?: LogPayload): void {
    console.debug(message, payload);
  }

  createChild(): Logger {
    return this;
  }
}

export const fallbackLogger: Logger = new FallbackLogger();
```

- [ ] 将 `packages/agent-core/src/mcp` 下所有源码复制到 `packages/mcp-host/src`（保持目录结构，包括 `built-in` 与 `oauth`）：

```bash
cd /Users/ranwei/workspace/ody-code
SRC=packages/agent-core/src/mcp
DST=packages/mcp-host/src

cp "$SRC/auth-tool.ts" "$DST/auth-tool.ts"
cp "$SRC/client-http.ts" "$DST/client-http.ts"
cp "$SRC/client-shared.ts" "$DST/client-shared.ts"
cp "$SRC/client-stdio.ts" "$DST/client-stdio.ts"
cp "$SRC/config-loader.ts" "$DST/config-loader.ts"
cp "$SRC/connection-manager.ts" "$DST/connection-manager.ts"
cp "$SRC/output.ts" "$DST/output.ts"
cp "$SRC/session-config.ts" "$DST/session-config.ts"
cp "$SRC/tool-naming.ts" "$DST/tool-naming.ts"
cp "$SRC/trace-recorder.ts" "$DST/trace-recorder.ts"
cp "$SRC/types.ts" "$DST/types.ts"

mkdir -p "$DST/built-in"
cp "$SRC/built-in/"*.ts "$DST/built-in/"

mkdir -p "$DST/oauth"
cp "$SRC/oauth/"*.ts "$DST/oauth/"
```

- [ ] 重写跨包 import。在 `packages/mcp-host/src` 中执行：

```bash
cd /Users/ranwei/workspace/ody-code/packages/mcp-host/src

# errors / config schema / logging types / version / abort / input-schema / mcp-events
rg -l "from '#\\/errors'" . | xargs -I{} sed -i "" "s|from '#/errors'|from '@odysseythink/agent-core-shared'|g" {}
rg -l "from '#\\/config\\/schema'" . | xargs -I{} sed -i "" "s|from '#/config/schema'|from '@odysseythink/agent-core-shared'|g" {}
rg -l "from '#\\/logging\\/types'" . | xargs -I{} sed -i "" "s|from '#/logging/types'|from '@odysseythink/agent-core-shared'|g" {}
rg -l "from '#\\/version'" . | xargs -I{} sed -i "" "s|from '#/version'|from '@odysseythink/agent-core-shared'|g" {}
rg -l "from '#\\/utils\\/abort'" . | xargs -I{} sed -i "" "s|from '#/utils/abort'|from '@odysseythink/agent-core-shared'|g" {}
rg -l "from '#\\/tools\\/support\\/input-schema'" . | xargs -I{} sed -i "" "s|from '#/tools/support/input-schema'|from '@odysseythink/agent-core-shared'|g" {}
rg -l "from '#\\/rpc\\/events'" . | xargs -I{} sed -i "" "s|from '#/rpc/events'|from '@odysseythink/agent-core-shared'|g" {}

# agent-core 内部 logger 实现 → 本地 fallback logger
rg -l "from '#\\/logging\\/logger'" . | xargs -I{} sed -i "" "s|from '#/logging/logger'|from './logger'|g" {}

# agent-core 内部 config/path → 本地 paths helper
rg -l "from '#\\/config\\/path'" . | xargs -I{} sed -i "" "s|from '#/config/path'|from './paths'|g" {}

# auth-tool 的相对跨模块引用
rg -l "from '\\.\\./\\.\\./src/loop'" . | xargs -I{} sed -i "" "s|from '../../src/loop'|from '@odysseythink/agent-core-shared'|g" {}
rg -l "from '\\.\\./\\.\\./src/tools/support/input-schema'" . | xargs -I{} sed -i "" "s|from '../../src/tools/support/input-schema'|from '@odysseythink/agent-core-shared'|g" {}
rg -l "from '\\.\\./\\.\\./src/rpc/events'" . | xargs -I{} sed -i "" "s|from '../../src/rpc/events'|from '@odysseythink/agent-core-shared'|g" {}
```

- [ ] 验证 `packages/mcp-host/src` 内无 `#/` 或 `src/` 形式的跨包残留 import：

```bash
rg "from '#\\/|from '\.\./\.\./src/|from '\.\./\.\./\.\./src/" /Users/ranwei/workspace/ody-code/packages/mcp-host/src
```

预期：无匹配。

- [ ] 修改 `packages/mcp-host/src/connection-manager.ts`，给 `McpConnectionManagerOptions` 增加可选 `clientVersion` 并在创建客户端时透传：

```ts
export interface McpConnectionManagerOptions {
  readonly envLookup?: (name: string) => string | undefined;
  readonly oauthService?: McpOAuthService;
  readonly log?: Logger;
  /**
   * Client version sent to MCP servers in the `initialize` handshake.
   * Production callers should pass the real app version; when omitted the
   * clients fall back to the package-level default version.
   */
  readonly clientVersion?: string;
}
```

在 `createClient` 中（约原文件第 289-299 行）改为：

```ts
private createClient(config: McpServerConfig, name: string): RuntimeMcpClient {
  const toolCallTimeoutMs = config.toolTimeoutMs;
  const clientVersion = this.options.clientVersion;
  if (config.transport === 'stdio') {
    return new StdioMcpClient(config, { toolCallTimeoutMs, clientVersion });
  }
  return new HttpMcpClient(config, {
    toolCallTimeoutMs,
    envLookup: this.options.envLookup,
    oauthProvider: this.resolveOAuthProvider(config, name),
    clientVersion,
  });
}
```

- [ ] 修改 `packages/mcp-host/src/auth-tool.ts` 中三个 import（若 sed 已覆盖则二次检查）：

```ts
import type {
  ExecutableTool,
  ExecutableToolContext,
  ExecutableToolResult,
} from '@odysseythink/agent-core-shared';
import { toInputJsonSchema } from '@odysseythink/agent-core-shared';
import {
  MCP_OAUTH_AUTHORIZATION_URL_TOOL_UPDATE,
  type McpOAuthAuthorizationUrlUpdateData,
} from '@odysseythink/agent-core-shared';
```

- [ ] 修改 `packages/mcp-host/src/client-shared.ts` 中的 `getCoreVersion` import（sed 已覆盖），保留 `KIMI_MCP_CLIENT_VERSION` 作为回退值：

```ts
import { getCoreVersion } from '@odysseythink/agent-core-shared';
```

- [ ] 编写 `packages/mcp-host/src/index.ts` 公开所有 agent-core 需要的能力：

```ts
export * from './auth-tool';
export * from './built-in';
export * from './connection-manager';
export * from './oauth';
export * from './output';
export * from './session-config';
export * from './tool-naming';
export * from './trace-recorder';
export * from './types';
```

- [ ] 在 `packages/mcp-host` 运行 typecheck：

```bash
pnpm typecheck
```

预期：无错误。

- [ ] 提交：`git add packages/mcp-host/src && git commit -m "refactor(mcp-host): migrate mcp source from agent-core"`

---

### Task B3-3: 迁移 MCP 单元测试到新包

**Depends on:** Task B3-2
**Files:**
- Create: `packages/mcp-host/test/helpers/execute-tool.ts`
- Create: `packages/mcp-host/test/auth-tool.test.ts`
- Create: `packages/mcp-host/test/client-http.test.ts`
- Create: `packages/mcp-host/test/client-stdio.test.ts`
- Create: `packages/mcp-host/test/config-loader.test.ts`
- Create: `packages/mcp-host/test/oauth-store.test.ts`
- Create: `packages/mcp-host/test/output.test.ts`
- Create: `packages/mcp-host/test/tool-naming.test.ts`
- Create: `packages/mcp-host/test/trace-recorder.test.ts`
- Create: `packages/mcp-host/test/built-in/chrome-devtools.test.ts`
- Create: `packages/mcp-host/test/built-in/registry.test.ts`
- Create: `packages/mcp-host/test/built-in/resolve-root.test.ts`
- Create: `packages/mcp-host/test/fixtures/*.mjs`

以下测试依赖 agent-core 的 `Session`/`Agent`/`ToolManager`，留在 agent-core，仅更新 import：
- `packages/agent-core/test/mcp/connection-manager.test.ts`
- `packages/agent-core/test/mcp/tool-manager-mcp.test.ts`
- `packages/agent-core/test/mcp/built-in-integration.test.ts`

- [ ] 创建测试 helper `packages/mcp-host/test/helpers/execute-tool.ts`：

```ts
import type {
  ExecutableTool,
  ExecutableToolContext,
  ExecutableToolResult,
  ToolExecution,
} from '@odysseythink/agent-core-shared';

export type TestExecutableToolContext<Input> = ExecutableToolContext & {
  readonly args: Input;
};

export async function executeTool<Input>(
  tool: ExecutableTool<Input>,
  context: TestExecutableToolContext<Input>,
): Promise<ExecutableToolResult> {
  const { args, ...executionContext } = context;
  const resolved = tool.resolveExecution(args);
  const execution = isPromiseLike(resolved) ? await resolved : resolved;
  if (execution.isError === true) return execution;
  return execution.execute(executionContext);
}

function isPromiseLike(
  value: ToolExecution | Promise<ToolExecution>,
): value is Promise<ToolExecution> {
  return typeof (value as Promise<ToolExecution>).then === 'function';
}
```

- [ ] 复制可独立运行的测试与 fixtures：

```bash
cd /Users/ranwei/workspace/ody-code
SRC=packages/agent-core/test/mcp
DST=packages/mcp-host/test

mkdir -p "$DST/helpers" "$DST/built-in" "$DST/fixtures"

cp "$SRC/auth-tool.test.ts" "$DST/auth-tool.test.ts"
cp "$SRC/client-http.test.ts" "$DST/client-http.test.ts"
cp "$SRC/client-stdio.test.ts" "$DST/client-stdio.test.ts"
cp "$SRC/config-loader.test.ts" "$DST/config-loader.test.ts"
cp "$SRC/oauth-store.test.ts" "$DST/oauth-store.test.ts"
cp "$SRC/output.test.ts" "$DST/output.test.ts"
cp "$SRC/tool-naming.test.ts" "$DST/tool-naming.test.ts"
cp "$SRC/trace-recorder.test.ts" "$DST/trace-recorder.test.ts"

cp "$SRC/built-in/chrome-devtools.test.ts" "$DST/built-in/chrome-devtools.test.ts"
cp "$SRC/built-in/registry.test.ts" "$DST/built-in/registry.test.ts"
cp "$SRC/built-in/resolve-root.test.ts" "$DST/built-in/resolve-root.test.ts"

cp "$SRC/fixtures/"*.mjs "$DST/fixtures/"
```

- [ ] 重写新包测试中的 import：

```bash
cd /Users/ranwei/workspace/ody-code/packages/mcp-host/test

# src/mcp/* → #/*
rg -l "from '\.\./\.\./src/mcp/" . | xargs -I{} sed -i "" "s|from '../../src/mcp/|from '#/|g" {}
rg -l "from '\.\./\.\./\.\./src/mcp/" . | xargs -I{} sed -i "" "s|from '../../../src/mcp/|from '#/|g" {}

# agent-core errors → shared
rg -l "from '\.\./\.\./src/errors'" . | xargs -I{} sed -i "" "s|from '../../src/errors'|from '@odysseythink/agent-core-shared'|g" {}

# agent-core config schema → shared
rg -l "from '\.\./\.\./\.\./src/config/schema'" . | xargs -I{} sed -i "" "s|from '../../../src/config/schema'|from '@odysseythink/agent-core-shared'|g" {}

# auth-tool 特殊依赖
sed -i "" "s|from '../../src/loop'|from '@odysseythink/agent-core-shared'|g" auth-tool.test.ts
sed -i "" "s|from '../../src/rpc/events'|from '@odysseythink/agent-core-shared'|g" auth-tool.test.ts
sed -i "" "s|from '../tools/fixtures/execute-tool'|from '../helpers/execute-tool'|g" auth-tool.test.ts
```

- [ ] 验证 `packages/mcp-host/test` 中无 `src/` 形式的残留 import：

```bash
rg "from '\.\./\.\./src/|from '\.\./\.\./\.\./src/|from '../tools/fixtures/'" /Users/ranwei/workspace/ody-code/packages/mcp-host/test
```

预期：无匹配。

- [ ] 运行 `packages/mcp-host` 测试：

```bash
pnpm test
```

预期：全部通过（部分测试需要本地 Node 可执行 fixtures，超时较长，保持默认 vitest 超时）。

- [ ] 提交：`git add packages/mcp-host/test && git commit -m "test(mcp-host): migrate mcp unit tests"`

---

### Task B3-4: 更新 agent-core 调用方并清理旧目录

**Depends on:** Task B3-3
**Files:**
- Modify: `packages/agent-core/src/agent/tool/index.ts:10-15`
- Modify: `packages/agent-core/src/rpc/core-impl.ts:39`
- Modify: `packages/agent-core/src/rpc/core-impl.ts:117-119`
- Modify: `packages/agent-core/src/session/index.ts:18-23`
- Modify: `packages/agent-core/src/session/index.ts:170-173`
- Modify: `packages/agent-core/src/agent/index.ts:18`
- Modify: `packages/agent-core/test/mcp/connection-manager.test.ts`
- Modify: `packages/agent-core/test/mcp/tool-manager-mcp.test.ts`
- Delete: `packages/agent-core/src/mcp/`
- Delete: 已迁移到新包的 agent-core 单元测试文件

- [ ] 修改 `packages/agent-core/src/agent/tool/index.ts` 中所有 MCP import 为新包：

```ts
import { createMcpAuthTool } from '@odysseythink/mcp-host';
import type { McpConnectionManager, McpServerEntry } from '@odysseythink/mcp-host';
import { ChromeTraceRecorder } from '@odysseythink/mcp-host';
import { mcpResultToExecutableOutput } from '@odysseythink/mcp-host';
import { isMcpToolName, qualifyMcpToolName } from '@odysseythink/mcp-host';
import type { MCPClient } from '@odysseythink/mcp-host';
```

- [ ] 修改 `packages/agent-core/src/rpc/core-impl.ts`：

```ts
import { resolveSessionMcpConfig, type SessionMcpConfig } from '@odysseythink/mcp-host';
```

```ts
import { BuiltInMcpRegistry } from '@odysseythink/mcp-host';
import { createChromeDevToolsServerDefinition } from '@odysseythink/mcp-host';
import { BuiltInRootNotFoundError } from '@odysseythink/mcp-host';
```

- [ ] 修改 `packages/agent-core/src/session/index.ts`：

顶部 import 改为：

```ts
import {
  McpConnectionManager,
  McpOAuthService,
  type McpServerEntry,
  type SessionMcpConfig,
} from '@odysseythink/mcp-host';
```

并在文件顶部追加：

```ts
import { getCoreVersion } from '#/version';
```

将 `McpConnectionManager` 构造（约第 170-173 行）改为传入 `clientVersion`：

```ts
this.mcp = new McpConnectionManager({
  oauthService: new McpOAuthService({ kimiHomeDir: options.kimiHomeDir }),
  log: this.log,
  clientVersion: options.appVersion ?? getCoreVersion(),
});
```

- [ ] 修改 `packages/agent-core/src/agent/index.ts`：

```ts
import type { McpConnectionManager } from '@odysseythink/mcp-host';
```

- [ ] 修改 `packages/agent-core/test/mcp/connection-manager.test.ts`：

```ts
import { OdyError } from '@odysseythink/agent-core-shared';
import { McpConnectionManager, type McpServerEntry } from '@odysseythink/mcp-host';
import { JsonFileStore, McpOAuthService } from '@odysseythink/mcp-host';
```

- [ ] 修改 `packages/agent-core/test/mcp/tool-manager-mcp.test.ts`：

```ts
import type { MCPClient } from '@odysseythink/mcp-host';
```

- [ ] 删除旧源码目录与已迁移的测试文件（保留集成测试与 fixtures）：

```bash
cd /Users/ranwei/workspace/ody-code

rm -rf packages/agent-core/src/mcp

rm -f packages/agent-core/test/mcp/auth-tool.test.ts
rm -f packages/agent-core/test/mcp/client-http.test.ts
rm -f packages/agent-core/test/mcp/client-stdio.test.ts
rm -f packages/agent-core/test/mcp/config-loader.test.ts
rm -f packages/agent-core/test/mcp/oauth-store.test.ts
rm -f packages/agent-core/test/mcp/output.test.ts
rm -f packages/agent-core/test/mcp/tool-naming.test.ts
rm -f packages/agent-core/test/mcp/trace-recorder.test.ts

rm -f packages/agent-core/test/mcp/built-in/chrome-devtools.test.ts
rm -f packages/agent-core/test/mcp/built-in/registry.test.ts
rm -f packages/agent-core/test/mcp/built-in/resolve-root.test.ts
rmdir packages/agent-core/test/mcp/built-in 2>/dev/null || true
```

保留：
- `packages/agent-core/test/mcp/connection-manager.test.ts`
- `packages/agent-core/test/mcp/tool-manager-mcp.test.ts`
- `packages/agent-core/test/mcp/built-in-integration.test.ts`
- `packages/agent-core/test/mcp/fixtures/`

- [ ] 搜索并确认 agent-core 内无 `#/mcp` 或 `../mcp` 残留引用：

```bash
cd packages/agent-core
rg "from '#\\/mcp|from '\.\./mcp|from '\.\./\.\./mcp" src test
```

预期：无匹配。

- [ ] 运行全 workspace typecheck（shared-signature 变更必须全树验证）：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm -r --filter './packages/*' run typecheck
```

预期：所有包（包括 agent-core 与 mcp-host）无类型错误。

- [ ] 运行 mcp-host 自身测试与 agent-core 集成测试（可选但推荐）：

```bash
pnpm --filter @odysseythink/mcp-host test
pnpm --filter @odysseythink/agent-core test -- test/mcp/connection-manager.test.ts test/mcp/tool-manager-mcp.test.ts test/mcp/built-in-integration.test.ts
```

预期：通过。

- [ ] 提交：`git add packages/agent-core packages/mcp-host && git commit -m "refactor(agent-core): consume mcp-host and remove old src/mcp"`

---

## Local Self-Review (Part B3)

- [ ] **Spec-coverage**: 本 Part 覆盖设计中的 MCP 子系统拆包、无反向依赖、调用点切换、旧目录清理。
- [ ] **Placeholder scan**: 无 TODO/TBD；每个 import 映射、文件路径、构造参数均给出具体代码或命令。
- [ ] **No phantom tasks**: 每个 Task 都产生可验证的代码/测试/typecheck 变更。
- [ ] **Dependency soundness**: B3-1 → B3-2 → B3-3 → B3-4 顺序满足；B3 依赖 `shared.md` A13，无后续 Part 符号。
- [ ] **Caller & build soundness**: B3-4 一次性完成所有 agent-core 调用方（含测试）的 import 切换与 `McpConnectionManager` 构造参数更新，并以 `pnpm -r --filter './packages/*' run typecheck` 全树验证；`clientVersion` 由 `Session` 传入真实 app 版本，运行时 consumer（MCP `initialize` handshake）trace 到 `SessionOptions.appVersion ?? getCoreVersion()`。
- [ ] **Test-the-risk**: `auth-tool.test.ts` 在新包内继续断言 synthetic auth 工具的状态流转、URL 透传与 reconnect 调用次数；`tool-naming.test.ts` / `output.test.ts` / `config-loader.test.ts` 等继续覆盖边界行为。
- [ ] **Type consistency**: `McpConnectionManagerOptions`、`SessionMcpConfig`、`McpServerEntry`、`MCPClient` 等类型名与接口保持与 `agent-core` 原有定义一致；`clientVersion` 为新增可选字段，不影响已有类型。
