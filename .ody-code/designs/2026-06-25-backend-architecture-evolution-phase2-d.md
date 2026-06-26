# Phase 2-D：Mode 概念统一与 ModeBehavior 重构

> **Document Type**: Detailed Design · **Audit Level**: Deep · **Status**: Draft

## 执行摘要

Phase 2-D 要厘清 `SessionModeKind`（交互阶段）与 agent `profile`（角色/工具集/system prompt）的层次关系，并通过 `SessionModeBehavior` 策略对象统一四个 session mode 的行为，产出架构文档与类型收敛，为后续 `office-hours` 拆包与 Rust Host 反转扫清概念混淆。

## Resolved decisions

1. **Scope** [C:USER]: 概念层 + 文档 + 类型收敛 + 轻量重构，进一步采用 `ModeBehavior` 策略对象统一 enter/exit/handoff。
2. **Data** [C:USER]: 严格两层类型：`SessionModeKind`（4 种交互阶段）与 `RuntimeMode`（含 `normal`）。
3. **Integration** [C:USER]: 从 `packages/agent-core/src/agent/session-mode/index.ts` 统一导出，全面替换相关字面量。
4. **Error** [C:USER]: 运行时防御 + 测试修复；未知 mode 字符串回退到 `'normal'` 并 warn。
5. **Security** [C:USER]: 类型封闭 + 禁止从用户输入动态构造 mode。
6. **Observability** [C:USER]: 文档 + 类型测试 + 反向审查检查单。
7. **Ops** [C:USER]: 无 feature flag，`OdyConfig.sessionMode`/`defaultSessionMode` 直接扩为 `RuntimeMode` 并保持向后兼容。
8. **Refactor boundary** [C:USER]: `ModeBehavior` 策略对象，统一注入变体调度 + 生命周期钩子，并进一步统一 enter/exit/handoff。
9. **Architecture** [C:USER]: `SessionMode` 退化为调度器；`ModeBehaviorRegistry` 注册各 mode 的 behavior；`SessionModeInjector` 抽象基类统一注入逻辑。

## Scope

### Scope In

1. [C:USER] 定义两层类型 `SessionModeKind` 与 `RuntimeMode`，从 `agent/session-mode` 统一导出。
2. [C:USER] 替换 `Agent.ModeKey`、`SystemPromptContext.sessionMode`、`SkillCatalog` 参数、`CoreAPI/SessionAPI` 中所有 mode 字符串字面量。
3. [C:USER] 扩展 `agent-core-shared` 的 `OdyConfig.sessionMode`/`defaultSessionMode` schema 到 `RuntimeMode`。
4. [C:USER] 引入 `SessionModeBehavior<TKind>` 策略接口与注册表，把 plan/design/office-hours/game-design 的目录解析、模型 key、handoff 目标、注入器类、状态副作用纳入 behavior。
5. [C:USER] 提取 `SessionModeInjector` 抽象基类，统一 `full/sparse/reentry` 变体调度、`onContextClear` 状态记忆、`inject()` 模板方法。
6. [C:USER] 新增 `docs/architecture/modes-vs-profiles.md` 与反向检查单。
7. [C:USER] 新增类型与行为测试，覆盖类型守卫、注册表、注入器调度、状态原子性。

### Scope Out

1. [C:USER] 不改 RPC 契约方法名/签名（只改类型引用）。
2. [C:USER] 不改 profile 加载/继承逻辑（`packages/agent-core/src/profile/resolve.ts`）。
3. [C:USER] 不改 mode-specific 的 contract 文本内容（`plan-mode-contract.ts`、`design-mode-contract.ts` 等）。
4. [C:USER] 不改 office-hours/game-design state store 实现（`packages/agent-core/src/office-hours/state.ts`）。
5. [C:USER] 不新增 mode（本次只降低未来新增成本）。
6. [C:USER] 不迁移到 Rust/Wasm（属于后续 H 轨道）。
7. [C:USER] 不改 TUI 中 mode 的展示字符串（`apps/ody-code`）。
8. [C:INFERRED] 不改 `modeModels` 的键命名（`officeHours`/`gameDesign` 等），只通过 behavior 的 `modeModelKey` 字段映射，避免配置层破坏性变更。

## Architecture

