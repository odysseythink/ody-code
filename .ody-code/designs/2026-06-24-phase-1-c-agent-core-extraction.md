# Phase 1-C: agent-core 拆包启动

> 本设计对应后端架构演进总路线图中 Phase 1-C(`backend-architecture-evolution-roadmap.md:128`)的详细展开。
> 目标:把 `code-review`、`e2e-testing`、`mcp` 三个子系统从 `packages/agent-core` 单体中剥出为独立 peer 包,并建立最小共享包与依赖图守护。

---

## Scope In/Out [C:USER]

**In Scope:**
- 新建 `@odysseythink/agent-core-shared`:承载被多包引用的基础类型与极小工具。
- 新建 `@odysseythink/code-review`:从 `packages/agent-core/src/code-review/` 整体迁移。
- 新建 `@odysseythink/e2e-testing`:从 `packages/agent-core/src/e2e-testing/` 整体迁移。
- 新建 `@odysseythink/mcp-host`:从 `packages/agent-core/src/mcp/` 整体迁移。
- 新建 `@odysseythink/integration-tests`:存放跨包集成测试。
- 更新 `agent-core` 内部调用者,改为直接从新包导入。
- CI 引入 `madge` 进行包间循环依赖检测。
- 同步更新 `pnpm-workspace.yaml` 与 `flake.nix` 的 workspace 列表。

**Out of Scope (Deferred):**
- `office-hours/` 拆包:需等 Phase 2-D 的 mode 概念统一 [C:USER]。
- 任何业务逻辑重写:[C:USER] 只移文件 + 调 import。
- Rust/Wasm 迁移:属于 Phase 1-A / Phase 4。
- Transport/Worker 改造:属于 Phase 0/1-B。
- `agent-core` 对三新包的 re-export:[C:USER] 明确不再 re-export,调用者直接导入新包。

---

## Reuse Analysis

| # | 复用对象 | 位置 | 复用方式 | 说明 |
|---|---|---|---|---|
| 1 | 包构建工具链 | `packages/telemetry/package.json` | 照模板新建 | `tsdown` + `vitest` + `#/*` imports 已在多个 peer 包验证,直接复用 [C:INFERRED]。 |
| 2 | 错误模块 | `packages/agent-core/src/errors/` | 迁移至 shared 包 | 自包含,被 code-review/e2e-testing/mcp 共同引用 [C:INFERRED]。 |
| 3 | 配置 schema 类型 | `packages/agent-core/src/config/schema.ts` | 按需迁移/拆分 | 需把 `McpServerConfig`/`E2EConfig`/`OdyConfig` 等下沉到 shared,同时解除对 `agent/permission/matches-rule` 和 `session/hooks/types` 的内部依赖 [C:INFERRED]。 |
| 4 | Logger 接口 | `packages/agent-core/src/logging/types.ts` | 迁移至 shared 包 | 纯类型,被 mcp 引用 [C:INFERRED]。 |
| 5 | Abort 工具 | `packages/agent-core/src/utils/abort.ts` | 迁移至 shared 包 | 自包含,被 mcp 引用 [C:INFERRED]。 |
| 6 | Version 工具 | `packages/agent-core/src/version.ts` | 迁移/改造至 shared 包 | 需改为读取 shared 自身 `package.json`,原 agent-core 再封装 [C:INFERRED]。 |
| 7 | Tool 执行类型 | `packages/agent-core/src/loop/types.ts` | 迁移类型至 shared 包 | 仅移动 `ExecutableTool*` 等被 mcp 引用的类型,不迁实现 [C:INFERRED]。 |
| 8 | Input JSON Schema | `packages/agent-core/src/tools/support/input-schema.ts` | 迁移至 shared 包 | 自包含,被 mcp 引用 [C:INFERRED]。 |
| 9 | MCP 事件常量 | `packages/agent-core/src/rpc/events.ts` | 抽取至 shared 包 | 只抽取 `MCP_OAUTH_AUTHORIZATION_URL_TOOL_UPDATE` 与 `McpOAuthAuthorizationUrlUpdateData`,其它事件留在 agent-core [C:INFERRED]。 |
| 10 | Wasm diff | `packages/agent-core/src/utils/wasm-diff.ts` | 迁移至 code-review 包 | 仅被 code-review 使用,随宿主移动 [C:USER]。 |
| 11 | 循环依赖检测 | 当前无 | 新增 `madge` | 当前 monorepo 没有 `madge`/`dpdm`,需新增 [C:INFERRED]。 |

