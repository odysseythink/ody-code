# Game Design Mode

## Scope

### In Scope

- 新增 CLI 入口 `--game-design`，仅通过该参数可进入 game-design 模式 [C:USER]。
- 新增会话模式 `game-design`，与 `office-hours` 平级 [C:USER]。
- 将上游游戏设计技能库（`skill.md` + 22 个主模块 + 11 个 companion 文件）以构建时嵌入方式引入 [C:USER]。
- `skill.md` 的核心流程与索引常驻注入到 game-design 上下文 [C:USER]。
- 22 个主模块与 11 个 companion 文件作为 `game-design/<name>` 命名空间下的 Skill 注册，仅在 `game-design` 模式下可见 [C:USER]。
- 模式运行形态仿 office-hours：受限单目标模式，退出时回到 `normal`，不支持向 plan/design 接力 [C:USER]。
- 产物形式：一份主设计文档 `game-design.md`，模型可创建同目录下的附属 `.md` 文件 [C:USER]。
- 工具集合：完整复刻 office-hours 工具概念，适配为 game-design 语义（enter/exit、语言、profile、learnings、routing、artifact sync） [C:USER]。
- 状态持久化：所有状态与产物均存放在当前项目的 `.ody-code/game-design/` 下 [C:USER]。
- UI/标签语言跟随现有 i18n 体系与用户 `userLanguage` [C:USER]。
- 新增独立 telemetry 事件 `game_design_started` / `game_design_completed` [C:USER]。
- 默认启用，不放在实验性 flag 后 [C:USER]（覆盖 AGENTS.md 默认实验性门控建议）。

### Out of Scope / Deferred

- 不支持通过 `--session-mode game-design` 进入；仅 `--game-design` 参数入口 [C:USER]。
- 不支持 game-design 与 plan/design 之间的接力 [C:USER]。
- 不迁移 office-hours 的全局 `~/.ody-code/office-hours/` 状态存储路径；game-design 仅使用项目级存储 [C:USER]。
- 不对 33 个 .md 文件做语义切分或向量化检索；全部按 Skill 整篇加载 [C:INFERRED]。
- 首次实现不暴露用户可配置的 `modeModels.game-design` 默认值；用户可自行在 `config.toml` 中设置，代码仅复用现有 `modeModels[kind]` 读取逻辑 [C:INFERRED]。

## 上游来源说明

- 上游技能库路径：`/Users/ranwei/workspace/game_work/53ad898cdbc8734d8bb5c6a6ddf5cec4-0a2eae1c91f9a06a081de73f92f6ed86fbce1194` [C:UPSTREAM]。
- 内容基于《游戏设计的100个原理》体系，包含 6 大维度、22 个专项能力、11 个 companion 文件 [C:UPSTREAM]。
- 具体模块清单见 Part 4 `skills.md`。

## 架构概览

```
用户执行 `ody-code --game-design`
  → CLI (apps/ody-code/src/cli)
      解析 --game-design，校验冲突，强制 uiMode='shell'
      → runGameDesign()
  → TUI (apps/ody-code/src/tui)
      创建 OdyTUI，cliOptions.sessionMode='game-design'
      → session.setSessionMode('game-design')
  → SDK / RPC (packages/node-sdk, packages/agent-core/src/rpc)
      透传 mode 到 agent.rpcMethods.enterPlan({ kind: 'game-design' })
  → Agent Core (packages/agent-core/src/agent)
      SessionMode.enter('game-design')
        → 解析 .ody-code/game-design/ 目录
        → 切换 context 分区到 'game-design'
        → 若 modeModels['game-design'] 配置可用则切换模型
      InjectionManager 每轮注入 GameDesignInjector
        → 注入 skill.md 核心流程 + 可用 Skill 清单 + 产物路径提醒
      ToolManager 暴露 game-design 专用工具
        → Enter/Exit、语言、profile、learnings、routing、artifact sync
      SkillManager 注册 game-design/* Skill（hiddenInModes 排除其他模式）
  → 模型调用 Write/Edit 受限在 .ody-code/game-design/ 文件集
  → 用户 /exit 或模型 ExitGameDesignMode 退出
      → 回到 normal 模式，产物保留在项目目录
```

## Parts 清单

| # | 文件 | 范围 | 状态 |
|---|---|---|---|
| 1 | [2026-06-18-game-design-mode/agent-core.md](2026-06-18-game-design-mode/agent-core.md) | SessionMode、注入器、工具、状态存储 | done |
| 2 | [2026-06-18-game-design-mode/cli.md](2026-06-18-game-design-mode/cli.md) | CLI 选项、runner、校验、telemetry | done |
| 3 | [2026-06-18-game-design-mode/tui.md](2026-06-18-game-design-mode/tui.md) | TUI 类型、徽章、命令可见性、快捷键 | done |
| 4 | [2026-06-18-game-design-mode/skills.md](2026-06-18-game-design-mode/skills.md) | 技能库构建嵌入、注册、命名空间、可见性 | done |

