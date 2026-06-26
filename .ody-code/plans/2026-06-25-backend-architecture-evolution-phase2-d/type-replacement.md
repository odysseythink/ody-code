# Part 6: 全仓库 mode 字面量替换

> **Scope**: 将 `Agent.ModeKey` 及散落在 RPC、Skill、Profile、Session、Checkpoint、node-sdk、apps/ody-code 中的 mode 字符串字面量统一收敛到 `RuntimeMode` / `SessionModeKind`；本次为共享签名变更，必须在一个任务内完成全部调用方迁移并以全树 typecheck 收尾。

## File Structure

| File | Responsibility |
|---|---|
| `packages/agent-core/src/agent/index.ts` | 移除 `ModeKey`，重导出 `RuntimeMode`；内部字段/方法全部改用 `RuntimeMode` |
| `packages/agent-core/src/agent/replay/index.ts` | `ReplayBuilder` 改用 `RuntimeMode` |
| `packages/agent-core/src/session/checkpoint/checkpoint.ts` | `currentMode` 改用 `RuntimeMode` |
| `packages/agent-core/src/session/checkpoint/integrity.ts` | 移除本地 `ModeKey`，校验数组纳入 `game-design` |
| `packages/agent-core/src/rpc/resumed.ts` | `AgentReplayRecord.mode` 改用 `RuntimeMode` |
| `packages/agent-core/src/profile/types.ts` | `SystemPromptContext.sessionMode` 改用 `RuntimeMode` |
| `packages/agent-core/src/skill/types.ts` | `hiddenInModes` / `SkillCatalog` 改用 `RuntimeMode` |
| `packages/agent-core/src/skill/registry.ts` | 技能过滤 API 参数改用 `RuntimeMode` |
| `packages/agent-core/src/rpc/core-api.ts` | `SessionAPI.listSkills` sessionMode 改用 `RuntimeMode` |
| `packages/agent-core/src/rpc/events.ts` | `AgentStatusUpdatedEvent.sessionMode` 改用 `RuntimeMode` |
| `packages/agent-core/src/rpc/core-impl.ts` | `currentMode` 推导与 `buildResultForMode` 调用适配 |
| `packages/agent-core/src/session/rpc.ts` | `SessionAPIImpl.listSkills` 参数改用 `RuntimeMode` |
| `packages/agent-core/src/session/index.ts` | `Session.listSkills` 参数改用 `RuntimeMode` |
| `packages/agent-core-shared/src/config.ts` | 确认 `sessionMode` / `defaultSessionMode` schema 已是 `RuntimeMode` |
| `packages/node-sdk/src/types.ts` | `CreateSessionOptions` / `SessionStatus.sessionMode` 改用 `RuntimeMode` |
| `packages/node-sdk/src/rpc.ts` | `SetSessionModeRpcInput.mode` 改用 `RuntimeMode` |
| `packages/node-sdk/src/index.ts` | 重导出 `RuntimeMode` 供 apps/ody-code 使用 |
| `apps/ody-code/src/cli/options.ts` | CLI `--session-mode` 类型与校验纳入 `game-design` |
| `apps/ody-code/src/cli/commands.ts` | 类型断言改用 `RuntimeMode` |
| `apps/ody-code/src/cli/run-game-design.ts` | 移除 `as any` |
| `apps/ody-code/src/tui/commands/types.ts` | `SessionMode` 改为 `RuntimeMode` 别名 |
| 全仓库测试文件 | 同步替换 `ModeKey` 与 mode 字面量类型 |

## Dependency Overview

```
Part 1 (types.md) ──┐
Part 2 (config.md) ─┼─► Part 6 (type-replacement.md) ──► Part 7 (tests-docs.md)
Part 5 (session-mode.md) ──┘
```

