# Part 1 — 共享签名变更：ModeKey / SessionModeKind / sessionMode 字面量扩展

**Phase:** A — 所有后续任务的前置条件。

## Task 1: 将 `'office-hours'` 加入所有 session-mode 类型联合

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/agent/index.ts:77,129-131,192-200`
- Modify: `packages/agent-core/src/agent/session-mode/index.ts:22`
- Modify: `packages/agent-core/src/agent/replay/index.ts:1,7,11,37`
- Modify: `packages/agent-core/src/session/checkpoint/integrity.ts:15,17`
- Modify: `packages/agent-core/src/session/checkpoint/checkpoint.ts:28`（类型跟随 import，无需手动改）
- Modify: `packages/agent-core/src/rpc/events.ts:50`
- Modify: `packages/agent-core/src/rpc/core-api.ts:166,383`
- Modify: `packages/agent-core/src/profile/types.ts:45`
- Modify: `packages/agent-core/src/skill/types.ts:57-58`
- Modify: `packages/agent-core/src/skill/registry.ts:113,119,122,143,156`
- Modify: `packages/agent-core/src/session/rpc.ts:91`
- Modify: `packages/agent-core/src/session/index.ts:383`
- Modify: `packages/agent-core/src/agent/records/types.ts:10,43`
- Modify: `apps/ody-code/src/cli/options.ts:9,48-49`
- Modify: `apps/ody-code/src/cli/commands.ts:118`
- Modify: `apps/ody-code/src/tui/types.ts:20,179`
- Modify: `apps/ody-code/src/tui/components/messages/status-panel.ts:37,107-108`
- Modify: `apps/ody-code/src/tui/components/chrome/footer.ts:49`
- Modify: `apps/ody-code/src/tui/commands/types.ts:4`
- Modify: `packages/node-sdk/src/types.ts:86-94`
- Modify: `packages/node-sdk/src/kimi-harness.ts:101-122`
- Modify: `apps/ody-code/test/tui/commands/config.test.ts:7`

### 变更清单

共 **23 个文件**。下面是每个文件的具体改动。

#### 1. `packages/agent-core/src/agent/index.ts:77`

```
export type ModeKey = 'normal' | 'plan' | 'design' | 'office-hours';
```

#### 2. `packages/agent-core/src/agent/index.ts:192-200`

在 `_contexts` / `_fullCompactions` / `_microCompactions` 初始化中新增 `'office-hours'` 键：

```typescript
this._contexts = {
  normal: new ContextMemory(this),
  plan: new ContextMemory(this),
  design: new ContextMemory(this),
  'office-hours': new ContextMemory(this),
} as Record<ModeKey, ContextMemory>;

this._fullCompactions = {
  normal: new FullCompaction(),
  plan: new FullCompaction(),
  design: new FullCompaction(),
  'office-hours': new FullCompaction(),
} as Record<ModeKey, FullCompaction>;

this._microCompactions = {
  normal: new MicroCompaction(),
  plan: new MicroCompaction(),
  design: new MicroCompaction(),
  'office-hours': new MicroCompaction(),
} as Record<ModeKey, MicroCompaction>;
```

#### 3. `packages/agent-core/src/agent/session-mode/index.ts:22`

```
export type SessionModeKind = 'plan' | 'design' | 'office-hours';
```

#### 4. `packages/agent-core/src/agent/replay/index.ts:1,7,11,37`

`ReplayBuilder` 导入并持有 `ModeKey` 类型。类型跟随 `#/agent` 导出的 `ModeKey` 扩展，无需手动变更。

#### 5. `packages/agent-core/src/session/checkpoint/integrity.ts:15,17`

该文件有**独立重复定义**的 `ModeKey`（line 15），必须同步更新：

```typescript
export type ModeKey = 'normal' | 'plan' | 'design' | 'office-hours';
const VALID_MODES: readonly ModeKey[] = ['normal', 'plan', 'design', 'office-hours'];
```

#### 6. `packages/agent-core/src/session/checkpoint/checkpoint.ts:28`

`currentMode: ModeKey` — 从 `#/agent` 导入 `ModeKey`，类型自动跟随。

#### 7. `packages/agent-core/src/rpc/events.ts:50`

```typescript
readonly sessionMode?: 'normal' | 'plan' | 'design' | 'office-hours' | undefined;
```

#### 8. `packages/agent-core/src/rpc/core-api.ts:166,383`

