# Game Design Mode — TUI 层

## Scope

本部分覆盖 `apps/ody-code/src/tui` 中所有需要感知 `game-design` 模式的位置：类型、启动状态、页脚徽章、状态面板、命令可见性、键盘快捷键、本地化键值。

## 数据流

```
runGameDesign()
  → OdyTUI(harness, { cliOptions: { sessionMode: 'game-design' }, officeHours: false })
  → createInitialAppState() 将 sessionMode 设为 'game-design'
  → OdyTUI.start() 调用 session.setSessionMode('game-design')
  → 注入器开始工作
  → FooterComponent.render() 渲染 game-design 徽章
  → StatusPanel 显示 game-design 行
  → 用户输入 / 命令可见性过滤
```

## 类型与接口

### `SessionMode` TUI 类型扩展

**文件**：`apps/ody-code/src/tui/commands/types.ts:4`

```ts
export type SessionMode = 'normal' | 'plan' | 'design' | 'office-hours' | 'game-design';
```

### `AppState` 扩展

**文件**：`apps/ody-code/src/tui/types.ts`

将 `sessionMode` 字段的联合类型扩展为包含 `'game-design'` [C:INFERRED]。

### `TUIStartupOptions` 与 `OdyTUIStartupInput`

**文件**：`apps/ody-code/src/tui/types.ts`、`apps/ody-code/src/tui/ody-tui.ts:137-148`

- 不新增独立的 `gameDesign` 布尔字段 [C:INFERRED]；`cliOptions.sessionMode === 'game-design'` 是单一事实来源。
- `OdyTUIStartupInput` 中 `officeHours` 字段保留，用于 backward-compatible 判断，game-design 通过 `cliOptions.sessionMode` 推导。

## 调用点

### 1. 初始 AppState

**文件**：`apps/ody-code/src/tui/ody-tui.ts:150-183`

```ts
function createInitialAppState(input: OdyTUIStartupInput): AppState {
  return {
    // ...
    sessionMode: input.officeHours ? 'office-hours' : input.cliOptions.sessionMode,
    // ...
  };
}
```

- 由于 `runGameDesign` 传入 `cliOptions.sessionMode = 'game-design'` 且 `officeHours = false`，此处自动得到 `'game-design'`。

### 2. 启动时设置会话模式

**文件**：`apps/ody-code/src/tui/ody-tui.ts:538-539`

扩展现有 office-hours 分支为通用特殊模式分支：

```ts
if (session !== undefined) {
  if (startup.officeHours) {
    await session.setSessionMode('office-hours');
  } else if (startup.cliOptions.sessionMode === 'game-design') {
    await session.setSessionMode('game-design');
  }
}
```

### 3. 技能 slash 命令过滤

**文件**：`apps/ody-code/src/tui/ody-tui.ts:304-309`

```ts
const mode = this.state.appState.sessionMode;
const skillCommands = mode === 'office-hours' || mode === 'game-design'
  ? []
  : this.skillCommands;
```

- [C:INFERRED] 在 game-design 模式下不显示 skill slash 命令，避免界面杂乱；模型仍可通过 `Skill` 工具调用技能。

### 4. 列出技能时过滤

**文件**：`apps/ody-code/src/tui/ody-tui.ts:344-345`

```ts
skills = await session.listSkills(mode && mode !== 'normal' ? { sessionMode: mode } : undefined);
```

- `listSkills` 在 node-sdk/agent-core 中已扩展接受 `'game-design'`，返回该模式下可见的技能。

### 5. 状态同步

**文件**：`apps/ody-code/src/tui/ody-tui.ts:1040-1046`

```ts
const mode = this.state.appState.sessionMode;
const isSpecialMode = mode === 'office-hours' || mode === 'game-design';
// ...
sessionMode: mode,
sessionModeFilePath: isSpecialMode ? null : (sessionModeFilePath ?? null),
```

- [C:INFERRED] 专用模式（office-hours / game-design）的文件路径由 `SessionMode` 内部管理，TUI 状态同步时同样置空，避免与 status 快照冲突。

### 6. 页脚徽章

**文件**：`apps/ody-code/src/tui/components/chrome/footer.ts:31`

```ts
const EMOJIS: Record<string, string> = {
  normal: '⚒️',
  plan: '📝',
  design: '✏️',
  'office-hours': '🏢',
  'game-design': '🎮',        // [C:INFERRED]
};
```

**文件**：`apps/ody-code/src/tui/components/chrome/footer.ts:51-82`

`renderModeBadge` 函数签名扩展：

```ts
function renderModeBadge(
  mode: 'normal' | 'plan' | 'design' | 'office-hours' | 'game-design',
  colors: ColorPalette,
  fileName?: string,
  userLanguage?: 'en' | 'zh' | undefined,
): string
```

颜色逻辑扩展：

```ts
const bgColor =
  mode === 'design'
    ? colors.accent
    : mode === 'plan'
      ? colors.primary
      : mode === 'office-hours'
        ? colors.warning
        : mode === 'game-design'
          ? colors.success    // [C:INFERRED] 使用 success 色作为游戏设计模式标识
          : colors.textMuted;
```

显示标签：

```ts
const displayLabel = mode === 'office-hours'
  ? t('tui.footer.officeHours', userLanguage)
  : mode === 'game-design'
    ? t('tui.footer.gameDesign', userLanguage) ?? 'game-design'
    : mode;
```

### 7. 状态面板

**文件**：`apps/ody-code/src/tui/components/messages/status-panel.ts:38`

```ts
readonly sessionMode: 'normal' | 'plan' | 'design' | 'office-hours' | 'game-design';
```

