# Part B2: 创建 `@odysseythink/e2e-testing`

本 Part 将 `packages/agent-core/src/e2e-testing` 整体迁移为独立包 `@odysseythink/e2e-testing`。
新包只依赖 `@odysseythink/agent-core-shared` 与 `@odysseythink/kaos`，不反向依赖 `agent-core`。
迁移后 `agent-core` 中的 `RunE2ETestsTool`、`ExitPlanModeTool`、`NormalModeTaskCheckpoint` 改为从新包导入，
`agent-core/src/e2e-testing` 目录删除。

**依赖上游:**
- `shared.md` Task A2 (`errors` 已迁移到 shared)
- `shared.md` Task A10 (`config/schema` 已迁移到 shared)

**Phase B2 任务图:**

```
B2-1 包骨架
   ↓
B2-2 迁移源码 + 重建导出
   ↓
B2-3 迁移单元测试到新包
   ↓
B2-4 更新 agent-core 调用方并清理旧目录
```

**风险与开放问题:**
- `E2EPlanEnricher` 等源码使用 `OdyError` / `E2EConfigSchema`，必须在 shared 迁移完成后执行。
- `generator.ts` 为 `ExitPlanModeTool` 生成的测试模板包含指向 `agent-core` 源文件的相对路径字符串；这些字符串属于 ody-code 自测行为，保留在新包中，不引入实际 import。
- `integration.test.ts` / `plan-enrichment.e2e.test.ts` / `generator.test.ts` / `core.test.ts` 依赖 `agent-core` 内部类型与工具，留在 `agent-core` 中作为集成测试，仅更新 import。

### Task B2-1: 创建 e2e-testing 包骨架

**Depends on:** `shared.md` Task A2, Task A10  
**Files:**
- Create: `packages/e2e-testing/package.json`
- Create: `packages/e2e-testing/tsconfig.json`
- Create: `packages/e2e-testing/vitest.config.ts`
- Create: `packages/e2e-testing/src/index.ts`

- [ ] 编写 `packages/e2e-testing/package.json`：

```json
{
  "name": "@odysseythink/e2e-testing",
  "version": "0.1.0",
  "private": true,
  "description": "E2E test generation and execution framework for Ody Code",
  "license": "MIT",
  "type": "module",
  "imports": {
    "#/*": [
      "./src/*.ts",
      "./src/*/index.ts"
    ]
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
    "pathe": "^2.0.3",
    "picomatch": "^4.0.4",
    "zod": "catalog:"
  },
  "devDependencies": {
    "@types/picomatch": "^4.0.3"
  }
}
```

- [ ] 编写 `packages/e2e-testing/tsconfig.json`：

```json
{
  "extends": "../../tsconfig.json",
  "compilerOptions": {},
  "include": ["src", "test"]
}
```

- [ ] 编写 `packages/e2e-testing/vitest.config.ts`：

```ts
import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    name: 'e2e-testing',
    include: ['test/**/*.test.ts'],
  },
});
```

- [ ] 编写占位 `packages/e2e-testing/src/index.ts`：

```ts
export {};
```

- [ ] 安装依赖并验证骨架通过：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm install
cd packages/e2e-testing
pnpm typecheck
```

预期：无错误，`error code 0`。

- [ ] 提交：`git add packages/e2e-testing && git commit -m "chore(e2e-testing): bootstrap package"`

### Task B2-2: 迁移 e2e-testing 源码并重建导出

**Depends on:** Task B2-1  
**Files:**
- Create: `packages/e2e-testing/src/types.ts`
- Create: `packages/e2e-testing/src/config.ts`
- Create: `packages/e2e-testing/src/errors.ts`
- Create: `packages/e2e-testing/src/executor.ts`
- Create: `packages/e2e-testing/src/result-cache.ts`
- Create: `packages/e2e-testing/src/git-status.ts`
- Create: `packages/e2e-testing/src/impact-analyzer.ts`
- Create: `packages/e2e-testing/src/impact-map.ts`
- Create: `packages/e2e-testing/src/recursive-impact-analyzer.ts`
- Create: `packages/e2e-testing/src/plan-enricher.ts`
- Create: `packages/e2e-testing/src/generator.ts`
- Create: `packages/e2e-testing/src/registry.ts`
- Create: `packages/e2e-testing/src/generators/nodejs-jest.ts`
- Create: `packages/e2e-testing/src/generators/python-pytest.ts`
- Create: `packages/e2e-testing/src/generators/go.ts`
- Modify: `packages/e2e-testing/src/index.ts`

本任务所有源码逻辑保持与 `packages/agent-core/src/e2e-testing` 一致，仅修改 import 来源。

- [ ] 复制源码文件：

```bash
cd /Users/ranwei/workspace/ody-code
SRC=packages/agent-core/src/e2e-testing
DST=packages/e2e-testing/src