## Data Models

跨组件关键数据结构定义在各 part 中：

| 数据结构 | 定义位置 | 关键字段 |
|---|---|---|
| `CLIOptions`（扩展） | [cli.md](2026-06-18-game-design-mode/cli.md) | `gameDesign: boolean` |
| `TUIStartupOptions` / `AppState`（扩展） | [tui.md](2026-06-18-game-design-mode/tui.md) | `sessionMode: '...' \| 'game-design'` |
| `SessionModeKind` / `ModeKey`（扩展） | [agent-core.md](2026-06-18-game-design-mode/agent-core.md) | 新增 `'game-design'` |
| `GameDesignStateStore` | [agent-core.md](2026-06-18-game-design-mode/agent-core.md) | `appendProfile`, `readProfile`, `appendLearning`, `searchLearnings`, `appendAnalytics`, `getSessionSummary` |
| `GameDesignProfileEntry` | [agent-core.md](2026-06-18-game-design-mode/agent-core.md) | `date, mode, projectSlug, pillars, audience, platform, genre, signals, designDoc` |
| `GameDesignLearningEntry` | [agent-core.md](2026-06-18-game-design-mode/agent-core.md) | `ts, skill, type, key, insight, confidence, source, branch` |
| `SkillDefinition`（生成实例） | [skills.md](2026-06-18-game-design-mode/skills.md) | `name: 'game-design/<stem>', hiddenInModes: ['normal','plan','design','office-hours']` |

## Algorithms

