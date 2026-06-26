# Part 7: 测试、文档与最终验收

> **Scope**: 补充 `RuntimeMode` / `ModeBehaviorRegistry` / `SessionModeInjector` / `SessionMode` 行为测试，撰写 `docs/architecture/modes-vs-profiles.md` 架构文档，并运行全树 typecheck + 目标测试集完成验收。

## File Structure

| File | Responsibility |
|---|---|
| `packages/agent-core/test/agent/session-mode-types.test.ts`（新） | 类型常量与守卫函数测试 |
| `packages/agent-core/test/agent/session-mode-behaviors.test.ts`（新） | `ModeBehaviorRegistry` 与 behavior 元数据测试 |
| `packages/agent-core/test/agent/injection/session-mode-injector.test.ts`（新） | `BaseSessionModeInjector.computeVariant` 与 `onContextClear` 测试 |
| `packages/agent-core/test/agent/session-mode.test.ts`（修改） | `enter()` 原子回滚、`exit()` 幂等性测试 |
| `docs/architecture/modes-vs-profiles.md`（新） | mode vs profile 架构文档与反向检查单 |

## Dependency Overview

```
Part 1 ──► Task 7.1 (mode-types tests)
Part 3 ──► Task 7.2 (behaviors tests)
Part 4 ──► Task 7.3 (injector tests)
Part 5 ──► Task 7.4 (session-mode integration tests)
Task 7.1/7.2/7.3/7.4/7.5 ──► Task 7.6 (final verification)
```

- Task 7.1 依赖 Part 1 产出的 `SESSION_MODE_KINDS` / `RUNTIME_MODES` / 守卫函数。
- Task 7.2 依赖 Part 3 产出的 `ModeBehaviorRegistry` 与四个 behavior 类。
- Task 7.3 依赖 Part 4 产出的 `BaseSessionModeInjector`。
- Task 7.4 依赖 Part 5 产出的重构后 `SessionMode`。
- Task 7.5 依赖前面所有 Part 的概念收敛结果。
- Task 7.6 依赖 7.1–7.5 全部完成。

## Risks & Open Questions

- **R1**: 测试文件放在 `packages/agent-core/test/...` 而非设计稿中的 `src/.../__tests__`，需遵循仓库现有约定。
- **R2**: `SessionMode.enter()` 失败回滚测试需要可控地让 `behavior.onEnter()` 抛错，因此需要注入自定义 behavior 或 mock `kaos.writeText`。
- **R3**: 架构文档若与后续实现细节冲突，应在验收前由人工核对。

---

### Task 7.1: 类型常量与守卫函数测试

**Depends on:** Part 1 (`types.md`)

**Files:**
- Create: `packages/agent-core/test/agent/session-mode-types.test.ts`

- [ ] 创建测试文件并写入：

```ts
import { describe, it, expect, vi } from 'vitest';
import {
  SESSION_MODE_KINDS,
  RUNTIME_MODES,
  isSessionModeKind,
  isRuntimeMode,
  normalizeRuntimeMode,
} from '../../src/agent/session-mode';

describe('mode type constants and guards', () => {
  it('SESSION_MODE_KINDS contains exactly the four interaction phases', () => {
    expect(SESSION_MODE_KINDS).toEqual(['plan', 'design', 'office-hours', 'game-design']);
  });

  it('RUNTIME_MODES includes all session mode kinds plus normal', () => {
    expect(RUNTIME_MODES).toEqual([...SESSION_MODE_KINDS, 'normal']);
  });

  it('isSessionModeKind accepts only the four kinds', () => {
    expect(isSessionModeKind('plan')).toBe(true);
    expect(isSessionModeKind('design')).toBe(true);
    expect(isSessionModeKind('office-hours')).toBe(true);
    expect(isSessionModeKind('game-design')).toBe(true);
    expect(isSessionModeKind('normal')).toBe(false);
    expect(isSessionModeKind('foo')).toBe(false);
  });

  it('isRuntimeMode accepts all five runtime values', () => {
    expect(isRuntimeMode('normal')).toBe(true);
    expect(isRuntimeMode('plan')).toBe(true);
    expect(isRuntimeMode('design')).toBe(true);
    expect(isRuntimeMode('office-hours')).toBe(true);
    expect(isRuntimeMode('game-design')).toBe(true);
    expect(isRuntimeMode('foo')).toBe(false);
  });

  it('normalizeRuntimeMode returns valid modes as-is and warns on unknown', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    expect(normalizeRuntimeMode('office-hours')).toBe('office-hours');
    expect(normalizeRuntimeMode('foo')).toBe('normal');
    expect(warn).toHaveBeenCalledWith(expect.stringContaining('foo'));
    warn.mockRestore();
  });
});
```