- 依赖 Part 1：已产出 `RuntimeMode`、`SessionModeKind`、守卫函数并从 `agent/session-mode` 导出。
- 依赖 Part 2：已扩展 `OdyConfigSchema` / `OdyConfigPatchSchema` 到 `RuntimeMode`。
- 依赖 Part 5：`SessionMode` 类已重构完成，内部 `kind` 等字段类型稳定。
- 本 Part 为单任务（共享签名不可拆分），结束后方可进入 Part 7 的集成测试与文档。

## Risks & Open Questions

- **R1**: `apps/ody-code` 本地 `SessionMode` 与 `RuntimeMode` 对齐后，依赖 `@odysseythink/ody-code-sdk` 导出 `RuntimeMode`；若未重导出，TUI 会编译失败。
- **R2**: `checkpoint/integrity.ts` 原 `VALID_MODES` 不含 `game-design`，扩展后需确认旧 checkpoint 不会因此被判定为非法（旧 checkpoint 不可能出现 `game-design`，因为此前 game-design 不写入 checkpoint）。
- **R3**: 全树 grep 可能遗漏字符串字面量；必须以 `pnpm -r typecheck` 为最终硬门。

---

### Task 6: 全仓库 mode 字面量替换（共享签名单一任务）

**Depends on:** Part 1 (`types.md`)、Part 2 (`config.md`)、Part 5 (`session-mode.md`)

**Files:**
- Modify: `packages/agent-core/src/agent/index.ts`
- Modify: `packages/agent-core/src/agent/replay/index.ts`
- Modify: `packages/agent-core/src/session/checkpoint/checkpoint.ts`
- Modify: `packages/agent-core/src/session/checkpoint/integrity.ts`
- Modify: `packages/agent-core/src/rpc/resumed.ts`
- Modify: `packages/agent-core/src/profile/types.ts`
- Modify: `packages/agent-core/src/skill/types.ts`
- Modify: `packages/agent-core/src/skill/registry.ts`
- Modify: `packages/agent-core/src/rpc/core-api.ts`
- Modify: `packages/agent-core/src/rpc/events.ts`
- Modify: `packages/agent-core/src/rpc/core-impl.ts`
- Modify: `packages/agent-core/src/session/rpc.ts`
- Modify: `packages/agent-core/src/session/index.ts`
- Modify: `packages/agent-core-shared/src/config.ts`（仅核对，若 Part 2 已改）
- Modify: `packages/node-sdk/src/types.ts`
- Modify: `packages/node-sdk/src/rpc.ts`
- Modify: `packages/node-sdk/src/index.ts`
- Modify: `apps/ody-code/src/cli/options.ts`
- Modify: `apps/ody-code/src/cli/commands.ts`
- Modify: `apps/ody-code/src/cli/run-game-design.ts`
- Modify: `apps/ody-code/src/tui/commands/types.ts`
- Modify: 全仓库测试文件中引用 `ModeKey` 或 mode 字面量的位置
- Create: `packages/agent-core/test/agent/session-mode/runtime-mode-export.test.ts`

> **共享签名规则**：本任务一次性替换所有被多个调用方共享的 mode 类型/字面量，并在最后运行 `pnpm -r typecheck`。不可把同一签名拆到多个任务。

#### Step 1: 编写失败测试，确认 `Agent.RuntimeMode` 尚未导出

- [ ] 在 `packages/agent-core/test/agent/session-mode/runtime-mode-export.test.ts` 写入：

```ts
import { expect, it } from 'vitest';
import { RuntimeMode, isRuntimeMode, normalizeRuntimeMode } from '../../../src/agent/session-mode';
import { Agent } from '../../../src/agent';

it('RuntimeMode is exported and includes all five values', () => {
  expect(isRuntimeMode('normal')).toBe(true);
  expect(isRuntimeMode('plan')).toBe(true);
  expect(isRuntimeMode('design')).toBe(true);
  expect(isRuntimeMode('office-hours')).toBe(true);
  expect(isRuntimeMode('game-design')).toBe(true);
  expect(isRuntimeMode('foo')).toBe(false);
  expect(normalizeRuntimeMode('foo')).toBe('normal');
});

it('Agent namespace exposes RuntimeMode', () => {
  // 类型级断言：若 Agent.RuntimeMode 未导出，此行编译失败。
  type T = Agent.RuntimeMode;
  const check: T = 'normal';
  expect(check).toBe('normal');
});
```

