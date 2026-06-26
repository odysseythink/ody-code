# Phase D: 端到端测试与验证

本部分覆盖全树回归验证、Session 级别 MCP 集成测试，以及 native 构建的 smoke test。

---

## Dependency Overview

```
Task 9 (全树 typecheck + 现有测试回归)
  ↓
Task 10 (Session 集成测试 + smoke test)
```

Task 9 是所有代码修改完成后的必做回归；Task 10 在其之后运行，确保新增集成路径实际可工作。

---

### Task 9: 全树 typecheck + 现有测试回归

**Depends on:** `distribution.md: Task 8`（所有代码修改已完成）

**Files:**
- **Modify:** 无（纯验证任务，不修改源码）

**风险：** 新增的 `browser` 字段、`BuiltInMcpRegistry`、`ChromeTraceRecorder` 等引入的类型变更可能导致现有测试编译失败；`ToolManager.registerMcpServer` 的 hook 注入可能意外破坏现有 MCP 测试。

这是一个**非测试性**验证任务：无新测试代码，但需运行全树构建和现有测试套件。

- [ ] 运行全树 TypeScript 类型检查：

```bash
pnpm -r typecheck
```

预期：零错误、零警告（与基线一致）。若出现与本次变更相关的错误（如 `browser` 字段在 `KimiConfig` 类型中未找到），回退到对应任务修复。

- [ ] 运行 `packages/agent-core` 的测试套件（重点覆盖 MCP 和配置模块）：

```bash
cd packages/agent-core && pnpm vitest run
```

预期：全部通过。特别关注以下已有测试文件：
- `test/mcp/tool-manager-mcp.test.ts` — `ToolManager` MCP 工具注册逻辑
- `test/mcp/connection-manager.test.ts` — MCP 连接管理
- `test/config/schema.test.ts`（或对应配置文件测试）— 配置 schema 解析
- `test/harness/runtime.test.ts` — `KimiCore` Session 创建流程

若任何测试失败，使用 `vitest --reporter=verbose` 定位失败用例并修复。

- [ ] 运行 `apps/ody-code` 的测试：

```bash
cd apps/ody-code && pnpm vitest run
```

预期：全部通过。

- [ ] Commit（若测试或类型检查中发现并修复了问题，单独 commit；若全部通过，此任务不产生新 commit，仅作为质量关卡记录）：

```bash
# 仅当需要修复时才 commit
git add <fixed-files>
git commit -m "fix: address typecheck and test regressions from built-in mcp feature"
```

---

### Task 10: Session 集成测试 + smoke test

**Depends on:** `Task 9`（全树类型和测试基线已绿）

**Files:**
- **Create:** `packages/agent-core/test/mcp/built-in-integration.test.ts`
- **Modify:** `apps/ody-code/scripts/native/smoke.mjs`

**风险：** `createSession` 时 `mergeBuiltInMcpConfig` 若未正确处理 `built-in/` 目录缺失场景，会阻塞 Session 创建；native zip 未包含 `built-in/` 目录导致运行时找不到 server。

#### 子任务 10a: Session MCP 集成测试

- [ ] 写失败测试：验证 `KimiCore.createSession` 会将 `chrome-devtools` 注入 Session 的 MCP 配置。

```typescript
// packages/agent-core/test/mcp/built-in-integration.test.ts
import { mkdir, mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  createRPC,
  KimiCore,
  type CoreAPI,
  type SDKAPI,
  type ApprovalResponse,
} from '../../src';

describe('Built-in chrome-devtools MCP integration', () => {
  let tmp: string;

  afterEach(async () => {
    if (tmp !== undefined) {
      await rm(tmp, { recursive: true, force: true });
    }
  });

  it('injects chrome-devtools server config into new sessions', async () => {
    tmp = await mkdtemp(join(tmpdir(), 'kimi-core-built-in-'));
    const homeDir = join(tmp, 'home');
    const workDir = join(tmp, 'work');
    await mkdir(homeDir, { recursive: true });
    await mkdir(workDir, { recursive: true });

    const [coreRpc, sdkRpc] = createRPC<CoreAPI, SDKAPI>();
    const core = new KimiCore(coreRpc, { homeDir });
    const rpc = await sdkRpc({
      emitEvent: vi.fn(),
      requestApproval: vi.fn(
        async (): Promise<ApprovalResponse> => ({ decision: 'rejected' }),
      ),
      requestQuestion: vi.fn(async () => null),
      openExternal: vi.fn(async () => ({ opened: false })),
      toolCall: vi.fn(async () => ({ output: '' })),
    });

    const created = await rpc.createSession({
      id: 'ses_builtin_test',
      workDir,
    });
    const session = core.sessions.get(created.id);
    expect(session).toBeDefined();

    // Session ctor fire-and-forget's loadMcpServers; wait for it to finish.
    await session!.mcp.waitForInitialLoad();

    const entries = session!.mcp.list();
    const chromeDevTools = entries.find((e) => e.name === 'chrome-devtools');
    expect(chromeDevTools).toBeDefined();
    // In the test environment the vendored built-in directory usually does not
    // exist (tests run from packages/agent-core/test), so the server typically
    // lands in `failed`.  What matters is that the server was registered and
    // attempted to connect rather than being silently omitted.
    expect(['pending', 'connected', 'failed']).toContain(
      chromeDevTools!.status,
    );
  }, 30000);
});
```