- [ ] 运行测试：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm vitest run packages/agent-core/test/agent/session-mode-types.test.ts
```

**Expected outcome**: 若 Part 1 已正确导出常量与守卫，则 5 个测试全部通过；若未导出，会出现 `SyntaxError: The requested module ... does not provide an export named 'SESSION_MODE_KINDS'` 等导入错误。此时回到 Part 1 修复导出。

- [ ] 提交：

```bash
git add packages/agent-core/test/agent/session-mode-types.test.ts
git commit -m "test(agent-core): add RuntimeMode/SessionModeKind constant and guard tests"
```

---

### Task 7.2: ModeBehaviorRegistry 与 behavior 元数据测试

**Depends on:** Part 3 (`behaviors.md`)

**Files:**
- Create: `packages/agent-core/test/agent/session-mode-behaviors.test.ts`

- [ ] 创建测试文件并写入：

```ts
import { describe, it, expect } from 'vitest';
import {
  createDefaultModeBehaviorRegistry,
  ModeBehaviorRegistry,
} from '../../src/agent/session-mode/behaviors';
import { PlanModeBehavior } from '../../src/agent/session-mode/behaviors/plan';
import { DesignModeBehavior } from '../../src/agent/session-mode/behaviors/design';
import { OfficeHoursModeBehavior } from '../../src/agent/session-mode/behaviors/office-hours';
import { GameDesignModeBehavior } from '../../src/agent/session-mode/behaviors/game-design';
import { OdyError, ErrorCodes } from '@odysseythink/agent-core-shared';

describe('ModeBehaviorRegistry', () => {
  it('resolves each registered behavior by kind', () => {
    const registry = createDefaultModeBehaviorRegistry();
    expect(registry.resolve('plan')).toBeInstanceOf(PlanModeBehavior);
    expect(registry.resolve('design')).toBeInstanceOf(DesignModeBehavior);
    expect(registry.resolve('office-hours')).toBeInstanceOf(OfficeHoursModeBehavior);
    expect(registry.resolve('game-design')).toBeInstanceOf(GameDesignModeBehavior);
  });

  it('throws INTERNAL_ERROR for unregistered kind', () => {
    const registry = new ModeBehaviorRegistry();
    expect(() => registry.resolve('plan' as never)).toThrow(
      new OdyError(ErrorCodes.INTERNAL_ERROR, 'Unknown session mode kind: plan'),
    );
  });

  it('lists all four registered kinds', () => {
    const registry = createDefaultModeBehaviorRegistry();
    expect(registry.kinds).toEqual(
      expect.arrayContaining(['plan', 'design', 'office-hours', 'game-design']),
    );
    expect(registry.kinds).toHaveLength(4);
  });

  it('maps each behavior to the correct output subdirectory', () => {
    const registry = createDefaultModeBehaviorRegistry();
    expect(registry.resolve('plan').outputSubdirectory).toBe('plans');
    expect(registry.resolve('design').outputSubdirectory).toBe('designs');
    expect(registry.resolve('office-hours').outputSubdirectory).toBe('products');
    expect(registry.resolve('game-design').outputSubdirectory).toBe('game-design');
  });

  it('defines handoff targets for plan and design only', () => {
    const registry = createDefaultModeBehaviorRegistry();
    expect(registry.resolve('plan').handoffTarget).toBe('normal');
    expect(registry.resolve('design').handoffTarget).toBe('plan');
    expect(registry.resolve('office-hours').handoffTarget).toBeUndefined();
    expect(registry.resolve('game-design').handoffTarget).toBeUndefined();
  });

  it('flags design as the only session-tracking mode', () => {
    const registry = createDefaultModeBehaviorRegistry();
    expect(registry.resolve('design').supportsDesignSessions).toBe(true);
    expect(registry.resolve('plan').supportsDesignSessions).toBeFalsy();
    expect(registry.resolve('office-hours').supportsDesignSessions).toBeFalsy();
    expect(registry.resolve('game-design').supportsDesignSessions).toBeFalsy();
  });
});
```

- [ ] 运行测试：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm vitest run packages/agent-core/test/agent/session-mode-behaviors.test.ts
```

