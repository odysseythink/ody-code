# Phase 2-D：Mode 概念统一与 ModeBehavior 重构实施计划

**Goal:** 通过 `SessionModeKind` / `RuntimeMode` 两层类型收敛、`ModeBehaviorRegistry` 策略对象与 `SessionModeInjector` 注入器基类，统一四个 session mode 的 enter/exit/handoff 行为，并替换全仓库 mode 字符串字面量为类型安全引用。

**Architecture:** `SessionMode` 退化为调度器，所有 mode-specific 行为（输出目录、model key、handoff 目标、生命周期副作用）由 `ModeBehaviorRegistry` 解析出的 `SessionModeBehavior` 承载；`SessionModeInjector` 抽象基类统一 full/sparse/reentry 变体调度与 `onContextClear` 状态记忆，各 mode 注入器只提供 reminder 文本与少量扩展上下文。类型层严格区分 `SessionModeKind`（4 种交互阶段）与 `RuntimeMode`（含 `normal`）。

**Tech Stack:** TypeScript, Vitest, Zod, pnpm monorepo (`packages/agent-core`, `packages/agent-core-shared`, `packages/node-sdk`).

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

| 路径 | 职责 |
|---|---|
| `packages/agent-core/src/agent/session-mode/types.ts` (new) | `SESSION_MODE_KINDS` / `RUNTIME_MODES` 常量、`SessionModeKind` / `RuntimeMode` 类型、类型守卫与 `normalizeRuntimeMode` |
| `packages/agent-core/src/agent/session-mode/behaviors/index.ts` (new) | `ModeBehaviorRegistry`、`BaseSessionModeBehavior`、4 个具体 behavior |
| `packages/agent-core/src/agent/session-mode/behaviors/plan.ts` (new) | `PlanModeBehavior` |
| `packages/agent-core/src/agent/session-mode/behaviors/design.ts` (new) | `DesignModeBehavior` |
| `packages/agent-core/src/agent/session-mode/behaviors/office-hours.ts` (new) | `OfficeHoursModeBehavior` |
| `packages/agent-core/src/agent/session-mode/behaviors/game-design.ts` (new) | `GameDesignModeBehavior` |
| `packages/agent-core/src/agent/injection/session-mode-injector.ts` (new) | `SessionModeInjector` 抽象基类 |
| `packages/agent-core/src/agent/session-mode/index.ts` | 统一导出类型与 `SessionMode` 类；重构为 behavior 调度 |
| `packages/agent-core/src/agent/injection/plan-mode.ts` | 改为继承 `SessionModeInjector` |
| `packages/agent-core/src/agent/injection/design-mode.ts` | 改为继承 `SessionModeInjector`，保留 `mockupAvailable` 扩展 |
| `packages/agent-core/src/agent/injection/office-hours.ts` | 改为继承 `SessionModeInjector` |
| `packages/agent-core/src/agent/injection/game-design.ts` | 改为继承 `SessionModeInjector` |
| `packages/agent-core/src/agent/injection/manager.ts` | 更新注入器列表类型引用 |
| `packages/agent-core/src/agent/index.ts` | `ModeKey` → `RuntimeMode`；`_contexts` / `_fullCompactions` / `_microCompactions` 键类型；`setContextMode` 签名；`useProfile` / `emitStatusUpdated` |
| `packages/agent-core/src/rpc/core-api.ts` | `EnterPlanPayload.kind`、`SessionAPI.listSkills` 参数、`AgentAPI` 中使用 mode 的位置 |
| `packages/agent-core/src/session/rpc.ts` | `listSkills` 参数类型 |
| `packages/agent-core/src/session/index.ts` | `listSkills` 参数类型；`refreshSessionRuntimeConfig` 分支 |
| `packages/agent-core/src/skill/registry.ts` | `listInvocableSkills`、`getModelSkillListing`、`getUnavailableSkillsReminder` 参数类型 |
| `packages/agent-core/src/profile/types.ts` | `SystemPromptContext.sessionMode` |
| `packages/agent-core-shared/src/config.ts` | `OdyConfigSchema.sessionMode` / `defaultSessionMode` 扩为 `RuntimeMode`；patch schema 同步 |
| `packages/agent-core-shared/src/errors/codes.ts` | 新增 `INTERNAL_ERROR`（若缺失） |
| `packages/node-sdk/src/types.ts` | `sessionMode` 字段类型 |
| `packages/agent-core/src/rpc/events.ts` | `AgentStatusUpdatedEvent` 中 `sessionMode` 字段 |
| `docs/architecture/modes-vs-profiles.md` (new) | 架构文档与反向检查单 |
| 各 `test/` 文件 | 跟随类型重命名与行为变更更新 |

## Dependency Overview

```
Phase A — 类型与配置基础
  Task 1: RuntimeMode 类型与守卫 (types.ts)
  Task 2: OdyConfig schema 扩展 (agent-core-shared)

Phase B — Behavior / Injector 基础设施
  Task 3: ModeBehaviorRegistry + BaseSessionModeBehavior
  Task 4: 4 个具体 ModeBehavior
  Task 5: SessionModeInjector 抽象基类
  Task 6: 4 个 mode 注入器改为继承基类

Phase C — SessionMode 重构
  Task 7: SessionMode 改为 behavior 调度

Phase D — 全仓库类型替换
  Task 8: Agent / RPC / Skill / Profile / Session 中 mode 字面量替换
  Task 9: node-sdk / events / 其余调用点替换

Phase E — 测试、文档、验收
  Task 10: 新增类型/behavior/injector/session-mode 测试
  Task 11: 架构文档与反向检查单
  Task 12: 全仓库 typecheck 与相关测试
```