---

## Architecture / Design

### 新增包与依赖方向(单向无环)

```
                        ┌─────────────────────┐
                        │  @odysseythink/...  │
                        │  integration-tests  │
                        └──────────┬──────────┘
                                   │ devDependencies
        ┌──────────────────────────┼──────────────────────────┐
        │                          │                          │
        ▼                          ▼                          ▼
┌───────────────┐        ┌───────────────┐        ┌───────────────┐
│  code-review  │        │  e2e-testing  │        │   mcp-host    │
│               │        │               │        │               │
└───────┬───────┘        └───────┬───────┘        └───────┬───────┘
        │                        │                        │
        └────────────────────────┼────────────────────────┘
                                 │
                                 ▼
                ┌───────────────────────────────┐
                │  @odysseythink/agent-core     │
                │  (depends on all new packages)│
                └───────────────────────────────┘
                                 │
                                 ▼
                ┌───────────────────────────────┐
                │ @odysseythink/agent-core-shared│
                └───────────────────────────────┘
```

### 数据流

1. `agent-core` 内部的 tools / rpc / agent 等调用者,把 `import { X } from '#/code-review/...'` 改为 `import { X } from '@odysseythink/code-review'`。
2. 三新包把 `import { Y } from '#/errors'`、`#/config/schema` 等改为从 `@odysseythink/agent-core-shared` 导入。
3. `agent-core-shared` 成为这些基础类型/工具的唯一真实来源;`agent-core` 本身也改为从 shared 导入这些原语。
4. 跨模块集成测试集中到 `@odysseythink/integration-tests`,它通过 workspace `devDependencies` 引用所有相关包。

---

## Data Models

### `@odysseythink/agent-core-shared` 导出清单 [C:INFERRED]

```ts
// errors
export { ErrorCodes, OdyError, OdyErrorCode, OdyErrorOptions, OdyErrorPayload,
         fromOdyErrorPayload, isOdyError, makeErrorPayload, toOdyErrorPayload } from './errors';

// config types/schemas (仅下沉被外部包需要的部分)
export { McpServerConfigSchema, McpServerConfig, McpServerStdioConfig, McpServerHttpConfig,
         E2EConfigSchema, E2EConfig,
         OdyConfigSchema, OdyConfig, OdyConfigPatch } from './config';

// logging interface
export type { Logger, LogLevel, LogContext, LogPayload } from './logging';

// abort utilities
export { abortError, abortable, UserCancellationError, isUserCancellation,
         createDeadlineAbortSignal, DeadlineAbortSignal, linkAbortSignal } from './abort';

// version
export { getCoreVersion } from './version';

// tool execution types (type-only)
export type { ExecutableTool, ExecutableToolContext, ExecutableToolResult,
              ToolExecution, ToolCall } from './tool-execution';

// input schema helper
export { toInputJsonSchema } from './input-schema';

// mcp events
export { MCP_OAUTH_AUTHORIZATION_URL_TOOL_UPDATE } from './mcp-events';
export type { McpOAuthAuthorizationUrlUpdateData } from './mcp-events';
```

### 各新包导出(保持与原目录一致)

- `@odysseythink/code-review`:原 `code-review/*` 全部公开接口。
- `@odysseythink/e2e-testing`:原 `e2e-testing/*` 全部公开接口。
- `@odysseythink/mcp-host`:原 `mcp/*` 全部公开接口。

### `package.json` 模板(复用 peer 包惯例) [C:INFERRED]

```json
{
  "name": "@odysseythink/xxxx",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "imports": { "#/*": ["./src/*.ts", "./src/*/index.ts"] },
  "exports": { ".": { "types": "./src/index.ts", "default": "./src/index.ts" } },
  "scripts": {
    "build": "tsdown",
    "test": "vitest run",
    "typecheck": "tsc -p tsconfig.json --noEmit",
    "clean": "rm -rf dist"
  }
}
```

---

## Algorithms

### A1. 拆包迁移算法 [C:INFERRED]

输入:源目录 `src/<module>`、目标包 `packages/<package>/src`、依赖映射表 `M: oldImport → newImport`
输出:目标包文件 + 所有 import 已重写