**Expected outcome**: 6 个测试全部通过。若 `createDefaultModeBehaviorRegistry` 或 behavior 类路径/字段与 Part 3 不一致，根据错误调整 Part 3 实现或本测试的导入/断言。

- [ ] 提交：

```bash
git add packages/agent-core/test/agent/session-mode-behaviors.test.ts
git commit -m "test(agent-core): add ModeBehaviorRegistry and behavior metadata tests"
```

---

### Task 7.3: SessionModeInjector 变体调度测试

**Depends on:** Part 4 (`injector.md`)

**Files:**
- Create: `packages/agent-core/test/agent/injection/session-mode-injector.test.ts`

- [ ] 创建测试文件并写入：

```ts
import { describe, it, expect } from 'vitest';
import { BaseSessionModeInjector } from '../../../src/agent/injection/session-mode-injector';
import type { Agent } from '../../../src/agent';

const OPTIONS = { fullRefreshTurns: 5, dedupMinTurns: 2 } as const;

class FakeInjector extends BaseSessionModeInjector {
  readonly options = OPTIONS;
  active = false;
  modeActiveReturn = false;

  isModeActive(): boolean {
    return this.modeActiveReturn;
  }

  getEntryReminder(): string { return 'ENTRY'; }
  getReentryReminder(): string { return 'REENTRY'; }
  getFullReminder(): string { return 'FULL'; }
  getSparseReminder(): string { return 'SPARSE'; }
  getExitReminder(): string { return 'EXIT'; }
}

function makeAgent(): Agent {
  const history: Array<{ role: string }> = [];
  return {
    context: { history },
    sessionMode: { sessionModeFilePath: null },
  } as unknown as Agent;
}

describe('BaseSessionModeInjector.computeVariant', () => {
  it('returns full when injectedAt is null', () => {
    const agent = makeAgent();
    const injector = new FakeInjector(agent);
    expect(injector['computeVariant'](null, agent.context.history, OPTIONS)).toBe('full');
  });

  it('returns null when only one assistant turn has passed', () => {
    const agent = makeAgent();
    agent.context.history.push({ role: 'assistant' });
    const injector = new FakeInjector(agent);
    expect(injector['computeVariant'](0, agent.context.history, OPTIONS)).toBeNull();
  });

  it('returns sparse when exactly dedupMinTurns assistant turns have passed', () => {
    const agent = makeAgent();
    agent.context.history.push({ role: 'assistant' }, { role: 'assistant' });
    const injector = new FakeInjector(agent);
    expect(injector['computeVariant'](0, agent.context.history, OPTIONS)).toBe('sparse');
  });

  it('returns full when fullRefreshTurns assistant turns have passed', () => {
    const agent = makeAgent();
    for (let i = 0; i < 5; i++) agent.context.history.push({ role: 'assistant' });
    const injector = new FakeInjector(agent);
    expect(injector['computeVariant'](0, agent.context.history, OPTIONS)).toBe('full');
  });

  it('returns full when a user message interrupts assistant turns', () => {
    const agent = makeAgent();
    agent.context.history.push(
      { role: 'assistant' },
      { role: 'assistant' },
      { role: 'user' },
    );
    const injector = new FakeInjector(agent);
    expect(injector['computeVariant'](0, agent.context.history, OPTIONS)).toBe('full');
  });
});

describe('BaseSessionModeInjector.onContextClear', () => {
  it('remembers whether the mode was active', () => {
    const agent = makeAgent();
    const injector = new FakeInjector(agent);
    injector.modeActiveReturn = true;
    injector.onContextClear();
    expect(injector['wasActive']).toBe(true);
    injector.modeActiveReturn = false;
    injector.onContextClear();
    expect(injector['wasActive']).toBe(false);
  });
});
```