- [ ] 运行测试并确认编译失败：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm vitest run packages/agent-core/test/agent/session-mode/runtime-mode-export.test.ts
```

**Expected failure**: `Property 'RuntimeMode' does not exist on type 'typeof Agent'` 或类似 TS2339 错误（因 `Agent` 尚未重导出 `RuntimeMode`）。

#### Step 2: 在 `agent-core` 内部收敛到 `RuntimeMode`

- [ ] 修改 `packages/agent-core/src/agent/index.ts`：
  - 删除第 83 行本地类型 `export type ModeKey = 'normal' | 'plan' | 'design' | 'office-hours' | 'game-design';`
  - 在导出区新增重导出：

```ts
export type { RuntimeMode } from './session-mode';
```

  - 将第 51 行 `import { SessionMode } from './session-mode';` 改为：

```ts
import { SessionMode, type RuntimeMode } from './session-mode';
```

  - 替换内部所有 `ModeKey` 为 `RuntimeMode`：
    - 第 142 行 `private readonly _contexts: Record<ModeKey, ContextMemory>;`
    - 第 143 行 `private readonly _fullCompactions: Record<ModeKey, FullCompaction>;`
    - 第 144 行 `private readonly _microCompactions: Record<ModeKey, MicroCompaction>;`
    - 第 152 行 `private _activeMode: ModeKey = 'normal';`
    - 第 157 行 `private _pendingContextSwitch: ModeKey | null = null;`
    - 第 217、224、231 行 `as Record<ModeKey, ...>`
    - 第 278 行 `get contexts(): Readonly<Record<ModeKey, ContextMemory>>`
    - 第 295 行 `setContextMode(mode: ModeKey): void`

- [ ] 修改 `packages/agent-core/src/agent/replay/index.ts`：

```ts
import type { Agent, RuntimeMode } from '..';

export class ReplayBuilder {
  private _mode: RuntimeMode = 'normal';

  setMode(mode: RuntimeMode): void {
    this._mode = mode;
  }

  buildResultForMode(mode: RuntimeMode): readonly AgentReplayRecord[] {
    return this.records.filter((r) => {
      if (r.type !== 'message') return true;
      return r.mode === mode;
    });
  }
}
```

- [ ] 修改 `packages/agent-core/src/session/checkpoint/checkpoint.ts`：

```ts
import type { RuntimeMode } from '../../agent/session-mode';

export interface SessionCheckpointPayload {
  // ...
  currentMode: RuntimeMode;
  // ...
}
```

- [ ] 修改 `packages/agent-core/src/session/checkpoint/integrity.ts`：
  - 删除本地 `ModeKey` 类型与 `VALID_MODES`。
  - 导入 `RuntimeMode` 与 `RUNTIME_MODES`：

```ts
import { RuntimeMode, RUNTIME_MODES } from '../../agent/session-mode';
```

  - 将校验改为：

```ts
const currentMode = typed.currentMode as string | undefined;
if (!RUNTIME_MODES.includes(currentMode as RuntimeMode)) {
  errors.push(`Invalid currentMode: ${String(currentMode)}`);
  ok = false;
}
```

- [ ] 修改 `packages/agent-core/src/rpc/resumed.ts`：

```ts
import type { RuntimeMode } from '#/agent/session-mode';

export type AgentReplayRecord =
  | { type: 'message'; message: ContextMessage; mode?: RuntimeMode }
  | { type: 'session_mode_updated'; enabled: boolean; kind?: SessionModeKind }
  | // ...
```

- [ ] 修改 `packages/agent-core/src/profile/types.ts`：

```ts
import type { RuntimeMode } from '../agent/session-mode';