```
Agent
├── sessionMode: SessionMode        [调度器]
├── skills: SkillManager
├── tools: ToolManager
├── injection: InjectionManager
└── useProfile(profile, context)
    └── SystemPromptContext.sessionMode: RuntimeMode

SessionMode
├── kind: SessionModeKind
├── behavior: SessionModeBehavior   [由 ModeBehaviorRegistry 解析]
├── enter()  → behavior.onEnter()
├── exit()   → behavior.onExit()
├── cancel() → behavior.onCancel()
└── handoffTo(target)

ModeBehaviorRegistry
├── register<T>(behavior)
├── resolve(kind) → SessionModeBehavior
└── kinds: readonly SessionModeKind[]

SessionModeBehavior (per mode)
├── outputSubdirectory
├── modeModelKey
├── injectorClass
├── handoffTarget
├── onEnter(ctx)
├── onExit(ctx)
└── onCancel(ctx)

SessionModeInjector (abstract)
├── full/sparse/reentry 变体调度
├── onContextClear 状态记忆
└── inject() 模板方法
```

数据流：
- `Agent.useProfile(profile, context)` → `profile.systemPrompt(context)`，其中 `context.sessionMode` 为当前 `RuntimeMode`。
- `SkillRegistry.getModelSkillListing(sessionMode)` → 按 `hiddenInModes` 过滤技能。
- `InjectionManager` → 从当前 behavior 取 `injectorClass` 实例化，每步调用 `inject()`。
- `SessionMode.enter(kind)` → `registry.resolve(kind)` → `behavior.onEnter()` → 目录解析 / model 切换 / context 分区。

## Data Models

### Mode 类型层 [C:USER]

```ts
// packages/agent-core/src/agent/session-mode/types.ts (new)

export const SESSION_MODE_KINDS = ['plan', 'design', 'office-hours', 'game-design'] as const;
export type SessionModeKind = typeof SESSION_MODE_KINDS[number];

export const RUNTIME_MODES = [...SESSION_MODE_KINDS, 'normal'] as const;
export type RuntimeMode = typeof RUNTIME_MODES[number];

export function isSessionModeKind(value: string): value is SessionModeKind;
export function isRuntimeMode(value: string): value is RuntimeMode;
export function normalizeRuntimeMode(value: string): RuntimeMode;
// 未知字符串回退到 'normal' 并 warn log。
```

### Behavior 与 Injector 接口 [C:USER]

```ts
export interface ModeEnterContext {
  agent: Agent;
  id: string;
  restoreTargetAlias: string | undefined;
}

export interface ModeExitContext {
  agent: Agent;
  id?: string;
  sessionModeFilePath: string | null;
}

export interface SessionModeBehavior<TKind extends SessionModeKind> {
  readonly kind: TKind;
  readonly outputSubdirectory: string;                 // e.g. 'plans', 'designs'
  readonly modeModelKey: string;                        // e.g. 'plan', 'officeHours'
  readonly injectorClass: new (agent: Agent) => SessionModeInjector;
  readonly handoffTarget?: 'plan' | 'normal';           // design→plan, plan→normal
  readonly supportsDesignSessions?: boolean;            // only design tracks sessions

  onEnter(ctx: ModeEnterContext): Promise<void> | void;
  onExit(ctx: ModeExitContext): Promise<void> | void;
  onCancel(ctx: ModeExitContext): Promise<void> | void;
}

export interface SessionModeInjector {
  readonly injectionVariant: string;
  onContextClear(): void;
  inject(): Promise<void>;
  getInjection(): string | Promise<string | undefined> | undefined;
}

export interface SessionModeInjectorOptions {
  fullRefreshTurns: number;
  dedupMinTurns: number;
}
```

### 类型替换约定 [C:USER]

- `Agent.ModeKey` → 改名为 `Agent.RuntimeMode`，引用 `RuntimeMode`。
- `SystemPromptContext.sessionMode` → `RuntimeMode`。
- `SkillCatalog.listInvocableSkills(sessionMode?: RuntimeMode)`。
- `SkillCatalog.getModelSkillListing(sessionMode?: RuntimeMode)`。
- `SkillCatalog.getUnavailableSkillsReminder(sessionMode: RuntimeMode)` [C:INFERRED] 从仅 `plan|design` 放宽到 `RuntimeMode`，因为设计统一后所有 runtime mode 都可查询隐藏技能。
- `CoreAPI.listSkills` / `SessionAPI.listSkills` 的 `sessionMode` 参数 → `RuntimeMode`。
- `OdyConfig.sessionMode` / `defaultSessionMode` schema → `RuntimeMode`（仍兼容旧值 `'plan' | 'design'`）。