在 `buildStatusReportLines` 的 `rows` 中新增一行：

```ts
{ label: t('tui.statusPanel.gameDesign', lang), value: sessionMode === 'game-design' ? t('tui.statusPanel.on', lang) : t('tui.statusPanel.off', lang) },
```

或更简洁地复用 office-hours 行逻辑，按 `sessionMode` 选择对应标签 [C:INFERRED]。

### 8. 工具栏提示

**文件**：`apps/ody-code/src/tui/components/chrome/footer.ts:114-132`

现有 `shift+tab: cycle plan/design mode` 已 `hiddenInModes: ['office-hours']`，扩展为：

```ts
{ text: 'shift+tab: cycle plan/design mode', hiddenInModes: ['office-hours', 'game-design'] },
```

### 9. 命令可见性

**文件**：`apps/ody-code/src/tui/commands/registry.ts`

将现有常量：

```ts
const OFFICE_HOURS_HIDDEN: readonly SessionMode[] = ['office-hours'];
```

扩展为：

```ts
const SPECIAL_MODE_HIDDEN: readonly SessionMode[] = ['office-hours', 'game-design'];
```

并将所有 `hiddenInModes: OFFICE_HOURS_HIDDEN` 替换为 `hiddenInModes: SPECIAL_MODE_HIDDEN` [C:INFERRED]。这样所有 office-hours 中隐藏的命令在 game-design 中也隐藏，保持两种专用模式 UI 一致。

保留可见的命令：
- `/exit`
- `/help`
- `/version`

### 10. 编辑器键盘快捷键

**文件**：`apps/ody-code/src/tui/controllers/editor-keyboard.ts:124-125`

```ts
const mode = host.state.appState.sessionMode;
if (mode === 'office-hours' || mode === 'game-design') {
  // 禁用与模式切换相关的快捷键
}
```

- [C:INFERRED] 专用模式下禁用 `shift+tab` 循环等正常模式切换快捷键。

### 11. 本地化键值

**文件**：`packages/agent-core/src/i18n/types.ts`

新增：

```ts
| 'tui.footer.gameDesign'
| 'tui.statusPanel.gameDesign'
```

**文件**：`packages/agent-core/src/i18n/translations.ts`

```ts
// en
'tui.footer.gameDesign': 'Game Design',
'tui.statusPanel.gameDesign': 'Game Design',

// zh
'tui.footer.gameDesign': '游戏设计',
'tui.statusPanel.gameDesign': '游戏设计',
```

## 算法

### 命令可见性过滤

输入：命令 `c`，当前 `mode`
输出：布尔值

```
1. 若 c.hiddenInModes 未定义或为空 → 返回 true
2. 返回 !c.hiddenInModes.includes(mode)
```

### 页脚徽章渲染

输入：`mode`, `colors`, `fileName`, `userLanguage`
输出：ANSI 字符串

```
1. emoji ← EMOJIS[mode] ?? ''
2. bgColor ← 按 mode 选择 colors 中的色值
3. textColor ← luminance(bgColor) > 0.5 ? 黑 : 白
4. displayLabel ← 按 mode 选择本地化标签，fallback 为 mode 本身
5. label ← fileName ? `${emoji} ${displayLabel} · ${fileName}` : `${emoji} ${displayLabel}`
6. 返回 chalk.bgHex(bgColor).hex(textColor)(`【 ${label} 】`)
```

## 错误处理

| 错误类 | 立即处理 | 降级路径 | 恢复条件 |
|--------|---------|---------|---------|
| `setSessionMode('game-design')` 失败 | TUI 显示错误状态，保持 `normal` 模式 | 无 | 重试或退出重进 |
| 未知 `sessionMode` 值 | TypeScript 编译期报错；运行时 `renderModeBadge` fallback 到 muted 色 | 无 | 修复类型扩展 |
| 命令在 game-design 下被拦截 | `resolveSlashCommandInput` 返回 `blocked`，TUI 显示 `Not available in current mode` | 无 | 用户切换到允许命令的模式 |

## 测试断言

1. `apps/ody-code/test/tui/components/chrome/footer.test.ts` 新增：
   - `renderModeBadge('game-design', colors)` 包含 `🎮` 且非空。
   - `tipsForMode('game-design')` 不包含 `shift+tab: cycle plan/design mode`。

2. `apps/ody-code/test/tui/components/messages/status-panel.test.ts` 新增：
   - `sessionMode === 'game-design'` 时状态行显示 `'Game Design'` / `'游戏设计'`。

3. `apps/ody-code/test/tui/commands/visibility.test.ts` 新增：
   - `/plan` 在 `game-design` 模式下不可见。
   - `/exit` 在 `game-design` 模式下可见。
   - `/help` 在 `game-design` 模式下可见。

4. `apps/ody-code/test/tui/tui-startup.test.ts`（或相关启动测试）新增：
   - `cliOptions.sessionMode = 'game-design'` 时，初始 `AppState.sessionMode === 'game-design'`。
   - `OdyTUI.start()` 调用 `session.setSessionMode('game-design')`。

5. `apps/ody-code/test/tui/tui-message-flow.test.ts` 新增：
   - `Shift-Tab` 在 `game-design` 模式下被忽略。

## 本地说明

- TUI 层对 `game-design` 的处理与 `office-hours` 高度对称：专用模式徽章、精简命令面板、禁用模式切换快捷键。
- 不新增独立的 `gameDesign` 布尔字段，避免状态冗余；`sessionMode === 'game-design'` 是单一事实来源 [C:INFERRED]。
- 技能清单通过 `session.listSkills({ sessionMode: 'game-design' })` 获取，注入器负责把清单喂给模型。