for f in types.ts config.ts errors.ts executor.ts result-cache.ts git-status.ts impact-analyzer.ts impact-map.ts recursive-impact-analyzer.ts plan-enricher.ts generator.ts registry.ts; do
  cp "$SRC/$f" "$DST/$f"
done

mkdir -p "$DST/generators"
for f in nodejs-jest.ts python-pytest.ts go.ts; do
  cp "$SRC/generators/$f" "$DST/generators/$f"
done
```

- [ ] 重写跨包 import。在新包内执行：

```bash
cd /Users/ranwei/workspace/ody-code/packages/e2e-testing/src

# 来源：shared 中的 errors / config schema
rg -l "from '#\\/errors'" . | xargs -I{} sed -i "s|from '#/errors'|from '@odysseythink/agent-core-shared'|g" {}
rg -l "from '#\\/config\\/schema'" . | xargs -I{} sed -i "s|from '#/config/schema'|from '@odysseythink/agent-core-shared'|g" {}
```

验证无遗留 `#/` import：

```bash
rg "from '#\\/" /Users/ranwei/workspace/ody-code/packages/e2e-testing/src
```

预期：无匹配。

- [ ] 编写 `packages/e2e-testing/src/index.ts`：

```ts
export type {
  E2EPriority,
  ProjectStructure,
  Feature,
  TestFile,
  AffectedTool,
  ImpactAnalysisResult,
  RunContext,
  E2ETestGenerator,
  TestCaseResult,
  TestSuiteResult,
  E2EExecutionResult,
} from './types';

export type { ResolvedE2EConfig } from './config';
export { E2EConfigResolver } from './config';

export {
  E2EConfigValidationError,
  E2ENoMatchingGeneratorError,
} from './errors';

export { E2ETestExecutor } from './executor';
export { computeCacheKey, E2ETestResultCache } from './result-cache';
export { detectChangedFiles } from './git-status';
export { ImpactAnalyzer } from './impact-analyzer';
export { RecursiveImpactAnalyzer } from './recursive-impact-analyzer';
export { E2EPlanEnricher } from './plan-enricher';
export { TypeScriptVitestGenerator } from './generator';
export { E2EGeneratorRegistry, registry } from './registry';
export { NodejsJestGenerator, parseJestJson } from './generators/nodejs-jest';
export { PythonPytestGenerator, parsePytestJsonReport } from './generators/python-pytest';
export { GoGenerator, parseGoTestJson } from './generators/go';
```

- [ ] 运行新包 typecheck：

```bash
cd /Users/ranwei/workspace/ody-code/packages/e2e-testing
pnpm typecheck
```

预期：无错误，`error code 0`。

- [ ] 提交：`git add packages/e2e-testing/src && git commit -m "feat(e2e-testing): migrate source from agent-core"`

### Task B2-3: 迁移 e2e-testing 单元测试到新包