## Algorithms

### SessionMode.enter() 模板 [C:USER]

```
enter(id, createFile=false, emitStatus=true, kind='plan'):
  if isActive and kind == currentKind:
    return
  if isActive:
    exit()

  behavior = registry.resolve(kind)
  restoreTargetAlias = config.modelAlias

  // 先设置内部状态，再调用 behavior，便于 behavior 读取当前 kind/path
  isActive = true
  kind = kind
  sessionModeId = id
  sessionModeFilePath = null

  try:
    behavior.onEnter({ agent, id, restoreTargetAlias })
      // 默认由 BaseSessionModeBehavior 处理：
      // 1. resolve output directory via outputSubdirectory
      // 2. ensure .gitignore if project-scoped
      // 3. switch model via modeModels[modeModelKey] if usable auth
    setContextMode(kind)
    logRecord({ type: 'session_mode.enter', id, kind })
  catch error:
    rollbackState()
    restoreModelIfNeeded()
    setContextMode('normal')
    throw error

  if emitStatus:
    emitStatusUpdated()
```

### SessionMode.exit() 模板 [C:USER]

```
exit(id):
  if not isActive:
    return

  behavior = registry.resolve(currentKind)
  restore pre-mode model alias if captured

  behavior.onExit({ agent, id, sessionModeFilePath })
    // design: closeCurrentDesignSession, set lastCompletedDesignFilePath
    // others: 默认无额外副作用

  logRecord({ type: 'session_mode.exit', id })
  setContextMode('normal')
  replayBuilder.push({ type: 'session_mode_updated', enabled: false, kind })
  clear internal state
  emitStatusUpdated()
```

### SessionModeInjector 变体调度 [C:USER]

```
abstract class SessionModeInjector {
  protected injectedAt: number | null = null
  protected wasActive = false

  abstract readonly injectionVariant: string
  abstract readonly options: SessionModeInjectorOptions
  abstract isModeActive(): boolean

  // reminder 钩子
  abstract getEntryReminder(path: string | null): string
  abstract getReentryReminder(path: string | null): string
  abstract getFullReminder(path: string | null): string
  abstract getSparseReminder(path: string | null): string
  abstract getExitReminder(path: string | null): string

  // 子类可覆盖以传入 contract 文本所需上下文
  getExtraContext(): Record<string, unknown> { return {} }

  onContextClear():
    injectedAt = null
    wasActive = isModeActive()

  async inject():
    injection = await getInjection()
    if injection !== undefined:
      injectedAt = agent.context.history.length
      agent.context.appendSystemReminder(injection, {
        kind: 'injection',
        variant: injectionVariant,
      })

  async getInjection():
    active = isModeActive()
    path = agent.sessionMode.sessionModeFilePath

    if not active:
      if not wasActive:
        return undefined
      wasActive = false
      injectedAt = null
      return getExitReminder(path)

    if not wasActive:
      wasActive = true
      injectedAt = null
      content = await readModeContent()
      return content.trim().length > 0
        ? getReentryReminder(path)
        : getEntryReminder(path)

    variant = computeVariant(injectedAt, agent.context.history, options)
    if variant == null:
      return undefined
    if variant == 'reentry':
      return getReentryReminder(path)
    return variant == 'full'
      ? getFullReminder(path)
      : getSparseReminder(path)

  computeVariant(injectedAt, history, options):
    if injectedAt == null:
      return 'full'
    assistantTurnsSince = 0
    for i from injectedAt + 1 to history.length - 1:
      msg = history[i]
      if msg.role == 'assistant':
        assistantTurnsSince += 1
        continue
      if msg.role == 'user':
        return 'full'
    if assistantTurnsSince >= options.fullRefreshTurns:
      return 'full'
    if assistantTurnsSince >= options.dedupMinTurns:
      return 'sparse'
    return null
}
```

### ModeBehaviorRegistry [C:USER]

