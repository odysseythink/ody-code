# Part D: 集成测试、workspace 配置、madge 守护

本 Part 创建 `@odysseythink/integration-tests` 包，把 `agent-core` 中同时依赖 `agent-core` 与新拆包（`mcp-host`、`e2e-testing`、`code-review`）的跨模块集成测试迁移过去；同步更新 `flake.nix` 的 workspace 列表；在根 `package.json` 与 `Makefile` 中引入 `madge` 循环依赖检测；最后以全 workspace typecheck、测试、`madge:circular` 与 Nix 构建作为通过条件。

**依赖上游：**
- `agent-core-cleanup.md` Task C3（agent-core 内部目录已清理、package.json 已依赖新包）
- `mcp-host.md` Task B3-4（MCP 调用点已切换、可独立测试）
- `e2e-testing.md` Task B2-4（E2E 调用点已切换、可独立测试）
- `code-review.md` Task B1.4（code-review 包已可用）

**Phase D 任务图：**

```
D1 创建 integration-tests 包骨架
   ↓
D2 暴露 agent-core 测试 helper + 迁移 MCP 集成测试
   ↓
D3 迁移 E2E 集成测试
   ↓
D4 更新 flake.nix、新增 madge 脚本与 Makefile target
   ↓
D5 全树验证（typecheck / test / madge / nix）
```

**风险与开放问题：**

| # | 风险 | 缓解措施 |
|---|---|---|
| R1 | 集成测试依赖 `agent-core/test` 内部 helper，迁出后无法解析。 | D2 在 `agent-core` 新增 `./test/helpers` 子路径导出，供 `integration-tests` 内部使用。 |
| R2 | 迁移后 `flake.nix` 的 `workspacePaths`/`workspaceNames` 漏列新包，导致 Nix 构建丢文件或依赖未拉取。 | D4 显式补齐 5 个新包（shared、code-review、e2e-testing、mcp-host、integration-tests）并更新 `pnpmDeps` hash。 |
| R3 | `madge` 扫描 test 目录或把测试 import 误判为源码循环依赖。 | `madge:circular` 仅扫描 `packages/*/src/**/*.ts`，排除 `test/` / `dist/` / `node_modules/`。 |
| R4 | Nix `pnpmDeps.hash` 在 lockfile 变更后需要手动更新。 | D5 以 `nix build .#ody-code` 失败输出中的预期 hash 更新 `flake.nix`，并再次构建通过。 |

---

### Task D1: 创建 `@odysseythink/integration-tests` 包骨架

**Depends on:** `agent-core-cleanup.md` Task C3
**Files:**
- Create: `packages/integration-tests/package.json`
- Create: `packages/integration-tests/tsconfig.json`
- Create: `packages/integration-tests/vitest.config.ts`
- Create: `packages/integration-tests/src/index.ts`

- [ ] 编写 `packages/integration-tests/package.json`：

```json
{
  "name": "@odysseythink/integration-tests",
  "version": "0.1.0",
  "private": true,
  "description": "Cross-package integration tests for agent-core subsystem extraction",
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
    "test": "vitest run",
    "typecheck": "tsc -p tsconfig.json --noEmit",
    "clean": "rm -rf dist"
  },
  "dependencies": {
    "@odysseythink/agent-core": "workspace:^",
    "@odysseythink/agent-core-shared": "workspace:^",
    "@odysseythink/code-review": "workspace:^",
    "@odysseythink/e2e-testing": "workspace:^",
    "@odysseythink/kaos": "workspace:^",
    "@odysseythink/kosong": "workspace:^",
    "@odysseythink/mcp-host": "workspace:^",
    "@modelcontextprotocol/sdk": "^1.29.0",
    "pathe": "^2.0.3",
    "zod": "catalog:"
  }
}
```

- [ ] 编写 `packages/integration-tests/tsconfig.json`：

```json
{
  "extends": "../../tsconfig.json",
  "compilerOptions": {},
  "include": ["src", "test"]
}
```

- [ ] 编写 `packages/integration-tests/vitest.config.ts`：

