# TUI Footer 模式显示增强 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use gpowers:subagent-driven-development (recommended) or gpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 增强 TUI Footer 的模式显示：反色背景标签 + emoji + 文件名，并将模式信息从 Line 1 迁移到 Line 2 左侧。

**Architecture:** 后端三层透传新增 `planFilePath` 字段（agent-core RPC event → node-sdk SessionStatus → ody-code AppState），TUI Footer 从 AppState 读取并渲染反色 ANSI 标签，同时重构 Line 2 布局将 transient hint 迁移到右侧。

**Tech Stack:** TypeScript, chalk (ANSI 颜色), pi-tui (visibleWidth / truncateToWidth), pnpm workspace monorepo

---

## File Structure

| 包 | 文件 | 职责 |
|---|---|---|
| `agent-core` | `packages/agent-core/src/rpc/events.ts` | `AgentStatusUpdatedEvent` 接口新增 `planFilePath` |
| `agent-core` | `packages/agent-core/src/agent/index.ts` | `emitStatusUpdated()` 组装事件时增加 `planFilePath` |
| `node-sdk` | `packages/node-sdk/src/types.ts` | `SessionStatus` 接口新增 `planFilePath` |
| `node-sdk` | `packages/node-sdk/src/rpc.ts` | `getStatus()` 从 `rpc.getPlan()` 结果提取 `path` 字段 |
| `ody-code` | `apps/ody-code/src/tui/types.ts` | `AppState` 新增 `planFilePath` |
| `ody-code` | `apps/ody-code/src/tui/kimi-tui.ts` | `syncRuntimeState()` 同步 `planFilePath` |
| `ody-code` | `apps/ody-code/src/tui/components/chrome/footer.ts` | 移除 Line 1 模式徽章；新增 `planFileName()` / `luminance()` / `renderModeBadge()`；重构 Line 2 布局 |
| `ody-code` | `apps/ody-code/test/tui/components/chrome/footer.test.ts` | Footer 渲染行为测试（build/plan/design × 有/无文件名 × 截断） |

---

## Dependency Overview

```
Task 1 (agent-core events) ─┐
                            ├──► Task 3 (ody-code AppState + sync)
Task 2 (node-sdk types) ────┘         │
                                      ▼
                            Task 4 (ody-code Footer 渲染)
                                      │
                                      ▼
                            Task 5 (ody-code Footer 测试)
                                      │
                                      ▼
                            Task 6 (全项目验证)
```

Task 1 和 Task 2 可以并行（不同包，无交叉依赖）。Task 3 依赖 Task 2（ody-code 消费 node-sdk 的 `SessionStatus`）。Task 4 依赖 Task 3（Footer 读取 `AppState.planFilePath`）。Task 5 依赖 Task 4（测试覆盖新渲染逻辑）。Task 6 验证全量编译和测试回归。

---

---

## Phase 1: 后端字段暴露（agent-core + node-sdk）

