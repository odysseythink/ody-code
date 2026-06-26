# Phase 1-C: agent-core 拆包启动 — Implementation Plan

**Goal:** 将 `packages/agent-core` 中的 `code-review`、`e2e-testing`、`mcp` 三个子系统拆分为独立 peer 包，同时新建最小共享包 `@odysseythink/agent-core-shared` 承载基础类型与工具，并引入 `madge` 守护包间无环依赖。

**Architecture:** 新增 `@odysseythink/agent-core-shared` 作为底层原语（错误、日志接口、abort、version、配置 schema、工具执行类型、MCP 事件常量、flags、wasm-loader/string）的唯一来源；`code-review`、`e2e-testing`、`mcp-host` 三只新包仅依赖 shared；`agent-core` 改为从三只新包与 shared 导入，不再对内部目录做 re-export。跨包集成测试集中到 `@odysseythink/integration-tests`。依赖图严格单向无环。

**Tech Stack:** TypeScript 6.x / pnpm workspace / `tsdown` / `vitest` / `madge` / Nix (`flake.nix`).

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

```
packages/agent-core-shared/
  package.json
  tsconfig.json
  vitest.config.ts
  src/
    index.ts
    errors/
    logging.ts
    abort.ts
    version.ts
    tool-execution.ts
    input-schema.ts
    mcp-events.ts
    permission-pattern.ts
    hook-events.ts
    config.ts
    flags/
    wasm-loader.ts
    wasm-string.ts

packages/code-review/
  package.json
  tsconfig.json
  vitest.config.ts
  src/
    index.ts
    diff.ts
    executor.ts
    model-resolver.ts
    prompt.ts
    report.ts
    simplicity.ts
    types.ts
    wasm-diff.ts
    wasm-loader.ts
    wasm-string.ts
  test/

packages/e2e-testing/
  package.json
  tsconfig.json
  vitest.config.ts
  src/
    index.ts
    config.ts
    errors.ts
    executor.ts
    generator.ts
    generators/*.ts
    git-status.ts
    impact-analyzer.ts
    impact-map.ts
    plan-enricher.ts
    recursive-impact-analyzer.ts
    registry.ts
    result-cache.ts
    types.ts
  test/

packages/mcp-host/
  package.json
  tsconfig.json
  vitest.config.ts
  src/
    index.ts
    auth-tool.ts
    client-http.ts
    client-shared.ts
    client-stdio.ts
    config-loader.ts
    connection-manager.ts
    output.ts
    session-config.ts
    tool-naming.ts
    trace-recorder.ts
    types.ts
    built-in/
    oauth/
  test/

packages/integration-tests/
  package.json
  tsconfig.json
  vitest.config.ts
  test/

packages/agent-core/               # 删除 src/code-review、src/e2e-testing、src/mcp
  src/index.ts                     # 移除 code-review re-export
  src/config/index.ts              # 改从 shared 重导出 schema
  src/session/hooks/types.ts       # 改从 shared 导入 HOOK_EVENT_TYPES
  src/agent/permission/matches-rule.ts  # 改从 shared 导入 parsePattern
  src/...                          # 其余 import 改为 @odysseythink/*

pnpm-workspace.yaml                # 新增 5 个包
flake.nix                          # workspacePaths / workspaceNames 同步
package.json                       # madge script / devDependency
Makefile                           # madge:circular target
```

---

## Dependency Overview

```
integration-tests (dev) ──► code-review, e2e-testing, mcp-host, agent-core, shared

          code-review ──────┐
          e2e-testing ──────┼──► agent-core-shared
          mcp-host    ──────┘
                 │
                 ▼
          agent-core ─────────► agent-core-shared
```

### Phases

