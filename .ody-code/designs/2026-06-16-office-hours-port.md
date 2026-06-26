# Office Hours 内置命令移植设计

> 将 `/Users/ranwei/workspace/go_work/gpowers/roles/skills/office-hours` 完整移植为 ody-code 的 `--office-hours` 启动模式。

## Scope In / Out

### In Scope [C:USER]

1. **CLI 入口**：`ody --office-hours` 启动参数，进入专用 TUI 模式。
2. **Session Mode**：新增 `office-hours` session mode；仅在带 `--office-hours` 参数时可用。
3. **核心工作流**：完整移植 YC Office Hours 的 Phase 1-6（上下文收集、Startup/Builder 模式诊断、前提挑战、方案生成、设计文档、结束语）。
4. **设计文档输出**：生成 `.ody-code/designs/` 下的设计文档，复用现有 design-mode 文件命名与只写约束。
5. **应用生命周期**：进入 office-hours mode 后只做该流程；设计文档写完后自动退出 ody 软件。
6. **Builder Profile**：在 `ODY_CODE_HOME` 下持久化 session 历史、信号计数、资源去重，用于 welcome-back tiers。
7. **Telemetry**：接入 `packages/telemetry`，记录 office-hours 启动、阶段、结果。
8. **Learnings**：本地 `~/.ody-code/learnings.jsonl` 记录可复用发现。
9. **Routing**：向项目 `CLAUDE.md` 注入 office-hours 路由规则（如果文件存在）。
10. **Artifacts Sync**：对接 gbrain（如果已配置），保存设计文档索引。

### Out of Scope (Deferred) [C:USER]

1. **非 office-hours 模式下的 `/office-hours` 命令**：用户明确不需要该 slash 命令 [C:USER]。
2. **从普通 mode 切换到 office-hours mode**：启动后锁定在 office-hours mode，不支持运行时切换 [C:USER]。
3. **gstack 专有基础设施**：不移植 `gpowers-update-check`、`gpowers-brain-sync` 等外部二进制；使用 ody-code 等价实现 [C:USER]。
4. **Bun 依赖的 browse setup**：ody-code 不使用 bun browse；WebSearch 工具直接可用 [C:INFERRED]。
5. **Telemetry 的远程 opt-in 详细策略**：复用现有 telemetry 开关，不新增独立 consent 流程 [C:INFERRED]。

## Prior Art

- **上游**：gstack `office-hours` v2.0.0 技能，完整 prompt 工作流约 2000 行，分 Startup/Builder 两模式。
- **ody-code 现有**：`design-mode` 与 `plan-mode` 已提供 session-mode 框架、只写文件保护、periodic injector、entry/exit 工具。`writing-plan` 命令展示了如何通过 CLI 参数锁定 session-mode 文件路径。
- **缺失需新建**：`office-hours` 专用 session kind、独立 injector/prompt、builder-profile 持久化、本地 learnings、CLAUDE.md routing。

## Architecture Overview

```
User runs `ody --office-hours` in project dir
  │
  ▼
CLI parser (apps/ody-code/src/cli or entry) detects flag
  │
  ▼
TUI bootstrap creates single-purpose Session with mode = 'office-hours' [C:USER]
  │
  ▼
SessionMode.enter('office-hours') creates isolated context partition,
resolves .ody-code/designs/<date>-<topic>.md
  │
  ▼
OfficeHoursInjector injects the full YC workflow prompt
  │
  ▼
LLM drives multi-turn conversation via AskUserQuestion (one at a time)
  │
  ▼
On design approval, model writes design doc via Write/Edit to the assigned path
  │
  ▼
OfficeHoursCompletionHandler flushes telemetry/profile, logs learnings,
updates CLAUDE.md routing if needed, triggers gbrain sync if configured,
then calls host.stop() to exit app [C:USER]
```

## Parts Manifest

| # | File | Scope | Status |
|---|---|---|---|
| 1 | [2026-06-16-office-hours-port/cli-entry.md](2026-06-16-office-hours-port/cli-entry.md) | CLI 参数解析、TUI 启动流程、单模式锁定 | done |
| 2 | [2026-06-16-office-hours-port/session-mode.md](2026-06-16-office-hours-port/session-mode.md) | office-hours session kind、injector、entry/exit 工具、文件路径解析 | done |
| 3 | [2026-06-16-office-hours-port/workflow.md](2026-06-16-office-hours-port/workflow.md) | Phase 1-6 工作流 prompt、模式路由、AskUserQuestion 序列、设计文档模板 | done |
| 4 | [2026-06-16-office-hours-port/state.md](2026-06-16-office-hours-port/state.md) | builder profile、session history、analytics、learnings 的持久化与读取 | done |
| 5 | [2026-06-16-office-hours-port/integrations.md](2026-06-16-office-hours-port/integrations.md) | telemetry、CLAUDE.md routing、gbrain artifacts sync | done |

## Data Models

跨组件的关键数据结构定义在各 part 中：

