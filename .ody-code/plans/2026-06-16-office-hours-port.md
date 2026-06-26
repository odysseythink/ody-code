# Office Hours 内置命令移植 Implementation Plan

**Goal:** 将上游 YC Office Hours 完整移植为 ody-code 的 `--office-hours` 启动模式，覆盖 CLI 入口、session mode、injector、Phase 1-6 工作流 prompt、builder profile 持久化、telemetry、CLAUDE.md routing 和 gbrain artifacts sync。

**Architecture:** 新增 `'office-hours'` 作为第四种 session mode（与 `plan`/`design` 并列）。CLI 通过 `--office-hours` 参数触发专用启动路径 `runOfficeHours()`，创建单用途 Session 并注入完整的 YC Office Hours workflow prompt。LLM 通过 `AskUserQuestion` 推进 Phase 1-6 诊断流程，最终通过 `ExitOfficeHoursModeTool` 写入设计文档并触发应用退出。

**Tech Stack:** TypeScript, Node.js ≥24.15, pnpm monorepo. 涉及 `packages/agent-core`（session mode 引擎、injector、tools、state store）、`apps/ody-code`（CLI 入口、TUI 启动）、`packages/telemetry`（事件上报）。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

| File | Create/Modify | Purpose |
|------|--------------|---------|
| `packages/agent-core/src/agent/index.ts:77,129-131,192-200` | Modify | `ModeKey` 扩展 + context partitions |
| `packages/agent-core/src/agent/session-mode/index.ts:22,593-606` | Modify | `SessionModeKind` 扩展 + 目录解析 |
| `packages/agent-core/src/agent/replay/index.ts:1,7,11,37` | Modify | ReplayBuilder ModeKey 引用 |
| `packages/agent-core/src/session/checkpoint/integrity.ts:15-17` | Modify | 独立 `ModeKey` + `VALID_MODES` |
| `packages/agent-core/src/session/checkpoint/checkpoint.ts:28` | Modify | `currentMode: ModeKey`（类型跟随） |
| `packages/agent-core/src/rpc/events.ts:50` | Modify | `sessionMode` 字面量 |
| `packages/agent-core/src/rpc/core-api.ts:383` | Modify | `listSkills` payload |
| `packages/agent-core/src/profile/types.ts:45` | Modify | `sessionMode` 字面量 |
| `packages/agent-core/src/skill/types.ts:57-58` | Modify | `SkillCatalog` 接口 |
| `packages/agent-core/src/skill/registry.ts:113-172` | Modify | `listInvocableSkills` 等实现 |
| `packages/agent-core/src/session/rpc.ts:91` | Modify | `listSkills` handler |
| `packages/agent-core/src/session/index.ts:383` | Modify | `listSkills` 方法 |
| `packages/agent-core/src/agent/injection/office-hours.ts` | **Create** | `OfficeHoursInjector` |
| `packages/agent-core/src/agent/injection/office-hours-contract.ts` | **Create** | Phase 1-6 工作流 prompt fragments |
| `packages/agent-core/src/agent/injection/manager.ts:20-30` | Modify | 注册 OfficeHoursInjector |
| `packages/agent-core/src/agent/tool/index.ts:388-466` | Modify | 注册 office-hours tools |
| `packages/agent-core/src/tools/builtin/office-hours/enter-office-hours.ts` | **Create** | `EnterOfficeHoursModeTool` |
| `packages/agent-core/src/tools/builtin/office-hours/exit-office-hours.ts` | **Create** | `ExitOfficeHoursModeTool` |
| `packages/agent-core/src/tools/builtin/office-hours/ensure-routing.ts` | **Create** | `EnsureClaudeMdRoutingTool` |
| `packages/agent-core/src/tools/builtin/office-hours/sync-artifact.ts` | **Create** | `SyncOfficeHoursArtifactTool` |
| `packages/agent-core/src/tools/builtin/office-hours/append-profile.ts` | **Create** | `AppendBuilderProfileTool` |
| `packages/agent-core/src/tools/builtin/office-hours/append-learning.ts` | **Create** | `AppendLearningTool` |
| `packages/agent-core/src/tools/builtin/office-hours/search-learnings.ts` | **Create** | `SearchLearningsTool` |
| `packages/agent-core/src/office-hours/state.ts` | **Create** | `OfficeHoursStateStore` + `FileSystemOfficeHoursStateStore` |
| `packages/agent-core/src/agent/permission/policies/plan-mode-guard-deny.ts` | Modify | 扩展写保护到 office-hours |
| `apps/ody-code/src/cli/commands.ts:70-85,105-125` | Modify | `--office-hours` 选项 + 解析 |
| `apps/ody-code/src/cli/options.ts:4-16,30-72` | Modify | `CLIOptions` 扩展 + 冲突校验 |
| `apps/ody-code/src/cli/run-office-hours.ts` | **Create** | `runOfficeHours()` 启动流程 |
| `apps/ody-code/src/main.ts:74-79` | Modify | `officeHours` 分支路由 |
| `apps/ody-code/src/tui/types.ts:15-41,174-183` | Modify | `AppState` + `TUIStartupOptions` 扩展 |
| `apps/ody-code/src/tui/ody-tui.ts:149-180,248-257,454-516` | Modify | `KimiTUIStartupInput`, `createInitialAppState`, `start()` |
| `apps/ody-code/src/tui/components/messages/status-panel.ts:37,101-108` | Modify | 新增 office-hours 状态显示 |
| `apps/ody-code/src/tui/components/chrome/footer.ts:49` | Modify | mode 字面量扩展 |
| `apps/ody-code/src/tui/commands/types.ts:4` | Modify | `SessionMode` 类型扩展 |
| `packages/node-sdk/src/types.ts:86-94` | Modify | `CreateSessionOptions.sessionMode` 扩展 |
| `packages/node-sdk/src/kimi-harness.ts:101-122` | Modify | `createSession` 处理 office-hours |