```
class ModeBehaviorRegistry {
  private behaviors = new Map<SessionModeKind, SessionModeBehavior<SessionModeKind>>()

  register<T extends SessionModeKind>(behavior: SessionModeBehavior<T>): void
    behaviors.set(behavior.kind, behavior)

  resolve(kind: SessionModeKind): SessionModeBehavior<SessionModeKind>
    behavior = behaviors.get(kind)
    if behavior == undefined:
      throw new OdyError(ErrorCodes.INTERNAL_ERROR, `Unknown session mode kind: ${kind}`)
    return behavior

  get kinds(): readonly SessionModeKind[]
    return Array.from(behaviors.keys())
}

// packages/agent-core/src/agent/session-mode/behaviors/index.ts
function createDefaultModeBehaviorRegistry(): ModeBehaviorRegistry {
  const registry = new ModeBehaviorRegistry()
  registry.register(new PlanModeBehavior())
  registry.register(new DesignModeBehavior())
  registry.register(new OfficeHoursModeBehavior())
  registry.register(new GameDesignModeBehavior())
  return registry
}
```

### BaseSessionModeBehavior 默认 onEnter [C:USER]

```
class BaseSessionModeBehavior<TKind extends SessionModeKind>
  implements SessionModeBehavior<TKind> {

  onEnter(ctx: ModeEnterContext):
    { dir, isProjectScoped } = await resolveSessionModeDirectory(this.outputSubdirectory, ctx.agent)
    if isProjectScoped:
      await ensureGitignore(ctx.agent.config.cwd)

    modeModelAlias = ctx.agent.kimiConfig?.modeModels?.[this.modeModelKey]
    if modeModelAlias != undefined and modelAliasHasUsableAuth(modeModelAlias):
      ctx.agent.config.update({ modelAlias: modeModelAlias })
      ctx.agent.refreshLlm()
      capturePreModeModelAlias(ctx.restoreTargetAlias)
}
```

## Error Handling

| 错误类别 | 立即处理 | 降级路径 | 恢复条件 |
|---|---|---|---|
| 未知 mode 字符串（历史记录/配置） [C:USER] | `normalizeRuntimeMode()` 回退到 `'normal'` 并 warn | 以 normal 模式继续运行 | 修复配置/记录后重启 |
| `ModeBehaviorRegistry.resolve(kind)` 未找到 [C:USER] | 抛 `OdyError(ErrorCodes.INTERNAL_ERROR)` | 不可恢复，属于代码缺陷 | 修复注册表 |
| `SessionModeInjector` 子类未实现抽象方法 [C:USER] | TypeScript 编译错误 | 不进入运行时 | 子类实现完整 |
| Behavior.onEnter 抛错 [C:USER] | `SessionMode.enter` 捕获并回滚状态（恢复模型、清除 path、setContextMode('normal')） | 保持在 normal 模式 | 解决触发错误的外部条件后重试 |
| Behavior.onExit/onCancel 抛错 [C:INFERRED] | 记录 error，继续清理内部状态 | 可能留下 context mode 未恢复；下次 `enter` 前强制 `setContextMode('normal')` | 手动或自动恢复 context mode |
| `OdyConfig.sessionMode` 非法 [C:USER] | schema 校验失败，启动时报 `CONFIG_INVALID` | 使用默认配置启动 | 用户修正 config.toml |

补充约定 [C:USER]：
- `SessionMode.enter()` 失败时必须保持调用前状态不变（原子性）。
- `cancel()` 与 `exit()` 必须幂等：多次调用不重复 `logRecord`。
- `DynamicInjector.onContextCompacted/onContextMessageRemoved` 已在基类实现，子类不覆盖。

## Testing

### 新增/修改测试文件 [C:USER]

1. `packages/agent-core/src/agent/session-mode/__tests__/mode-types.test.ts`（新）
   - 断言 `SESSION_MODE_KINDS` 严格等于 `['plan','design','office-hours','game-design']`。
   - 断言 `RUNTIME_MODES` 严格等于 `[...SESSION_MODE_KINDS, 'normal']`。
   - 断言 `isRuntimeMode('office-hours') === true`。
   - 断言 `isRuntimeMode('foo') === false`。
   - 断言 `normalizeRuntimeMode('foo') === 'normal'` 并触发 warn log。