| Data structure | Defined in | Key fields |
|---|---|---|
| `CLIOptions` (扩展) | [cli-entry.md](2026-06-16-office-hours-port/cli-entry.md) | `officeHours: boolean` |
| `TUIStartupOptions` (扩展) | [cli-entry.md](2026-06-16-office-hours-port/cli-entry.md) | `sessionMode: '...' \| 'office-hours'`; `officeHours: boolean` |
| `AppState` (扩展) | [cli-entry.md](2026-06-16-office-hours-port/cli-entry.md) | `sessionMode: '...' \| 'office-hours'` |
| `ModeKey` (扩展) | [session-mode.md](2026-06-16-office-hours-port/session-mode.md) | `'normal' \| 'plan' \| 'design' \| 'office-hours'` |
| `SessionModeKind` (扩展) | [session-mode.md](2026-06-16-office-hours-port/session-mode.md) | `'plan' \| 'design' \| 'office-hours'` |
| `BuilderProfileEntry` | [state.md](2026-06-16-office-hours-port/state.md) | `date, mode, projectSlug, signalCount, signals, designDoc, assignment, resourcesShown, topics` |
| `OfficeHoursAnalyticsEvent` | [state.md](2026-06-16-office-hours-port/state.md) | `ts, skill, event, branch, session, duration_s, outcome, count, categories` |
| `LearningEntry` | [state.md](2026-06-16-office-hours-port/state.md) | `ts, skill, type, key, insight, confidence, source, branch` |
| `TelemetryOfficeHoursEvent` | [integrations.md](2026-06-16-office-hours-port/integrations.md) | `event, project_slug, mode, signal_count, duration_s, outcome` |

## Algorithms (cross-cutting)

| Algorithm | Defined in | Summary |
|---|---|---|
| `selectStartupQuestions(productStage)` | [workflow.md](2026-06-16-office-hours-port/workflow.md) | 根据产品阶段从六问中选择 2-4 个关键问题 |
| `determineMode(userGoal)` | [workflow.md](2026-06-16-office-hours-port/workflow.md) | 从用户输入推断 startup 或 builder 模式 |
| `countFounderSignals(transcript)` | [workflow.md](2026-06-16-office-hours-port/workflow.md) | 统计 founder 信号数量用于 tier 选择 |
| `selectTier(sessionCount)` | [workflow.md](2026-06-16-office-hours-port/workflow.md) | 根据过往 session 次数选择 introduction/welcome_back/regular/inner_circle |
| `computeTier(profileEntries)` | [state.md](2026-06-16-office-hours-port/state.md) | 从 profile 条目计算当前 tier |
| `selectResources(profileEntries, candidates)` | [state.md](2026-06-16-office-hours-port/state.md) | 去重选择 2-3 个创始人资源 |
| `validateOptions(opts)` 扩展 | [cli-entry.md](2026-06-16-office-hours-port/cli-entry.md) | `--office-hours` 与其他选项冲突检测 |
| `runOfficeHours(opts, version)` | [cli-entry.md](2026-06-16-office-hours-port/cli-entry.md) | 专用启动流程：创建 harness → tui → 启动 → 退出 |

## Error Handling (cross-cutting summary)

各 part 均已包含详细的 Error & Degradation 表。跨组件关键路径：