## Dependency Overview

```
Phase A: 共享签名变更（所有任务的前置条件）
  Task 1: ModeKey + SessionModeKind 扩展为 'office-hours'，更新全部调用方

Phase B: CLI + TUI 接入（依赖 Task 1）
  Task 2: --office-hours CLI 参数 + 冲突校验  ──┐
  Task 3: runOfficeHours 启动流程                ├── 可并行
  Task 4: TUI startup + AppState wiring        ──┘

Phase C: Session Mode 引擎（依赖 Task 1，可与 Phase B 并行）
  Task 5: SessionMode.enter/exit('office-hours') + 目录解析
  Task 6: OfficeHoursInjector  ── (依赖 Task 5)
  Task 7: OfficeHours 工具 + ToolManager + permission guard ── (依赖 Task 5)

Phase D: 工作流 + 状态 + 集成（依赖 Phase B + C）
  Task 8:  office-hours-contract.ts prompt fragments ──┐
  Task 9:  OfficeHoursStateStore                        ├── 可并行
  Task 10: State tools (profile/learning)               │
  Task 11: Telemetry + routing + gbrain sync tools     ──┘
```

## Parts (generate one per invocation, in order)

| # | File | Scope | Status |
|---|---|---|---|
| 1 | 2026-06-16-office-hours-port/core-types.md | Task 1: 共享签名变更 — ModeKey/SessionModeKind/所有 sessionMode 字面量扩展 | done |
| 2 | 2026-06-16-office-hours-port/cli-tui.md | Tasks 2-4: CLI 入口、runOfficeHours、TUI wiring | done |
| 3 | 2026-06-16-office-hours-port/session-engine.md | Tasks 5-7: SessionMode 引擎、OfficeHoursInjector、工具注册 | done |
| 4 | 2026-06-16-office-hours-port/workflow-state.md | Tasks 8-11: prompt contract、state store、integration tools | done |

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | `ModeKey` 扩展遗漏调用方导致编译错误 | High | High | Task 1 包含完整的 19 文件清单 + 全局 typecheck |
| 2 | checkpoint 序列化不识别 `'office-hours'` | Low | High | `integrity.ts` 的 `VALID_MODES` 同步更新；未知 mode 降级为 normal |
| 3 | office-hours injector 与 design-mode injector 同时激活 | Medium | Medium | `OfficeHoursInjector` 在 `DesignModeInjector` 之后注册，isActive 检查互斥 |
| 4 | `ToolManager` 构造时 sessionMode 未激活导致条件注册失败 | High | Medium | 改为始终注册 office-hours tools，tool 内部检查 `kind !== 'office-hours'` 返回 `isError` |
| 5 | State store 文件写入权限失败 | Medium | Low | catch + warn，不阻塞流程 |

## Spec Coverage Table

| Spec Item (from design Scope In) | Task(s) | Status |
|---|---|---|
| 1. CLI 入口 `ody --office-hours` | Task 2, 3 | covered |
| 2. Session Mode `office-hours` | Task 1, 5 | covered |
| 3. Phase 1-6 核心工作流 prompt | Task 6, 8 | covered |
| 4. 设计文档输出 `.ody-code/office-hours/` | Task 5 (directory), Task 8 (template) | covered |
| 5. 应用生命周期（写完自动退出） | Task 7 (ExitOfficeHoursModeTool), Task 3 | covered |
| 6. Builder Profile 持久化 | Task 9, 10 | covered |
| 7. Telemetry 接入 | Task 11 | covered |
| 8. Learnings 本地记录 | Task 9, 10 | covered |
| 9. CLAUDE.md routing 注入 | Task 11 (EnsureClaudeMdRoutingTool) | covered |
| 10. Artifacts Sync (gbrain) | Task 11 (SyncOfficeHoursArtifactTool) | covered |

## Self-Review

- [x] 1. Spec-coverage table: all 10 Scope In items mapped → Task(s), no GAP.
- [x] 2. Placeholder scan: no TODO/TBD in any task — all 4 part files verified.
- [x] 3. No phantom tasks: every task produces a verifiable change (35+ file modifications across 11 tasks).
- [x] 4. Dependency soundness: every `Depends on:` satisfied by an earlier task/phase. Cross-phase dep (Task 6→Task 8) handled via contract stubs.
- [x] 5. Caller & build soundness: Task 1 updates ALL 23 files + ends with `pnpm -r typecheck`. Same signature not changed across multiple tasks.
- [x] 6. Test-the-risk: every state-mutating task has behavioral test assertions (tier calc edge cases, double-enter guard, resource dedup threshold, noop safety).
- [x] 7. Type consistency: `ModeKey`/`SessionModeKind` defined in Task 1 match all downstream usage. `BuilderProfileEntry`/`LearningEntry` match design spec.
<!-- e2e-enriched -->

### Task 1: Generate and run E2E tests

Based on the changed files, validate the following tools:
- ExitPlanModeTool (priority: critical)

Use the RunE2ETests tool after completing the implementation tasks above.