**Must-survive 输入检查：** 该测试不依赖过滤/匹配规则，无需额外枚举。

- [ ] 运行测试并确认失败（`chrome-devtools` 尚未出现在 MCP 列表中）：

```bash
cd packages/agent-core && pnpm vitest run test/mcp/built-in-integration.test.ts
```

预期失败：`expect(chromeDevTools).toBeDefined()` 失败，因为 `entries` 中无 `chrome-devtools`。

- [ ] 确认 Part 1-3 的实现已完成（`BuiltInMcpRegistry`、`mergeBuiltInMcpConfig`、`createChromeDevToolsServerDefinition`、`package.json` + `native package.mjs` 均已修改）。重新运行测试：

```bash
cd packages/agent-core && pnpm vitest run test/mcp/built-in-integration.test.ts
```

预期通过。若 `createSession` 抛出 `BUILT_IN_ROOT_NOT_FOUND`，说明 `createChromeDevToolsServerDefinition` 中的 `config.cwd` 未使用 getter 延迟解析，需回退到 `core-integration.md: Task 4` 修复。

- [ ] Commit：

```bash
git add packages/agent-core/test/mcp/built-in-integration.test.ts
git commit -m "test: verify chrome-devtools built-in server is injected into sessions"
```

#### 子任务 10b: native smoke test 扩展

- [ ] 修改 `apps/ody-code/scripts/native/smoke.mjs`，在现有验证之后追加 `built-in/` 目录存在性检查。

在文件第 80-81 行（`console.log(
Native smoke passed...`) 之前插入：

```typescript
import { stat } from 'node:fs/promises'; // 若顶部已有则省略
import { join } from 'node:path';         // 若顶部已有则省略

// ... existing code ...

const builtInPackageJson = join(appRoot, 'built-in', 'chrome-devtools', 'package.json');
try {
  await stat(builtInPackageJson);
  console.log(`Built-in chrome-devtools found: ${builtInPackageJson}`);
} catch {
  // In CI the native build step may run before vendoring; warn but do not fail.
  console.log(`Warning: built-in chrome-devtools not found at ${builtInPackageJson}`);
}
```

- [ ] **手动验证**：运行 native smoke test：

```bash
cd apps/ody-code && pnpm run build:native:sea && pnpm run test:native:smoke
```

预期输出包含 `Native smoke passed: <path>` 以及 `Built-in chrome-devtools found: <path>`（若 Task 7 已执行）或 `Warning: ...`（若未执行）。

- [ ] Commit：

```bash
git add apps/ody-code/scripts/native/smoke.mjs
git commit -m "test: verify built-in chrome-devtools in native smoke"
```

---

## Local Self-Review

- [ ] 1. **Spec-coverage table**

| 设计章节 | 覆盖状态 | 对应 Task(s) |
|---|---|---|
| Test: Session MCP 连接 | covered | Task 10a |
| Test: Tool 调用端到端 | no-op | 端到端 tool 调用需要真实 Chrome 实例，超出自动化单元测试范围；已在 Task 6 中测试 `ChromeTraceRecorder` 对 tool 结果的录制 |
| Test: Native 二进制路径 | covered | Task 10b |
| Done Criteria: 所有测试通过 | covered | Task 9 |
| Done Criteria: TypeScript 类型检查 | covered | Task 9 |
| Done Criteria: 原生构建成功 | covered | Task 10b |

- [ ] 2. **Placeholder scan**：无 TODO/TBD；所有命令和预期输出具体；测试代码完整。
- [ ] 3. **No phantom tasks**：
  - Task 9 产出全树类型检查和测试回归验证结果。
  - Task 10a 产出 `built-in-integration.test.ts` + commit。
  - Task 10b 产出 `smoke.mjs` 修改 + commit。
- [ ] 4. **Dependency soundness**：Task 9 依赖 `distribution.md: Task 8`（所有代码修改完成）；Task 10 依赖 Task 9（基线已绿）；无向后引用。
- [ ] 5. **Caller & build soundness**：
  - Task 10a 的新测试调用 `createRPC`、`KimiCore`、`createSession` 等现有公共 API，未改变任何共享签名。
  - `smoke.mjs` 的修改是局部追加逻辑，无外部调用者变更。
  - Task 10 完成后需运行 `pnpm -r typecheck` 确保新测试文件编译通过。
- [ ] 6. **Test-the-risk**：
  - Session 创建不阻塞：`built-in-integration.test.ts` 断言 `chrome-devtools` 出现在 MCP 列表中（即使状态为 `failed`），验证 `config.cwd` 的延迟解析不会导致 `createSession` 抛出。
  - native 分发遗漏：`smoke.mjs` 检查 `built-in/chrome-devtools/package.json` 存在性，若 Task 7（vendoring）未完成会打印警告，提醒开发者补全。
- [ ] 7. **Type一致性**：测试使用的 `ApprovalResponse`、`CoreAPI`、`SDKAPI` 均来自 `../../src`，与项目中现有类型一致；`chrome-devtools` server name 与 Part 2 中 `BuiltInMcpRegistry` 注册名一致。
