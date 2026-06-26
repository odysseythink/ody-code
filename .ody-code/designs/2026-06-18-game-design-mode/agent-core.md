# Game Design Mode — Agent Core 层

## Scope

本部分覆盖 `packages/agent-core` 中为 `game-design` 会话模式新增的扩展：类型、目录解析、注入器、enter/exit 与状态工具、状态存储、权限策略。

## 数据流

```
TUI / RPC 调用 setSessionMode('game-design')
  → SessionMode.enter(kind = 'game-design')
  → 解析 .ody-code/game-design/ 目录
  → 切换 context 分区到 'game-design'
  → 若 modeModels['game-design'] 配置可用且鉴权通过，切换模型
  → InjectionManager.inject() 每轮调用 GameDesignInjector
  → GameDesignInjector 注入 workflow 提示 + 可用 Skill 清单 + 产物路径提醒
  → 模型可调用 EnterGameDesignModeTool / ExitGameDesignModeTool / AppendGameDesignProfileTool 等
  → 模型 Write/Edit 受 PlanModeGuardDeny 限制在 .ody-code/game-design/ 文件集
```

## 类型与接口

### `SessionModeKind` 扩展

**文件**：`packages/agent-core/src/agent/session-mode/index.ts:23`

```ts
export type SessionModeKind = 'plan' | 'design' | 'office-hours' | 'game-design';
```

### `ModeKey` 扩展

**文件**：`packages/agent-core/src/agent/index.ts:80`

```ts
export type ModeKey = 'normal' | 'plan' | 'design' | 'office-hours' | 'game-design';
```

并在 `Agent` 构造函数中为 `'game-design'` 新增 `_contexts` / `_fullCompactions` / `_microCompactions` 分区 [C:INFERRED]。

### RPC 与会话类型扩展

以下类型需同步增加 `'game-design'`：

- `packages/agent-core/src/rpc/events.ts`：`sessionMode?: 'normal' | 'plan' | 'design' | 'office-hours' | 'game-design'` [C:INFERRED]
- `packages/agent-core/src/rpc/core-api.ts`：`EnterPlanPayload.kind?: SessionModeKind`（已接受任意 `SessionModeKind`，无需改动类型签名） [C:INFERRED]
- `packages/agent-core/src/session/index.ts`：`setSessionMode(mode: 'normal' | 'plan' | 'design' | 'office-hours' | 'game-design', ...)` [C:INFERRED]
- `packages/node-sdk/src/types.ts`：`CreateSessionOptions.sessionMode` 与 `SessionStatus.sessionMode` [C:INFERRED]
- `packages/node-sdk/src/rpc.ts`：`SetSessionModeRpcInput.mode` 与 `listSkills` 过滤参数 [C:INFERRED]

### `GameDesignStateStore`

**文件**：新建 `packages/agent-core/src/game-design/state.ts`

```ts
export interface GameDesignProfileEntry {
  readonly date: string;
  readonly mode: 'project' | 'review';
  readonly projectSlug: string;
  readonly pillars: readonly string[];
  readonly audience: string;
  readonly platform: string;
  readonly genre: string;
  readonly signals: readonly string[];
  readonly designDoc: string;
}

export interface GameDesignLearningEntry {
  readonly ts: string;
  readonly skill: 'game-design';
  readonly type: 'decision' | 'reference' | 'insight';
  readonly key: string;
  readonly insight: string;
  readonly confidence: number;
  readonly source: 'observed';
  readonly branch?: string;
}

export interface GameDesignAnalyticsEvent {
  readonly ts: string;
  readonly skill: 'game-design';
  readonly event: 'started' | 'completed' | 'phase_changed';
  readonly branch: string;
  readonly session: string;
  readonly duration_s?: number;
  readonly outcome?: 'success' | 'abort' | 'unknown';
  readonly count?: number;
  readonly categories?: string;
}

export interface GameDesignStateStore {
  appendProfile(entry: GameDesignProfileEntry): Promise<void>;
  readProfile(): Promise<readonly GameDesignProfileEntry[]>;
  appendAnalytics(event: GameDesignAnalyticsEvent): Promise<void>;
  appendLearning(entry: GameDesignLearningEntry): Promise<void>;
  searchLearnings(options: { limit: number; crossProject?: boolean }): Promise<readonly GameDesignLearningEntry[]>;
  getSessionSummary(): Promise<{ sessionCount: number }>;
}
```

### `FileSystemGameDesignStateStore`

```ts
export class FileSystemGameDesignStateStore implements GameDesignStateStore {
  constructor(private readonly kaos: Kaos, baseDir: string);
}
```

- `baseDir` 由调用方传入项目级路径 `.ody-code/game-design/` [C:USER]。
- 持久化文件：`profile.jsonl`、`learnings.jsonl`、`analytics.jsonl`。

