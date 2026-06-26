# Part 3: 状态透传 + TUI footer/status panel 本地化

本 Part 完成跨包的状态/RPC 类型透传（Task 9），以及 TUI footer 徽章（Task 10）与 `/status` 面板（Task 11）的本地化。Task 9 是唯一的共享签名变更任务，必须以全树 `typecheck` 收尾。

---

### Task 9: 共享状态/RPC 类型与透传

**Depends on:** Part 1、Part 2 全部完成（共享类型 `Agent.userLanguage`、工具 `SetOfficeHoursLanguage` 等已就绪）

**Files:**
- Modify: `packages/agent-core/src/rpc/events.ts` 44-54
- Modify: `packages/agent-core/src/rpc/core-api.ts` 346-375
- Modify: `packages/agent-core/src/index.ts` already exported `i18n` in Task 1; verify `export * from './i18n'`
- Modify: `packages/node-sdk/src/types.ts` 164-174
- Modify: `packages/node-sdk/src/rpc.ts` 384-423
- Modify: `packages/node-sdk/src/index.ts` +1 line
- Modify: `apps/ody-code/src/tui/types.ts` 15-41
- Modify: `apps/ody-code/src/tui/controllers/session-event-handler.ts` 545-562
- Modify: `apps/ody-code/src/tui/ody-tui.ts` 150-181, 1015-1045
- Test: `apps/ody-code/test/tui/controllers/session-event-handler.test.ts` +1 test

- [ ] Write the failing test — 追加到 `apps/ody-code/test/tui/controllers/session-event-handler.test.ts`（在 `describe('SessionEventHandler handleStatusUpdate'` 内部追加）：

```typescript
  it('propagates userLanguage from agent.status.updated event', () => {
    const host = makeHost();
    const handler = new SessionEventHandler(host);

    const event = {
      type: 'agent.status.updated',
      agentId: 'main',
      sessionId: 'ses-1',
      userLanguage: 'zh',
    } as Event;

    handler.handleEvent(event, vi.fn());

    expect(host.setAppState).toHaveBeenCalledWith(
      expect.objectContaining({
        userLanguage: 'zh',
      }),
    );
  });
```

- [ ] Run it and verify it FAILS:
```bash
pnpm --filter @odysseythink/ody-code test test/tui/controllers/session-event-handler.test.ts
```
Expected: TypeScript 编译报错 — `AgentStatusUpdatedEvent` 无 `userLanguage` 字段；`AppState` 无 `userLanguage`。

- [ ] Write the minimal implementation：

**Step 1 — `packages/agent-core/src/rpc/events.ts`**（line 44-54），添加 `userLanguage` 字段:

```typescript
export interface AgentStatusUpdatedEvent {
  readonly type: 'agent.status.updated';
  // ... existing fields ...
  readonly userLanguage?: SupportedLanguage | undefined;
}
```

添加 import: `import type { SupportedLanguage } from '#/i18n';`

**Step 2 — `packages/agent-core/src/rpc/core-api.ts`**（`AgentAPI`，line 346-375），在 `getUsage` 后追加:

```typescript
  getUserLanguage: (payload: EmptyPayload) => SupportedLanguage | undefined;
```

**Step 3 — `apps/ody-code/src/tui/types.ts`**（`AppState`，line 15-41），在 `mcpServersSummary` 前追加:

```typescript
  userLanguage?: 'en' | 'zh' | undefined;
```

导入：`import type { SupportedLanguage } from '@odysseythink/ody-code-sdk';` 但为了避免新增 SDK 导出类型，可直接用字面量 `'en' | 'zh'`。在 Task 11 后确保 SDK 导出。

**Step 4 — `apps/ody-code/src/tui/controllers/session-event-handler.ts`**（`handleStatusUpdate`，line 545-562），在现有字段复制后添加:

```typescript
    if (event.userLanguage !== undefined) patch.userLanguage = event.userLanguage;
```

**Step 5 — `apps/ody-code/src/tui/ody-tui.ts`**：

`createInitialAppState`（~line 156-181）中追加：

```typescript
    userLanguage: undefined,
```

`syncRuntimeState`（~line 1015-1045）的 `setAppState` 调用中追加：

```typescript
      userLanguage: status.userLanguage,
```

**Step 6 — `packages/node-sdk/src/types.ts`**（`SessionStatus`，line 164-174），追加：

```typescript
  readonly userLanguage?: 'en' | 'zh' | undefined;
```

**Step 7 — `packages/node-sdk/src/rpc.ts`**（`getStatus`，line 384-423），在 `return {` 前添加调用，在返回对象中添加字段：

```typescript
    const userLanguage = await rpc.getUserLanguage({ sessionId: input.sessionId, agentId });
    // ...
    return {
      // ... existing fields ...
      userLanguage,
    };
```

**Step 8 — `packages/node-sdk/src/index.ts`**，向 SDK 消费者导出 `t`、`SupportedLanguage`：