2. `packages/agent-core/src/agent/session-mode/__tests__/behaviors.test.ts`（新）
   - 断言 `ModeBehaviorRegistry.resolve('design')` 返回 `DesignModeBehavior`。
   - 断言未注册 kind 调用 `resolve()` 抛 `INTERNAL_ERROR`。
   - 断言四个 behavior 的 `outputSubdirectory` 分别为 `plans`/`designs`/`products`/`game-design`。
   - 断言 `DesignModeBehavior.handoffTarget === 'plan'`、`PlanModeBehavior.handoffTarget === 'normal'`、其余为 `undefined`。
   - 断言 `DesignModeBehavior.supportsDesignSessions === true`、其余为 `false`。

3. `packages/agent-core/src/agent/injection/__tests__/session-mode-injector.test.ts`（新）
   - 用最小 fake injector 覆盖 `computeVariant`：
     - `injectedAt = null` → `'full'`。
     - 1 个 assistant turn → `null`。
     - `DEDUP_MIN_TURNS` 个 assistant turns → `'sparse'`。
     - `FULL_REFRESH_TURNS` 个 assistant turns → `'full'`。
     - 中间出现 user message → `'full'`。
   - 断言 `onContextClear` 正确记忆 `wasActive`。

4. `packages/agent-core/src/agent/session-mode/__tests__/session-mode.test.ts`（修改/新增）
   - 断言 `enter()` 失败时状态原子回滚（`isActive`、`kind`、`sessionModeFilePath`、模型别名均不变）。
   - 断言 `exit()` 幂等：连续调用两次只产生一条 `session_mode.exit` 记录。

5. 类型级测试（通过 `pnpm typecheck`）
   - `Agent.ModeKey`、`SystemPromptContext.sessionMode`、`SkillCatalog` 参数、`CoreAPI.listSkills` 等位置无 `string` 字面量；全部引用 `RuntimeMode`。

### 文档与验收 [C:USER]

- `docs/architecture/modes-vs-profiles.md` 必须包含：
  - 一句话定义：mode 是交互阶段，profile 是角色/工具集/system prompt。
  - 决策矩阵：加新模式改哪些文件；加新 profile 改哪些文件。
  - 当前 4 个 mode 的职责与 handoff 关系图（design → plan → normal）。
  - `SystemPromptContext.sessionMode` 的使用规范：只用于与交互阶段相关的提示，不用于选择 profile。
- 反向检查单：附 5 道自测题，新人应能凭文档回答。

### Done Criteria [C:USER]

```bash
pnpm typecheck
pnpm test packages/agent-core/src/agent/session-mode/__tests__
pnpm test packages/agent-core/src/agent/injection/__tests__
```

所有新增与既有测试通过。

## Risk Register

| 编号 | 风险 | 可能性 | 影响 | 缓解 |
|---|---|---|---|---|
| R1 [C:INFERRED] | ModeBehavior 抽象过早，未来新增 mode 时 behavior 接口不稳定 | 中 | 中 | 基类提供默认实现；接口只暴露当前 4 个 mode 真正需要的钩子；变更通过新增可选字段兼容。 |
| R2 [C:INFERRED] | `SessionMode` 重构破坏 resume/replay 顺序（context 分区、logRecord 顺序） | 中 | 高 | 黄金测试：重构前后同一消息流的 `session_mode.enter/exit/cancel` 与 `session_mode_updated` 记录逐字节一致；重点测 direct plan↔design 切换。 |
| R3 [C:INFERRED] | 类型替换遗漏某些调用点，导致 `string` 与 `RuntimeMode` 混用 | 中 | 中 | `pnpm typecheck` 为硬门；新增类型测试枚举所有公开 API 的 mode 参数。 |
| R4 [C:INFERRED] | `BaseSessionModeBehavior` 默认 onEnter 无法覆盖 design 的 `startDesignSession` 等副作用 | 低 | 中 | design behavior 覆盖 `onEnter`/`onExit`/`onCancel`，在调用 `super.onEnter()` 前后插入专属逻辑；保留现有设计会话追踪。 |
| R5 [C:INFERRED] | `OdyConfig.sessionMode` 扩到含 `normal` 后与现有语义冲突（原只表示启动时进入 plan/design） | 中 | 低 | 文档明确 `sessionMode` 表示启动后默认进入的交互阶段，`'normal'` 为显式不进入任何 mode；保留旧值兼容。 |
| R6 [C:INFERRED] | Scope 膨胀导致 PR 过大 | 高 | 中 | 本次设计按一个 PR 交付，但代码改动按文件分批 review；任何 behavior 实现可独立回滚到旧 SessionMode 分支。 |
| R7 [C:INFERRED] | 注入器基类统一后，design 的 `mockupAvailable` 特殊逻辑被抹平 | 低 | 中 | `SessionModeInjector` 预留 `getExtraContext()` 钩子；design injector 覆盖以传入 `mockupAvailable`。 |