### `GameDesignInjector`

**文件**：新建 `packages/agent-core/src/agent/injection/game-design.ts`

```ts
export class GameDesignInjector extends DynamicInjector {
  protected override readonly injectionVariant = 'game_design';
  private wasActive = false;

  override onContextClear(): void;
  override async getInjection(): Promise<string | undefined>;
  protected getVariant(): 'full' | 'sparse' | null;
  private async currentGameDesignContent(): Promise<string>;
}
```

### `GameDesignContract`

**文件**：新建 `packages/agent-core/src/agent/injection/game-design-contract.ts`

```ts
export function gameDesignEntryReminder(designFilePath: SessionModeFilePath): string;
export function gameDesignFullReminder(designFilePath: SessionModeFilePath, skillListing: string): string;
export function gameDesignSparseReminder(designFilePath: SessionModeFilePath): string;
export function gameDesignReentryReminder(designFilePath: SessionModeFilePath): string;
export function gameDesignExitReminder(designFilePath: SessionModeFilePath | null): string;
```

### Enter/Exit 工具

**文件**：新建 `packages/agent-core/src/tools/builtin/game-design/enter-game-design.ts`

```ts
export class EnterGameDesignModeTool implements BuiltinTool<{}> {
  readonly name = 'EnterGameDesignMode' as const;
  resolveExecution(_args: {}): ToolExecution;
}
```

**文件**：新建 `packages/agent-core/src/tools/builtin/game-design/exit-game-design.ts`

```ts
export class ExitGameDesignModeTool implements BuiltinTool<{}> {
  readonly name = 'ExitGameDesignMode' as const;
  resolveExecution(_args: {}): ToolExecution;
}
```

### 状态与同步工具

**文件**：新建 `packages/agent-core/src/tools/builtin/game-design/set-language.ts`

```ts
export class SetGameDesignLanguageTool implements BuiltinTool<{ language: string }> {
  readonly name = 'SetGameDesignLanguage' as const;
}
```

**文件**：新建 `packages/agent-core/src/tools/builtin/game-design/append-profile.ts`

```ts
export class AppendGameDesignProfileTool implements BuiltinTool<{ entry: GameDesignProfileEntry }> {
  readonly name = 'AppendGameDesignProfile' as const;
}
```

**文件**：新建 `packages/agent-core/src/tools/builtin/game-design/append-learning.ts`

```ts
export class AppendGameDesignLearningTool implements BuiltinTool<{ entry: Omit<GameDesignLearningEntry, 'ts' | 'skill'> }> {
  readonly name = 'AppendGameDesignLearning' as const;
}
```

**文件**：新建 `packages/agent-core/src/tools/builtin/game-design/search-learnings.ts`

```ts
export class SearchGameDesignLearningsTool implements BuiltinTool<{ limit: number; crossProject?: boolean }> {
  readonly name = 'SearchGameDesignLearnings' as const;
}
```

**文件**：新建 `packages/agent-core/src/tools/builtin/game-design/ensure-routing.ts`

```ts
export class EnsureGameDesignRoutingTool implements BuiltinTool<{ skillName: string }> {
  readonly name = 'EnsureGameDesignRouting' as const;
}
```

**文件**：新建 `packages/agent-core/src/tools/builtin/game-design/sync-artifact.ts`

```ts
export class SyncGameDesignArtifactTool implements BuiltinTool<{ path?: string }> {
  readonly name = 'SyncGameDesignArtifact' as const;
}
```

## 调用点

### 1. 扩展 `SessionModeKind`

**文件**：`packages/agent-core/src/agent/session-mode/index.ts:23`

将联合类型扩展为包含 `'game-design'`。

### 2. 目录解析

**文件**：`packages/agent-core/src/agent/session-mode/index.ts:672`

将 `resolveSessionModeDirectory` 的子目录映射改为：

```ts
const subdir =
  kind === 'office-hours' ? 'office-hours'
  : kind === 'game-design' ? 'game-design'
  : kind === 'design' ? 'designs'
  : 'plans';
```

- 输出目录 `.ody-code/game-design/` [C:INFERRED]，与 `designs/` / `office-hours/` / `plans/` 并列。

### 3. 模型切换

`SessionMode.enter()` 中 `kind === 'plan' || kind === 'design'` 的 `modeModels` 逻辑 [C:INFERRED] 应扩展为包含 `'game-design'`：

```ts
if (kind === 'plan' || kind === 'design' || kind === 'game-design') {
  // 读取 agent.kimiConfig?.modeModels?.[kind]
  // 校验 auth 后切换模型
}
```

用户可在 `config.toml` 中配置：

```toml
[modeModels]
game-design = "deepseek-reasoner"
```

### 4. 状态存储实例化