- [ ] 运行测试：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm vitest run packages/agent-core/test/agent/injection/session-mode-injector.test.ts
```

**Expected outcome**: 7 个测试全部通过。若 `BaseSessionModeInjector` 的 `computeVariant` 实现或访问修饰符与 Part 4 不同，调整测试或 Part 4 实现。

- [ ] 提交：

```bash
git add packages/agent-core/test/agent/injection/session-mode-injector.test.ts
git commit -m "test(agent-core): add BaseSessionModeInjector variant scheduling tests"
```

---

### Task 7.4: SessionMode 进入/退出行为测试

**Depends on:** Part 5 (`session-mode.md`)

**Files:**
- Modify: `packages/agent-core/test/agent/session-mode.test.ts`

- [ ] 在文件末尾新增 `describe` 块：

```ts
describe('SessionMode enter/exit lifecycle', () => {
  it('rolls back state when behavior.onEnter throws', async () => {
    const agent = makeAgent();
    const sm = new SessionMode(agent, {
      resolve: () => ({
        kind: 'plan',
        outputSubdirectory: 'plans',
        modeModelKey: 'plan',
        injectorClass: class {
          constructor() {}
        } as never,
        onEnter: () => {
          throw new Error('boom');
        },
        onExit: () => {},
        onCancel: () => {},
      }),
      kinds: ['plan'],
    } as never);

    const originalAlias = agent.config.modelAlias;
    await expect(sm.enter('id-1', undefined, false, 'plan')).rejects.toThrow('boom');

    expect(sm.isActive).toBe(false);
    expect(sm.kind).toBe('plan'); // default remains plan, but isActive is false
    expect(sm.sessionModeFilePath).toBeNull();
    expect(agent.config.update).not.toHaveBeenCalledWith(expect.objectContaining({ modelAlias: 'plan-model' }));
    expect(agent.setContextMode).toHaveBeenCalledWith('normal');
    expect(agent.config.modelAlias).toBe(originalAlias);
  });

  it('exit is idempotent and logs only one record', async () => {
    const agent = makeAgent();
    const sm = new SessionMode(agent);
    await sm.enter('id-1', undefined, false, 'plan');
    sm.exit('id-1');
    sm.exit('id-1');

    const exitRecords = (agent.records.logRecord as ReturnType<typeof vi.fn>).mock.calls.filter(
      (call) => call[0]?.type === 'session_mode.exit',
    );
    expect(exitRecords).toHaveLength(1);
    expect(sm.isActive).toBe(false);
    expect(agent.setContextMode).toHaveBeenCalledWith('normal');
  });
});
```

- [ ] 运行测试：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm vitest run packages/agent-core/test/agent/session-mode.test.ts
```

**Expected outcome**: 新增 2 个测试通过。若 Part 5 的 `SessionMode` 未暴露 `isActive`/`kind` getter 或未在 `enter` 失败时回滚，修复 Part 5 实现。

- [ ] 提交：

```bash
git add packages/agent-core/test/agent/session-mode.test.ts
git commit -m "test(agent-core): add SessionMode enter rollback and exit idempotence tests"
```

---

### Task 7.5: 撰写 `docs/architecture/modes-vs-profiles.md`

**Depends on:** Part 1–6

**Files:**
- Create: `docs/architecture/modes-vs-profiles.md`

- [ ] 创建文档并写入：