export interface SystemPromptContext {
  // ...
  readonly sessionMode?: RuntimeMode;
}
```

- [ ] 修改 `packages/agent-core/src/skill/types.ts`：

```ts
import type { RuntimeMode } from '../agent/session-mode';

export interface SkillMetadata {
  // ...
  readonly hiddenInModes?: readonly RuntimeMode[] | undefined;
  // ...
}

export interface SkillCatalog {
  // ...
  listInvocableSkills(sessionMode?: RuntimeMode): readonly SkillDefinition[];
  getModelSkillListing(sessionMode?: RuntimeMode): string;
}
```

- [ ] 修改 `packages/agent-core/src/skill/registry.ts`：
  - 导入 `RuntimeMode`。
  - 替换 `listInvocableSkills`、`getModelSkillListing`、`getUnavailableSkillsReminder` 的签名：

```ts
listInvocableSkills(sessionMode?: RuntimeMode): readonly SkillDefinition[] { /* ... */ }
getModelSkillListing(sessionMode?: RuntimeMode): string { /* ... */ }
getUnavailableSkillsReminder(sessionMode: RuntimeMode): string { /* ... */ }
```

- [ ] 修改 `packages/agent-core/src/rpc/core-api.ts`：
  - 导入 `RuntimeMode`。
  - 替换 `SessionAPI.listSkills` 签名：

```ts
listSkills: (payload: EmptyPayload & { sessionMode?: RuntimeMode }) => readonly SkillSummary[];
```

- [ ] 修改 `packages/agent-core/src/rpc/events.ts`：

```ts
import type { RuntimeMode } from '../agent/session-mode';

export interface AgentStatusUpdatedEvent {
  // ...
  readonly sessionMode?: RuntimeMode | undefined;
  // ...
}
```

- [ ] 修改 `packages/agent-core/src/rpc/core-impl.ts`：
  - 将第 995–1001 行的 `currentMode` 推导改为显式 `RuntimeMode` 类型：

```ts
const currentMode: RuntimeMode = main.sessionMode.isActive ? main.sessionMode.kind : 'normal';
```

  - 第 1098–1100 行的 `buildResultForMode('normal')` 等调用保持原值，类型已收敛为 `RuntimeMode`。

- [ ] 修改 `packages/agent-core/src/session/rpc.ts`：

```ts
listSkills(payload: EmptyPayload & { sessionMode?: RuntimeMode }): Promise<readonly SkillSummary[]> {
  // ...
}
```

- [ ] 修改 `packages/agent-core/src/session/index.ts`：

```ts
async listSkills(options?: { sessionMode?: RuntimeMode }): Promise<readonly SkillSummary[]> {
  // ...
}
```

- [ ] 核对 `packages/agent-core-shared/src/config.ts` 中 `sessionMode` / `defaultSessionMode` schema 已是 `RuntimeMode`（由 Part 2 完成）。若 Part 2 尚未改，则改为：

```ts
import { RuntimeModeSchema } from '@odysseythink/agent-core'; // 或本地定义 z.enum(RUNTIME_MODES)

sessionMode: RuntimeModeSchema.optional(),
defaultSessionMode: RuntimeModeSchema.optional(),
```

并在 `OdyConfigPatchSchema` 中同步。

#### Step 3: 更新 `node-sdk` 类型并重导出 `RuntimeMode`

- [ ] 修改 `packages/node-sdk/src/types.ts`：

```ts
import type { RuntimeMode } from '@odysseythink/agent-core';

export interface CreateSessionOptions {
  // ...
  readonly sessionMode?: RuntimeMode;
  // ...
}

export interface SessionStatus {
  // ...
  readonly sessionMode: RuntimeMode;
  // ...
}
```

- [ ] 修改 `packages/node-sdk/src/rpc.ts`：

```ts
import type { RuntimeMode } from '@odysseythink/agent-core';