**Depends on:** Task B2-2  
**Files:**
- Create: `packages/e2e-testing/test/fixtures/fake-kaos.ts`
- Create: `packages/e2e-testing/test/resolver.test.ts`
- Create: `packages/e2e-testing/test/impact-analyzer.test.ts`
- Create: `packages/e2e-testing/test/executor.test.ts`
- Create: `packages/e2e-testing/test/result-cache.test.ts`
- Create: `packages/e2e-testing/test/recursive-impact-analyzer.test.ts`
- Create: `packages/e2e-testing/test/generators/go.test.ts`
- Create: `packages/e2e-testing/test/generators/nodejs-jest.test.ts`
- Create: `packages/e2e-testing/test/generators/python-pytest.test.ts`
- Create: `packages/e2e-testing/test/git-status.test.ts`
- Delete: `packages/agent-core/test/e2e-testing/config.test.ts`
- Delete: `packages/agent-core/test/e2e-testing/executor.test.ts`
- Delete: `packages/agent-core/test/e2e-testing/result-cache.test.ts`
- Delete: `packages/agent-core/test/e2e-testing/recursive-impact-analyzer.test.ts`
- Delete: `packages/agent-core/test/e2e-testing/go-generator.test.ts`
- Delete: `packages/agent-core/test/e2e-testing/nodejs-jest-generator.test.ts`
- Delete: `packages/agent-core/test/e2e-testing/python-pytest-generator.test.ts`
- Delete: `packages/agent-core/test/e2e-testing/changed-files.test.ts`

- [ ] 创建新的 fake Kaos 测试夹具 `packages/e2e-testing/test/fixtures/fake-kaos.ts`：

```ts
import type { Environment, Kaos, KaosProcess } from '@odysseythink/kaos';

function notImplemented(method: string): never {
  throw new Error(`FakeKaos.${method} not implemented — override in test`);
}

export const FAKE_OS_ENV: Environment = {
  osKind: 'Linux',
  osArch: 'x86_64',
  osVersion: 'test',
  shellName: 'bash',
  shellPath: '/bin/bash',
};

export function createFakeKaos(overrides?: Partial<Kaos>): Kaos {
  let cwd = overrides?.getcwd?.() ?? '/workspace';
  const base: Kaos = {
    name: 'fake',
    osEnv: FAKE_OS_ENV,
    pathClass: () => 'posix',
    normpath: (p: string) => p,
    gethome: () => '/home/test',
    getcwd: () => cwd,
    withCwd: (next: string) => createFakeKaos({ ...overrides, getcwd: () => next }),
    chdir: async (next: string) => {
      cwd = next;
    },
    stat: () => notImplemented('stat'),
    iterdir: () => notImplemented('iterdir'),
    glob: () => notImplemented('glob'),
    readBytes: () => notImplemented('readBytes'),
    readText: () => notImplemented('readText'),
    readLines: () => notImplemented('readLines'),
    writeBytes: () => notImplemented('writeBytes'),
    writeText: () => notImplemented('writeText'),
    mkdir: () => notImplemented('mkdir'),
    exec: () => notImplemented('exec'),
    execWithEnv: () => notImplemented('execWithEnv'),
  };
  return { ...base, ...overrides } as Kaos;
}
```

- [ ] 迁移已有测试（复制后批量改写 import）：

```bash
cd /Users/ranwei/workspace/ody-code
SRC=packages/agent-core/test/e2e-testing
DST=packages/e2e-testing/test

mkdir -p "$DST/generators"

cp "$SRC/executor.test.ts" "$DST/executor.test.ts"
cp "$SRC/result-cache.test.ts" "$DST/result-cache.test.ts"
cp "$SRC/recursive-impact-analyzer.test.ts" "$DST/recursive-impact-analyzer.test.ts"
cp "$SRC/go-generator.test.ts" "$DST/generators/go.test.ts"
cp "$SRC/nodejs-jest-generator.test.ts" "$DST/generators/nodejs-jest.test.ts"
cp "$SRC/python-pytest-generator.test.ts" "$DST/generators/python-pytest.test.ts"
cp "$SRC/changed-files.test.ts" "$DST/git-status.test.ts"

# 统一替换 import
for f in "$DST"/*.test.ts "$DST/generators"/*.test.ts; do
  sed -i "s|from '#/e2e-testing/\([^']*\)'|from './src/\1'|g" "$f"
  sed -i "s|from '../tools/fixtures/fake-kaos'|from './fixtures/fake-kaos'|g" "$f"
  sed -i "s|from '#/config/schema'|from '@odysseythink/agent-core-shared'|g" "$f"
  sed -i "s|from '../recursive-impact-analyzer'|from './src/recursive-impact-analyzer'|g" "$f"
  sed -i "s|from '../types'|from './src/types'|g" "$f"
  sed -i "s|from '../config'|from './src/config'|g" "$f"
done
```