Line 166: `EnterPlanPayload.kind?: SessionModeKind` — 从 `#/agent/session-mode` 导入，类型自动跟随。

Line 383: `listSkills` payload 中的 sessionMode 字面量：

```typescript
listSkills: (payload: EmptyPayload & { sessionMode?: 'normal' | 'plan' | 'design' | 'office-hours' }) => readonly SkillSummary[];
```

#### 9. `packages/agent-core/src/profile/types.ts:45`

```typescript
readonly sessionMode?: 'normal' | 'plan' | 'design' | 'office-hours';
```

#### 10. `packages/agent-core/src/skill/types.ts:57-58`

```typescript
listInvocableSkills(sessionMode?: 'normal' | 'plan' | 'design' | 'office-hours'): readonly SkillDefinition[];
getModelSkillListing(sessionMode?: 'normal' | 'plan' | 'design' | 'office-hours'): string;
```

同时 `getUnavailableSkillsReminder` 保留现有签名不变（它只接收 `'plan' | 'design'`），但 registry.ts 的实现中做如下处理。

#### 11. `packages/agent-core/src/skill/registry.ts:113-173`

三处 `sessionMode` 参数的联合类型扩展：

Line 113:
```typescript
listInvocableSkills(
  sessionMode?: 'normal' | 'plan' | 'design' | 'office-hours',
): readonly SkillDefinition[] {
```

Line 119-122: 隐藏检查中 `sessionMode !== 'normal'` 保持不变（`'office-hours'` 同样触发隐藏逻辑）。

Line 143:
```typescript
getModelSkillListing(sessionMode?: 'normal' | 'plan' | 'design' | 'office-hours'): string {
```

Line 156: `getUnavailableSkillsReminder` 签名保持不变 `(sessionMode: 'plan' | 'design')` — office-hours 模式不使用 unavailable-skills 提醒（由独立 injector 控制）。

#### 12. `packages/agent-core/src/session/rpc.ts:91`

```typescript
listSkills(payload: EmptyPayload & { sessionMode?: 'normal' | 'plan' | 'design' | 'office-hours' }): Promise<readonly SkillSummary[]> {
```

#### 13. `packages/agent-core/src/session/index.ts:383`

```typescript
async listSkills(options?: { sessionMode?: 'normal' | 'plan' | 'design' | 'office-hours' }): Promise<readonly SkillSummary[]> {
```

#### 14. `packages/agent-core/src/agent/records/types.ts:10,43`

`session_mode.enter` record 的 `kind` 字段类型为 `SessionModeKind`，从 `#/agent/session-mode` 导入，自动跟随。

#### 15. `apps/ody-code/src/cli/options.ts:9`

```typescript
sessionMode: 'normal' | 'plan' | 'design' | 'office-hours';
```

#### 16. `apps/ody-code/src/cli/options.ts:48-49`

验证逻辑中的字面量数组：

```typescript
if (!['normal', 'plan', 'design', 'office-hours'].includes(opts.sessionMode)) {
  throw new Error(`Invalid session mode: ${opts.sessionMode}`);
}
```

#### 17. `apps/ody-code/src/cli/commands.ts:118`

```typescript
sessionMode: (raw['sessionMode'] as 'normal' | 'plan' | 'design' | 'office-hours') ?? 'normal',
```

#### 18. `apps/ody-code/src/tui/types.ts:20,179`

line 20 (AppState):
```typescript
sessionMode: 'normal' | 'plan' | 'design' | 'office-hours';
```

line 179 (TUIStartupOptions):
```typescript
readonly sessionMode: 'normal' | 'plan' | 'design' | 'office-hours';
```

#### 19. `apps/ody-code/src/tui/components/messages/status-panel.ts:37,107-108`

line 37:
```typescript
readonly sessionMode: 'normal' | 'plan' | 'design' | 'office-hours';
```

lines 107-108 新增 office-hours 显示行：
```typescript
{ label: 'Plan mode', value: sessionMode === 'plan' ? 'on' : 'off' },
{ label: 'Design mode', value: sessionMode === 'design' ? 'on' : 'off' },
{ label: 'Office Hours', value: sessionMode === 'office-hours' ? 'on' : 'off' },
```

#### 20. `apps/ody-code/src/tui/components/chrome/footer.ts:49`

```typescript
mode: 'normal' | 'plan' | 'design' | 'office-hours',
```