### Task 1: agent-core — `AgentStatusUpdatedEvent` 增加 `planFilePath`

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/rpc/events.ts`
- Modify: `packages/agent-core/src/agent/index.ts`

- [ ] **Step 1: 修改事件接口**

在 `packages/agent-core/src/rpc/events.ts` 的 `AgentStatusUpdatedEvent` 接口中新增字段：

```ts
export interface AgentStatusUpdatedEvent {
  readonly type: 'agent.status.updated';
  readonly model?: string | undefined;
  readonly contextTokens?: number | undefined;
  readonly maxContextTokens?: number | undefined;
  readonly contextUsage?: number | undefined;
  readonly planMode?: boolean | undefined;
  readonly planKind?: PlanKind | undefined;
  readonly planFilePath?: string | undefined;   // 新增
  readonly permission?: PermissionMode | undefined;
  readonly usage?: UsageStatus | undefined;
}
```

- [ ] **Step 2: 修改事件组装代码**

在 `packages/agent-core/src/agent/index.ts` 的 `emitStatusUpdated()` 方法中（约第 419–429 行），在 `planKind` 之后增加 `planFilePath`：

```ts
this.emitEvent({
  type: 'agent.status.updated',
  model,
  contextTokens,
  maxContextTokens,
  contextUsage,
  planMode: this.planMode.isActive,
  planKind: this.planMode.kind,
  planFilePath: this.planMode.planFilePath ?? undefined,   // 新增
  permission: this.permission.mode,
  usage,
});
```

- [ ] **Step 3: agent-core 包类型检查**

Run: `cd packages/agent-core && tsc --noEmit`
Expected: 编译通过，无类型错误

- [ ] **Step 4: Commit**

```bash
git add packages/agent-core/src/rpc/events.ts packages/agent-core/src/agent/index.ts
git commit -m "feat(agent-core): expose planFilePath in status updated event"
```

---

### Task 2: node-sdk — `SessionStatus` 增加 `planFilePath` 并透传

**Depends on:** none（可与 Task 1 并行，不同包）

**Files:**
- Modify: `packages/node-sdk/src/types.ts`
- Modify: `packages/node-sdk/src/rpc.ts`

- [ ] **Step 1: 修改 SessionStatus 接口**

在 `packages/node-sdk/src/types.ts` 的 `SessionStatus` 接口中新增字段：

```ts
export interface SessionStatus {
  // ... existing fields ...
  readonly planMode: boolean;
  readonly planKind?: 'plan' | 'design';
  readonly planFilePath?: string;   // 新增
  readonly contextTokens: number;
  readonly maxContextTokens: number;
}
```

- [ ] **Step 2: 修改 getStatus() 组装逻辑**

在 `packages/node-sdk/src/rpc.ts` 的 `getStatus()` 方法中（约第 398–408 行），在 `planKind` 之后增加 `planFilePath`：

```ts
return {
  model: config.modelAlias ?? config.provider?.model,
  thinkingLevel: config.thinkingLevel,
  permission: permission.mode,
  planMode: plan !== null,
  planKind: plan?.kind,
  planFilePath: plan?.path,   // 新增
  contextTokens,
  maxContextTokens,
  contextUsage,
  usage: hasUsage ? usage : undefined,
};
```

- [ ] **Step 3: node-sdk 包类型检查**

Run: `cd packages/node-sdk && tsc --noEmit`
Expected: 编译通过

- [ ] **Step 4: Commit**

```bash
git add packages/node-sdk/src/types.ts packages/node-sdk/src/rpc.ts
git commit -m "feat(node-sdk): add planFilePath to SessionStatus"
```

---

## Phase 2: TUI 状态层（ody-code AppState + sync）

### Task 3: ody-code — `AppState` 增加 `planFilePath` 并同步

**Depends on:** Task 2（node-sdk `SessionStatus` 已包含 `planFilePath`）

**Files:**
- Modify: `apps/ody-code/src/tui/types.ts`
- Modify: `apps/ody-code/src/tui/kimi-tui.ts`

- [ ] **Step 1: AppState 新增字段**

在 `apps/ody-code/src/tui/types.ts` 的 `AppState` 接口中新增：

```ts
export interface AppState {
  // ... existing fields ...
  planMode: boolean;
  designMode?: boolean;
  planFilePath?: string;   // 新增
  thinking: boolean;
  // ...
}
```

- [ ] **Step 2: 初始化时设置默认值**

在 `apps/ody-code/src/tui/kimi-tui.ts` 的 `createInitialAppState()` 中（约第 157 行），`designMode` 之后增加：

```ts
return {
  // ...
  planMode: input.cliOptions.plan,
  designMode: input.cliOptions.design ?? false,
  planFilePath: undefined,   // 新增
  // ...
};
```

- [ ] **Step 3: syncRuntimeState() 透传字段**

在 `apps/ody-code/src/tui/kimi-tui.ts` 的 `syncRuntimeState()` 中（约第 1017–1033 行），在 `designMode` 之后增加：

```ts
this.setAppState({
  // ... existing fields ...
  planMode: status.planMode && status.planKind !== 'design',
  designMode: status.planMode && status.planKind === 'design',
  planFilePath: status.planFilePath,   // 新增
  contextTokens: status.contextTokens,
  maxContextTokens: status.maxContextTokens,
});
```

- [ ] **Step 4: ody-code 包类型检查**

Run: `cd apps/ody-code && tsc --noEmit`
Expected: 编译通过（新增可选字段不会破坏现有调用）

- [ ] **Step 5: Commit**

```bash
git add apps/ody-code/src/tui/types.ts apps/ody-code/src/tui/kimi-tui.ts
git commit -m "feat(ody-code): add planFilePath to AppState and syncRuntimeState"
```

---

## Phase 3: TUI Footer 渲染重构

### Task 4: ody-code — Footer 渲染逻辑重构

**Depends on:** Task 3（`AppState.planFilePath` 已可用）

**Files:**
- Modify: `apps/ody-code/src/tui/components/chrome/footer.ts`

- [ ] **Step 1: 新增辅助函数**

在 `apps/ody-code/src/tui/components/chrome/footer.ts` 的模块级别新增以下函数（放在现有辅助函数之后，FooterComponent 类之前）：

```ts
/**
 * 计算 hex 颜色的 WCAG 2.1 相对亮度。
 * 返回值范围 [0, 1]；> 0.5 视为亮色背景，否则为暗色背景。
 */