export interface SetSessionModeRpcInput extends SessionIdRpcInput {
  readonly mode: RuntimeMode;
  readonly sourceFilePath?: string;
}
```

- [ ] 修改 `packages/node-sdk/src/index.ts`，在重导出区新增：

```ts
export type { RuntimeMode, SessionModeKind } from '@odysseythink/agent-core';
```

#### Step 4: 更新 `apps/ody-code` 的 mode 类型

- [ ] 修改 `apps/ody-code/src/cli/options.ts`：
  - 导入 `RuntimeMode`：

```ts
import type { RuntimeMode } from '@odysseythink/ody-code-sdk';
```

  - 第 9 行改为：

```ts
export interface CliOptions {
  // ...
  sessionMode: RuntimeMode;
  // ...
}
```

  - 第 50 行校验数组纳入 `game-design`：

```ts
if (!['normal', 'plan', 'design', 'office-hours', 'game-design'].includes(opts.sessionMode)) {
  throw new OptionConflictError(
    `Invalid --session-mode: ${opts.sessionMode}. Must be normal, plan, design, office-hours, or game-design.`,
  );
}
```

- [ ] 修改 `apps/ody-code/src/cli/commands.ts` 第 134 行：

```ts
sessionMode: (raw['sessionMode'] as RuntimeMode) ?? 'normal',
```

- [ ] 修改 `apps/ody-code/src/cli/run-game-design.ts` 第 66 行，移除 `as any`：

```ts
cliOptions: { ...opts, sessionMode: 'game-design', gameDesign: true },
```

- [ ] 修改 `apps/ody-code/src/tui/commands/types.ts`：

```ts
import type { RuntimeMode } from '@odysseythink/ody-code-sdk';