- [ ] 编写 `packages/e2e-testing/test/resolver.test.ts`：

```ts
import { describe, expect, it } from 'vitest';
import { E2EConfigSchema } from '@odysseythink/agent-core-shared';
import { E2EConfigResolver } from './src/config';
import type { OdyConfig } from '@odysseythink/agent-core-shared';

describe('E2EConfigResolver', () => {
  it('returns defaults for empty config', () => {
    const result = E2EConfigResolver.resolve({} as OdyConfig);
    expect(result.enabled).toBe(true);
    expect(result.strategy).toBe('smart');
    expect(result.criticalTools).toEqual(['ExitPlanModeTool']);
    expect(result.recursiveAnalysisEnabled).toBe(true);
    expect(result.cacheEnabled).toBe(true);
    expect(result.cacheDir).toBe('.ody-code/e2e-cache');
  });

  it('overrides enabled from raw', () => {
    const result = E2EConfigResolver.resolve({ e2e: { enabled: false } as any } as OdyConfig);
    expect(result.enabled).toBe(false);
  });

  it('throws for maxConcurrency 0', () => {
    expect(() => E2EConfigResolver.resolve({ e2e: { maxConcurrency: 0 } as any } as OdyConfig))
      .toThrow();
  });

  it('parses e2e section object directly', () => {
    const parsed = E2EConfigSchema.parse({
      enabled: true,
      strategy: 'critical-only',
      criticalTools: ['api'],
    });
    expect(parsed.strategy).toBe('critical-only');
    expect(parsed.criticalTools).toEqual(['api']);
  });
});
```

- [ ] 编写 `packages/e2e-testing/test/impact-analyzer.test.ts`：

```ts
import { describe, expect, it } from 'vitest';
import { ImpactAnalyzer } from './src/impact-analyzer';
import type { ResolvedE2EConfig } from './src/config';

const defaultConfig: ResolvedE2EConfig = {
  enabled: true, strategy: 'smart', criticalTools: ['ExitPlanModeTool'],
  failurePolicy: 'warn', maxConcurrency: 4, testTimeout: 30000,
  reportDir: '.ody-code/test-reports', generatedTestDir: '.ody-code/test-generated/e2e',
  recursiveAnalysisEnabled: true, maxRecursiveDepth: 3,
  cacheEnabled: true, cacheDir: '.ody-code/e2e-cache', cacheTtlDays: 7, cacheMaxEntries: 20,
};

describe('ImpactAnalyzer', () => {
  it('matches exit-plan-mode.ts to ExitPlanModeTool as critical', () => {
    const result = ImpactAnalyzer.analyze(
      ['packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts'],
      defaultConfig,
    );
    expect(result.affectedTools).toEqual([
      { toolId: 'ExitPlanModeTool', priority: 'critical' },
    ]);
  });

  it('returns empty for unrelated file with smart strategy', () => {
    const result = ImpactAnalyzer.analyze(['unrelated.ts'], defaultConfig);
    expect(result.affectedTools).toHaveLength(0);
  });

  it('returns general for always strategy with no matches', () => {
    const config = { ...defaultConfig, strategy: 'always' as const };
    const result = ImpactAnalyzer.analyze(['unrelated.ts'], config);
    expect(result.affectedTools).toEqual([
      { toolId: 'general', priority: 'nice-to-have' },
    ]);
  });

  it('filters non-critical with critical-only strategy', () => {
    const config = { ...defaultConfig, strategy: 'critical-only' as const, criticalTools: ['ExitPlanModeTool'] };
    const result = ImpactAnalyzer.analyze(
      ['packages/agent-core/src/tools/builtin/planning/enter-plan-mode.ts'],
      config,
    );
    expect(result.affectedTools).toHaveLength(0);
  });

  it('normalizes backslash paths', () => {
    const result = ImpactAnalyzer.analyze(
      ['packages\\agent-core\\src\\tools\\builtin\\planning\\exit-plan-mode.ts'],
      defaultConfig,
    );
    expect(result.affectedTools).toHaveLength(1);
  });
});
```