```
function migrateModule(srcDir, destPkgDir, importMap):
  copy srcDir → destPkgDir/src/<module>
  create destPkgDir/src/index.ts exposing all public exports
  create destPkgDir/package.json, tsconfig.json (from peer template)

  for each .ts file under destPkgDir/src:
    for each import statement in file:
      if import source matches a key in importMap:
        replace source with importMap[source]

  remove srcDir from original agent-core
  update pnpm-workspace.yaml and flake.nix to include destPkgDir
```

### A2. Import 重写映射(以 code-review 为例) [C:INFERRED]

| 原 import | 新 import |
|---|---|
| `import { X } from '#/errors'` | `import { X } from '@odysseythink/agent-core-shared'` |
| `import { X } from '#/config'` | `import { X } from '@odysseythink/agent-core-shared'` |
| `import { X } from '../utils/wasm-diff'` | `import { X } from '#/<module>/wasm-diff'`(保留在 code-review 包内) |

`e2e-testing` 与 `mcp-host` 同理,仅映射表不同。

### A3. `agent-core` 内部调用点更新 [C:INFERRED]

下列调用点需把 `import ... from '#/code-review/...'` 改为 `import ... from '@odysseythink/code-review'`:

- `packages/agent-core/src/index.ts:87-108` —— 移除 code-review 相关 re-export。
- `packages/agent-core/src/rpc/core-api.ts:25` —— `CodeReviewDiffSource` 类型引用。
- `packages/agent-core/src/rpc/core-impl.ts:12-15` —— `fetchDiff`、`buildAuditDigest`、`createCodeReviewExecutor`、`resolveCodeReviewModel`。
- `packages/agent-core/src/tools/builtin/code-review/request-code-review.ts:10-14` —— code-review 工具实现。

`e2e-testing` 调用点:

- `packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts:19-21` —— `E2EPlanEnricher`、`E2EConfigResolver`、`registry`。
- `packages/agent-core/src/tools/builtin/test-review/review-tests.ts:9` —— `parseGitStatusShort`。
- `packages/agent-core/src/tools/builtin/e2e/run-e2e-tests.ts:8-12` —— `E2EConfigResolver`、`E2ETestExecutor`、`registry`、`detectChangedFiles`。
- `packages/agent-core/src/agent/compaction/normal-task-checkpoint.ts:19` —— `detectChangedFiles`。

`mcp-host` 调用点:

- `packages/agent-core/src/session/index.ts:23` —— MCP 会话配置相关。
- `packages/agent-core/src/agent/tool/index.ts:10-15` —— MCP tool 集成。
- `packages/agent-core/src/agent/index.ts:18` —— `McpConnectionManager` 类型。
- `packages/agent-core/src/rpc/core-impl.ts:39` 与 `112-114` —— `resolveSessionMcpConfig`、`BuiltInMcpRegistry`、`createChromeDevToolsServerDefinition`。

### A4. 循环依赖检测算法 [C:INFERRED]

```
function runDependencyGuard():
  for each workspace package P in [agent-core, code-review, e2e-testing, mcp-host, shared]:
    cycles = madge --circular packages/P/src
    if cycles is not empty:
      fail build with message: "Circular dependency detected in P: cycles"

  crossCycles = madge --circular --extensions ts packages/
  if crossCycles is not empty:
    fail build with message: "Cross-package circular dependency: crossCycles"
```

---

## Error Handling

| 错误类别 | 即时处理 | 降级路径 | 恢复条件 |
|---|---|---|---|
| 拆包 PR 引入循环依赖 | `madge` CI 失败,PR 禁止合并 | 回滚该 PR 或拆分为更小单元 | 消除循环后重新跑 `madge` 通过 |
| 新包类型检查失败 | `tsc --noEmit` 失败,PR 禁止合并 | 修复 import 路径或补全 shared 导出 | `pnpm typecheck` 全绿 |
| 单元测试随源码迁移后失败 | 该包 `vitest run` 失败 | 保留原实现,仅修复 import | 测试通过 |
| 集成测试在新包边界失败 | `integration-tests` 失败 | 把测试临时迁回 agent-core 或加 mock | 集成测试通过 |
| shared 包版本/导出遗漏 | 下游包编译失败 | 补充 shared 导出 | 全 workspace typecheck 通过 |
| `flake.nix` workspace 列表漏更新 | Nix 构建丢失文件或依赖 | 补齐 `workspacePaths` + `workspaceNames` | `nix build` 通过 |

---

## C6. Test Plan

### 单元测试迁移 [C:INFERRED]