```typescript
export { t, isSupportedLanguage, normalizeLanguage } from '@odysseythink/agent-core';
export type { SupportedLanguage, MessageKey } from '@odysseythink/agent-core';
```

**Step 9 — `apps/ody-code/src/tui/types.ts`** 改用从 SDK 导入的类型（替代步骤 3 的字面量）：

```typescript
import type { SupportedLanguage } from '@odysseythink/ody-code-sdk';
```

并将 `userLanguage` 字段改为 `userLanguage?: SupportedLanguage | undefined;`。

同样更新 `apps/ody-code/src/tui/controllers/session-event-handler.ts` 中的类型使用（`Event` 的 `userLanguage` 通过 `AgentStatusUpdatedEvent` 类型获得）。

- [ ] Run it and verify it PASSES + 全树 typecheck：
```bash
pnpm --filter @odysseythink/ody-code test test/tui/controllers/session-event-handler.test.ts
pnpm -r typecheck
```
Expected: 测试 PASS，全树 typecheck 零错误。

- [ ] Commit: `git add -A && git commit -m "feat: plumb userLanguage through AgentStatusUpdatedEvent, SessionStatus, AppState"`

---

### Task 10: Footer badge 本地化

**Depends on:** Task 9

**Files:**
- Modify: `apps/ody-code/src/tui/components/chrome/footer.ts` 48-74 (renderModeBadge)
- Test: `apps/ody-code/test/tui/components/chrome/footer.test.ts` +1 test

- [ ] Write the failing test — 追加到 `apps/ody-code/test/tui/components/chrome/footer.test.ts`（在 `describe('FooterComponent mode badge'` 内部）：

```typescript
  it('renders localized Chinese office-hours badge when userLanguage is zh', () => {
    const state: AppState = {
      ...baseAppState,
      sessionMode: 'office-hours',
      userLanguage: 'zh',
    };
    const footer = new FooterComponent(state, darkColors);
    const lines = footer.render(120);
    const line2 = stripAnsi(lines[1]!);
    expect(line2).toContain('办公时间');
    expect(line2).not.toContain('office-hours');
  });
```

- [ ] Run it and verify it FAILS:
```bash
pnpm --filter @odysseythink/ody-code test test/tui/components/chrome/footer.test.ts
```
Expected: 断言失败 — badge 仍显示 `office-hours`。

- [ ] Write the minimal implementation：

`apps/ody-code/src/tui/components/chrome/footer.ts`：

添加 import:
```typescript
import { t } from '@odysseythink/ody-code-sdk';
```

修改 `renderModeBadge` 函数（~line 48-74）—— 在构造 `label` 前本地化 `mode`：

```typescript
function renderModeBadge(
  mode: 'normal' | 'plan' | 'design' | 'office-hours',
  colors: ColorPalette,
  fileName?: string,
  userLanguage?: 'en' | 'zh' | undefined,
): string {
  const emoji = EMOJIS[mode] ?? '';
  const bgColor =
    mode === 'design'
      ? colors.accent
      : mode === 'plan'
        ? colors.primary
        : mode === 'office-hours'
          ? colors.warning
          : colors.textMuted;

  let textColor: string;
  try {
    textColor = luminance(bgColor) > 0.5 ? '#000000' : '#ffffff';
  } catch {
    textColor = '#ffffff';
  }

  const displayLabel = mode === 'office-hours'
    ? t('tui.footer.officeHours', userLanguage)
    : mode;
  const label = fileName
    ? `${emoji} ${displayLabel} · ${fileName}`
    : `${emoji} ${displayLabel}`;
  const padded = ` ${label} `;

  return chalk.bgHex(bgColor).hex(textColor)(`【${padded}】`);
}
```

修改调用点（~line 406）传入 `state.userLanguage`:

```typescript
    let badge = renderModeBadge(mode, colors, fileName ?? undefined, state.userLanguage);
```

- [ ] Run it and verify it PASSES:
```bash
pnpm --filter @odysseythink/ody-code test test/tui/components/chrome/footer.test.ts
```
Expected: 现有 10 tests + 1 new test = 11 tests PASS。

- [ ] Commit: `git add -A && git commit -m "feat: localize office-hours footer badge via t()"`

---

### Task 11: Status panel 本地化 + 全量测试/类型检查收尾

**Depends on:** Task 9, Task 10

**Files:**
- Modify: `apps/ody-code/src/tui/components/messages/status-panel.ts` 91-150
- Test: `apps/ody-code/test/tui/components/messages/status-panel.test.ts` +1 test

- [ ] Write the failing test — 追加到 `apps/ody-code/test/tui/components/messages/status-panel.test.ts`（在 `describe('status panel report lines'` 内部）：