- [ ] 运行新包测试：

```bash
cd /Users/ranwei/workspace/ody-code/packages/e2e-testing
pnpm test
```

预期：全部通过（原有测试逻辑未变，仅 import 调整）。

- [ ] 提交：`git add packages/e2e-testing/test && git commit -m "test(e2e-testing): migrate unit tests from agent-core"`

### Task B2-4: 更新 agent-core 调用方并清理旧目录

**Depends on:** Task B2-3  
**Files:**
- Modify: `packages/agent-core/src/tools/builtin/e2e/run-e2e-tests.ts` (import 来源)
- Modify: `packages/agent-core/src/tools/builtin/planning/exit-plan-mode.ts` (import 来源)
- Modify: `packages/agent-core/src/agent/compaction/normal-task-checkpoint.ts` (import 来源)
- Modify: `packages/agent-core/test/e2e-testing/core.test.ts` (import 来源)
- Modify: `packages/agent-core/test/e2e-testing/generator.test.ts` (import 来源)
- Modify: `packages/agent-core/test/e2e-testing/integration.test.ts` (import 来源)
- Modify: `packages/agent-core/test/e2e-testing/plan-enrichment.e2e.test.ts` (import 来源)
- Delete: `packages/agent-core/src/e2e-testing/`
- Delete: `packages/agent-core/test/e2e-testing/config.test.ts`
- Delete: `packages/agent-core/test/e2e-testing/executor.test.ts`
- Delete: `packages/agent-core/test/e2e-testing/result-cache.test.ts`
- Delete: `packages/agent-core/test/e2e-testing/recursive-impact-analyzer.test.ts`
- Delete: `packages/agent-core/test/e2e-testing/go-generator.test.ts`
- Delete: `packages/agent-core/test/e2e-testing/nodejs-jest-generator.test.ts`
- Delete: `packages/agent-core/test/e2e-testing/python-pytest-generator.test.ts`
- Delete: `packages/agent-core/test/e2e-testing/changed-files.test.ts`

本任务变更 import 来源，不改动业务逻辑。由于 `e2e-testing` 源码已迁出，`agent-core` 中任何 `#/e2e-testing/*` 引用都必须切到新包。

- [ ] 先搜索 `agent-core` 中所有 `#/e2e-testing` 引用，确认范围：

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core
rg "from '#\\/e2e-testing" src test -n
```

预期命中文件：`src/tools/builtin/e2e/run-e2e-tests.ts`、`src/tools/builtin/planning/exit-plan-mode.ts`、`src/agent/compaction/normal-task-checkpoint.ts`、`test/e2e-testing/core.test.ts`、`test/e2e-testing/generator.test.ts`、`test/e2e-testing/integration.test.ts`、`test/e2e-testing/plan-enrichment.e2e.test.ts`。

- [ ] 修改 `src/tools/builtin/e2e/run-e2e-tests.ts`：

```ts
// 替换前
import { E2EConfigResolver } from '#/e2e-testing/config';
import { E2ETestExecutor } from '#/e2e-testing/executor';
import { registry } from '#/e2e-testing/registry';
import { detectChangedFiles } from '#/e2e-testing/git-status';

// 替换后
import {
  E2EConfigResolver,
  E2ETestExecutor,
  registry,
  detectChangedFiles,
} from '@odysseythink/e2e-testing';
```

- [ ] 修改 `src/tools/builtin/planning/exit-plan-mode.ts`：

```ts
// 替换前
import { E2EPlanEnricher } from '#/e2e-testing/plan-enricher';
import { E2EConfigResolver } from '#/e2e-testing/config';
import { registry } from '#/e2e-testing/registry';

// 替换后
import {
  E2EPlanEnricher,
  E2EConfigResolver,
  registry,
} from '@odysseythink/e2e-testing';
```

- [ ] 修改 `src/agent/compaction/normal-task-checkpoint.ts`：

```ts
// 替换前
import { detectChangedFiles } from '../../e2e-testing/git-status';

