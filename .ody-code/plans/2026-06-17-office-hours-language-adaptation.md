# Office-Hours 语言自适应 Implementation Plan

**Goal:** 让 Office-Hours 模式根据用户输入语言自动切换：LLM 收到语言指令、通过内置工具记录语言码、所有 Office-Hours 工具输出与 TUI 状态标签按中文/英文本地化，并支持会话恢复。

**Architecture:** 在 `packages/agent-core` 新增最小 i18n 层（`SupportedLanguage`、`t()`），由 `SetOfficeHoursLanguage` 工具在会话开始时写入 `Session.metadata.custom['userLanguage']`；`Agent` 持有运行时 `userLanguage` 并通过 `AgentStatusUpdatedEvent` 透传给 TUI。所有 Office-Hours 工具调用 `t(key, agent.userLanguage)` 替换硬编码英文；TUI footer 徽章与 `/status` 面板读取 `AppState.userLanguage` 渲染本地化文本。

**Tech Stack:** TypeScript, pnpm workspace, Vitest, Zod, `package.json#imports` (`#/*`).

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

新增文件（Create）

| 文件 | 职责 |
|---|---|
| `packages/agent-core/src/i18n/types.ts` | `SupportedLanguage`、`MessageKey` 类型定义 |
| `packages/agent-core/src/i18n/translations.ts` | `en` / `zh` 翻译表 |
| `packages/agent-core/src/i18n/index.ts` | `t()`、`isSupportedLanguage()`、`normalizeLanguage()` |
| `packages/agent-core/src/tools/builtin/office-hours/set-language.ts` | `SetOfficeHoursLanguage` 内置工具 |
| `packages/agent-core/src/tools/builtin/office-hours/set-language.md` | 工具 description（给 LLM 看的 markdown） |
| `packages/agent-core/test/i18n/index.test.ts` | i18n 模块单元测试 |
| `packages/agent-core/test/i18n/language.test.ts` | 语言校验/归一化测试 |
| `packages/agent-core/test/agent/user-language.test.ts` | Agent 语言属性/回调测试 |
| `packages/agent-core/test/session/user-language-persistence.test.ts` | Session metadata 持久化测试 |
| `packages/agent-core/test/tools/builtin/office-hours/set-language.test.ts` | 语言设置工具测试 |
| `packages/agent-core/test/tools/builtin/office-hours/enter-exit.test.ts` | Enter/Exit 工具本地化测试 |
| `packages/agent-core/test/tools/builtin/office-hours/state-tools.test.ts` | AppendBuilderProfile / AppendLearning / SearchLearnings 本地化测试 |
| `packages/agent-core/test/tools/builtin/office-hours/artifact-tools.test.ts` | SyncOfficeHoursArtifact / EnsureClaudeMdRouting 本地化测试 |

修改文件（Modify）

| 文件 | 职责 |
|---|---|
| `packages/agent-core/src/agent/injection/office-hours-contract.ts` | entry/full/sparse/reentry prompt 顶部注入 Language 指令 |
| `packages/agent-core/src/agent/index.ts` | Agent 增加 `userLanguage`、`setUserLanguage()`、`getUserLanguage()` RPC、状态事件携带语言 |
| `packages/agent-core/src/session/index.ts` | `instantiateAgent` 传递 `userLanguage` 与持久化回调 |
| `packages/agent-core/src/agent/tool/index.ts` | 注册 `SetOfficeHoursLanguageTool` |
| `packages/agent-core/src/tools/builtin/index.ts` | 导出 `set-language` |
| `packages/agent-core/src/rpc/events.ts` | `AgentStatusUpdatedEvent` 增加 `userLanguage` |
| `packages/agent-core/src/rpc/core-api.ts` | `AgentAPI` 增加 `getUserLanguage` |
| `packages/agent-core/src/index.ts` | 导出 `SupportedLanguage`、`MessageKey`、`t` 等 |
| `packages/node-sdk/src/types.ts` | `SessionStatus` 增加 `userLanguage` |
| `packages/node-sdk/src/rpc.ts` | `getStatus()` 调用 `rpc.getUserLanguage()` |
| `packages/node-sdk/src/index.ts` | 向 SDK 消费者 re-export `t`、`SupportedLanguage` |
| `apps/ody-code/src/tui/types.ts` | `AppState` 增加 `userLanguage` |
| `apps/ody-code/src/tui/controllers/session-event-handler.ts` | `agent.status.updated` 透传 `userLanguage` |
| `apps/ody-code/src/tui/ody-tui.ts` | 初始状态 / `syncRuntimeState` 读写 `userLanguage` |
| `apps/ody-code/src/tui/components/chrome/footer.ts` | office-hours badge 本地化 |
| `apps/ody-code/src/tui/components/messages/status-panel.ts` | `/status` Office Hours 行本地化 |
| `apps/ody-code/test/tui/components/chrome/footer.test.ts` | badge 中文断言 |
| `apps/ody-code/test/tui/components/messages/status-panel.test.ts` | 状态面板中文断言 |
| `apps/ody-code/test/tui/controllers/session-event-handler.test.ts` | 事件透传测试 |

## Dependency Overview

按子系统拆分为 3 个 Part，Part 内部顺序执行，Part 之间也顺序依赖。