| 算法 | 定义位置 | 摘要 |
|---|---|---|
| `generateGameDesignSkillsIndex` | [skills.md](2026-06-18-game-design-mode/skills.md) | 扫描 sources/*.md，生成 import、SkillDefinition、registerGameDesignSkills |
| `buildSkillDefinition` | [skills.md](2026-06-18-game-design-mode/skills.md) | 调用 parseSkillText，设置名称、namespace、hiddenInModes |
| `extractWorkflow` | [skills.md](2026-06-18-game-design-mode/skills.md) | 从 skill.md 提取 Phase 1-8 工作流内容供注入器使用 |
| `GameDesignInjector.getInjection` | [agent-core.md](2026-06-18-game-design-mode/agent-core.md) | 根据激活状态、文件内容、变体决定注入 entry/reentry/full/sparse/exit reminder |
| `GameDesignInjector.getVariant` | [agent-core.md](2026-06-18-game-design-mode/agent-core.md) | 基于 assistant turn 数决定 full/sparse/null 变体 |
| `resolveSessionModeDirectory` 扩展 | [agent-core.md](2026-06-18-game-design-mode/agent-core.md) | `kind === 'game-design'` 映射到 `.ody-code/game-design/` |
| `validateOptions` 扩展 | [cli.md](2026-06-18-game-design-mode/cli.md) | `--game-design` 与 prompt/session/continue/sessionMode/yolo/auto/officeHours 冲突检测 |
| `runGameDesign` | [cli.md](2026-06-18-game-design-mode/cli.md) | 专用启动流程，telemetry 事件 `game_design_*` |
| `isCommandVisibleInMode` | [tui.md](2026-06-18-game-design-mode/tui.md) | 使用 `SPECIAL_MODE_HIDDEN` 在 office-hours 与 game-design 中隐藏相同命令 |
| `FileSystemGameDesignStateStore.searchLearnings` | [agent-core.md](2026-06-18-game-design-mode/agent-core.md) | 读取 JSONL，过滤 branch，返回最近 limit 条 |

## Error Handling

各 part 已包含详细 Error & Degradation 表。跨组件关键路径总结：

| 场景 | 立即处理 | 降级路径 | 恢复条件 |
|---|---|---|---|
| `--game-design` 与 `--prompt`/`--session`/`--continue`/`--session-mode`/`--yolo`/`--auto`/`--office-hours` 冲突 | `OptionConflictError` → stderr + exit 1 | 无 | 用户修正参数 |
| `SessionMode.enter('game-design')` 目录创建失败 | catch 中恢复模型、重置状态、抛出；可回退 homedir | 使用 `~/.ody-code/game-design/` | 修复项目目录权限 |
| 状态存储写入失败 | 工具返回 `isError`，日志 warn | 无 | 修复目录权限后重试 |
| 注入器读取设计文件失败 | catch 返回空字符串 | 无 | 文件后续被模型写入 |
| 模型尝试写入模式文件集外路径 | `PlanModeGuardDeny` 拒绝 | 无 | 用户让模型写入 `.ody-code/game-design/` 内 |
| `setSessionMode('game-design')` 失败 | TUI 显示错误状态，保持 normal | 无 | 重试或退出重进 |

## Self-Review

### 高影响决策验证

**D1. Skill 可见性过滤规则**

规则：生成的 game-design Skill 设置 `hiddenInModes: ['normal','plan','design','office-hours']`，依赖 `listInvocableSkills` 的现有逻辑：

```
hidden = sessionMode !== 'normal' && hiddenInModes.includes(sessionMode)
```

验证用例（已通过 `node -e` 执行）：

| 输入 sessionMode | 预期 | 结果 |
|---|---|---|
| 'normal' | visible | visible |
| 'plan' | hidden | hidden |
| 'design' | hidden | hidden |
| 'office-hours' | hidden | hidden |
| 'game-design' | visible | visible |

**D2. `--game-design` 冲突选项**

校验分支必须拒绝的组合：

| 输入 | 预期 | 说明 |
|---|---|---|
| `{ gameDesign: true, prompt: 'x' }` | 冲突错误 | prompt 与专用模式互斥 |
| `{ gameDesign: true, officeHours: true }` | 冲突错误 | 两个专用模式互斥 |
| `{ gameDesign: true, yolo: true }` | 冲突错误 | 权限模式固定 manual |
| `{ gameDesign: true, sessionMode: 'design' }` | 冲突错误 | session-mode 与专用模式互斥 |
| `{ gameDesign: true }` | 通过，uiMode='shell' | 合法入口 |

**D3. 模式文件集写入守卫**

`SessionMode.isWritableSessionModePath` 规则：

| 输入 path | game-design 主文件路径 | 预期 | 说明 |
|---|---|---|---|
| `.ody-code/game-design/2026-06-18-topic.md` | 同左 | 允许 | 主文件 |
| `.ody-code/game-design/2026-06-18-topic/appendix.md` | `.ody-code/game-design/2026-06-18-topic.md` | 允许 | 同 stem 子目录 |
| `src/index.ts` | `.ody-code/game-design/2026-06-18-topic.md` | 拒绝 | 不在文件集内 |
| `.ody-code/game-design/other.md` | `.ody-code/game-design/2026-06-18-topic.md` | 拒绝 | 非主文件且不在 stem 子目录 |

### 四镜审视

- **Security**：检查 `hiddenInModes` 无 false negative（game-design 模式下必须可见）和 false positive（normal 模式下必须隐藏）。已用 `node -e` 验证。状态存储仅写入项目目录 `.ody-code/game-design/`，无敏感信息上报告警。telemetry 事件名中不含 PII。
- **Test**：每个行为均给出 must-pass 与 must-reject 断言。D1 的 must-reject 用例（'plan' 可见）与 must-pass 用例（'game-design' 可见）不相矛盾。Write 守卫的 must-reject 用例 `src/index.ts` 不会被允许。
- **Ops**：状态存储使用 append-only JSONL，重复进入不会覆盖历史；目录名 `.ody-code/game-design/` 与模式名一致。33 个 .md 文件构建内联可能增加 bundle size，已在 Risk Register 中记录监控措施。
- **Integration**：`SessionModeKind`、`ModeKey`、`SkillRegistry.hiddenInModes`、`raw-text-plugin`、`OdyTUIStartupInput.cliOptions.sessionMode`、`InjectionManager` 注入器数组均已在代码中验证存在。设计落地路径为用户指定的 `--game-design` CLI 参数，未静默改向。
- **Scope**：本设计仍是一个连贯的 CLI 模式新增任务，按 CLI/TUI/AgentCore/Skills 拆分为 Part 文件，未拆成独立项目。

## User Final Approval

- 审计级别：Deep [C:USER]
- [C:INFERRED] 假设签批：全部接受（2026-06-18） [C:USER]
- 最终设计批准：待 ExitDesignMode 确认

## Reuse Analysis

| # | 源文件 | 可复用组件 | 使用方式 |
|---|---|---|---|
| 1 | `packages/agent-core/src/agent/session-mode/index.ts` | `SessionModeKind`, `SessionMode.enter/exit/cancel`, `resolveSessionModeDirectory` | 扩展联合类型，新增 `'game-design'` 分支 [C:INFERRED] |
| 2 | `packages/agent-core/src/agent/index.ts` | `ModeKey`, `_contexts/_fullCompactions/_microCompactions`, `setContextMode`, `rpcMethods.enterPlan` | 扩展 `ModeKey`，新增 context 分区；`enterPlan` 已接受任意 `SessionModeKind` [C:INFERRED] |
| 3 | `packages/agent-core/src/agent/injection/office-hours.ts` | `OfficeHoursInjector` 的变体调度算法 | 复制结构实现 `GameDesignInjector`，调整 reminder 内容 [C:INFERRED] |
| 4 | `packages/agent-core/src/tools/builtin/office-hours/*.ts` | Enter/Exit/Profile/Learning/Routing/Artifact 工具 | 复制并改名为 game-design 语义 [C:USER] |
| 5 | `packages/agent-core/src/skill/registry.ts` | `registerBuiltinSkill`, `listInvocableSkills` 的 `hiddenInModes` 过滤 | 直接复用；为 game-design Skill 设置 `hiddenInModes: ['normal','plan','design','office-hours']` [C:INFERRED] |
| 6 | `build/raw-text-plugin.mjs` + `register-raw-text-loader.mjs` | .md/.yaml 构建时内联 | 复用；通过生成脚本产出 `index.ts` 统一 import 所有技能 Markdown [C:INFERRED] |
| 7 | `apps/ody-code/src/cli/run-office-hours.ts` | runner 结构与 telemetry 模式 | 复制为 `run-game-design.ts`，事件名改为 `game_design_*` [C:USER] |
| 8 | `apps/ody-code/src/tui/commands/registry.ts` | `OFFICE_HOURS_HIDDEN` 常量 | 扩展为 `SPECIAL_MODE_HIDDEN: ['office-hours', 'game-design']` [C:INFERRED] |
| 9 | `apps/ody-code/src/tui/components/chrome/footer.ts` | 模式徽章与本地化 | 增加 `game-design` 分支 [C:INFERRED] |

## Assumptions & Unverified Items

| # | Assumption | Confidence | Impact if wrong | How to verify |
|---|---|---|---|---|
| 1 | `SkillDefinition.name` 允许包含 `/` 字符（`game-design/flow-state`） | Medium | Skill 注册或调用失败 | 在 `skill-tool.test.ts` 或临时脚本中注册并调用一个带 `/` 名称的 Skill |
| 2 | `raw-text-plugin.mjs` 能在 `packages/agent-core` 的构建配置中被复用 | High | 构建时 .md 导入失败 | 检查 `tsup` / `tsdown` 配置是否已使用该插件；写一个临时 import 测试 |
| 3 | 33 个 .md 文件的总字符量不会导致 bundle 显著膨胀或 context 溢出 | Medium | 包体积过大、注入 token 过多 | 统计总字符数；评估是否需拆分注入 |
| 4 | 项目级状态存储路径 `.ody-code/game-design/` 在首次进入时即可创建 | High | 状态写入失败 | 运行进入工具测试，断言目录存在 |
| 5 | 用户接受 `--game-design` 与 `--office-hours` 互斥 | High | CLI 行为冲突 | 已在选项冲突中明确 [C:INFERRED] |
| 6 | 不将新模式放在实验性 flag 后不会引发发布流程问题 | Medium | 违反项目默认规范 | 与维护者确认；本设计记录用户决策 [C:USER] |
| 7 | `PlanModeGuardDeny` 通过识别 `sessionMode.kind` 即可限制写入范围 | High | game-design 模式下 Write/Edit 权限过宽 | 检查 policy 实现并新增测试 |
| 8 | 产物主文档命名为 `game-design.md`（由 `SessionMode.resolveFilePathFromContent` 基于内容/topic 生成） | Medium | 产物文件名不符合预期 | 在测试中断言 path 包含 `.ody-code/game-design/` |

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | 33 个 .md 文件构建后导致 `agent-core` bundle 过大 | Medium | 安装包体积增加、加载慢 | 构建后监控 bundle size；如过大改为运行时按需加载 companion 文件 |
| 2 | `game-design/` Skill 命名与用户/项目 Skill 冲突 | Low | 同名 Skill 被覆盖或调用错误 | 使用 `game-design/` 前缀；builtin 注册时允许被用户版本覆盖或报错，需在实现时决定 |
| 3 | 注入的完整 skill.md 内容超出模型上下文预算 | Medium | 模型截断、性能下降 | 仅注入核心流程+索引，子模块作为 Skill 按需调用；监控 token 使用 |
| 4 | 项目级状态存储在团队协作时产生冲突 | Low | 多人同时写入 learnings/profile | 使用 append-only JSONL；避免覆盖写入 |
| 5 | 未正确隐藏 game-design Skill 导致 normal 模式下也能调用 | Medium | 行为泄漏、token 浪费 | 单元测试覆盖 `listInvocableSkills('normal')` 不包含 `game-design/*` |
| 6 | 默认启用跳过实验性 flag 违反团队规范 | Low | 发布受阻 | 在 design 中记录用户明确决策；实现前可与维护者二次确认 |

## User Final Approval

待通过 ExitDesignMode 流程收集最终批准后回填。