// 替换后
import { detectChangedFiles } from '@odysseythink/e2e-testing';
```

- [ ] 修改留在 agent-core 的集成测试 import：

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core/test/e2e-testing

for f in core.test.ts generator.test.ts integration.test.ts plan-enrichment.e2e.test.ts; do
  sed -i "s|from '#/e2e-testing/\([^']*\)'|from '@odysseythink/e2e-testing'|g" "$f"
  sed -i "s|from '#/config/schema'|from '@odysseythink/agent-core-shared'|g" "$f"
done
```

验证无遗留：

```bash
cd /Users/ranwei/workspace/ody-code/packages/agent-core
rg "from '#\\/e2e-testing" src test -n
```

预期：无匹配。

- [ ] 删除旧源码与已迁移测试：

```bash
cd /Users/ranwei/workspace/ody-code
rm -rf packages/agent-core/src/e2e-testing
rm -f packages/agent-core/test/e2e-testing/config.test.ts
rm -f packages/agent-core/test/e2e-testing/executor.test.ts
rm -f packages/agent-core/test/e2e-testing/result-cache.test.ts
rm -f packages/agent-core/test/e2e-testing/recursive-impact-analyzer.test.ts
rm -f packages/agent-core/test/e2e-testing/go-generator.test.ts
rm -f packages/agent-core/test/e2e-testing/nodejs-jest-generator.test.ts
rm -f packages/agent-core/test/e2e-testing/python-pytest-generator.test.ts
rm -f packages/agent-core/test/e2e-testing/changed-files.test.ts
```

- [ ] 运行全仓库 typecheck（含 test 文件）：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm -r --filter './packages/*' run typecheck
```

预期：无类型错误，`error code 0`。

- [ ] 运行受影响包测试：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm --filter @odysseythink/e2e-testing test
pnpm --filter @odysseythink/agent-core test
```

预期：均通过。

- [ ] 提交：`git add -A && git commit -m "refactor(agent-core): consume e2e-testing from extracted package"`

## Self-Review (Part B2)

- [ ] 1. Spec-coverage table

| 需求 | 覆盖任务 | 状态 |
|---|---|---|
| 创建独立 `@odysseythink/e2e-testing` 包骨架 | B2-1 | covered |
| 迁移 `e2e-testing` 全部源码并消除对 `agent-core` 的 import | B2-2 | covered |
| 迁移核心单元测试（executor/cache/analyzer/generators/git-status/resolver） | B2-3 | covered |
| 更新 `agent-core` 中所有 `#/e2e-testing` 调用方到新包 | B2-4 | covered |
| 删除 `packages/agent-core/src/e2e-testing` 及已迁移测试 | B2-4 | covered |
| 全仓库 typecheck 与测试通过 | B2-4 | covered |

- [ ] 2. Placeholder scan：无 TODO/TBD，无 "后续再实现"；所有源码与测试路径均已明确。
- [ ] 3. No phantom tasks：每个任务都有可验证产物（骨架 typecheck、源码 typecheck、新包测试、全仓库 typecheck + 测试）。
- [ ] 4. Dependency soundness：B2-1 → B2-2 → B2-3 → B2-4；B2 整体依赖 `shared.md` A2/A10，已在任务头声明。
- [ ] 5. Caller & build soundness：B2-4 中搜索并更新 `agent-core` 内所有 `#/e2e-testing` 调用方（源码 + 测试），并以 `pnpm -r --filter './packages/*' run typecheck` 全仓库类型检查收尾。
- [ ] 6. Test-the-risk：迁移的测试覆盖缓存读写/过期/上限、执行器 chunking、递归影响分析 BFS、生成器模板占位符替换、git changed-files 合并、配置解析默认值与异常；测试断言均基于实现常量。
- [ ] 7. Type consistency：新包 `ResolvedE2EConfig`、`E2ETestGenerator`、`TestSuiteResult` 等类型直接复用原 `e2e-testing` 定义，未做签名变更；`agent-core` 调用方使用相同导出名称。

---