```ts
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    name: 'integration-tests',
    include: ['test/**/*.test.ts'],
  },
});
```

- [ ] 编写占位 `packages/integration-tests/src/index.ts`：

```ts
export {};
```

- [ ] 安装依赖并验证骨架：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm install
cd packages/integration-tests
pnpm typecheck
```

预期：无错误，exit 0。

- [ ] 提交：`git add packages/integration-tests && git commit -m "chore(integration-tests): bootstrap package"`

---

### Task D2: 暴露 agent-core 测试 helper 并迁移 MCP 集成测试

**Depends on:** Task D1
**Files：**
- Modify: `packages/agent-core/package.json:35-48`
- Create: `packages/agent-core/test/helpers/index.ts`
- Create: `packages/integration-tests/test/mcp/connection-manager.test.ts`
- Create: `packages/integration-tests/test/mcp/tool-manager-mcp.test.ts`
- Create: `packages/integration-tests/test/mcp/built-in-integration.test.ts`
- Create: `packages/integration-tests/test/mcp/fixtures/*.mjs`
- Delete: `packages/agent-core/test/mcp/connection-manager.test.ts`
- Delete: `packages/agent-core/test/mcp/tool-manager-mcp.test.ts`
- Delete: `packages/agent-core/test/mcp/built-in-integration.test.ts`
- Delete: `packages/agent-core/test/mcp/fixtures/*.mjs`

- [ ] 在 `packages/agent-core/package.json` 的 `exports` 中新增测试 helper 子路径导出：

```json
  "exports": {
    ".": {
      "types": "./src/index.ts",
      "default": "./src/index.ts"
    },
    "./agent/records/migration": {
      "types": "./src/agent/records/migration/index.ts",
      "default": "./src/agent/records/migration/index.ts"
    },
    "./session/store": {
      "types": "./src/session/store/index.ts",
      "default": "./src/session/store/index.ts"
    },
    "./test/helpers": {
      "types": "./test/helpers/index.ts",
      "default": "./test/helpers/index.ts"
    }
  }
```

- [ ] 创建 `packages/agent-core/test/helpers/index.ts`：

```ts
export { createCommandKaos, testAgent, type TestAgentContext } from '../agent/harness/agent';
export { createScriptedGenerate } from '../agent/harness/scripted-generate';
export {
  DEFAULT_TEST_SYSTEM_PROMPT,
  eventSnapshot,
  generateInputSnapshot,
  generateInputsSnapshot,
  normalizeGenerateInput,
  type EventSnapshot,
  type EventSnapshotEntry,
  type GenerateCall,
  type GenerateInputSnapshot,
  type GenerateInputsSnapshot,
  type RpcSnapshotEntry,
  type WireSnapshotEntry,
} from '../agent/harness/snapshots';
export {
  createFakeKaos,
  FAKE_OS_ENV,
  PERMISSIVE_WORKSPACE,
  toolContentString,
} from '../tools/fixtures/fake-kaos';
export { executeTool, type TestExecutableToolContext } from '../tools/fixtures/execute-tool';
export { testKaos, TEST_OS_ENV } from '../fixtures/test-kaos';
```

- [ ] 复制 MCP 集成测试与 fixtures：

```bash
cd /Users/ranwei/workspace/ody-code
mkdir -p packages/integration-tests/test/mcp/fixtures
cp packages/agent-core/test/mcp/connection-manager.test.ts packages/integration-tests/test/mcp/connection-manager.test.ts
cp packages/agent-core/test/mcp/tool-manager-mcp.test.ts packages/integration-tests/test/mcp/tool-manager-mcp.test.ts
cp packages/agent-core/test/mcp/built-in-integration.test.ts packages/integration-tests/test/mcp/built-in-integration.test.ts
cp packages/agent-core/test/mcp/fixtures/*.mjs packages/integration-tests/test/mcp/fixtures/
```

- [ ] 重写集成测试中的 import 来源：

```bash
cd /Users/ranwei/workspace/ody-code/packages/integration-tests/test/mcp

# agent-core / shared / mcp-host 源码引用
rg -l "from '\.\./\.\./src/errors'" . | xargs -I{} sed -i "" "s|from '../../src/errors'|from '@odysseythink/agent-core-shared'|g" {}
rg -l "from '\.\./\.\./src/session/provider-manager'" . | xargs -I{} sed -i "" "s|from '../../src/session/provider-manager'|from '@odysseythink/agent-core'|g" {}
rg -l "from '\.\./\.\./src/mcp/connection-manager'" . | xargs -I{} sed -i "" "s|from '../../src/mcp/connection-manager'|from '@odysseythink/mcp-host'|g" {}
rg -l "from '\.\./\.\./src/mcp/oauth'" . | xargs -I{} sed -i "" "s|from '../../src/mcp/oauth'|from '@odysseythink/mcp-host'|g" {}
rg -l "from '\.\./\.\./src/rpc'" . | xargs -I{} sed -i "" "s|from '../../src/rpc'|from '@odysseythink/agent-core'|g" {}
rg -l "from '\.\./\.\./src/session'" . | xargs -I{} sed -i "" "s|from '../../src/session'|from '@odysseythink/agent-core'|g" {}
rg -l "from '\.\./\.\./src/session/rpc'" . | xargs -I{} sed -i "" "s|from '../../src/session/rpc'|from '@odysseythink/agent-core'|g" {}

# 测试 helper 引用
rg -l "from '\.\./agent/harness'" . | xargs -I{} sed -i "" "s|from '../agent/harness'|from '@odysseythink/agent-core/test/helpers'|g" {}
rg -l "from '\.\./agent/harness/agent'" . | xargs -I{} sed -i "" "s|from '../agent/harness/agent'|from '@odysseythink/agent-core/test/helpers'|g" {}
rg -l "from '\.\./tools/fixtures/execute-tool'" . | xargs -I{} sed -i "" "s|from '../tools/fixtures/execute-tool'|from '@odysseythink/agent-core/test/helpers'|g" {}
rg -l "from '\.\./fixtures/test-kaos'" . | xargs -I{} sed -i "" "s|from '../fixtures/test-kaos'|from '@odysseythink/agent-core/test/helpers'|g" {}
```

- [ ] 确认无残留相对内部引用：

```bash
rg "from '\.\./\.\./src/|from '\.\./agent/|from '\.\./tools/|from '\.\./fixtures/" /Users/ranwei/workspace/ody-code/packages/integration-tests/test/mcp
```

预期：无匹配。

- [ ] 删除 agent-core 中的旧 MCP 集成测试与 fixtures：

```bash
cd /Users/ranwei/workspace/ody-code
rm -f packages/agent-core/test/mcp/connection-manager.test.ts
rm -f packages/agent-core/test/mcp/tool-manager-mcp.test.ts
rm -f packages/agent-core/test/mcp/built-in-integration.test.ts
rm -f packages/agent-core/test/mcp/fixtures/*.mjs
rmdir packages/agent-core/test/mcp/fixtures 2>/dev/null || true
```

- [ ] 运行 integration-tests 的 MCP 测试：

```bash
cd packages/integration-tests
pnpm test test/mcp
```

预期：通过，exit 0。

- [ ] 提交：`git add packages/agent-core packages/integration-tests && git commit -m "test(integration-tests): migrate mcp integration tests"`

---

### Task D3: 迁移 E2E 集成测试

**Depends on:** Task D2
**Files：**
- Create: `packages/integration-tests/test/e2e-testing/core.test.ts`
- Create: `packages/integration-tests/test/e2e-testing/generator.test.ts`
- Create: `packages/integration-tests/test/e2e-testing/integration.test.ts`
- Create: `packages/integration-tests/test/e2e-testing/plan-enrichment.e2e.test.ts`
- Delete: `packages/agent-core/test/e2e-testing/core.test.ts`
- Delete: `packages/agent-core/test/e2e-testing/generator.test.ts`
- Delete: `packages/agent-core/test/e2e-testing/integration.test.ts`
- Delete: `packages/agent-core/test/e2e-testing/plan-enrichment.e2e.test.ts`

- [ ] 复制 E2E 集成测试：

```bash
cd /Users/ranwei/workspace/ody-code
mkdir -p packages/integration-tests/test/e2e-testing
cp packages/agent-core/test/e2e-testing/core.test.ts packages/integration-tests/test/e2e-testing/core.test.ts
cp packages/agent-core/test/e2e-testing/generator.test.ts packages/integration-tests/test/e2e-testing/generator.test.ts
cp packages/agent-core/test/e2e-testing/integration.test.ts packages/integration-tests/test/e2e-testing/integration.test.ts
cp packages/agent-core/test/e2e-testing/plan-enrichment.e2e.test.ts packages/integration-tests/test/e2e-testing/plan-enrichment.e2e.test.ts
```

- [ ] 重写 E2E 集成测试的 import 来源：

```bash
cd /Users/ranwei/workspace/ody-code/packages/integration-tests/test/e2e-testing

# e2e-testing 包
rg -l "from '#/e2e-testing" . | xargs -I{} sed -i "" "s|from '#/e2e-testing/\([^']*\)'|from '@odysseythink/e2e-testing'|g" {}

# agent-core
rg -l "from '\.\./\.\./src/config/toml'" . | xargs -I{} sed -i "" "s|from '../../src/config/toml'|from '@odysseythink/agent-core'|g" {}
rg -l "from '\.\./\.\./src/agent'" . | xargs -I{} sed -i "" "s|from '../../src/agent'|from '@odysseythink/agent-core'|g" {}
rg -l "from '#/tools/builtin/planning/exit-plan-mode'" . | xargs -I{} sed -i "" "s|from '#/tools/builtin/planning/exit-plan-mode'|from '@odysseythink/agent-core'|g" {}

# shared
rg -l "from '#/config/schema'" . | xargs -I{} sed -i "" "s|from '#/config/schema'|from '@odysseythink/agent-core-shared'|g" {}

# helpers
rg -l "from '\.\./tools/fixtures/fake-kaos'" . | xargs -I{} sed -i "" "s|from '../tools/fixtures/fake-kaos'|from '@odysseythink/agent-core/test/helpers'|g" {}
```

- [ ] 修正 `generator.test.ts` 的 repo-root 路径（该测试原假设 cwd 为仓库根目录）：

在文件顶部新增：

```ts
import { fileURLToPath } from 'node:url';
```

并在 `import { dirname, join, resolve } from 'pathe';` 后添加：

```ts
const REPO_ROOT = dirname(fileURLToPath(new URL('../..', import.meta.url)));
```

然后替换以下两处：

```ts
// 替换前
    projectRoot: join(process.cwd(), 'packages/agent-core'),

// 替换后
    projectRoot: join(REPO_ROOT, 'packages/agent-core'),
```

```ts
// 替换前
const OUTPUT_DIR = 'packages/agent-core/.ody-code/test-generated/e2e';

// 替换后
const OUTPUT_DIR = join(REPO_ROOT, 'packages/agent-core/.ody-code/test-generated/e2e');
```

- [ ] 确认无残留相对内部引用：

```bash
rg "from '\.\./\.\./src/|from '#/|from '\.\./tools/" /Users/ranwei/workspace/ody-code/packages/integration-tests/test/e2e-testing
```

预期：无匹配。

- [ ] 删除 agent-core 中的旧 E2E 集成测试：

```bash
cd /Users/ranwei/workspace/ody-code
rm -f packages/agent-core/test/e2e-testing/core.test.ts
rm -f packages/agent-core/test/e2e-testing/generator.test.ts
rm -f packages/agent-core/test/e2e-testing/integration.test.ts
rm -f packages/agent-core/test/e2e-testing/plan-enrichment.e2e.test.ts
rmdir packages/agent-core/test/e2e-testing 2>/dev/null || true
```

- [ ] 运行 integration-tests 的 E2E 测试：

```bash
cd packages/integration-tests
pnpm test test/e2e-testing
```

预期：通过，exit 0。

- [ ] 提交：`git add packages/agent-core packages/integration-tests && git commit -m "test(integration-tests): migrate e2e integration tests"`

---

### Task D4: 更新 workspace 配置与引入 madge 守护

**Depends on:** Task D3
**Files：**
- Modify: `flake.nix:64-90`
- Modify: `package.json:8-27`
- Modify: `package.json:29-47`
- Modify: `Makefile:1-67`

- [ ] 更新 `flake.nix` 的 `workspacePaths`：

```nix
      workspacePaths = [
        ./packages/agent-core
        ./packages/agent-core-shared
        ./packages/code-review
        ./packages/e2e-testing
        ./packages/integration-tests
        ./packages/kaos
        ./packages/kosong
        ./packages/mcp-host
        ./packages/node-sdk
        ./packages/oauth
        ./packages/telemetry
        ./apps/ody-code
        ./apps/vis
        ./apps/vis/server
        ./apps/vis/web
        ./docs
      ];
```

- [ ] 更新 `flake.nix` 的 `workspaceNames`（顺序与 paths 对应）：

```nix
      workspaceNames = [
        "@odysseythink/agent-core"
        "@odysseythink/agent-core-shared"
        "@odysseythink/code-review"
        "@odysseythink/e2e-testing"
        "@odysseythink/integration-tests"
        "@odysseythink/kaos"
        "@odysseythink/kosong"
        "@odysseythink/mcp-host"
        "@odysseythink/ody-code-sdk"
        "@odysseythink/kimi-code-oauth"
        "@odysseythink/ody-telemetry"
        "ody-code"
        "@odysseythink/vis"
        "@odysseythink/vis-server"
        "@odysseythink/vis-web"
        "ody-code-docs"
      ];
```

- [ ] 在根 `package.json` 的 `scripts` 中新增 `madge:circular`：

```json
  "scripts": {
    "build": "pnpm -r run build",
    "build:packages": "pnpm -r --filter './packages/*' run build",
    "dev:cli": "pnpm -C apps/ody-code run dev",
    "build:plugin-marketplace": "pnpm -C apps/ody-code run build:plugin-marketplace",
    "vis": "pnpm -C apps/vis run dev",
    "dev:docs": "pnpm -C docs install --ignore-workspace && pnpm -C docs run dev",
    "typecheck": "pnpm run build:packages && pnpm -r --filter './packages/*' run typecheck && pnpm --filter ody-code run typecheck",
    "madge:circular": "madge --circular --extensions ts packages/*/src/index.ts",
    "lint": "oxlint --type-aware",
    "lint:fix": "pnpm run lint --fix",
    "lint:pkg": "pnpm -r --filter '!@odysseythink/monorepo' exec publint && pnpm -r --filter './packages/*' exec attw --pack . --profile node16",
    "sherif": "sherif",
    "test": "vitest run",
    "test:watch": "vitest",
    "test:coverage": "vitest run --coverage",
    "clean": "pnpm -r run clean",
    "changeset": "changeset",
    "version": "changeset version",
    "version:release": "changeset version",
    "publish": "pnpm run typecheck && pnpm run lint && pnpm run sherif && pnpm run test && pnpm run build && pnpm run lint:pkg && changeset publish",
    "prepare": "simple-git-hooks"
  }