```markdown
# Modes vs Profiles

> **One-sentence definition**: A **mode** is an interaction phase (plan, design, office-hours, game-design, or normal); a **profile** is a role/tool-set/system-prompt configuration loaded from `.ody-code/profiles/`.

## Responsibility split

| Concern | Mode | Profile |
|---|---|---|
| Determines output directory | Yes (`plans/`, `designs/`, `.ody-code/products/`, `.ody-code/game-design/`) | No |
| Switches the active model via `modeModels` | Yes | No |
| Owns an isolated context partition | Yes (`Agent._contexts[mode]`) | No |
| Provides the system prompt | No | Yes |
| Decides which tools are visible | No (except via `hiddenInModes`) | Yes |
| Can be entered/exited mid-session | Yes | No (profile is applied, not entered) |

## Current modes and handoff graph

```
        ┌─────────────────┐
        │   office-hours  │
        └────────┬────────┘
                 │ enter/exit
                 ▼
        ┌─────────────────┐
        │   game-design   │
        └────────┬────────┘
                 │ enter/exit
                 ▼
        ┌─────────────────┐     handoffTo('plan')
        │     design      │ ───────────────────────►
        └────────┬────────┘                          │
                 │ enter/exit                        │
                 ▼                                   │
        ┌─────────────────┐                          │
        │      plan       │ ◄───────────────────────┘
        └────────┬────────┘
                 │ exit
                 ▼
        ┌─────────────────┐
        │     normal      │
        └─────────────────┘
```

- **normal**: free-form implementation; default partition.
- **plan**: write an implementation plan before coding; output goes to `.ody-code/plans/`.
- **design**: brainstorm/spec exploration; output goes to `.ody-code/designs/`; can hand off to `plan`.
- **office-hours**: startup/builder diagnostic flow; output goes to `.ody-code/products/`.
- **game-design**: guided game-design session; output goes to `.ody-code/game-design/`.

## `SystemPromptContext.sessionMode` usage rules

Use `sessionMode` in a system prompt **only** when the text being rendered is specific to the interaction phase:

- ✅ "You are in plan mode; follow the plan-mode contract."
- ✅ "You are in design mode; do not write implementation code."
- ❌ Deciding which profile to load. Profile selection is a separate concern.
- ❌ Changing tool visibility. Use `hiddenInModes` in skill metadata instead.

## Decision matrix: adding a new mode vs adding a new profile

| If you want to… | Add a **mode** | Add a **profile** |
|---|---|---|
| Change where files are written | ✅ | ❌ |
| Change the active model alias | ✅ | ❌ |
| Add a new context partition | ✅ | ❌ |
| Change the system prompt | ❌ | ✅ |
| Change available tools | ❌ | ✅ |
| Change the agent's role/persona | ❌ | ✅ |

## Files to touch

### Adding a mode

1. `packages/agent-core/src/agent/session-mode/types.ts` — add to `SESSION_MODE_KINDS` / `RUNTIME_MODES`.
2. `packages/agent-core/src/agent/session-mode/behaviors/<mode>.ts` — implement `SessionModeBehavior`.
3. `packages/agent-core/src/agent/session-mode/behaviors/index.ts` — register in default registry.
4. `packages/agent-core/src/agent/injection/<mode>-mode.ts` — implement `SessionModeInjector` if the mode needs injected reminders.
5. `packages/agent-core-shared/src/config.ts` — add `modeModels.<camelCaseKey>` if the mode has a dedicated model.
6. `apps/ody-code/src/tui/commands/types.ts` / `registry.ts` — update `SessionMode` and command visibility if needed.

### Adding a profile

1. Create `.ody-code/profiles/<name>.md` in the project or user profile directory.
2. Optionally create `.ody-code/profiles/<name>.toml` for tool lists.
3. No TypeScript changes required.

## Self-check questions

1. If a user enters `/plan`, which component decides that output goes to `.ody-code/plans/`?
2. Why should `SystemPromptContext.sessionMode` not be used to pick a profile?
3. Which mode can hand off to `plan`, and through which mechanism?
4. What is the difference between `SessionModeKind` and `RuntimeMode`?
5. To add a new interaction phase that needs its own model alias and output directory, would you add a mode or a profile?

## Answers

1. `PlanModeBehavior.outputSubdirectory`.
2. Profile selection is role/tool-set concern; mixing it with mode couples role to interaction phase.
3. `design` can hand off to `plan` via `DesignModeBehavior.handoffTarget`.
4. `SessionModeKind` = the four enterable interaction phases; `RuntimeMode` = `SessionModeKind` plus `normal`.
5. Add a mode.
```

