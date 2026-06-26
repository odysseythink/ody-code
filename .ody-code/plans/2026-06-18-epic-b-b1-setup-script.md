# Epic B-B1: `.ody-code/setup.sh` 会话启动权限门控运行 — Implementation Plan

**Goal:** 在 `Session.createMain()` 返回前自动检测并运行 `.ody-code/setup.sh`，经权限门控（manual 模式提示一次，auto/yolo 自动执行），失败不阻塞会话，注入结果提示；同时提供 `/setup` TUI 命令和 `/init` 模板生成。

**Architecture:** 核心 `SetupScriptRunner.runIfNeeded()` 管道：detect script → check permission（复用 `PermissionManager` mode + `requestApproval`）→ execute via `Kaos.withCwd().execWithEnv()` → persist metadata + inject system reminder + telemetry。TUI `/setup` 和 `/init` 扩展各自复用此管道。

**Tech Stack:** TypeScript 5.x, Node.js ≥24.15.0, pnpm 10.33.0, vitest, Kaos execution abstraction
> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| Task | File | Action | Location |
|------|------|--------|----------|
| T1 | `packages/agent-core/src/session/setup-script.ts` | Create | Types + detect + format helpers |
| T1 | `packages/agent-core/test/session/setup-script.test.ts` | Create | Unit tests for helpers |
| T2 | `packages/agent-core/src/session/setup-script.ts` | Modify | Append execute + runIfNeeded + persistAndInject |
| T2 | `packages/agent-core/test/session/setup-script.test.ts` | Modify | Append integration-style tests (mocked Kaos) |
| T3 | `packages/agent-core/src/agent/permission/index.ts` | Modify | Add `requestSetupScriptApproval()` method (~line 314) |
| T3 | `packages/agent-core/test/agent/permission.test.ts` | Modify | Add setup-script approval tests |
| T4 | `packages/agent-core/src/session/index.ts` | Modify | Wire SetupScriptRunner into `createMain()` (lines 186-193) |
| T4 | `packages/agent-core/test/session/setup-script.test.ts` | Modify | Append createMain integration test |
| T5 | `packages/agent-core/src/session/index.ts` | Modify | Extend `generateAgentsMd()` to also write setup.sh template (lines 329-357) |
| T5 | `packages/agent-core/test/session/setup-script.test.ts` | Modify | Append init-template test |
| T6 | `apps/ody-code/src/tui/commands/setup.ts` | Create | TUI `/setup` handler |
| T6 | `apps/ody-code/src/tui/commands/registry.ts` | Modify | Register `/setup` in BUILTIN_SLASH_COMMANDS (after init ~line 220) |
| T6 | `apps/ody-code/src/tui/commands/dispatch.ts` | Modify | Add case 'setup' → handleSetupCommand (~line 307) |
| T6 | `apps/ody-code/src/tui/commands/session.ts` | Modify | Re-export handleSetupCommand (~line 91) |

---

## Dependency Overview

```
Phase A (core, parallel):
  T1: Types + detect + format helpers ──────────────────┐
  T3: PermissionManager.requestSetupScriptApproval() ────┤
                                                          │
Phase B (core, sequential):                               │
  T2: execute + runIfNeeded + persistAndInject (dep T1)  │
                                                          │
Phase C (core wiring, sequential):                        │
  T4: Session.createMain() wiring (dep T2, T3) ◄─────────┘
  T5: generateAgentsMd() template extension (dep T1)

Phase D (TUI, sequential):
  T6: /setup slash command (dep T4)
```

| Phase | Tasks | What's shippable after |
|-------|-------|----------------------|
| A | T1, T3 | Types + helpers + permission gate, independently testable |
| B | T2 | Full setup-script runner, independently testable |
| C | T4, T5 | End-to-end agent-core wiring, testable via integration tests |
| D | T6 | User-visible /setup command, manually verifiable |

---

## Spec Coverage

| # | Spec Section (from design doc) | Task(s) | Status |
|---|-------------------------------|---------|--------|
| 1.1 | 检测 `.ody-code/setup.sh` 文件存在性 | T1 | covered |
| 1.2 | 会话 startup 时自动运行一次 | T4 | covered |
| 1.3a | manual 模式通过 PermissionManager 提示 | T3 | covered |
| 1.3b | auto/yolo 模式自动执行 | T2 | covered |
| 1.3c | approve for session 写入 session approval rule | T3 | covered |
| 1.4 | 执行环境复用 Kaos + shellPath | T2 | covered |
| 1.5 | 失败不阻塞会话启动，注入系统提示 | T2, T4 | covered |
| 1.6 | 执行状态持久化到 `session.metadata.custom.setupRun` | T2 | covered |
| 1.7 | 默认超时 300s，stdout/stderr 截断 64KB | T1, T2 | covered |
| 1.8 | `/setup` slash 命令供用户手动触发 | T6 | covered |
| 1.9 | `/init` 命令根据项目类型自动生成 `.ody-code/setup.sh` 模板 | T5 | covered |
| 1.10 | telemetry event + session log | T2 | covered |
| 2.1 | Out of scope: `.ody-code/verify.sh` | — | no-op |
| 2.2 | Out of scope: 失败反馈回路 | — | no-op |
| 2.3 | Out of scope: 容器化/沙箱执行 | — | no-op |
| 2.4 | Out of scope: 自定义脚本路径 | — | no-op |
| 2.5 | Out of scope: resume 时自动重新运行 | — | no-op |
| 2.6 | Out of scope: 实验性 flag 门控（默认启用） | — | no-op |

---

## Risks & Open Questions

| # | Risk / Question | Mitigation / Answer |
|---|----------------|---------------------|
| 1 | `Kaos.execWithEnv` 能否在 withCwd 后的实例上正常工作？ | 设计已验证：`Kaos.withCwd()` 返回新实例，`execWithEnv` 在 `bash.ts:212` 已使用，路径正确 |
| 2 | `PermissionManager.requestSetupScriptApproval` 的 `rpc.requestApproval` 在 headless 模式不可用时如何处理？ | 降级到 `approved`（与 BashTool null-rpc 行为一致）；若 `rpc` 未注入，`requestToolApproval` 已处理此路径（第 186-189 行） |
| 3 | `/init` subagent 生成 setup.sh 模板是否会大幅增加延迟？ | 模板生成本地同步完成（检测 lockfiles + render 模板），无需 LLM 调用；延迟 < 1s |
| 4 | TUI `/setup` 命令如何访问 `runSetupScriptIfNeeded`？ | **已解决**：T6 实现完整 RPC pipeline（core-api → core-impl → node-sdk/rpc → node-sdk/session），TUI 通过 `session.setup()` 调用 |
| 5 | T4 和 T5 都修改 `session/index.ts`，是否冲突？ | 修改不同方法（`createMain` vs `generateAgentsMd`），行范围不重叠 |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | 2026-06-18-epic-b-b1-setup-script/core.md | agent-core: SetupScriptRunner, permission, session wiring, /init extension | done |
| 2 | 2026-06-18-epic-b-b1-setup-script/tui.md | apps/ody-code: /setup slash command + registry + dispatch | done |