## Reuse Analysis

| 候选 | 文件 | 可复用性 | 说明 |
|---|---|---|---|
| `DynamicInjector` 生命周期 | `packages/agent-core/src/agent/injection/injector.ts` [C:INFERRED] | 适配后复用 | 抽象基类 `SessionModeInjector` 继承 `DynamicInjector`，复用 `injectedAt` 管理与 `onContextCompacted/onContextMessageRemoved` 逻辑。 |
| `SessionMode` 状态机 | `packages/agent-core/src/agent/session-mode/index.ts` [C:USER] | 替换核心调度 | 保留 `SessionMode` 类外壳与公共 API，但把 enter/exit/cancel/handoff 中的 mode-specific 分支抽到 `SessionModeBehavior`。 |
| `resolveSessionModeDirectory` / `findUniqueStemInDir` | `packages/agent-core/src/agent/session-mode/index.ts` [C:INFERRED] | 提取复用 | 作为 `BaseSessionModeBehavior` 的私有/保护方法复用，原 `SessionMode` 类不再直接包含这些逻辑。 |
| `modelAliasHasUsableAuth` | `packages/agent-core/src/agent/session-mode/index.ts` [C:INFERRED] | 提取复用 | 迁移到 `BaseSessionModeBehavior` 或独立工具函数，供 behavior 调用。 |
| Mode injector 变体算法 | `plan-mode.ts` / `design-mode.ts` / `office-hours.ts` / `game-design.ts` [C:USER] | 提取并替换 | 当前四个类有几乎相同的 `getVariant` 与 `onContextClear` 逻辑；统一为 `SessionModeInjector` 基类，子类只提供 reminder 文本。 |
| `SkillRegistry` 过滤逻辑 | `packages/agent-core/src/skill/registry.ts` [C:INFERRED] | 适配签名 | 仅把 `sessionMode` 参数类型从重复字面量改为 `RuntimeMode`，算法不变。 |
| `OdyConfig` schema | `packages/agent-core-shared/src/config.ts` [C:USER] | 扩展 | 把 `sessionMode`/`defaultSessionMode` 从 `'plan' | 'design'` 扩为 `RuntimeMode`，保持 `'plan' | 'design'` 兼容。 |

## Assumptions & Unverified Items

| # | 假设 | 来源 | 置信度 | 若错误的影响 | 验证方式 |
|---|---|---|---|---|---|
| A1 | `SessionModeKind` 的 4 个值是完整且稳定的集合，近期不会新增/删除 | [C:INFERRED] | 高 | 新增 mode 时需调整类型常量与注册表；删除 mode 需迁移旧记录 | 代码搜索 `sessionMode.enter` 的 `kind` 记录；确认 roadmap 无新增 mode 计划 |
| A2 | `normal` 是唯一的非 mode 运行态，且不应出现在 `SessionModeKind` 中 | [C:USER] | 高 | 类型混乱导致 `SessionMode` 类逻辑错误 | 当前代码中 `ModeKey` 已含 `'normal'`，`SessionModeKind` 不含；设计保持一致 |
| A3 | `modeModels` 的键命名（`officeHours`/`gameDesign`）保持现状最稳妥 | [C:INFERRED] | 中 | 若用户期望 TOML 中使用 kebab 名（`office-hours`），当前映射会显得不一致 | 文档中显式列出映射表；不改动 config schema 解析 |
| A4 | 注入器统一后，`DesignModeInjector` 的 `mockupAvailable` 可通过 `getExtraContext()` 钩子保留 | [C:INFERRED] | 中 | 若钩子不足，design reminder 会丢失 mockup 提示 | 实现时验证 design injector 子类可传入 mockupAvailable |
| A5 | `BaseSessionModeBehavior` 默认的目录解析/模型切换逻辑对 office-hours/game-design 也适用 | [C:INFERRED] | 中 | office-hours 实际输出目录可能不是 `.ody-code/products`，需 behavior 覆盖 | 实现时核对 `resolveSessionModeDirectory` 当前分支与 office-hours 实际落盘路径 |
| A6 | `SkillCatalog.getUnavailableSkillsReminder` 放宽到 `RuntimeMode` 不会引入错误提示 | [C:INFERRED] | 低 | office-hours/game-design 下可能错误提示某些技能不可用 | 实现后检查各 mode 的 reminder 输出；必要时保持该函数仍只接受 plan/design |
| A7 | 四个 mode injector 的变体调度参数（DEDUP_MIN_TURNS / FULL_REFRESH_TURNS）当前完全相同（均为 2/5） | [C:INFERRED] | 中 | 若实际不同，统一后会改变注入频率 | 实现前再次核对四个文件中的常量值 |
| A8 | `OdyConfig.sessionMode`/`defaultSessionMode` 在实际运行中被读取的位置有限，扩展 schema 不会引发连锁配置迁移 | [C:INFERRED] | 中 | 若多处代码假设其值为 `'plan' | 'design'`，扩展后需额外防御 | Grep 所有读取 `sessionMode`/`defaultSessionMode` 的代码路径 |

