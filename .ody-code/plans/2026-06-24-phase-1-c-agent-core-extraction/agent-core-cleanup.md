# Part C: 清理 agent-core 内部目录与 re-export

本 Part 在 B1/B2/B3 完成后执行 `packages/agent-core` 的最终清理：删除所有已迁出的内部源码目录，移除 index.ts 中对三子系统的 re-export，修正 workspace 中仍通过 `@odysseythink/agent-core` re-export 消费 code-review 能力或 MCP 常量的外部包（主要是 `node-sdk`），并运行全树 typecheck / test。

**依赖上游：**
- `shared.md` Task A13（shared 包可用）
- `code-review.md` Task B1.4（code-review 包可用、agent-core 内部 `#/code-review` 调用已切换）
- `e2e-testing.md` Task B2.4（e2e-testing 包可用、agent-core 内部 `#/e2e-testing` 调用已切换）
- `mcp-host.md` Task B3.4（mcp-host 包可用、agent-core 内部 `#/mcp` 调用已切换）

**Phase C 任务图：**

```
C1 更新 agent-core package.json 依赖
   ↓
C2 修正 node-sdk 对 code-review / shared 的引用
   ↓
C3 最终清理检查 + 全树 typecheck/test
```

**风险与开放问题：**

| # | 风险 | 缓解措施 |
|---|---|---|
| R1 | `agent-core` 删除 re-export 后，`node-sdk` 仍从 `@odysseythink/agent-core` 导入 code-review 相关符号，会编译失败。 | C2 显式将 `node-sdk` 中的 code-review 导出/类型改为从 `@odysseythink/code-review` 导入，MCP OAuth 常量改为从 `@odysseythink/agent-core-shared` 导入。 |
| R2 | `agent-core` 不再直接依赖 `@modelcontextprotocol/sdk`，但 `package.json` 中残留会导致 Nix 多拉依赖。 | C1 删除该依赖。 |
| R3 | 某个子目录或测试文件仍残留 `#/code-review`、`#/e2e-testing`、`#/mcp` 引用。 | C3 用 `rg` 全量扫描 `src` 与 `test`，并把扫描作为通过条件。 |

---

### Task C1: 更新 agent-core 的 package.json 依赖

**Depends on:** `code-review.md` Task B1.4、`e2e-testing.md` Task B2.4、`mcp-host.md` Task B3.4
**Files:**
- Modify: `packages/agent-core/package.json:56-77`

- [ ] 将 `dependencies` 替换为以下完整块（保留其他运行时依赖，新增 4 个 workspace 包，移除不再直接使用的 `@modelcontextprotocol/sdk`）：

```json
  "dependencies": {
    "@antfu/utils": "^9.3.0",
    "@odysseythink/agent-core-shared": "workspace:^",
    "@odysseythink/code-review": "workspace:^",
    "@odysseythink/e2e-testing": "workspace:^",
    "@odysseythink/kaos": "workspace:^",
    "@odysseythink/kosong": "workspace:^",
    "@odysseythink/mcp-host": "workspace:^",
    "@mozilla/readability": "^0.6.0",
    "ajv": "^8.18.0",
    "ajv-formats": "^3.0.1",
    "js-yaml": "^4.1.1",
    "linkedom": "^0.18.12",
    "nunjucks": "^3.2.4",
    "open": "^10.2.0",
    "pathe": "^2.0.3",
    "picomatch": "^4.0.4",
    "proper-lockfile": "^4.1.2",
    "regexp.escape": "^2.0.1",
    "retry": "0.13.1",
    "smol-toml": "^1.6.1",
    "tar": "^7.5.13",
    "yauzl": "^3.3.0",
    "zod": "catalog:"
  }
```