```typescript
  it('localizes Office Hours row to Chinese when status has userLanguage zh', () => {
    const lines = buildStatusReportLines({
      colors: darkColors,
      version: '1.2.3',
      model: 'k2',
      workDir: '/tmp/project',
      sessionId: 'ses-1',
      sessionTitle: null,
      thinking: true,
      permissionMode: 'manual',
      sessionMode: 'office-hours',
      contextUsage: 0.25,
      contextTokens: 2500,
      maxContextTokens: 10000,
      availableModels: {
        k2: { provider: 'managed:ody-code', model: 'kimi-k2', maxContextSize: 10000, displayName: 'Kimi K2' },
      },
      status: {
        model: 'k2',
        thinkingLevel: 'high',
        permission: 'auto',
        sessionMode: 'office-hours',
        contextTokens: 2500,
        maxContextTokens: 10000,
        contextUsage: 0.25,
        userLanguage: 'zh',
      },
    }).map(strip);

    const output = lines.join('\n');
    expect(output).toContain('办公时间     开启');
  });

  it('shows English Office Hours label when userLanguage is undefined', () => {
    const lines = buildStatusReportLines({
      colors: darkColors,
      version: '1.2.3',
      model: 'k2',
      workDir: '/tmp/project',
      sessionId: 'ses-1',
      sessionTitle: null,
      thinking: true,
      permissionMode: 'manual',
      sessionMode: 'office-hours',
      contextUsage: 0.25,
      contextTokens: 2500,
      maxContextTokens: 10000,
      availableModels: {
        k2: { provider: 'managed:ody-code', model: 'kimi-k2', maxContextSize: 10000, displayName: 'Kimi K2' },
      },
      status: {
        model: 'k2',
        thinkingLevel: 'high',
        permission: 'auto',
        sessionMode: 'office-hours',
        contextTokens: 2500,
        maxContextTokens: 10000,
        contextUsage: 0.25,
      },
    }).map(strip);

    const output = lines.join('\n');
    expect(output).toContain('Office Hours   on');
  });
```

- [ ] Run it and verify it FAILS:
```bash
pnpm --filter @odysseythink/ody-code test test/tui/components/messages/status-panel.test.ts
```
Expected: 断言失败 — Office Hours 行显示英文标签。

- [ ] Write the minimal implementation：

`apps/ody-code/src/tui/components/messages/status-panel.ts`：

添加 import:
```typescript
import { t } from '@odysseythink/ody-code-sdk';
```

修改 `buildStatusReportLines`（~line 101-109）—— 读取语言并本地化 Office Hours 行：

```typescript
  const lang = options.status?.userLanguage;
  const sessionMode = options.status?.sessionMode ?? options.sessionMode;
  const sessionId = options.sessionId.trim().length > 0 ? options.sessionId : 'none';
  const rows: FieldRow[] = [
    { label: 'Model', value: formatModelStatus(options) },
    { label: 'Directory', value: options.workDir },
    { label: 'Permissions', value: permission },
    { label: 'Plan mode', value: sessionMode === 'plan' ? 'on' : 'off' },
    { label: 'Design mode', value: sessionMode === 'design' ? 'on' : 'off' },
    { label: t('tui.statusPanel.officeHours', lang), value: sessionMode === 'office-hours' ? t('tui.statusPanel.on', lang) : t('tui.statusPanel.off', lang) },
    { label: 'Session', value: sessionId },
  ];
```

- [ ] Run it and verify it PASSES:
```bash
pnpm --filter @odysseythink/ody-code test test/tui/components/messages/status-panel.test.ts
```
Expected: 4 tests PASS（原有 2 + 新增 2）。

- [ ] 全量回归测试：
```bash
pnpm --filter @odysseythink/agent-core test
pnpm --filter @odysseythink/ody-code test
pnpm -r typecheck
```
Expected: 全部测试 PASS，类型检查零错误。

- [ ] Commit: `git add -A && git commit -m "feat: localize /status Office Hours row labels via t()"`

---

## Part 3 Local Self-Review

- [ ] 2. Placeholder scan：所有步骤包含完整代码/测试/命令，无 `TODO`/`TBD`。
- [ ] 3. No phantom tasks：3 个任务均产生文件修改与新增测试。
- [ ] 4. Dependency soundness：Task 9 依赖前序 Parts 完成的 Agent 修改；Task 10、11 依赖 Task 9 的 AppState 类型更新。
- [ ] 5. Caller & build soundness：Task 9 修改了 `AgentStatusUpdatedEvent`、`SessionStatus`、`AppState`、`AgentAPI` 四个共享类型。`AgentStatusUpdatedEvent` 的新字段 `userLanguage` 是可选的，现有使用该事件的所有调用方（`session-event-handler`、测试）均已完成更新覆盖。`AgentAPI` 新增 `getUserLanguage` 方法，已在 `agent.rpcMethods` 实现，`node-sdk/src/rpc.ts` 中已调用。Task 9 以 `pnpm -r typecheck` 收尾，确保全树编译通过。同一个签名只在本 Task 变更一次。
- [ ] 6. Test-the-risk：TUI 事件透传、footer badge、status panel 的中文/英文/回退场景均有行为断言。
- [ ] 7. Type consistency：`userLanguage` 字段名与类型（`SupportedLanguage | undefined`）在 `AgentStatusUpdatedEvent`、`SessionStatus`、`AppState` 中保持一致。