```

- [ ] 在根 `package.json` 的 `devDependencies` 中新增 `madge`：

```json
  "devDependencies": {
    "@arethetypeswrong/cli": "0.18.2",
    "@changesets/changelog-github": "0.7.0",
    "@changesets/cli": "2.30.0",
    "@microsoft/api-extractor": "7.58.7",
    "@types/node": "^22.15.3",
    "@vitest/coverage-v8": "4.1.4",
    "lint-staged": "16.4.0",
    "madge": "^7.0.0",
    "oxlint": "1.59.0",
    "oxlint-tsgolint": "0.20.0",
    "pkg-pr-new": "0.0.75",
    "publint": "0.3.18",
    "sherif": "1.11.1",
    "simple-git-hooks": "2.13.1",
    "tsdown": "0.22.0",
    "tsx": "^4.21.0",
    "typescript": "6.0.2",
    "vitest": "4.1.4"
  }
```

- [ ] 在 `Makefile` 新增 `madge:circular` target：

```make
madge:circular:
	pnpm run madge:circular
```

（保持制表符缩进，与现有 target 一致。）

- [ ] 同步 lockfile 并运行 madge：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm install
pnpm run madge:circular
```

预期：输出类似 `No circular dependencies found`，exit 0。

- [ ] 提交：`git add flake.nix package.json Makefile pnpm-lock.yaml && git commit -m "chore(repo): add new packages to flake, add madge circular check"`