各 phase 内部顺序不可颠倒；Phase A/B 可独立启动但 B 依赖 A；Phase C 依赖 B；Phase D 依赖 A/C；Phase E 依赖 D。

## Risks & Open Questions

| 风险 | 缓解 |
|---|---|
| `ModeKey` 改为 `RuntimeMode` 影响 `_contexts` 等 Record 键类型与运行时分支 | 全替换任务中同步更新所有字面量；结束时 `pnpm -r typecheck` |
| `BaseSessionModeBehavior` 默认 onEnter 无法覆盖 design 的 session 追踪 | design behavior 覆盖 `onEnter`/`onExit`/`onCancel`，先 `super.onEnter()` 再插入专属逻辑 |
| `getUnavailableSkillsReminder` 放宽到 `RuntimeMode` 可能给 office-hours/game-design 错误提示 | 实现后检查 reminder 输出；必要时保持该函数仍只接受 plan/design（文档标注） |
| 注入器统一后 design 的 `mockupAvailable` 丢失 | `SessionModeInjector` 预留 `getExtraContext()` 钩子 |

## Spec-Coverage Table

| 设计章节 | 需求 | 覆盖任务 | 状态 |
|---|---|---|---|
| Scope In 1 | 定义两层类型 `SessionModeKind` / `RuntimeMode` | Task 1 | covered |
| Scope In 2 | 替换所有 mode 字符串字面量 | Task 8, Task 9 | covered |
| Scope In 3 | 扩展 `OdyConfig.sessionMode` / `defaultSessionMode` | Task 2 | covered |
| Scope In 4 | 引入 `SessionModeBehavior` 与注册表 | Task 3, Task 4 | covered |
| Scope In 5 | 提取 `SessionModeInjector` 抽象基类 | Task 5 | covered |
| Scope In 6 | 新增 `docs/architecture/modes-vs-profiles.md` | Task 11 | covered |
| Scope In 7 | 新增类型与行为测试 | Task 10, Task 12 | covered |
| Scope Out 1 | 不改 RPC 契约方法名/签名 | — | no-op |
| Scope Out 2 | 不改 profile 加载/继承逻辑 | — | no-op |
| Scope Out 3 | 不改 mode-specific contract 文本 | — | no-op |
| Scope Out 4 | 不改 state store 实现 | — | no-op |
| Error Handling | 未知 mode 回退 normal；注册表未注册抛错；enter 失败回滚 | Task 1, Task 3, Task 7 | covered |
| Testing | 类型/behavior/injector/session-mode 测试 | Task 10 | covered |
| Done Criteria | `pnpm typecheck` + 相关测试通过 | Task 12 | covered |

## Self-Review

- [ ] 1. **Spec-coverage table**: 上表已映射设计文档的 Scope In 1–7、Scope Out 1–4、Error Handling、Testing、Done Criteria，无 GAP。
- [ ] 2. **Placeholder scan**：所有 part 文件均无 TODO/TBD/"implement later"；每步给出具体文件、行号、代码、命令与预期输出。
- [ ] 3. **No phantom tasks**：每个 task 产生可验证变更（新建文件、修改文件、测试通过、typecheck 通过、changeset），无 `--allow-empty` 提交，无 "already done in Task N" 绕过。
- [ ] 4. **Dependency soundness**：Part 1–7 的 `Depends on:` 均指向前序 part；index 的 Task 1–12 顺序与 part 文件任务顺序一致，后序任务不引用未定义的符号。
- [ ] 5. **Caller & build soundness**：共享签名变更（`ModeKey` → `RuntimeMode`、Skill/RPC/Profile/Session 参数类型）全部集中在 Part 6 Task 6 单任务内完成，包含测试文件，并以 `pnpm -r typecheck` 收尾；同一签名未跨任务拆分。
- [ ] 6. **Test-the-risk**：
  - Part 1 测试覆盖 `RUNTIME_MODES` 集合与 `isRuntimeMode`/`normalizeRuntimeMode` 的合法/非法输入。
  - Part 3 测试覆盖 `ModeBehaviorRegistry.resolve` 未注册 kind 的抛错与 behavior 元数据。
  - Part 4 测试覆盖 `computeVariant` 阈值边界与用户打断路径。
  - Part 5 测试覆盖 `enter()` 失败原子回滚与 `exit()` 幂等性。
  - 每个 must-survive 输入（`'normal'`, `'plan'`, `'design'`, `'office-hours'`, `'game-design'`）均通过守卫测试，must-reject（`'foo'`）被正确回退。
- [ ] 7. **Type consistency**：`RuntimeMode` / `SessionModeKind` 两层类型贯穿 Part 1–7；`SessionModeKind` 仅用于 enterable 交互阶段，`RuntimeMode` 用于含 `normal` 的运行时上下文，与设计文档约定一致。

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-25-backend-architecture-evolution-phase2-d/types.md` | RuntimeMode 类型与守卫 | done |
| 2 | `2026-06-25-backend-architecture-evolution-phase2-d/config.md` | OdyConfig schema 扩展 | done |
| 3 | `2026-06-25-backend-architecture-evolution-phase2-d/behaviors.md` | ModeBehaviorRegistry + 4 behaviors | done |
| 4 | `2026-06-25-backend-architecture-evolution-phase2-d/injector.md` | SessionModeInjector 基类与注入器重构 | done |
| 5 | `2026-06-25-backend-architecture-evolution-phase2-d/session-mode.md` | SessionMode 类重构 | done |
| 6 | `2026-06-25-backend-architecture-evolution-phase2-d/type-replacement.md` | 全仓库 mode 字面量替换 | done |
| 7 | `2026-06-25-backend-architecture-evolution-phase2-d/tests-docs.md` | 测试、文档、最终验收 | done |
