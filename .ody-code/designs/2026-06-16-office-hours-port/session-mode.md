# Part 2 — Office-Hours Session Mode 与 Injector

## Scope

新增 `office-hours` 作为第四种 session mode，与 `plan`/`design` 共享只写单一文件机制，但使用独立的上下文分区、注入器、入口工具和退出语义。

## Interfaces

```typescript
// packages/agent-core/src/agent/index.ts
export type ModeKey = 'normal' | 'plan' | 'design' | 'office-hours'; // [C:USER]

// packages/agent-core/src/agent/session-mode/index.ts
export type SessionModeKind = 'plan' | 'design' | 'office-hours'; // [C:USER]

// packages/agent-core/src/rpc/core-api.ts
export interface EnterPlanPayload {
  readonly kind?: SessionModeKind;
  readonly fileStem?: string;
  readonly sourceFilePath?: string;
}

// packages/agent-core/src/agent/tool/index.ts 或新增文件
export class EnterOfficeHoursModeTool implements BuiltinTool<{}> {
  readonly name = 'EnterOfficeHoursMode' as const;
  readonly description: string;
  readonly parameters: Record<string, unknown>;
  constructor(agent: Agent);
  resolveExecution(_args: {}): ToolExecution;
}

export class ExitOfficeHoursModeTool implements BuiltinTool<{ approved: boolean }> {
  readonly name = 'ExitOfficeHoursMode' as const;
  readonly description: string;
  readonly parameters: Record<string, unknown>;
  constructor(agent: Agent);
  resolveExecution(args: { approved: boolean }): ToolExecution;
}
```

## Data Flow

```
TUI.start() calls session.setSessionMode('office-hours')
  │
  ▼
SDKRpcClient.setSessionMode({ mode: 'office-hours' })
  │
  ▼
core-impl.enterPlan({ kind: 'office-hours' })
  │
  ▼
Agent.enterPlan({ kind: 'office-hours' })
  │
  ▼
SessionMode.enter('office-hours')
  - creates context partition
  - resolves .ody-code/designs/<date>-<topic>.md
  - emits session_mode.enter record
  - sets model alias from modeModels.office-hours if configured
  │
  ▼
InjectionManager injects OfficeHoursInjector
  - full reminder on first turn
  - sparse reminder every N assistant turns
  - exit reminder when mode ends
```

## Algorithms

### SessionMode.resolveSessionModeDirectory 扩展

```
private async resolveSessionModeDirectory(kind: SessionModeKind): Promise<{ dir: string; isProjectScoped: boolean }>
  if kind === 'office-hours' then
    dir ← join(cwd, '.ody-code', 'office-hours')
  else if kind === 'design' then
    dir ← join(cwd, '.ody-code', 'designs')
  else
    dir ← join(cwd, '.ody-code', 'plans')
  // remainder unchanged: mkdir, fallback to homedir on EACCES
```

### OfficeHoursInjector.getInjection

```
override async getInjection(): Promise<string | undefined>
  const isActive = agent.sessionMode.isActive && agent.sessionMode.kind === 'office-hours'
  const path = agent.sessionMode.sessionModeFilePath

  if !isActive then
    if !this.wasActive return undefined
    this.wasActive = false
    this.injectedAt = null
    return officeHoursExitReminder(this.designDocPath)

  const content = await this.currentDesignContent()
  if !this.wasActive then
    this.wasActive = true
    this.injectedAt = null
    return officeHoursEntryReminder(path)

  const variant = this.getVariant()  // same cadence as DesignModeInjector
  if variant === null return undefined
  if variant === 'reentry' return officeHoursReentryReminder(path)
  return variant === 'full'
    ? officeHoursFullReminder(path)
    : officeHoursSparseReminder(path)
```

### EnterOfficeHoursModeTool.execute

```
execute: async () =>
  if agent.sessionMode.isActive then
    return { isError: true, output: 'Office hours mode is already active.' }
  await agent.sessionMode.enter(undefined, undefined, undefined, 'office-hours')
  return { output: officeHoursEntryReminder(agent.sessionMode.sessionModeFilePath) }
```

### ExitOfficeHoursModeTool.execute

```
execute: async () =>
  if !agent.sessionMode.isActive or agent.sessionMode.kind !== 'office-hours' then
    return { isError: true, output: 'Office hours mode is not active.' }
  const data = await agent.sessionMode.data()
  agent.sessionMode.exit()
  return {
    output: officeHoursCompletedReminder(data?.path ?? null),
    // signal host to stop app
    stopHost: true,
  }
```

## Call-Site Integration

### 1. packages/agent-core/src/agent/index.ts:77 [C:USER]

```typescript
export type ModeKey = 'normal' | 'plan' | 'design' | 'office-hours';
```

### 2. packages/agent-core/src/agent/index.ts:192-200 [C:INFERRED]

`_contexts` / `_fullCompactions` / `_microCompactions` 的初始化自动扩展：

```typescript
this._contexts = {
  normal: new ContextMemory(this),
  plan: new ContextMemory(this),
  design: new ContextMemory(this),
  'office-hours': new ContextMemory(this),
};
```

### 3. packages/agent-core/src/agent/session-mode/index.ts:22 [C:USER]

```typescript
export type SessionModeKind = 'plan' | 'design' | 'office-hours';
```

### 4. packages/agent-core/src/agent/session-mode/index.ts:593-606 [C:USER]

`resolveSessionModeDirectory` 增加 office-hours 分支（见 Algorithms）。

### 5. packages/agent-core/src/agent/injection/manager.ts:20-30 [C:USER]