| Phase | File | Scope | When it ships |
|---|---|---|---|
| A | `shared.md` | 创建 `@odysseythink/agent-core-shared`，把 errors / logging / abort / version / tool-execution / input-schema / mcp-events / permission-pattern / hook-events / config / flags / wasm-loader / wasm-string 迁出，并同步修改 agent-core 内部 import。 | shared 包独立 typecheck/test 通过，且 agent-core 仍能 typecheck。 |
| B1 | `code-review.md` | 创建 `@odysseythink/code-review`，迁移源码与 wasm-diff 相关工具，重写 import，更新 agent-core 调用点。 | code-review 包 test 通过，agent-core 调用新包编译通过。 |
| B2 | `e2e-testing.md` | 创建 `@odysseythink/e2e-testing`，迁移源码与测试，重写 import，更新 agent-core 调用点。 | e2e-testing 包 test 通过，agent-core 调用新包编译通过。 |
| B3 | `mcp-host.md` | 创建 `@odysseythink/mcp-host`，迁移源码与测试，重写 import，处理 logger fallback，更新 agent-core 调用点。 | mcp-host 包 test 通过，agent-core 调用新包编译通过。 |
| C | `agent-core-cleanup.md` | 删除 agent-core 内部已迁出的目录，清理 index.ts re-export，全树 typecheck。 | agent-core typecheck/test 通过。 |
| D | `integration-and-ci.md` | 创建 integration-tests 包并迁移跨模块测试；更新 pnpm-workspace.yaml、flake.nix；新增 madge 脚本与 Makefile target。 | 全 workspace typecheck/test 通过，`pnpm madge:circular` 通过。 |

Phases B1/B2/B3 互相独立，可在 Phase A 完成后并行执行。

---

## Risks & Open Questions

| # | 风险 | 缓解措施 |
|---|---|---|
| R1 | `config/schema.ts` 内部依赖 `agent/permission/matches-rule` 与 `session/hooks/types`；直接迁移会产生循环依赖。 | 将 `parsePattern` / `isValidPermissionPattern` 与 `HOOK_EVENT_TYPES` 抽入 shared，schema 改为从 shared 导入；agent-core 的 matches-rule 与 hooks/types 也改为从 shared 导入。 |
| R2 | `wasm-diff` 迁到 `code-review` 后，其依赖 `wasm-loader` / `wasm-string` 仍留在 agent-core 会造成 code-review → agent-core 反向依赖。 | 将 `wasm-loader` / `wasm-string` 及所依赖的 `flags` 系统一并下沉到 shared；agent-core 与 code-review 均从 shared 导入。 |
| R3 | `mcp-host` 的 `connection-manager.ts` 原使用 agent-core 全局 `log`；迁出后无默认 logger。 | 在 `mcp-host` 内提供一个最小 fallback logger（console），生产代码由 `Session` 传入 session logger 保持原有行为；同步更新所有构造调用点。 |
| R4 | 移除 agent-core re-export 后，外部 consumer 可能通过 `@odysseythink/agent-core` 引用已迁移类型。 | 全仓库 Grep 确认无 `@odysseythink/agent-core` 对 CodeReview*/Mcp*/E2E* 的导出使用；在 agent-core-cleanup 任务中再次验证。 |
| R5 | `madge` 配置过严或误判 test 目录。 | 脚本仅扫描 `packages/*/src/**/*.ts`，排除 `test/` / `dist/`；仅把源码循环依赖作为失败条件。 |
| R6 | `flake.nix` workspace 列表漏更新导致 Nix 构建丢文件或依赖未拉取。 | 在 integration-and-ci 任务中同步更新 `workspacePaths` 与 `workspaceNames`，并以 `nix build .#ody-code` 作为最终验证。 |

---

## Spec-coverage table

| 设计章节 / 需求 | 覆盖 Part | 覆盖状态 |
|---|---|---|
| 新建 `@odysseythink/agent-core-shared` | shared.md | covered |
| 新建 `@odysseythink/code-review` | code-review.md | covered |
| 新建 `@odysseythink/e2e-testing` | e2e-testing.md | covered |
| 新建 `@odysseythink/mcp-host` | mcp-host.md | covered |
| 新建 `@odysseythink/integration-tests` | integration-and-ci.md | covered |
| 更新 agent-core 内部调用者，改为从新包导入 | code-review.md, e2e-testing.md, mcp-host.md, agent-core-cleanup.md | covered |
| CI 引入 `madge` 循环依赖检测 | integration-and-ci.md | covered |
| 同步 `pnpm-workspace.yaml` 与 `flake.nix` | integration-and-ci.md | covered |
| `office-hours/` 等明确排除 | — | no-op |
| agent-core 不再 re-export 三新包 | agent-core-cleanup.md | covered |