| 原测试位置 | 新测试位置 | 断言要求 |
|---|---|---|
| `packages/agent-core/test/code-review/*.test.ts` | `packages/code-review/test/*.test.ts` | 所有原断言通过;仅 import 路径改为新包或 shared |
| `packages/agent-core/test/e2e-testing/*.test.ts` | `packages/e2e-testing/test/*.test.ts` | 同上 |
| `packages/agent-core/test/mcp/*.test.ts` | `packages/mcp-host/test/*.test.ts` | 同上 |

### 集成测试集中 [C:USER]

原跨模块引用(如 `integration.test.ts`、`plan-enrichment.e2e.test.ts`、`connection-manager.test.ts` 等)迁移到 `packages/integration-tests/test/`,并通过 workspace devDependencies 引用所需包:

```json
{
  "devDependencies": {
    "@odysseythink/agent-core": "workspace:*",
    "@odysseythink/code-review": "workspace:*",
    "@odysseythink/e2e-testing": "workspace:*",
    "@odysseythink/mcp-host": "workspace:*"
  }
}
```

### Done Criteria(必须全部通过) [C:INFERRED]

```bash
# 1. 全 workspace 类型检查通过
pnpm typecheck

# 2. 全 workspace 测试通过
pnpm test

# 3. 循环依赖检测通过
pnpm madge:circular

# 4. 包 lint 通过
pnpm lint:pkg

# 5. Nix 构建通过(验证 flake.nix workspace 列表正确)
nix build .#ody-code
```

---

## C7. Risk Register

| 编号 | 风险 | 可能性 | 影响 | 具体缓解 |
|---|---|---|---|---|
| R1 | `config/schema.ts` 拆到 shared 时内部依赖(`matches-rule`、`session/hooks`)解耦失败 | 中 | 高:共享包被迫引入 agent-core 内部,产生循环 | 仅迁移类型/ schemas;把 `isValidPermissionPattern` 与 `HOOK_EVENT_TYPES` 的引用解耦为独立校验函数或类型-only 副本 [C:INFERRED] |
| R2 | `rpc/events.ts` 仅抽取 MCP 常量时遗漏其它包所需事件 | 低 | 中:编译失败 | 在抽取前用 Grep 确认仅 mcp 引用 `MCP_OAUTH_AUTHORIZATION_URL_TOOL_UPDATE`;抽取后跑全量 typecheck [C:INFERRED] |
| R3 | 移除 agent-core re-export 后外部消费者(apps/ody-code 等) import 断裂 | 中 | 中:UI 或 CLI 编译失败 | 在 PR 中同步搜索并替换所有 `@odysseythink/agent-core` 中对已迁移模块的导入 [C:INFERRED] |
| R4 | `madge` 规则过严或配置不当导致误报 | 低 | 低:CI 噪音 | 仅检测 `.ts` 源码文件,排除 `test/` 与 `dist/`;允许对测试 devDependency 的循环做显式豁免 [C:INFERRED] |
| R5 | 新包 `package.json` 中的 `#/*` imports 与 vitest/tsc 解析不一致 | 低 | 中:测试或构建失败 | 复用现有 peer 包(`telemetry`/`kaos`/`oauth`)的已验证模板 [C:INFERRED] |
| R6 | `wasm-diff` 迁移到 code-review 后加载 wasm 的路径问题 | 低 | 中:diff 功能回归 | 保留现有 wasm 加载逻辑,仅调整相对路径;跑 `diff-wasm-parity.test.ts` 黄金测试 [C:USER] |

---

## C9. Assumptions & Unverified Items