```
Part 1: core (i18n + Agent/Session + SetOfficeHoursLanguage)
  Task 1: i18n 模块
  Task 2: Agent userLanguage 运行时 + 回调
  Task 3: Session 持久化
  Task 4: SetOfficeHoursLanguage 工具与注册

Part 2: tools (prompt + 7 个 Office-Hours 工具本地化)
  Task 5: office-hours-contract prompt 注入 Language 指令
  Task 6: Enter/Exit Office-Hours 工具本地化
  Task 7: AppendBuilderProfile / AppendLearning / SearchLearnings 本地化
  Task 8: SyncOfficeHoursArtifact / EnsureClaudeMdRouting 本地化

Part 3: tui (状态透传 + footer + status panel)
  Task 9: 共享状态/RPC 类型与透传（含全树 typecheck）
  Task 10: Footer badge 本地化
  Task 11: Status panel 本地化 + 全量测试/类型检查收尾
```

- Task 2 依赖 Task 1（使用 `SupportedLanguage`、`t`）。
- Task 3 依赖 Task 2（使用 `AgentOptions.userLanguage` / `setUserLanguage`）。
- Task 4 依赖 Task 2、Task 3（使用 Agent 运行时字段与持久化）。
- Task 5–8 依赖 Task 1（使用 `t()`）和 Task 4（工具注册完成，但不依赖语义）。
- Task 9 依赖 Task 2（`Agent.userLanguage` 存在）与 Task 4（`SetOfficeHoursLanguage` 事件字段来源），并修改跨包共享类型。
- Task 10、11 依赖 Task 9（`AppState.userLanguage`、`SessionStatus.userLanguage`）。

## Risks & Open Questions

| # | 风险 | 缓解措施 | 所属 Task |
|---|---|---|---|
| 1 | LLM 不调用 `SetOfficeHoursLanguage`，所有输出保持英文回退 | 在 prompt 顶部强语言指令，并把工具放在 Office-Hours 工具列表前部 | 5, 4 |
| 2 | 新增 `getUserLanguage` RPC 破坏 node-sdk / 外部 host 的 RPC 契约 | 一次改完 `AgentAPI`、`SessionStatus`、`node-sdk/src/rpc.ts` 并做全树 `typecheck` | 9 |
| 3 | TUI 直接依赖 `@odysseythink/agent-core` 违反项目约束 | 通过 `@odysseythink/ody-code-sdk` re-export `t` 与 `SupportedLanguage` | 9 |
| 4 | 中文翻译表越写越大，后续维护困难 | 所有 key 采用 `namespace.key` 命名，新增语言只扩展 translations 对象 | 1 |
| 5 | 工具输出中的动态路径/变量未正确替换 | 统一使用 `{placeholder}` 占位符并在工具侧 `replace`；测试覆盖包含路径场景 | 6–8 |

## Parts

| # | 文件 | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-17-office-hours-language-adaptation/core.md` | i18n + Agent/Session + SetOfficeHoursLanguage | done |
| 2 | `2026-06-17-office-hours-language-adaptation/tools.md` | prompt 注入 + 7 个 Office-Hours 工具本地化 | done |
| 3 | `2026-06-17-office-hours-language-adaptation/tui.md` | 状态透传 + TUI footer/status panel 本地化 | done |

## Spec-Coverage Table

| 设计 Scope 条目 | 覆盖 Task(s) | 状态 |
|---|---|---|
| Office-hours LLM prompt 注入 Language 指令 | 5 | covered |
| 用户语言检测与状态存储（`SetOfficeHoursLanguage` + `Session.metadata.custom['userLanguage']`） | 1, 2, 3, 4 | covered |
| Office-hours 工具输出本地化（7 个工具） | 6, 7, 8 | covered |
| TUI 标签本地化（footer badge、status panel、`/status`） | 9, 10, 11 | covered |
| 可扩展 i18n 框架（`SupportedLanguage`、`MessageKey`、`t()`） | 1 | covered |
| Out-of-scope：其它模式/工具 description/跨会话持久化/CLI help/通用 i18n/实验开关 | — | no-op |

## Self-Review

- [ ] 1. Spec-coverage table: 见上表，无 GAP。
- [ ] 2. Placeholder scan：所有任务提供完整代码/命令/测试，无 `TODO`/`TBD`/“implement later”。
- [ ] 3. No phantom tasks：每个任务产生可验证的代码/测试变更；无 `--allow-empty` 或“已在 Task N 完成”。
- [ ] 4. Dependency soundness：每个 `Depends on` 指向前序任务；无引用后序符号。
- [ ] 5. Caller & build soundness：共享签名变更集中在 Task 9，同步更新 `AgentAPI`、`SessionStatus`、`AppState` 的所有调用方（含测试），并以 `pnpm -r typecheck` 收尾。
- [ ] 6. Test-the-risk：语言设置、metadata 写入、状态事件、TUI 渲染均有行为断言；语言归一化测试覆盖 `ZH-CN`、`fr`、`''` 等边界。
- [ ] 7. Type consistency：`userLanguage` 字段名与 `SupportedLanguage` 类型在 Agent/Session/RPC/TUI/SDK 中保持一致。