**文件**：`packages/agent-core/src/session/index.ts:49-51` 附近

在 `Session` 中为 game-design 模式实例化 `FileSystemGameDesignStateStore`：

```ts
const gameDesignStateStore = new FileSystemGameDesignStateStore(
  kaos,
  join(cwd, '.ody-code', 'game-design'),
);
```

并通过 `AgentOptions.officeHoursStateStore` 传入 `Agent`（复用该字段以避免修改 `Agent` 构造函数签名的最小改动；实现时可将该字段理解为“专用模式状态存储”） [C:INFERRED]。

**替代方案** [C:INFERRED]：为 `AgentOptions` 新增 `gameDesignStateStore?: GameDesignStateStore`，在 `Agent` 中保存为独立字段。若选择此方案，tools 需从 `agent.gameDesignStateStore` 读取。

### 5. 注入器注册

**文件**：`packages/agent-core/src/agent/injection/manager.ts:23-34`

```ts
this.injectors = [
  new PluginSessionStartInjector(agent),
  new TodoListReminderInjector(agent),
  new PlanModeInjector(agent),
  new DesignModeInjector(agent),
  new OfficeHoursInjector(agent),
  new GameDesignInjector(agent),          // [C:INFERRED]
  new PermissionModeInjector(agent),
  ...(flags.enabled('repo-knowledge') ? [new KnowledgeMicroagentInjector(agent)] : []),
];
```

### 6. 工具注册

**文件**：`packages/agent-core/src/tools/builtin/index.ts`

追加：

```ts
export * from './game-design/enter-game-design';
export * from './game-design/exit-game-design';
export * from './game-design/set-language';
export * from './game-design/append-profile';
export * from './game-design/append-learning';
export * from './game-design/search-learnings';
export * from './game-design/ensure-routing';
export * from './game-design/sync-artifact';
```

### 7. Agent 工具暴露

**文件**：`packages/agent-core/src/agent/tool/index.ts:407-474`

在 office-hours 工具实例化之后追加 game-design 工具实例：

```ts
new b.EnterGameDesignModeTool(this.agent),
new b.ExitGameDesignModeTool(this.agent),
new b.SetGameDesignLanguageTool(this.agent),
new b.AppendGameDesignProfileTool(this.agent),
new b.AppendGameDesignLearningTool(this.agent),
new b.SearchGameDesignLearningsTool(this.agent),
new b.EnsureGameDesignRoutingTool(this.agent),
new b.SyncGameDesignArtifactTool(this.agent),
```

### 8. 权限策略

**文件**：`packages/agent-core/src/agent/permission/policies/default-tool-approve.ts`

- `AppendGameDesignProfileTool` / `AppendGameDesignLearningTool` 为追加项目目录内 JSONL 的操作，可预批准 [C:INFERRED]。

**文件**：`packages/agent-core/src/agent/permission/policies/plan-mode-guard-deny.ts`

- 将 `'game-design'` 识别为受限模式，限制 `Write` / `Edit` 仅能写入当前 `sessionMode.sessionModeFilePath` 及其 `<stem>/` 子目录下的 `.md` 文件 [C:INFERRED]。

## 算法

### `GameDesignInjector.getInjection()`

输入：当前 `agent.sessionMode`、历史消息、设计文件内容
输出：要注入的提示字符串或 `undefined`

```
1. isActive ← agent.sessionMode.isActive 且 agent.sessionMode.kind === 'game-design'
2. path ← agent.sessionMode.sessionModeFilePath
3. 若 !isActive：
   a. 若 !wasActive → 返回 undefined
   b. wasActive ← false; injectedAt ← null
   c. 返回 gameDesignExitReminder(path)
4. 若 !wasActive：
   a. wasActive ← true; injectedAt ← null
   b. content ← await currentGameDesignContent()
   c. 若 content.trim().length > 0 → 返回 gameDesignReentryReminder(path)
   d. 否则 → 返回 gameDesignEntryReminder(path)
5. variant ← getVariant()
6. 若 variant === null → 返回 undefined
7. 若 variant === 'full'：
   a. listing ← agent.skills?.registry.getModelSkillListing('game-design') ?? ''
   b. 返回 gameDesignFullReminder(path, listing)
8. 返回 gameDesignSparseReminder(path)
```

### `GameDesignInjector.getVariant()`

与 `OfficeHoursInjector.getVariant()` 一致：

```
1. 若 injectedAt === null → 返回 'full'
2. assistantTurnsSince ← 0
3. 对 i 从 injectedAt+1 到 history.length-1：
   a. 若 role === 'assistant' → assistantTurnsSince += 1
   b. 若 role === 'user' → 返回 'full'
4. 若 assistantTurnsSince >= 5 → 返回 'full'
5. 若 assistantTurnsSince >= 2 → 返回 'sparse'
6. 返回 null
```