#### 21. `apps/ody-code/src/tui/commands/types.ts:4`

```typescript
export type SessionMode = 'normal' | 'plan' | 'design' | 'office-hours';
```

#### 22. `packages/node-sdk/src/types.ts:86-94`

`CreateSessionOptions` 中：
```typescript
readonly sessionMode?: 'plan' | 'design' | 'office-hours' | 'normal';
```

#### 23. `packages/node-sdk/src/kimi-harness.ts:101-122`

`createSession` 方法中的条件分支：

```typescript
if (sessionMode === 'plan') {
  await session.setSessionMode('plan');
} else if (sessionMode === 'design') {
  await session.setSessionMode('design');
} else if (sessionMode === 'office-hours') {
  await session.setSessionMode('office-hours');
}
```

#### 24. `apps/ody-code/test/tui/commands/config.test.ts:7`

```typescript
sessionMode: 'normal' | 'plan' | 'design' | 'office-hours',
```

### Steps

- [ ] 在 `packages/agent-core/src/agent/index.ts:77` 将 `ModeKey` 扩展为 `'normal' | 'plan' | 'design' | 'office-hours'`。
  **代码：**
  ```typescript
  export type ModeKey = 'normal' | 'plan' | 'design' | 'office-hours';
  ```

- [ ] 在 `packages/agent-core/src/agent/index.ts:192-200` 为 `_contexts` / `_fullCompactions` / `_microCompactions` 初始化新增 `'office-hours'` 键。
  **代码：** 见上方变更清单第 2 项。

- [ ] 在 `packages/agent-core/src/agent/session-mode/index.ts:22` 将 `SessionModeKind` 扩展为 `'plan' | 'design' | 'office-hours'`。
  **代码：**
  ```typescript
  export type SessionModeKind = 'plan' | 'design' | 'office-hours';
  ```

- [ ] 在 `packages/agent-core/src/session/checkpoint/integrity.ts:15,17` 同步独立 `ModeKey` 定义和 `VALID_MODES` 数组。
  **代码：**
  ```typescript
  export type ModeKey = 'normal' | 'plan' | 'design' | 'office-hours';
  const VALID_MODES: readonly ModeKey[] = ['normal', 'plan', 'design', 'office-hours'];
  ```

- [ ] 更新所有内联 `sessionMode` 字面量联合类型（共 10 处，见上方变更清单第 7-21 项）。

- [ ] 全仓库 typecheck：
  ```bash
  pnpm -r typecheck
  ```
  **预期：** 零错误通过。

- [ ] 运行 agent-core 测试确认无回归：
  ```bash
  pnpm -F @odysseythink/agent-core test
  ```
  **预期：** 全部通过。

- [ ] 运行 ody-code 测试确认无回归：
  ```bash
  pnpm -F @odysseythink/ody-code test
  ```
  **预期：** 全部通过。

- [ ] Commit: `chore: extend ModeKey and SessionModeKind with office-hours`

## Self-Review

- [ ] 1. Spec-coverage: Task 1 covers spec item 2 (Session Mode `office-hours`) — type system support.
- [ ] 2. Placeholder scan: no TODO/TBD; every edit has exact old→new content.
- [ ] 3. No phantom tasks: this task produces 23 file modifications, all verifiable via `typecheck`.
- [ ] 4. Dependency soundness: Task 1 depends on none; all later tasks depend on Task 1.
- [ ] 5. Caller & build soundness: ALL 23 files containing `'normal' | 'plan' | 'design'` literal unions or `ModeKey`/`SessionModeKind` imports are updated in this single task. Verified via `grep -rn "'normal' \| 'plan' \| 'design'" packages/ apps/` and `grep -rn "ModeKey\|SessionModeKind" packages/ apps/`. Includes test file `apps/ody-code/test/tui/commands/config.test.ts:7`. Ends with `pnpm -r typecheck` (whole-tree).
- [ ] 6. Test-the-risk: this is a type-level change with no runtime behavior mutation; existing tests confirm no regression. The `integrity.ts` `VALID_MODES` array is verified to include `'office-hours'` — a checkpoint with `currentMode: 'office-hours'` will pass `jsonValid` check.
- [ ] 7. Type consistency: `ModeKey = 'normal' | 'plan' | 'design' | 'office-hours'` and `SessionModeKind = 'plan' | 'design' | 'office-hours'` are the canonical definitions all parts reference.