| Scenario | Immediate handling | Degradation | Recovery |
|---|---|---|---|
| `--office-hours` 与其他参数冲突 | `OptionConflictError` → stderr + exit 1 | N/A | 用户修正参数 |
| `SessionMode.enter('office-hours')` 失败 | RPC 冒泡错误 | TUI 显示 + 退出 | 修复权限或配置 |
| 设计文件写入失败 | tool 返回 `isError` | 模型重试或换路径 | 磁盘可写 |
| Profile/analytics 写入失败 | catch + warn | 无 tier 信息，fallback 到 introduction | 手动修复 `ODY_CODE_HOME` 权限 |
| gbrain 未配置 | tool 返回信息性输出 | 设计文档仅本地保存 | 配置 gbrain |

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify | Confirmed |
|---|---|---|---|---|---|---|
| 1 | ody-code CLI 入口支持解析 `--office-hours` 并传给 TUI bootstrap。 | [C:USER] | 无法启动专用模式；需要新增 CLI 解析。 | 检查 `apps/ody-code/src` 的入口与参数解析代码。 | ✓ |
| 2 | `SessionModeKind` 可从 `'plan' \| 'design'` 扩展为包含 `'office-hours'` 而不破坏 checkpoint/resume。 | [C:USER] | resume 可能失败；需要迁移旧 checkpoint。 | 检查 `packages/agent-core/src/session/checkpoint` 序列化代码。 | ✓ |
| 3 | `Agent.contexts` / `_fullCompactions` / `_microCompactions` 的 `Record<ModeKey, ...>` 可以通过新增 ModeKey 自动获得新分区。 | [C:USER] | 若未全部覆盖则运行时访问 undefined。 | 全局搜索 `ModeKey` 与 `'normal' \| 'plan' \| 'design'` 字面量。 | ✓ |
| 4 | `SlashCommandHost` / TUI bootstrap 可以通过初始化参数跳过普通 slash 命令注册，仅保留 office-hours 流程所需能力。 | [C:USER] | 可能需要在 TUI 中隐藏非相关命令或禁用切换。 | 检查 `apps/ody-code/src/tui` 启动与命令注册代码。 | ✓ |
| 5 | 用户 home 目录 `ODY_CODE_HOME` 默认存在且可写。 | [C:USER] | profile/analytics 写入失败。 | 复用现有 ody-code home 解析逻辑。 | ✓ |
| 6 | 设计文件路径由本设计自行指定；系统未显式分配路径，因此使用 `.ody-code/designs/2026-06-16-office-hours-port.md`。 | [C:USER] | 若路径冲突则设计无法保存。 | 本次设计即写入该路径；实际实现时由 host 分配。 | ✓ |
| 7 | gbrain 配置检测复用现有 `.gbrain/config.json` / `claude.json` MCP 检查逻辑。 | [C:USER] | artifacts sync 可能无法识别已配置环境。 | 检查上游 gstack artifacts sync 与 ody-code MCP/gbrain 集成现状。 | ✓ |

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | 新增 session kind 破坏 checkpoint 兼容性 | Medium | High | 在 checkpoint 序列化中处理未知 kind 的降级；添加迁移测试。 |
| 2 | office-hours prompt 过长，挤占上下文窗口 | High | Medium | 将 prompt 拆分为 injector 的 full/sparse 变体；仅激活时注入完整流程。 |
| 3 | 多轮 AskUserQuestion 导致用户中途退出，设计文档未生成 | Medium | Medium | 每次回答后自动写入中间状态；退出前提示保存草稿。 |
| 4 | builder profile / analytics 写入 ODY_CODE_HOME 与项目数据分离，导致跨项目识别失败 | Low | Medium | profile 以 device/user 维度索引，不依赖项目路径。 |
| 5 | CLAUDE.md routing 注入与项目现有内容冲突 | Low | Low | 追加前检查是否已有 `## Skill routing`，避免重复。 |
| 6 | 设计文件路径与人类设计文档同名冲突 | Low | Medium | 使用 `office-hours-` 前缀或独立子目录；实现时通过 `findUniqueStem` 去重。 |

## Self-Review

### Security
- **Checked**: 所有文件路径均限定在工作目录或 `ODY_CODE_HOME` 内，无任意文件写。CLAUDE.md routing 注入在追加前检查已有内容，不覆盖。artifact sync 仅通过 gbrain MCP tool 或 shell 命令，不直接操作远程。
- **Found**: 无已知安全漏洞。builder profile 存储项目 slug 但不存储用户数据之外的 PII。
- **Fixed**: nothing found.

### Test
- **Checked**: 每个 part 包含具体的 must-pass 测试（`expect(...).toThrow(...)`、`expect(mode).toBe('office-hours')`、`expect(entries).toHaveLength(1)`）。
- **Found**: Part 2 的 `ModeKey` 扩展影响 14 个文件；测试覆盖了 `SessionMode`、`OfficeHoursInjector`、tools 的核心行为，但 checkpoint migration 测试需要额外关注。
- **Fixed**: 将 `ModeKey` 扩展的完整文件列表补入 Part 2 的 Call-Site Integration（Code Audit 确认）。

### Ops
- **Checked**: office-hours 作为独立 CLI 入口，不与普通 shell 模式共存。设计文档写完自动退出，不留下悬挂进程。injector 使用与 design-mode 相同的 full/sparse cadence 防止 token 浪费。
- **Found**: `ToolManager` 构造时 `sessionMode` 未激活，静态条件注册不可行。修复为始终注册 + 运行时 `isError` 检查。
- **Fixed**: 更新 Part 2 和 Part 5 的工具注册策略。

### Integration
- **Checked**: 所有 call-site 都已验证存在且路径正确。`packages/agent-core/src/agent/index.ts` 的 `ModeKey`、`SessionMode` 类的 `enter()`/`exit()`、`InjectionManager` 构造器、`ToolManager.initializeBuiltinTools`、`apps/ody-code/src/cli/commands.ts`、`main.ts`、`run-shell.ts`、`tui/types.ts` 均有明确的插入点和代码示例。
- **Found**: `packages/agent-core/src/session/checkpoint/integrity.ts:15` 有独立的 `ModeKey` 类型定义，与 `src/agent/index.ts` 分开维护。已记录在 joint file list。
- **Fixed**: 未修改代码（此为设计文档），但所有 integration point 已在各 part 中标注。

### Scope
- **Checked**: 设计覆盖了 CLI 入口、session mode、injector、prompt contract、状态持久化、辅助集成 5 个子系统，全部有独立的 part 文件。索引文件不含 per-component 细节。
- **Found**: Scope In 包含 10 项，Out 包含 5 项，风险寄存器有 6 条，假设表有 7 条 [C:INFERRED]。所有 [C:USER] 决策已记录来源。
- **Fixed**: nothing found.

## User Final Approval

<!-- 留空，等待 ExitDesignMode 时由用户确认后填充或由 ExitDesignMode 流程管理 -->