### `gameDesignFullReminder` 契约内容

输入：`designFilePath`, `skillListing`
输出：字符串

```
1. 输出固定前置：语言指令、模式激活声明、HARD GATES。
2. 输出工作流程（基于上游 skill.md Phase 1-8）：
   - Phase 1: 概念定义（3 根支柱、问题陈述、80/20、约束三角）
   - Phase 2: 核心循环设计
   - Phase 3: 机制与平衡（难度/心流、动态难度、加倍减半、奖惩、角色属性）
   - Phase 4: 关卡与体验（挑战分类、谜题、节奏、体验结构、环境叙事）
   - Phase 5: 视觉与交互（视觉引导、Fitts、Hick、黄金比例）
   - Phase 6: 玩家心理（认知偏差、决策、错误处理、归因偏差）
   - Phase 7: 原型与测试
   - Phase 8: 团队管理
3. 输出可用 Skill 清单：skillListing（仅 game-design 模式下可见的命名空间 Skill）。
4. 输出文件目标：仅写入 designFilePath 及其 stem 子目录下的 .md 附件。
5. 输出回合纪律：每次助手回复结尾推荐调用 ExitGameDesignMode；调用 Skill 时使用 `game-design/<name>`。
```

### `FileSystemGameDesignStateStore.searchLearnings`

```
1. 读取 learningsPath 文本
2. 按行过滤空行，逐行 JSON.parse 为 GameDesignLearningEntry
3. 若 !crossProject → 过滤 branch 与当前分支匹配（或 branch 未定义）的条目
4. 返回 entries.slice(-limit).reverse()
```

## 错误处理

| 错误类 | 立即处理 | 降级路径 | 恢复条件 |
|--------|---------|---------|---------|
| 已在 `game-design` 模式 | 工具返回 `isError: true` + 已激活提示 | 无 | 用户继续当前会话 |
| 处于其他模式时进入 | 工具返回 `isError: true` + 请先退出提示 | 无 | 用户先退出当前模式 |
| 目录创建失败（权限） | `SessionMode.enter()` catch 中恢复模型、重置状态、抛出 | 回退到 homedir 子目录 `~/.ody-code/game-design/` | 修复项目目录权限 |
| 注入器读取文件失败 | catch 返回空字符串，不影响主流程 | 无 | 文件后续被模型写入 |
| 状态存储写入失败 | 工具返回 `isError: true`，不阻塞会话 | 无 | 修复目录权限后重试 |
| 权限策略拒绝 Write/Edit | 返回 permission-denied 结果，提示仅可写入模式文件集 | 无 | 用户让模型写入正确路径 |

## 测试断言

1. `packages/agent-core/test/agent/session-mode/session-mode.test.ts` 新增：
   - `enter('game-design')` 后 `kind === 'game-design'` 且 `isActive === true`。
   - `resolveSessionModeDirectory('game-design')` 返回 `{ dir: '<cwd>/.ody-code/game-design', isProjectScoped: true }`。
   - `enter('game-design')` 后 `contextMode === 'game-design'`。

2. `packages/agent-core/test/agent/injection/game-design-contract.test.ts`（新建）：
   - 未激活时返回 `undefined`。
   - 首次激活且文件为空时返回 `gameDesignEntryReminder`。
   - 首次激活且文件有内容时返回 `gameDesignReentryReminder`。
   - 连续助手 turn 后返回 `full` / `sparse` 变体。
   - 退出后返回 `gameDesignExitReminder`。
   - `full` reminder 包含 `game-design/` 前缀 Skill 清单。

3. `packages/agent-core/test/tools/builtin/game-design/enter-exit.test.ts`（新建）：
   - `EnterGameDesignModeTool` 在非 game-design 模式下成功进入。
   - 已在 game-design 模式下调用 enter → 返回错误。
   - 在 office-hours 模式下调用 enter → 返回错误。
   - `ExitGameDesignModeTool` 在 game-design 模式下成功退出。
   - 未在 game-design 模式下调用 exit → 返回错误。

4. `packages/agent-core/test/tools/builtin/game-design/state-tools.test.ts`（新建）：
   - `AppendGameDesignProfileTool` 向 `.ody-code/game-design/profile.jsonl` 追加一条记录。
   - `AppendGameDesignLearningTool` 向 `learnings.jsonl` 追加一条记录。
   - `SearchGameDesignLearningsTool` 按 limit 返回最近条目。

5. `packages/agent-core/test/agent/permission/policies/plan-mode-guard-deny.test.ts` 新增：
   - game-design 激活时，允许写入 `game-design.md` 及其 `game-design/<stem>/` 下 `.md`。
   - game-design 激活时，拒绝写入 `src/index.ts`。