export type SessionMode = RuntimeMode;
```

保持 `KimiSlashCommand.hiddenInModes` 类型为 `readonly SessionMode[]`，从而一次性让全 TUI 的 `hiddenInModes` 数组获得 `RuntimeMode` 类型约束。

#### Step 5: 迁移全仓库测试文件

- [ ] 搜索所有仍在引用 `ModeKey` 或旧 mode 字面量联合类型的测试与源码：

```bash
cd /Users/ranwei/workspace/ody-code
rg "ModeKey" packages/ apps/ --type ts --type tsx
rg "'normal' \| 'plan' \| 'design' \| 'office-hours'" packages/ apps/ --type ts --type tsx
rg "'normal' \| 'plan' \| 'design' \| 'office-hours' \| 'game-design'" packages/ apps/ --type ts --type tsx
```

- [ ] 对每个命中文件，按以下规则替换：
  - `ModeKey` 导入/类型 → `RuntimeMode`。
  - 五值 mode 联合类型 → `RuntimeMode`。
  - 四值 `SessionModeKind` 联合类型 → `SessionModeKind`（保持用于仅交互阶段的语义）。
  - `AgentReplayRecord.mode?: string` → `RuntimeMode`。

- [ ] 特别注意以下高频测试文件（已确认当前引用 mode 类型）：
  - `packages/agent-core/test/agent/session-mode.test.ts`
  - `packages/agent-core/test/agent/session-mode-writing-plan.test.ts`
  - `packages/agent-core/test/agent/injection/plan-mode.test.ts`
  - `packages/agent-core/test/agent/injection/design-mode.test.ts`
  - `packages/agent-core/test/agent/injection/game-design.test.ts`
  - `packages/agent-core/test/agent/injection/knowledge-microagent.test.ts`
  - `packages/agent-core/test/agent/plan.test.ts`
  - `packages/agent-core/test/agent/resume.test.ts`
  - `packages/agent-core/test/agent/tool.test.ts`
  - `packages/agent-core/test/agent/turn.test.ts`
  - `packages/agent-core/test/agent/basic.test.ts`
  - `packages/agent-core/test/agent/permission.test.ts`
  - `packages/agent-core/test/agent/config.test.ts`
  - `packages/agent-core/test/session/checkpoint/resume.test.ts`
  - `packages/agent-core/test/session/checkpoint/coordinator.test.ts`
  - `packages/agent-core/test/tools/show-design-mockup.test.ts`
  - `packages/agent-core/test/tools/builtin-current.test.ts`
  - `packages/agent-core/test/tools/builtin/office-hours/*.test.ts`
  - `packages/agent-core/test/tools/builtin/game-design/*.test.ts`
  - `packages/integration-tests/test/e2e-testing/*.test.ts`

#### Step 6: 运行新增测试与全树类型检查

- [ ] 运行新增测试，确认 `Agent.RuntimeMode` 导出成功：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm vitest run packages/agent-core/test/agent/session-mode/runtime-mode-export.test.ts
```

**Expected output**: 2 passing。

- [ ] 运行全树 typecheck（共享签名变更的硬门）：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm -r typecheck
```

**Expected output**: 所有 workspace `typecheck` 通过，无 `ModeKey` 相关错误。

- [ ] 手动验证无残留 `ModeKey` 与旧字面量：

```bash
cd /Users/ranwei/workspace/ody-code
rg "ModeKey" packages/ apps/ --type ts --type tsx
rg "'normal' \| 'plan' \| 'design' \| 'office-hours'" packages/ apps/ --type ts --type tsx
rg "'normal' \| 'plan' \| 'design' \| 'office-hours' \| 'game-design'" packages/ apps/ --type ts --type tsx
```

**Expected observation**: 仅命中 `docs/`、`docs/plans/`、`.ody-code/plans/`、`.changeset/` 等文档/计划文件，源码与测试中无命中。若仍有命中，返回 Step 5 修复。

- [ ] 提交：

```bash
git add -A
git commit -m "refactor(agent-core): converge mode literals to RuntimeMode across the tree"
```

---

## Local Self-Review

- [ ] 1. **Spec-coverage table**:
  - `Agent.ModeKey` → `Agent.RuntimeMode`：Task 6 Step 2 covered。
  - `SystemPromptContext.sessionMode` → `RuntimeMode`：Task 6 Step 2 covered。
  - `SkillCatalog` 参数 → `RuntimeMode`：Task 6 Step 2 covered。
  - `CoreAPI/SessionAPI.listSkills sessionMode` → `RuntimeMode`：Task 6 Step 2 covered。
  - Checkpoint `currentMode` 与 `VALID_MODES` 纳入 `game-design`：Task 6 Step 2 covered。
  - `OdyConfig.sessionMode/defaultSessionMode` → `RuntimeMode`：Part 2 + Task 6 Step 2 核对 covered。
  - `node-sdk` / `apps/ody-code` mode 类型对齐：Task 6 Step 3/4 covered。
  - 测试文件同步：Task 6 Step 5 covered。
- [ ] 2. **Placeholder scan**：无 TODO/TBD；每处修改均给出具体文件、行号、代码。
- [ ] 3. **No phantom tasks**：Task 6 产生可验证变更（测试文件 + 全树 typecheck + 无残留 grep），不以 `--allow-empty` 提交。
- [ ] 4. **Dependency soundness**：Task 6 仅依赖 Part 1/2/5 已产出的 `RuntimeMode` / 扩展 schema / 重构后 `SessionMode`；未引用 Part 7 内容。
- [ ] 5. **Caller & build soundness**：Task 6 在单任务内更新所有共享 mode 签名的调用方（含测试），并以 `pnpm -r typecheck` 收尾；同一签名未跨任务拆分。
- [ ] 6. **Test-the-risk**：新增测试断言 `isRuntimeMode` 对全部 5 个合法值返回 true、对非法值返回 false，并断言 `normalizeRuntimeMode('foo') === 'normal'`；`Agent.RuntimeMode` 类型级断言确保导出正确。
- [ ] 7. **Type consistency**：`RuntimeMode` 来自 Part 1 定义的 `RUNTIME_MODES`；`SessionModeKind` 仍用于仅交互阶段场景（`EnterPlanPayload.kind`、`session_mode_updated.kind`、`SessionModeData.kind` 等），与 Part 1 两层类型约定一致。