function luminance(hex: string): number {
  const rgb = [hex.slice(1, 3), hex.slice(3, 5), hex.slice(5, 7)].map((h) => {
    const c = parseInt(h, 16) / 255;
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * rgb[0]! + 0.7152 * rgb[1]! + 0.0722 * rgb[2]!;
}

/**
 * 将完整 planFilePath 截断为文件名。null/undefined → null。
 */
function planFileName(path: string | null | undefined): string | null {
  if (!path) return null;
  const name = path.split('/').pop() ?? path;
  return name || null;
}

/**
 * 渲染反色模式标签。返回带 ANSI 背景色 + 前景色的字符串。
 */
function renderModeBadge(
  mode: 'build' | 'plan' | 'design',
  colors: ColorPalette,
  fileName?: string,
): string {
  const EMOJIS: Record<string, string> = { build: '⚒️', plan: '📝', design: '✏️' };
  const emoji = EMOJIS[mode]!;
  const bgColor =
    mode === 'design' ? colors.accent : mode === 'plan' ? colors.primary : colors.textMuted;
  const textColor = luminance(bgColor) > 0.5 ? '#000000' : '#ffffff';

  const label = fileName ? `${emoji} ${mode} · ${fileName}` : `${emoji} ${mode}`;
  const padded = ` ${label} `;
  return chalk.bgHex(bgColor).hex(textColor)(`【${padded}】`);
}
```

- [ ] **Step 2: 移除 Line 1 的模式徽章**

在 `FooterComponent.render()` 的 Line 1 组装逻辑中（约第 288–297 行），删除以下代码：

```ts
// 删除以下块：
const mode = state.designMode ? 'design' : state.planMode ? 'plan' : 'build';
const modeColor = state.designMode
  ? colors.accent
  : state.planMode
    ? colors.primary
    : colors.textMuted;
left.push(chalk.hex(modeColor).bold(mode));
```

- [ ] **Step 3: 重构 Line 2 布局**

在 `FooterComponent.render()` 的 Line 2 组装逻辑中（约第 364–387 行），替换为：

```ts
// ── Line 2: mode badge (left) + transient hint or context (right) ──
const mode = state.designMode ? 'design' : state.planMode ? 'plan' : 'build';
const modeColor =
  mode === 'design' ? colors.accent : mode === 'plan' ? colors.primary : colors.textMuted;
const fileName = planFileName(state.planFilePath);
const modeBadge = renderModeBadge(mode, modeColor, fileName ?? undefined);
const modeBadgeWidth = visibleWidth(modeBadge);

const contextText = formatContextStatus(
  state.contextUsage,
  state.contextTokens,
  state.maxContextTokens,
);
const contextWidth = visibleWidth(contextText);

let line2: string;
if (this.transientHint) {
  const maxHintWidth = Math.max(0, width - modeBadgeWidth - contextWidth - 2);
  const shownHint =
    visibleWidth(this.transientHint) <= maxHintWidth
      ? this.transientHint
      : truncateToWidth(this.transientHint, maxHintWidth, '…');
  const hintWidth = visibleWidth(shownHint);
  const middlePad = Math.max(0, width - modeBadgeWidth - hintWidth - contextWidth);
  line2 =
    modeBadge +
    ' '.repeat(middlePad) +
    chalk.hex(colors.warning).bold(shownHint) +
    ' ' +
    chalk.hex(colors.text)(contextText);
} else {
  const rightPad = Math.max(0, width - modeBadgeWidth - contextWidth);
  line2 = modeBadge + ' '.repeat(rightPad) + chalk.hex(colors.text)(contextText);
}
```

**注意：** `truncateToWidth` 和 `visibleWidth` 已从 `@earendil-works/pi-tui` 导入。

- [ ] **Step 4: 构建检查**

Run: `cd apps/ody-code && tsc --noEmit`
Expected: 编译通过

- [ ] **Step 5: Commit**

```bash
git add apps/ody-code/src/tui/components/chrome/footer.ts
git commit -m "feat(ody-code): refactor footer mode display with inverted badge + filename"
```

---

## Phase 4: 测试覆盖

### Task 5: ody-code — Footer 渲染测试

**Depends on:** Task 4（Footer 渲染逻辑已重构）

**Files:**
- Modify: `apps/ody-code/test/tui/components/chrome/footer.test.ts`

- [ ] **Step 1: 扩展测试基态**

在现有测试文件的 `appState` 常量中新增 `planFilePath: undefined`：

```ts
const appState: AppState = {
  // ... existing fields ...
  planMode: false,
  planFilePath: undefined,   // 新增
  theme: 'dark',
  // ...
};
```

- [ ] **Step 2: 编写 build 模式渲染测试**

```ts
it('renders build mode badge on line 2 with inverted colors', () => {
  const footer = new FooterComponent(appState, darkColors);
  const lines = footer.render(120);
  expect(lines[1]).toContain('⚒️');
  expect(lines[1]).toContain('build');
  expect(lines[1]).toContain('【');
});
```

- [ ] **Step 3: 编写 plan 模式 + 文件名测试**

```ts
it('renders plan mode badge with filename on line 2', () => {
  const state = { ...appState, planMode: true, planFilePath: '/home/alice/project/plans/fearless-mako.md' };
  const footer = new FooterComponent(state, darkColors);
  const lines = footer.render(120);
  expect(lines[1]).toContain('📝');
  expect(lines[1]).toContain('plan');
  expect(lines[1]).toContain('fearless-mako.md');
  expect(lines[1]).toContain('【');
});
```

- [ ] **Step 4: 编写 design 模式 + 文件名测试**

```ts
it('renders design mode badge with filename on line 2', () => {
  const state = { ...appState, designMode: true, planFilePath: '/home/alice/project/designs/brave-otter.md' };
  const footer = new FooterComponent(state, darkColors);
  const lines = footer.render(120);
  expect(lines[1]).toContain('✏️');
  expect(lines[1]).toContain('design');
  expect(lines[1]).toContain('brave-otter.md');
});
```

- [ ] **Step 5: 编写 planFilePath 缺失回退测试**

```ts
it('falls back to mode-only badge when planFilePath is absent', () => {
  const state = { ...appState, planMode: true, planFilePath: undefined };
  const footer = new FooterComponent(state, darkColors);
  const lines = footer.render(120);
  expect(lines[1]).toContain('📝');
  expect(lines[1]).toContain('plan');
  expect(lines[1]).not.toContain('·');
});
```

- [ ] **Step 6: 编写文件名截断测试**

```ts
it('truncates long filename with ellipsis', () => {
  const longName = 'a'.repeat(200) + '.md';
  const state = { ...appState, planMode: true, planFilePath: `/plans/${longName}` };
  const footer = new FooterComponent(state, darkColors);
  const lines = footer.render(80);
  expect(lines[1]).toContain('…');
  expect(lines[1]).not.toContain(longName);
});
```

- [ ] **Step 7: 运行测试**

Run: `cd apps/ody-code && pnpm test test/tui/components/chrome/footer.test.ts`
Expected: 全部通过

- [ ] **Step 8: Commit**

```bash
git add apps/ody-code/test/tui/components/chrome/footer.test.ts
git commit -m "test(ody-code): add footer mode badge rendering tests"
```

---

## Phase 5: 全项目验证

### Task 6: 全项目类型检查与回归测试

**Depends on:** Task 1–5（所有代码改动已完成）

**Files:**
- 全项目验证，不修改文件

- [ ] **Step 1: 根级类型检查**

Run: `tsc --noEmit`（从项目根目录运行）
Expected: 编译通过，无类型错误

- [ ] **Step 2: agent-core 包测试**

Run: `cd packages/agent-core && pnpm test`
Expected: 全部通过

- [ ] **Step 3: node-sdk 包测试**

Run: `cd packages/node-sdk && pnpm test`
Expected: 全部通过

- [ ] **Step 4: ody-code Footer 测试**

Run: `cd apps/ody-code && pnpm test test/tui/components/chrome/footer.test.ts`
Expected: 全部通过

- [ ] **Step 5: ody-code TUI 相关测试**

Run: `cd apps/ody-code && pnpm test test/tui/`
Expected: 全部通过（无回归）

- [ ] **Step 6: Commit（如有测试数据快照更新）**

若测试快照需要更新：

```bash
# 仅当测试输出显示快照不匹配时执行
pnpm test -- --update
```

否则无需额外 commit。

---

## Self-Review

- [ ] **1. Spec coverage (build the table).**

| Spec section | Task(s) | Status |
|---|---|---|
| §1 Scope — In/Out | — | no-op (范围定义，无代码) |
| §2 Architecture & Data Flow | Task 1–3 | covered |
| §3 Interfaces & Type Signatures | Task 1–3 | covered |
| §4 Footer Rendering Logic | Task 4 | covered |
| §5 State Sync Integration | Task 3 | covered |
| §6 Error Handling & Degradation | Task 4, 5 | covered |
| §7 Testing Plan | Task 5 | covered |
| §8 Risk Register | Task 6 | no-op (风险登记，无代码) |

- [ ] **2. Placeholder scan:** 无 TODO/TBD/"implement later"/"fill in details"。每个步骤包含完整代码或精确命令。

- [ ] **3. No phantom tasks:** 6 个任务均产生可验证的代码/测试变更。无 `--allow-empty`、无 "already done in Task N"。

- [ ] **4. Dependency soundness:** Task 1 和 Task 2 互相独立（不同包）；Task 3 依赖 Task 2；Task 4 依赖 Task 3；Task 5 依赖 Task 4；Task 6 依赖 Task 1–5。无反向依赖或外部未完成任务引用。

- [ ] **5. Caller & build soundness:** 本次改动仅新增可选字段（`planFilePath?: string`），不修改现有函数签名或接口形状，因此不存在 stale caller 问题。Task 4 修改 Footer 内部辅助函数，无外部调用者。每个任务结束时均要求 `tsc --noEmit` 验证。

- [ ] **6. Test-the-risk:** Task 5 的测试覆盖了核心渲染逻辑：build/plan/design 三种模式、有/无文件名、文件名截断。这些是状态变更（`AppState.planFilePath` → Footer 渲染输出）的直接断言。

- [ ] **7. Type consistency:** `planFilePath` 在所有三层中均为 `string | undefined`（或 `string | null | undefined`），名称一致。`renderModeBadge` 的参数名与 AppState 字段名对应清晰。