---

## Global Self-Review

- [ ] 1. Spec-coverage table：见上表，所有设计章节/需求均已映射到 Part，无 GAP。
- [ ] 2. Placeholder scan：所有 Part 文件均无 TODO/TBD，无 "后续再实现"，无 dead-code 占位。
- [ ] 3. No phantom tasks：每个 Part 的每个 Task 都产生可验证的代码/测试/配置/typecheck 变更，无 `--allow-empty`。
- [ ] 4. Dependency soundness：Index 中 Phases A → B1/B2/B3（并行） → C → D 顺序合理；跨 Part 依赖（如 C 依赖 B1/B2/B3、D 依赖 C）均为先完成的 Part；无引用后续 Part 才创建的符号。
- [ ] 5. Caller & build soundness：所有 shared-signature 变更（如 `McpConnectionManagerOptions.clientVersion`、agent-core index 删除 code-review re-export）均在同一 Part 内更新所有调用方（含测试），并以 `pnpm -r --filter './packages/*' run typecheck` 全树验证；`flake.nix` workspacePaths / workspaceNames 一一对应。
- [ ] 6. Test-the-risk：所有状态变更/边界/跨包集成行为均由行为级测试覆盖（新包单元测试、迁移的集成测试、madge 循环依赖扫描、Nix 构建门禁），不仅是编译检查。
- [ ] 7. Type consistency：跨 Part 引用的类型、签名、属性名、包名均保持一致；Part 之间无同名异义符号。

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-24-phase-1-c-agent-core-extraction/shared.md` | 创建 shared 包并下沉基础原语 | done |
| 2 | `2026-06-24-phase-1-c-agent-core-extraction/code-review.md` | 创建 code-review 包 | done |
| 3 | `2026-06-24-phase-1-c-agent-core-extraction/e2e-testing.md` | 创建 e2e-testing 包 | done |
| 4 | `2026-06-24-phase-1-c-agent-core-extraction/mcp-host.md` | 创建 mcp-host 包 | done |
| 5 | `2026-06-24-phase-1-c-agent-core-extraction/agent-core-cleanup.md` | 清理 agent-core 内部目录与 re-export | done |
| 6 | `2026-06-24-phase-1-c-agent-core-extraction/integration-and-ci.md` | 集成测试、workspace 配置、madge 守护 | done |
<!-- e2e-enriched -->

### Task 1: Generate and run E2E tests

Based on the changed files, validate the following areas:
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/background (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/compaction (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/config (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/context (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/cron (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/injection (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/permission (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/permission/policies (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/records (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/replay (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/session-mode (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/skill (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/tool (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/turn (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/usage (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/errors (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/loop (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/mcp (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/rpc (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/rpc/transports (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/session/checkpoint (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/session/export (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/session (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/skill/builtin (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/skill (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/background (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/collaboration (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/file (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/game-design (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/goal (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/idea (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/office-hours (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/planning (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/shell (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/state (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/visual (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/web (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/cron (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/providers/web-search (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/support (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/test/agent/background (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/test/agent/cron/harness (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/test/agent/harness (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/test/agent/records/migration (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/test/loop/fixtures (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/test/tools/fixtures (priority: important)
- /Users/ranwei/workspace/ody-code/packages/node-sdk/src (priority: important)

For any externally-facing interface you changed (HTTP endpoint/handler, RPC, or
CLI command), add a test that drives it through that interface and asserts on the
response (status code + parsed body), then run the suite. If the interface
requires authentication, supply a valid credential so the authorized path is
exercised and also assert the unauthorized case (401/403). You may also use the
RunE2ETests tool to scaffold and run E2E tests.