```typescript
import { OfficeHoursInjector } from './office-hours';

this.injectors = [
  new PluginSessionStartInjector(agent),
  new TodoListReminderInjector(agent),
  new PlanModeInjector(agent),
  new DesignModeInjector(agent),
  new OfficeHoursInjector(agent),   // after design so it wins when active
  new PermissionModeInjector(agent),
];
```

### 6. ModeKey 扩展的全局影响

将 `ModeKey` 从 `'normal' | 'plan' | 'design'` 扩展为 `'normal' | 'plan' | 'design' | 'office-hours'` 需要更新以下文件（由 Code Audit 确认）：

| 文件 | 位置 | 改动 |
|---|---|---|
| `src/agent/index.ts:77` | `AgentOptions` 类型定义 | 扩展 `ModeKey` |
| `src/agent/index.ts:192-200` | `_contexts`/`_fullCompactions`/`_microCompactions` 初始化 | 新增 `'office-hours'` 键 |
| `src/agent/replay/index.ts:1,7,11,37` | `ReplayBuilder._mode` 和 `buildResultForMode` | 扩展类型 |
| `src/session/checkpoint/integrity.ts:15-18` | 独立的 `ModeKey` 类型和 `VALID_MODES` | 扩展类型 + 新增值 |
| `src/session/checkpoint/checkpoint.ts:28` | `currentMode: ModeKey` | 类型自动跟随 |
| `src/rpc/events.ts:50` | `sessionMode?: 'normal' \| 'plan' \| 'design'` | 扩展字面量 |
| `src/profile/types.ts:45` | `sessionMode?: 'normal' \| 'plan' \| 'design'` | 扩展字面量 |
| `src/skill/types.ts:57-58` | `listInvocableSkills()` 和 `getModelSkillListing()` 签名 | 扩展 sessionMode 类型 |
| `src/skill/registry.ts:113,119,122,143,145,156,160,172` | `listInvocableSkills` 和 `getUnavailableSkillsReminder` 实现 | 扩展类型检查 |
| `apps/ody-code/src/cli/options.ts:48-49` | `sessionMode` 校验 | 允许 `'office-hours'` |
| `apps/ody-code/src/cli/commands.ts:114` | 默认值解析 | 类型扩展 |
| `apps/ody-code/src/tui/types.ts:18,30,179` | `AppState.sessionMode`、`TUIStartupOptions.sessionMode` | 扩展字面量 |
| `apps/ody-code/src/tui/components/messages/status-panel.ts` | sessionMode 显示逻辑 | 扩展 case |

### 7. packages/agent-core/src/agent/tool/index.ts 工具注册 [C:INFERRED]

由于 `ToolManager` 构造时 `sessionMode` 尚未激活，不能静态判断。改为始终注册 office-hours tools，在 tool 内部检查 `sessionMode.kind !== 'office-hours'` 时返回 `isError: true`。

在 `ToolManager.initializeBuiltinTools` 中新增：

```typescript
new b.EnterOfficeHoursModeTool(this.agent),
new b.ExitOfficeHoursModeTool(this.agent),
new b.EnsureClaudeMdRoutingTool(this.agent),
new b.SyncOfficeHoursArtifactTool(this.agent),
new b.AppendBuilderProfileTool(this.agent),
new b.AppendLearningTool(this.agent),
new b.SearchLearningsTool(this.agent),
```

### 7. packages/agent-core/src/agent/index.ts:448-473 [C:USER]

`Agent.enterPlan` 已接受 `SessionModeKind`，无需改动签名；`payload.kind` 传入 `'office-hours'` 即可。

### 8. packages/agent-core/src/agent/session-mode/index.ts:enter() [C:INFERRED]

`enter()` 内部 `_kind` 初始值保持 `'plan'`，由参数覆盖。新增 `case 'office-hours'` 无特殊分支即可复用通用逻辑。

## Error & Degradation

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| `SessionMode.enter('office-hours')` 抛出 | 通过 RPC/CLI 冒泡 | TUI 显示错误并退出 | 修复工作目录权限或模式实现 |
| `modeModels.office-hours` 不存在 | 复用 design-mode 逻辑：warn 并保留当前模型 | 使用默认模型继续 | 用户在 config.toml 配置 `mode_models.office-hours` |
| OfficeHoursInjector 读取设计文件失败 | catch 返回 undefined | 本次不注入 reminder | 文件写入后下次注入恢复 |
| Exit tool 在非 office-hours mode 调用 | 返回 isError | 忽略 | 正确状态后可用 |

## Test Plan

1. **SessionMode enter/exit office-hours**（`packages/agent-core/test/agent/session-mode-office-hours.test.ts` 新增）：
   - `expect(sessionMode.kind).toBe('office-hours')` after `enter()`.
   - `expect(sessionMode.isActive).toBe(false)` after `exit()`.
   - 验证文件目录为 `.ody-code/office-hours/`.

2. **OfficeHoursInjector 变体逻辑**：
   - 首次注入返回 `full` reminder。
   - 2 个 assistant turns 后返回 `sparse`。
   - 5 个 assistant turns 后返回 `full`。
   - 用户回复后返回 `full`。

3. **Enter/Exit tools**（`packages/agent-core/test/agent/tool/enter-office-hours.test.ts` 新增）：
   - `EnterOfficeHoursModeTool` 进入 mode 成功。
   - 重复进入返回 `isError`。
   - `ExitOfficeHoursModeTool` 在非 active 状态返回 `isError`。

## Done Criteria

- `pnpm -F @odysseythink/agent-core typecheck` passes.
- `pnpm -F @odysseythink/agent-core test` passes.
- `office-hours` 出现在 `ModeKey` 与 `SessionModeKind` 的所有使用处且无编译错误。