- [ ] 同步 workspace 依赖链接：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm install
```

预期：`Lockfile is up to date` 或正常更新，exit 0。

- [ ] 运行 agent-core 类型检查，确认 workspace 依赖可解析：

```bash
cd packages/agent-core
pnpm typecheck
```

预期：无类型错误，exit 0。

- [ ] 提交：`git add packages/agent-core/package.json pnpm-lock.yaml && git commit -m "chore(agent-core): add peer workspace deps and drop direct mcp sdk"`

---

### Task C2: 修正 node-sdk 对 code-review / shared 的引用

**Depends on:** Task C1
**Files:**
- Modify: `packages/node-sdk/package.json:68-74`
- Modify: `packages/node-sdk/src/index.ts:79-83`
- Modify: `packages/node-sdk/src/types.ts:67-74`
- Modify: `packages/node-sdk/src/kimi-harness.ts:1-16`
- Modify: `packages/node-sdk/src/events.ts:13`

- [ ] 在 `packages/node-sdk/package.json` 的 `devDependencies` 中新增 `@odysseythink/agent-core-shared` 与 `@odysseythink/code-review`：

```json
  "devDependencies": {
    "@odysseythink/agent-core": "workspace:^",
    "@odysseythink/agent-core-shared": "workspace:^",
    "@odysseythink/code-review": "workspace:^",
    "@odysseythink/kaos": "workspace:^",
    "@odysseythink/kimi-code-oauth": "workspace:^",
    "@odysseythink/kosong": "workspace:^",
    "@types/yazl": "^2.4.6"
  }
```

- [ ] 修改 `packages/node-sdk/src/index.ts` 中的 code-review re-export：

```ts
// Code review report markdown renderer
export { renderCodeReviewReportToMarkdown } from '@odysseythink/code-review';
// Code review model resolver
export { resolveCodeReviewModel } from '@odysseythink/code-review';
export type { ResolveModelOverrides } from '@odysseythink/code-review';
```

- [ ] 修改 `packages/node-sdk/src/types.ts` 中的 code-review 类型来源：

```ts
export type {
  CodeReviewDiffSource,
  CodeReviewRequestInput,
  CodeReviewReport,
  CodeReviewFinding,
  CodeReviewProgress,
  CodeReviewProgressStage,
} from '@odysseythink/code-review';
```

- [ ] 修改 `packages/node-sdk/src/kimi-harness.ts`，把 `CodeReviewReport` 从 agent-core 的 import 中移除，并新增独立 import：

替换前：
```ts
import {
  ensureConfigFile,
  ErrorCodes,
  OdyError,
  getRootLogger,
  noopTelemetryClient,
  resolveConfigPath,
  resolveOdyHome,
  resolveLoggingConfig,
  withTelemetryContext,
  type CodeReviewReport,
  type ExperimentalFlagMap,
  type TelemetryClient,
  type TelemetryContextPatch,
  type TelemetryProperties,
} from '@odysseythink/agent-core';
```

替换后：
```ts
import {
  ensureConfigFile,
  ErrorCodes,
  OdyError,
  getRootLogger,
  noopTelemetryClient,
  resolveConfigPath,
  resolveOdyHome,
  resolveLoggingConfig,
  withTelemetryContext,
  type ExperimentalFlagMap,
  type TelemetryClient,
  type TelemetryContextPatch,
  type TelemetryProperties,
} from '@odysseythink/agent-core';
import type { CodeReviewReport } from '@odysseythink/code-review';
```

- [ ] 修改 `packages/node-sdk/src/events.ts` 中的 MCP OAuth 常量来源：

```ts
export { MCP_OAUTH_AUTHORIZATION_URL_TOOL_UPDATE } from '@odysseythink/agent-core-shared';
```

- [ ] 同步 workspace 链接：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm install
```

预期：exit 0。

- [ ] 运行 node-sdk 类型检查：

```bash
cd packages/node-sdk
pnpm typecheck
```

预期：无类型错误，exit 0。

- [ ] 提交：`git add packages/node-sdk && git commit -m "refactor(node-sdk): import code-review/mcp-constants from extracted packages"`

---

### Task C3: 最终清理检查与全树验证

**Depends on:** Task C2
**Files：**
- Verify: `packages/agent-core/src/*` 无 `#/code-review`、`#/e2e-testing`、`#/mcp` 等内部引用
- Verify: `packages/agent-core/test/code-review`、`packages/agent-core/test/e2e-testing` 已删除（仅保留仍需要的集成测试文件，不保留空目录）
- Verify: `packages/agent-core/src/code-review`、`packages/agent-core/src/e2e-testing`、`packages/agent-core/src/mcp` 已删除

