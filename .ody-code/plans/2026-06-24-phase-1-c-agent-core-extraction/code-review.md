# Part B1: 创建 `@odysseythink/code-review`

本 Part 将 `packages/agent-core/src/code-review/` 整体迁移为独立 peer 包 `@odysseythink/code-review`，并把 `wasm-diff` 及其 wasm 加载依赖一并移入。完成后 agent-core 内部对 `#/code-review/*` 的引用全部改为 `@odysseythink/code-review`。

---

### Task B1.1: 创建 `code-review` 包骨架

**Depends on:** `shared.md: Task A13`  
**Files:**
- Create: `packages/code-review/package.json`
- Create: `packages/code-review/tsconfig.json`
- Create: `packages/code-review/vitest.config.ts`
- Create: `packages/code-review/src/index.ts`

- [ ] 编写 `packages/code-review/package.json`：

```json
{
  "name": "@odysseythink/code-review",
  "version": "0.1.0",
  "private": true,
  "description": "Code review subsystem extracted from agent-core",
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
    "@odysseythink/agent-core-shared": "workspace:^"
  }
}
```

- [ ] 编写 `packages/code-review/tsconfig.json`：

```json
{
  "extends": "../../tsconfig.json",
  "compilerOptions": {},
  "include": ["src", "test"]
}
```

- [ ] 编写 `packages/code-review/vitest.config.ts`：

```ts
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    name: 'code-review',
    include: ['test/**/*.test.ts'],
  },
});
```

- [ ] 编写占位 `packages/code-review/src/index.ts`：

```ts
export {};
```

- [ ] 运行 typecheck 确认骨架通过：

```bash
cd packages/code-review && pnpm typecheck
```

- [ ] 提交。

---

### Task B1.2: 迁移源码并创建公开 `index.ts`

**Depends on:** Task B1.1  
**Files:**
- Create: `packages/code-review/src/diff.ts`
- Create: `packages/code-review/src/executor.ts`
- Create: `packages/code-review/src/model-resolver.ts`
- Create: `packages/code-review/src/prompt.ts`
- Create: `packages/code-review/src/report.ts`
- Create: `packages/code-review/src/simplicity.ts`
- Create: `packages/code-review/src/types.ts`
- Create: `packages/code-review/src/wasm-diff.ts`
- Modify: `packages/code-review/src/index.ts`

- [ ] 将 `packages/agent-core/src/code-review/*.ts` 全部复制到 `packages/code-review/src/`。

- [ ] 将 `packages/agent-core/src/utils/wasm-diff.ts` 复制到 `packages/code-review/src/wasm-diff.ts`。

- [ ] 修改 `packages/code-review/src/diff.ts` 中的 wasm-diff import：

```ts
import { formatGitDiff } from './wasm-diff';
```

- [ ] 修改 `packages/code-review/src/wasm-diff.ts` 中的 wasm-loader/string import：

```ts
import {
  loadWasmModule,
  wrapWithFallback,
  type WasmFlagId,
  type LoadContext,
} from '@odysseythink/agent-core-shared';
import { callWasmStringFunction } from '@odysseythink/agent-core-shared';
```

并调整 `WASM_PATH` 的相对路径。原路径为：

```ts
new URL('../../../../rust-ody/target/wasm32-unknown-unknown/release/ody_rust.wasm', import.meta.url)
```

新包位于 `packages/code-review/src/wasm-diff.ts`，到 `rust-ody` 的相对路径为 `../../../rust-ody/...`（向上 3 层到 repo root，再进入 rust-ody），因此改为：

```ts
new URL('../../../rust-ody/target/wasm32-unknown-unknown/release/ody_rust.wasm', import.meta.url)
```

- [ ] 修改 `packages/code-review/src/model-resolver.ts`：

```ts
import { ErrorCodes, OdyError } from '@odysseythink/agent-core-shared';
import type { OdyConfig } from '@odysseythink/agent-core-shared';
```

- [ ] 更新 `packages/code-review/src/index.ts` 公开全部接口：

```ts
export { fetchDiff, buildDiffSource, parsePrNumber } from './diff';
export type { CodeReviewDiffSource } from './diff';
export { createCodeReviewExecutor } from './executor';
export type { CodeReviewExecutorDeps } from './executor';
export { resolveCodeReviewModel } from './model-resolver';
export type { ResolveModelOverrides } from './model-resolver';
export { buildReviewPrompt, parseReviewReport } from './prompt';
export { renderCodeReviewReportToMarkdown } from './report';
export {
  parseSimplicityReport,
  buildSimplicityReviewPrompt,
  buildSimplicityAuditPrompt,
  buildAuditDigest,
} from './simplicity';
export type {
  SimplicityTag,
  RepoAuditDigest,
  FileSnippet,
} from './simplicity';
export type {
  CodeReviewRequestInput,
  CodeReviewReport,
  CodeReviewFinding,
  CodeReviewProgress,
  CodeReviewProgressStage,
} from './types';
export { loadWasmDiffModule, initDiffWasm, computeTextDiff, formatGitDiff } from './wasm-diff';
export type { DiffModule } from './wasm-diff';
```

- [ ] 在 `packages/code-review` 运行 typecheck：

```bash
pnpm typecheck
```

预期：无错误。

- [ ] 提交。

---

### Task B1.3: 更新 agent-core 内部对 code-review 的调用点

