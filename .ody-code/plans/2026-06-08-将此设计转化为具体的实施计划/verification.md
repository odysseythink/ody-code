# Phase D: 测试与全树验证

Phase D 是全局验证阶段，所有代码变更完成后必须执行。本阶段仅一个任务，但它是整个计划的守门任务（gatekeeper）。

---

## Phase D 依赖图

```
Task 17: Whole-tree typecheck + regression test run
  └─ depends on: Task 1..16 (all implementation complete)
```

---

### Task 17: 全树类型检查 + 回归测试运行

**Depends on:** Task 1, Task 2, Task 3, Task 4, Task 5, Task 6, Task 7, Task 8, Task 9, Task 10, Task 11, Task 12, Task 13, Task 14, Task 15, Task 16

**Files:**
- Modify: `packages/agent-core/package.json` (仅确认 `puppeteer-core` 在 `devDependencies` 或 `dependencies` 中)
- Modify: 所有 Task 1-16 已修改/新建的文件（本次任务不新增代码，只做验证）
- Test: `pnpm -r typecheck`, `pnpm -r test`

**Goal:** 确认整个 monorepo 在浏览器原生工具集成后仍能干净编译、现有测试无回归。

#### Step 1: 确认 workspace 依赖完整性
- [ ] 运行 `pnpm install`，确认 `puppeteer-core` 被正确安装到 `packages/agent-core/node_modules/.pnpm`：
  ```bash
  ls packages/agent-core/node_modules/puppeteer-core/package.json
  ```
  **期望：** 文件存在，版本匹配 `package.json` 中指定的范围。

#### Step 2: 全树类型检查
- [ ] 从仓库根目录运行：
  ```bash
  pnpm -r typecheck
  ```
  **期望：** 所有 workspace package（`agent-core`, `node-sdk`, `kaos`, `kosong`, `oauth`, `telemetry`, `apps/ody-code`, `apps/vis/*`, `docs`）均通过 TypeScript 编译，零 `TS` 错误。特别关注以下新增/修改文件无类型错误：
  - `packages/agent-core/src/browser/connection.ts`
  - `packages/agent-core/src/browser/types.ts`
  - `packages/agent-core/src/tools/builtin/browser/*.ts`
  - `packages/agent-core/src/agent/permission/policies/browser-host.ts`
  - `packages/agent-core/src/agent/permission/policies/browser-tool-ask.ts`
  - `packages/agent-core/src/mcp/built-in/registry.ts`
  - `packages/agent-core/src/agent/index.ts`
  - `packages/agent-core/src/agent/tool/index.ts`
  - `packages/agent-core/src/session/index.ts`

- [ ] 若出现 puppeteer-core 类型缺失错误（如 `Cannot find module 'puppeteer-core'`）：
  - 检查 `packages/agent-core/package.json` 的 `dependencies`（非 `devDependencies`，因为 `node-sdk` / `apps/ody-code` 可能间接消费这些类型）。
  - 确认 `packages/agent-core/tsconfig.json` 的 `compilerOptions.types` 或 `include` 未排除 `node_modules` 中的类型定义。

#### Step 3: `packages/agent-core` 专用测试运行
- [ ] 运行 agent-core 包内所有测试：
  ```bash
  cd packages/agent-core && pnpm test
  ```
  **期望：** 以下测试全部通过：
  - `test/browser/connection.test.ts`（Task 5）
  - `test/browser/tools.test.ts`（Task 10）
  - `test/agent/permission/browser-host.test.ts`（Task 16）
  - `test/agent/permission/browser-tool-ask.test.ts`（Task 13 更新）
  - `test/config/browser-config.test.ts`（Task 2 追加）

#### Step 4: 全 workspace 回归测试
- [ ] 从仓库根目录运行：
  ```bash
  pnpm -r test
  ```
  **期望：** 无新增失败测试。若出现与浏览器相关的失败（如 puppeteer 在无头 CI 环境中尝试 `launch()`）：
  - 确认 `BrowserConnectionManager` 在测试中使用的是 mock（Task 5 已提供），不会真正调用 puppeteer。
  - 确认 CI 环境设置了 `ODY_CODE_EXPERIMENTAL_BROWSER=1` 或测试用 `vi.stubEnv` 正确隔离了环境变量。

#### Step 5: lint 检查
- [ ] 运行：
  ```bash
  pnpm -r lint
  ```
  **期望：** 零 lint 错误，零格式化差异。浏览器工具相关的文件（`packages/agent-core/src/browser/**/*.ts`, `packages/agent-core/src/tools/builtin/browser/**/*.ts`）需通过 `oxlint` / `eslint` 规则。

#### Step 6: 运行时冒烟测试（手动验证）
- [ ] 在本地终端启动 TUI，执行一次需要浏览器工具的对话：
  ```bash
  cd apps/ody-code && pnpm dev
  ```
  然后向 agent 发送请求如："打开 https://example.com 并告诉我页面标题"。
  **期望：**
  - TUI 中出现权限弹窗 `Allow BrowserBrowse(example.com)?`，首次访问必须 ask（`BrowserHostPermissionPolicy` 未命中 session cache）。
  - 授权后页面内容被正确提取并返回。
  - 第二次访问同一 host 时不再弹窗，直接放行（session-level cache）。
  - 运行结束后执行 `/quit`，确认进程正常退出，无残留 Chromium 进程：
    ```bash
    ps aux | grep chromium | grep -v grep
    ```
    **期望：** 无残留。

#### Step 7: 提交
- [ ] Commit：
  ```bash
  git add -A
  git commit -m "test(browser): whole-tree typecheck and regression verification

  - Verify pnpm -r typecheck passes across all workspace packages
  - Verify pnpm -r test passes with zero new failures
  - Manual smoke test: BrowserBrowse permission flow, session cache,
    and process cleanup on Session.close()"
  ```

---

## Local Self-Review

- [ ] 1. **Spec-coverage table within this part:** Task 17 覆盖设计中的"Test: Whole-tree typecheck"需求。
- [ ] 2. **Placeholder scan:** 无 TODO/TBD。Task 17 的所有步骤均为可执行的验证命令。
- [ ] 3. **No phantom tasks:** Task 17 的产出是一个通过验证的 commit + 运行日志。无 `--allow-empty`。
- [ ] 4. **Dependency soundness:** Task 17 的 `Depends on:` 列出全部 Task 1-16，均在前面 phases 中完成。
- [ ] 5. **Caller & build soundness:** Task 17 的核心动作是 `pnpm -r typecheck`，它天然覆盖全树 caller，不修改任何共享签名。若类型检查失败，前面的 tasks 已经修复签名问题。
- [ ] 6. **Test-the-risk:** Task 17 本身是验证层而非实现层，其"测试"行为体现在 Step 3-4 的运行命令。无状态变更需额外断言。
- [ ] 7. **Type consistency:** Task 17 不引入新类型，只验证前面 tasks 引入的类型在整个 monorepo 中一致。