| # | 假设 | 来源 | 置信度 | 若错误的影响 | 验证方式 |
|---|---|---|---|---|---|
| 1 | `packages/agent-core/src/code-review/` 的所有外部调用者均已在 `agent-core` 内部,无 apps 直接引用。 | 内部复用扫描 | 中 | 漏改外部 import,编译失败 | Grep `from.*code-review` 与 `from.*@odysseythink/agent-core.*code-review` 全仓库 |
| 2 | `e2e-testing` 与 `mcp` 同理,无 apps 直接引用。 | 内部复用扫描 | 中 | 同 #1 | 同 #1 |
| 3 | `config/schema.ts` 中 `McpServerConfig`/`E2EConfig`/`OdyConfig` 可以独立下沉到 shared 包,且能解除对 `agent/permission/matches-rule` 和 `session/hooks/types` 的依赖。 | 代码阅读 | 中 | shared 包被迫依赖 agent-core 内部,产生循环 | 实际移动文件并跑 `tsc --noEmit` |
| 4 | `loop/types.ts` 中的 `ExecutableTool*` 等类型不依赖其它 agent-core 内部实现。 | 代码阅读 | 中 | shared 包需额外迁移依赖,扩大范围 | 读取 `loop/types.ts` 并验证其 import 图 |
| 5 | `rpc/events.ts` 中仅 `MCP_OAUTH_AUTHORIZATION_URL_TOOL_UPDATE` 与 `McpOAuthAuthorizationUrlUpdateData` 被 `mcp` 引用。 | 代码阅读 | 高 | 抽取不完整导致编译失败 | Grep 确认 mcp 对其它事件类型无引用 |
| 6 | 新包采用 `tsdown` + `vitest` + `#/*` imports 模板后,构建/测试/类型检查均与现有 peer 包行为一致。 | 现有 peer 包模板 | 高 | 新包构建失败 | 按模板创建并跑 `build`/`test`/`typecheck` |
| 7 | 单元测试可以按源码边界迁移到新包,而跨模块测试可以集中到 `integration-tests` 且不产生循环依赖。 | 用户决策 | 中 | 测试组织混乱或 devDependency 循环 | 迁移后跑 `madge --circular` 包含 test 目录 |
| 8 | `madge` 可以配置为检测本 monorepo 的包间循环依赖。 | 开源工具已知能力 | 中 | CI 无法自动守护 | 安装后跑 `madge --circular packages/*/src` |
| 9 | `wasm-diff` 迁移到 code-review 包后,其 wasm 资源加载路径只需调整相对路径即可工作。 | 用户决策 | 中 | diff 功能回归 | 跑 `diff-wasm-parity.test.ts` |
| 10 | 移除 agent-core re-export 后,`apps/ody-code` 与 `apps/vis` 中不存在对已迁移模块的导入。 | 内部复用扫描 | 中 | 外部应用编译失败 | Grep `@odysseythink/agent-core` 使用并结合 `pnpm typecheck` |

---

## Self-Review

**Security**: 检查了共享包暴露面。共享包仅包含类型、错误码、工具函数与配置 schema,不包含密钥、PII 处理或网络逻辑;新包均为 `private`,不发布到 npm。未发现新增攻击面。

**Test**: 检查了每个行为的对偶用例。
- 拆包成功:对应 Done Criteria 中的 `pnpm typecheck` / `pnpm test` 全绿。
- 循环依赖被拦截:新增 must-reject 场景——CI `pnpm madge:circular` 必须失败于人为制造的 `A→B→A` import,已列入 Test Plan。
- 无遗留 `#/<module>` import:作为 must-reject 场景,在 PR review 中通过 Grep 验证。

**Ops**: 检查了新增成本与标识冲突。
- 新增 5 个 private workspace 包,不会增加 npm 发布面;`pnpm install` 时间略有增加,可接受。
- 新包版本统一从 `0.1.0` 开始,不存在与现有包版本冲突。
- `madge` CI 步骤增加约数秒,可接受。

**Integration**: 验证了设计依赖的代码/钩子真实存在。
- 已读 `packages/agent-core/src/errors.ts`、`errors/index.ts`、`config/schema.ts`、`logging/types.ts`、`utils/abort.ts`、`version.ts`、`loop/index.ts`、`tools/support/input-schema.ts`、`rpc/events.ts`、`utils/wasm-diff.ts` 等文件,确认存在。
- 已确认 `pnpm-workspace.yaml` 与 `flake.nix` 存在且需同步更新。
- 已确认现有 peer 包 `telemetry`/`kaos`/`oauth` 使用 `tsdown`/`vitest` 模板。
- 设计文件按用户最终确认路径写入 `.ody-code/designs/2026-06-24-phase-1-c-agent-core-extraction.md`,无静默改目标。

**Scope**: 本设计仍是一个连贯主题——`agent-core` 拆包启动。`office-hours`、Rust、Transport 已明确排除;未膨胀为多个独立项目。无需拆分。

---

## User Approval

- 审计级别:Deep [C:USER]
- 设计文件:`.ody-code/designs/2026-06-24-phase-1-c-agent-core-extraction.md`
- 状态:待审批

待审批后,下一步推荐运行 `/plan` 生成具体实施计划。