- [ ] 搜索 agent-core 内部残留引用（必须无任何匹配）：

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core
rg "from '#\\/code-review|from '#\\/e2e-testing|from '#\\/mcp|from '\.\./code-review|from '\.\./e2e-testing|from '\.\./mcp|from '\.\./\.\./code-review|from '\.\./\.\./e2e-testing|from '\.\./\.\./mcp" src test
```

预期：`rg` 无输出，exit code 1（无匹配时 ripgrep 返回 1）。若输出任何匹配，必须先回到对应 Part 修复。

- [ ] 确认旧源码目录已删除：

```bash
cd /Users/ranwei/workspace/ody-code
test ! -d packages/agent-core/src/code-review
test ! -d packages/agent-core/src/e2e-testing
test ! -d packages/agent-core/src/mcp
test ! -d packages/agent-core/test/code-review
echo "old directories removed"
```

预期输出：`old directories removed`。

- [ ] 确认 `packages/agent-core/src/index.ts` 不再 re-export code-review 符号：

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core
rg "renderCodeReviewReportToMarkdown|resolveCodeReviewModel|CodeReviewRequestInput|CodeReviewFinding|buildSimplicityReviewPrompt" src/index.ts
```

预期：无匹配。

- [ ] 运行全 workspace 类型检查（必须包含 test 文件）：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm -r --filter './packages/*' run typecheck
```

预期：所有 package 输出 `success`，exit 0。

- [ ] 运行 agent-core 测试，确保集成测试未因目录删除而损坏：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm --filter @odysseythink/agent-core test
```

预期：全部通过，exit 0。

- [ ] 提交：`git add -A && git commit -m "refactor(agent-core): remove migrated subsystems and finalize re-exports"`

---

## Local Self-Review (Part C)

- [ ] **1. Spec-coverage table**

| 需求 | 覆盖任务 | 状态 |
|---|---|---|
| agent-core 不再直接依赖 `@modelcontextprotocol/sdk` | C1 | covered |
| agent-core 依赖 workspace 新包（shared / code-review / e2e-testing / mcp-host） | C1 | covered |
| agent-core 不再 re-export code-review 符号 | B1.3 + C3 验证 | covered |
| node-sdk 改从 `@odysseythink/code-review` 导入 code-review 能力 | C2 | covered |
| node-sdk 改从 `@odysseythink/agent-core-shared` 导入 MCP OAuth 常量 | C2 | covered |
| 确认已迁出源码目录与测试目录已删除 | C3 | covered |
| 确认 agent-core 内部无 `#/code-review`、`#/e2e-testing`、`#/mcp` 残留引用 | C3 | covered |
| 全 workspace typecheck 与 agent-core 测试通过 | C3 | covered |

- [ ] **2. Placeholder scan**：无 TODO/TBD；所有 import 替换均给出具体来源包与符号；所有验证命令给出预期输出。
- [ ] **3. No phantom tasks**：C1/C2/C3 均有可验证产物（package.json 变更、node-sdk typecheck、agent-core 残留扫描、全树 typecheck、agent-core 测试）。
- [ ] **4. Dependency soundness**：C1 依赖 B1/B2/B3 完成；C2 依赖 C1（workspace 链接已同步）；C3 依赖 C2；无引用后续任务才创建的符号。
- [ ] **5. Caller & build soundness**：agent-core index 删除 re-export 属于 shared-signature 变化，其外部 consumer `node-sdk` 在同一 Part 的 C2 中完成更新；C3 以 `pnpm -r --filter './packages/*' run typecheck` 全树验证，包含 test 文件；未把同一签名改动拆到多个任务。
- [ ] **6. Test-the-risk**：C3 的 `rg` 残留引用扫描与旧目录存在性检查是行为级断言（若目录未删或 import 未改即失败）；agent-core 集成测试继续覆盖 Session/McpConnectionManager/ToolManager 的交互；node-sdk typecheck 验证其公开导出仍可解析到正确包。
- [ ] **7. Type consistency**：node-sdk 中 `CodeReviewReport`、`CodeReviewDiffSource`、`ResolveModelOverrides` 等类型名与 `@odysseythink/code-review` 导出完全一致；`MCP_OAUTH_AUTHORIZATION_URL_TOOL_UPDATE` 常量名与 `@odysseythink/agent-core-shared` 导出完全一致。