**Depends on:** Task B1.2  
**Files:**
- Modify: `packages/agent-core/src/index.ts:86-108`
- Modify: `packages/agent-core/src/rpc/core-api.ts:25`
- Modify: `packages/agent-core/src/rpc/core-impl.ts:12-15`
- Modify: `packages/agent-core/src/tools/builtin/code-review/request-code-review.ts:10-14`
- Modify: 所有 `packages/agent-core/test/code-review/*.test.ts` 中的 import

- [ ] 搜索所有 agent-core 内部对 `#/code-review` 的引用：

```bash
cd packages/agent-core
rg "from '#\/code-review" src test -n
```

预期文件：
- `src/index.ts:87-108`
- `src/rpc/core-api.ts:25`
- `src/rpc/core-impl.ts:12-15`
- `src/tools/builtin/code-review/request-code-review.ts:10-14`
- 若干 test 文件

- [ ] 修改 `packages/agent-core/src/index.ts`：删除 lines 86-108 的 code-review re-export 块。

- [ ] 修改 `packages/agent-core/src/rpc/core-api.ts`：

```ts
import type { CodeReviewDiffSource } from '@odysseythink/code-review';
```

- [ ] 修改 `packages/agent-core/src/rpc/core-impl.ts`：

```ts
import { fetchDiff as codeReviewFetchDiff } from '@odysseythink/code-review';
import { buildAuditDigest } from '@odysseythink/code-review';
import { createCodeReviewExecutor } from '@odysseythink/code-review';
import { resolveCodeReviewModel } from '@odysseythink/code-review';
```

可合并为单条 import：

```ts
import {
  buildAuditDigest,
  createCodeReviewExecutor,
  fetchDiff as codeReviewFetchDiff,
  resolveCodeReviewModel,
} from '@odysseythink/code-review';
```

- [ ] 修改 `packages/agent-core/src/tools/builtin/code-review/request-code-review.ts`：

```ts
import { fetchDiff } from '@odysseythink/code-review';
import { buildReviewPrompt, parseReviewReport } from '@odysseythink/code-review';
import { resolveCodeReviewModel } from '@odysseythink/code-review';
import { renderCodeReviewReportToMarkdown } from '@odysseythink/code-review';
import type { CodeReviewDiffSource, CodeReviewReport } from '@odysseythink/code-review';
```

- [ ] 更新测试文件 `packages/agent-core/test/code-review/*.test.ts` 中的 `#/code-review` import 为 `@odysseythink/code-review`。

- [ ] 运行 agent-core typecheck：

```bash
cd packages/agent-core && pnpm typecheck
```

- [ ] 提交。

---

### Task B1.4: 迁移测试到 code-review 包并验证

**Depends on:** Task B1.3  
**Files:**
- Create: `packages/code-review/test/audit-scanner.test.ts`
- Create: `packages/code-review/test/diff-wasm-parity.test.ts`
- Create: `packages/code-review/test/diff.test.ts`
- Create: `packages/code-review/test/executor.test.ts`
- Create: `packages/code-review/test/model-resolver.test.ts`
- Create: `packages/code-review/test/prompt.test.ts`
- Create: `packages/code-review/test/request-code-review.test.ts`
- Create: `packages/code-review/test/simplicity.test.ts`
- Modify: 上述文件中的 import 路径
- Delete: `packages/agent-core/test/code-review/`
- Delete: `packages/agent-core/src/code-review/`

- [ ] 将 `packages/agent-core/test/code-review/*.test.ts` 复制到 `packages/code-review/test/`。

- [ ] 更新这些测试文件中的 import：
  - `#/code-review/*` → `@odysseythink/code-review` 或相对路径 `./src/*`
  - `#/errors` → `@odysseythink/agent-core-shared`
  - `#/config/schema` 或 `#/config` → `@odysseythink/agent-core-shared`
  - `#/utils/wasm-diff` → `./src/wasm-diff` 或 `@odysseythink/code-review`

具体搜索：

```bash
cd packages/code-review
rg "from '#" test -n
```

逐个替换。

- [ ] 删除 agent-core 中已迁移的源码与测试目录：

```bash
rm -rf packages/agent-core/src/code-review
rm -rf packages/agent-core/test/code-review
```

- [ ] 运行 code-review 包测试：

```bash
cd packages/code-review && pnpm test
```

预期：所有测试通过。若 `diff-wasm-parity.test.ts` 依赖 wasm 文件不存在，可接受其 fallback JS 路径通过。

- [ ] 运行 agent-core typecheck：

```bash
cd packages/agent-core && pnpm typecheck
```

- [ ] 运行全 packages typecheck：

```bash
pnpm -r --filter './packages/*' run typecheck
```

- [ ] 提交。

---

## Local Self-Review (Part B1)

- [ ] **Spec-coverage**: 覆盖设计中新建 `@odysseythink/code-review` 与 agent-core 调用点更新需求。
- [ ] **Placeholder scan**: 无 TODO；给出完整 package.json、index.ts、import 替换清单。
- [ ] **No phantom tasks**: 每个 Task 产生可验证的包创建/源码迁移/测试通过变更。
- [ ] **Dependency soundness**: 仅依赖 shared.md 完成后导出的 shared 包；无反向依赖 agent-core。
- [ ] **Caller & build soundness**: Task B1.3 列出并更新所有 `#/code-review` 调用点（含测试），Task B1.4 以全 packages typecheck 结束。
- [ ] **Test-the-risk**: 迁移黄金测试 `diff-wasm-parity.test.ts`，验证 wasm 路径调整后行为一致；其余原断言保留。
- [ ] **Type consistency**: 公开导出的类型名与原 `code-review/*` 一致；`CodeReviewDiffSource` 等从 `@odysseythink/code-review` 导出。