- [ ] 手动验证：阅读生成的文档，并逐一回答上述 5 道自测题，确认答案与文档内容一致。

- [ ] 提交：

```bash
git add docs/architecture/modes-vs-profiles.md
git commit -m "docs(architecture): add modes-vs-profiles guide and self-check"
```

---

### Task 7.6: 最终验收

**Depends on:** Task 7.1–7.5

**Files:**
- Verify: 全仓库 TypeScript 与测试

- [ ] 运行全树 typecheck：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm -r typecheck
```

**Expected output**: 所有 workspace 通过，无 TS 错误。

- [ ] 运行目标测试集：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm vitest run packages/agent-core/test/agent/session-mode-types.test.ts \
  packages/agent-core/test/agent/session-mode-behaviors.test.ts \
  packages/agent-core/test/agent/injection/session-mode-injector.test.ts \
  packages/agent-core/test/agent/session-mode.test.ts
```

**Expected output**: 所有测试通过。

- [ ] 运行全仓库 session-mode 与 injection 相关测试（回归）：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm vitest run packages/agent-core/test/agent/session-mode \
  packages/agent-core/test/agent/injection
```

**Expected output**: 所有相关测试通过。

- [ ] 生成 changeset（若本 PR 包含用户可感知行为变化，本次主要为内部重构，可用 `patch`）：

```bash
cd /Users/ranwei/workspace/ody-code
pnpm changeset
# 选择 @odysseythink/agent-core，输入 summary:
# "refactor: converge session mode literals to RuntimeMode and unify ModeBehavior lifecycle"
```

- [ ] 提交 changeset：

```bash
git add .changeset
git commit -m "chore: add changeset for mode concept unification"
```

---

## Local Self-Review

- [ ] 1. **Spec-coverage table**:
  - 类型常量与守卫测试：Task 7.1 covered。
  - `ModeBehaviorRegistry` 与 behavior 元数据测试：Task 7.2 covered。
  - `SessionModeInjector.computeVariant` 与 `onContextClear` 测试：Task 7.3 covered。
  - `SessionMode.enter` 原子回滚与 `exit` 幂等测试：Task 7.4 covered。
  - `docs/architecture/modes-vs-profiles.md` 与反向检查单：Task 7.5 covered。
  - 全树 typecheck 与目标测试集通过：Task 7.6 covered。
- [ ] 2. **Placeholder scan**：无 TODO/TBD；所有测试给出完整代码，文档给出完整内容。
- [ ] 3. **No phantom tasks**：每任务产生可验证变更（新测试文件、文档修改、测试运行、changeset）。
- [ ] 4. **Dependency soundness**：Task 7.1–7.5 仅依赖 Part 1–5 已产出符号；Task 7.6 依赖 7.1–7.5；无向前依赖。
- [ ] 5. **Caller & build soundness**：本 Part 不修改共享签名，仅添加测试与文档；最终验收使用 `pnpm -r typecheck`。
- [ ] 6. **Test-the-risk**：
  - Task 7.1 对 `isRuntimeMode`/`normalizeRuntimeMode` 的合法/非法输入做显式断言，覆盖 must-pass（5 个合法值）与 must-reject（`'foo'`）输入。
  - Task 7.2 对 `ModeBehaviorRegistry.resolve` 的未注册 kind 断言抛错，避免静默回退。
  - Task 7.3 对 `computeVariant` 的阈值边界（`dedupMinTurns=2`、`fullRefreshTurns=5`）与用户打断路径做显式断言。
  - Task 7.4 对 `enter()` 失败后的 `isActive`/`sessionModeFilePath`/模型别名做行为断言，对 `exit()` 的记录数量做行为断言。
- [ ] 7. **Type consistency**：测试中使用的类型/字段名（`SESSION_MODE_KINDS`、`RUNTIME_MODES`、`outputSubdirectory`、`handoffTarget`、`supportsDesignSessions`、`computeVariant`、`wasActive`、`isActive`、`sessionModeFilePath`）与 Part 1–5 定义一致。