---

### Task D5: 全树验证与 Nix hash 更新

**Depends on:** Task D4
**Files：**
- Verify: 全 workspace typecheck/test/madge 通过
- Modify: `flake.nix:139`（`pnpmDeps.hash`）

- [ ] 运行全 packages 类型检查：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm -r --filter './packages/*' run typecheck
```

预期：所有包通过，exit 0。

- [ ] 运行全 workspace 测试：

```bash
pnpm test
```

预期：全部通过，exit 0。

- [ ] 运行循环依赖检测：

```bash
pnpm run madge:circular
```

预期：无循环依赖，exit 0。

- [ ] 运行 Nix 构建并更新 hash：

```bash
nix build .#ody-code
```

首次运行会因为 lockfile /workspace 变更而失败，并打印类似：

```
error: hash mismatch in fixed-output derivation:
         specified: sha256-HpRlxlXZoVqAzrdMdSWhLcTRM1DvDvytVbzIGBo8QUo=
         got:    sha256-XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX=
```

将 `flake.nix` 中 `pnpmDeps.hash` 替换为 `got:` 后面的新 hash，然后再次运行：

```bash
nix build .#ody-code
```

预期：第二次构建成功，exit 0。

- [ ] 提交：`git add -A && git commit -m "ci(repo): finalize workspace split and nix hash"`

---

## Local Self-Review (Part D)

- [ ] **1. Spec-coverage table**

| 需求 | 覆盖任务 | 状态 |
|---|---|---|
| 创建 `@odysseythink/integration-tests` 包 | D1 | covered |
| 暴露 agent-core 测试 helper 供集成测试包使用 | D2 | covered |
| 迁移 MCP 跨模块集成测试（connection-manager / tool-manager-mcp / built-in-integration） | D2 | covered |
| 迁移 E2E 跨模块集成测试（core / generator / integration / plan-enrichment） | D3 | covered |
| 更新 `flake.nix` workspacePaths / workspaceNames | D4 | covered |
| 引入 `madge` 循环依赖检测脚本与 Makefile target | D4 | covered |
| 全 workspace typecheck / test / madge 通过 | D5 | covered |
| Nix 构建通过且 hash 已更新 | D5 | covered |

- [ ] **2. Placeholder scan**：无 TODO/TBD；所有 package.json、flake.nix、Makefile 修改均给出完整代码块；所有 sed 替换与路径调整均给出具体命令。
- [ ] **3. No phantom tasks**：D1-D5 每一步均有可验证产物（骨架 typecheck、测试通过、madge 输出、Nix 构建成功）。
- [ ] **4. Dependency soundness**：D1 依赖 C3；D2 依赖 D1；D3 依赖 D2；D4 依赖 D3；D5 依赖 D4；无引用后续任务才创建的符号。
- [ ] **5. Caller & build soundness**：新增 `agent-core/test/helpers` 导出由 D2 一并创建；D4 同步更新 flake.nix 的 workspace 列表；D5 以全 workspace typecheck 与 Nix 构建验证所有 consumer（包括 test 文件）可解析。
- [ ] **6. Test-the-risk**：D2/D3 迁移的集成测试继续断言 MCP 连接生命周期、ToolManager 注册行为、E2E 计划富化、生成器模板占位符替换等跨模块行为；D5 的 `madge:circular` 与全测试运行是行为级门禁。
- [ ] **7. Type consistency**：测试 helper 导出名称与 agent-core 内部 helper 名称完全一致；集成测试从新包导入的类型/函数名与原 `#/mcp/*`、`#/e2e-testing/*` 名称一致；`flake.nix` 中 workspacePaths 与 workspaceNames 顺序一一对应。