## Self-Review

### 最高风险决策与用例

**D1. `normalizeRuntimeMode` 回退规则** [C:USER]
- 输入 `'plan'` → `'plan'`
- 输入 `'office-hours'` → `'office-hours'`
- 输入 `'foo'` → `'normal'`（warn）

**D2. `ModeBehaviorRegistry.resolve` 对未注册 kind 的处理** [C:USER]
- 输入 `'plan'` → 返回 `PlanModeBehavior`
- 输入 `'design'` → 返回 `DesignModeBehavior`
- 输入 `'unknown'` → 抛 `INTERNAL_ERROR`

**D3. `computeVariant` 变体调度** [C:USER]
- 历史 `[assistant, assistant, assistant]`，`injectedAt=0`，`dedupMin=2` → `'sparse'`（2 个 assistant turns since injectedAt）
- 历史 `[assistant, assistant, user]`，`injectedAt=0` → `'full'`（user 打断）
- 历史 `[assistant, assistant, assistant, assistant, assistant, assistant]`，`injectedAt=0`，`fullRefresh=5` → `'full'`（达到 full refresh 阈值）
- `injectedAt = null` → `'full'`（首次注入）
- 历史 `[assistant, assistant]`，`injectedAt=0`，`dedupMin=2` → `null`（仅 1 个 assistant turn，未达 dedup）

### 四透镜检查

- **Security** [C:INFERRED]：mode 字符串不进入认证/ secret 路径；`normalizeRuntimeMode` 仅用于日志与降级，不会把非法值传给文件系统操作；注入器 reminder 文本来自受控的 contract 文件，不拼接用户输入。
- **Test** [C:INFERRED]：每个行为都有 must-pass 与 must-reject 用例；`computeVariant` 的边界用例覆盖 injectedAt=null、user 出现、assistant 计数阈值；D1-D3 的对抗输入已在上面列出。
- **Ops** [C:INFERRED]：`ModeBehaviorRegistry` 在 Agent 初始化时一次性创建，无热更新需求；`SessionModeInjector` 每 Agent 实例一套，无跨 Agent 共享状态；behavior 注册表全局只读，线程安全。
- **Integration** [C:INFERRED]：所有依赖的数据源/字段/钩子均已通过 Read/Grep 验证存在（`SessionModeKind`、`ModeKey`、`SystemPromptContext.sessionMode`、`SkillCatalog`、`OdyConfig.sessionMode`、`DynamicInjector`、`InjectionManager` 硬编码注入器列表）。
- **Scope** [C:INFERRED]：本次仍是一个相干子系统（mode 概念统一 + 重构），未拆出独立产品；office-hours 拆包只是后续解锁项，不在本次实现。

### 修复记录

- 无。

## User Final Approval

- **审批状态**: ✅ 已批准
- **审批时间**: 2026-06-24
- **审批方式**: Deep 审计门通过
  - 第 1 部分：7 个章节关键论断全部接受。
  - 第 2 部分：8 个 [C:INFERRED] 假设全部接受。
- **选定方法**: C — ModeBehavior 策略对象 + 注入器基类 + 类型统一 + 文档
- **后续动作**: 进入 `/plan` 制定实施计划。
