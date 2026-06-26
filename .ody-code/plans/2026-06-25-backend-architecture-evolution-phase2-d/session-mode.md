# Part 5：SessionMode 类重构

本 part 让 `SessionMode` 退化为调度器：维护状态、协调 behavior、处理回滚与 handoff，把所有 mode-specific 的 enter/exit/cancel 副作用（目录解析、model 切换、design session 追踪）委托给 `ModeBehaviorRegistry` 解析出的 `SessionModeBehavior`。同时为修复 ESM 下 `createDefaultModeBehaviorRegistry` 的循环导入问题，将 `behaviors/index.ts` 拆分为 `types.ts` / `base.ts` / `registry.ts`。

## Task 5.0：拆分 behavior 文件消除 ESM 循环导入

**Depends on:** Task 3.3, Task 4.1

**Files:**
- Create: `packages/agent-core/src/agent/session-mode/behaviors/types.ts`
- Create: `packages/agent-core/src/agent/session-mode/behaviors/base.ts`
- Create: `packages/agent-core/src/agent/session-mode/behaviors/registry.ts`
- Modify: `packages/agent-core/src/agent/session-mode/behaviors/index.ts`
- Modify: `packages/agent-core/src/agent/session-mode/behaviors/plan.ts`
- Modify: `packages/agent-core/src/agent/session-mode/behaviors/design.ts`
- Modify: `packages/agent-core/src/agent/session-mode/behaviors/office-hours.ts`
- Modify: `packages/agent-core/src/agent/session-mode/behaviors/game-design.ts`

### 背景

Part 3 使用 `require()` 实现 `createDefaultModeBehaviorRegistry` 的懒加载，但 `packages/agent-core` 是 ESM（`"type": "module"`），且 `require()` 在该包无现有使用。若使用静态 `import`，`behaviors/index.ts` 导出 `BaseSessionModeBehavior` 的同时又静态引入 `plan.ts`（而 `plan.ts` 从 `.` 引入 `BaseSessionModeBehavior`）会产生运行时循环依赖。因此把接口、基类、注册表拆到三个无环文件，并由 `index.ts` 统一 re-export。

### 步骤

- [ ] **Write the failing test / 复现编译失败**

  先验证当前 `createDefaultModeBehaviorRegistry` 无法运行：

  ```bash
  pnpm test packages/agent-core/src/agent/session-mode/__tests__/behaviors.test.ts
  ```

  预期失败：若已按 Part 3 实现，`require is not defined in ES module`（具体错误取决于 Node 版本与构建）。

- [ ] **Write the minimal implementation**

  1. 新建 `packages/agent-core/src/agent/session-mode/behaviors/types.ts`：

  ```ts
  import type { Agent } from '../../..';

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

  export interface SessionModeInjector {
    readonly injectionVariant: string;
    onContextClear(): void;
    onContextCompacted(compactedCount: number): void;
    onContextMessageRemoved(index: number): void;
    inject(): Promise<void>;
    getInjection(): string | Promise<string | undefined> | undefined;
  }

  export interface SessionModeInjectorOptions {
    fullRefreshTurns: number;
    dedupMinTurns: number;
  }

  export interface SessionModeBehavior<TKind extends import('../types').SessionModeKind> {
    readonly kind: TKind;
    readonly outputSubdirectory: string;
    readonly modeModelKey: string;
    readonly injectorClass: new (agent: Agent) => SessionModeInjector;
    readonly handoffTarget?: 'plan' | 'normal';
    readonly supportsDesignSessions?: boolean;

    onEnter(ctx: ModeEnterContext): Promise<void> | void;
    onExit(ctx: ModeExitContext): Promise<void> | void;
    onCancel(ctx: ModeExitContext): Promise<void> | void;
  }
  ```

  2. 新建 `packages/agent-core/src/agent/session-mode/behaviors/base.ts`：

  ```ts
  import { ensureGitignore } from '../../../utils/gitignore';
  import type { Agent } from '../../..';
  import type { SessionModeKind } from '../types';
  import { resolveSessionModeDirectory } from '../directory';
  import { modelAliasHasUsableAuth } from '../model-auth';
  import type { ModeEnterContext, ModeExitContext, SessionModeBehavior, SessionModeInjector } from './types';

  export abstract class BaseSessionModeBehavior<TKind extends SessionModeKind>
    implements SessionModeBehavior<TKind> {
    abstract readonly kind: TKind;
    abstract readonly outputSubdirectory: string;
    abstract readonly modeModelKey: string;
    abstract readonly injectorClass: new (agent: Agent) => SessionModeInjector;

    async onEnter(ctx: ModeEnterContext): Promise<void> {
      const { dir, isProjectScoped } = await resolveSessionModeDirectory(ctx.agent, this.kind);
      if (isProjectScoped) {
        try {
          await ensureGitignore(ctx.agent.config.cwd, ctx.agent.kaos);
        } catch (error) {
          ctx.agent.log?.warn('Failed to update .gitignore', { error });
        }
      }

      const modeModelAlias = ctx.agent.kimiConfig?.modeModels?.[this.modeModelKey];
      if (modeModelAlias !== undefined) {
        let resolved;
        let usable = false;
        try {
          resolved = ctx.agent.modelProvider?.resolveProviderConfig(modeModelAlias);
          usable = resolved === undefined || modelAliasHasUsableAuth(ctx.agent, modeModelAlias, resolved);
        } catch {
          ctx.agent.log?.warn(`modeModels.${this.modeModelKey} "${modeModelAlias}" not found, keeping current model`);
        }
        if (usable && modeModelAlias !== ctx.agent.config.modelAlias) {
          ctx.agent.config.update({ modelAlias: modeModelAlias });
          ctx.agent.refreshLlm();
        }
      }
    }

    async onExit(_ctx: ModeExitContext): Promise<void> {}
    async onCancel(_ctx: ModeExitContext): Promise<void> {}
  }
  ```

  3. 新建 `packages/agent-core/src/agent/session-mode/behaviors/registry.ts`：

  ```ts
  import { ErrorCodes, OdyError } from '@odysseythink/agent-core-shared';
  import type { SessionModeKind } from '../types';
  import { DesignModeBehavior } from './design';
  import { GameDesignModeBehavior } from './game-design';
  import { OfficeHoursModeBehavior } from './office-hours';
  import { PlanModeBehavior } from './plan';
  import type { SessionModeBehavior } from './types';

  export class ModeBehaviorRegistry {
    private readonly behaviors = new Map<SessionModeKind, SessionModeBehavior<SessionModeKind>>();

    register<T extends SessionModeKind>(behavior: SessionModeBehavior<T>): void {
      this.behaviors.set(behavior.kind, behavior);
    }

    resolve(kind: SessionModeKind): SessionModeBehavior<SessionModeKind> {
      const behavior = this.behaviors.get(kind);
      if (behavior === undefined) {
        throw new OdyError(ErrorCodes.INTERNAL, `Unknown session mode kind: ${kind}`);
      }
      return behavior;
    }

    get kinds(): readonly SessionModeKind[] {
      return Array.from(this.behaviors.keys());
    }
  }

  export function createDefaultModeBehaviorRegistry(): ModeBehaviorRegistry {
    const registry = new ModeBehaviorRegistry();
    registry.register(new PlanModeBehavior());
    registry.register(new DesignModeBehavior());
    registry.register(new OfficeHoursModeBehavior());
    registry.register(new GameDesignModeBehavior());
    return registry;
  }
  ```

  4. 修改 `packages/agent-core/src/agent/session-mode/behaviors/index.ts` 为纯 re-export：

  ```ts
  export { BaseSessionModeBehavior } from './base';
  export { createDefaultModeBehaviorRegistry, ModeBehaviorRegistry } from './registry';
  export type {
    ModeEnterContext,
    ModeExitContext,
    SessionModeBehavior,
    SessionModeInjector,
    SessionModeInjectorOptions,
  } from './types';
  ```

  5. 修改 4 个具体 behavior 文件，将 `BaseSessionModeBehavior` 的导入改为 `./base`，将 `SessionModeInjector` 等类型导入改为 `./types`：

  `packages/agent-core/src/agent/session-mode/behaviors/plan.ts`：

  ```ts
  import type { Agent } from '../../..';
  import { PlanModeInjector } from '../../injection/plan-mode';
  import { BaseSessionModeBehavior } from './base';
  import type { SessionModeInjector } from './types';

  export class PlanModeBehavior extends BaseSessionModeBehavior<'plan'> {
    readonly kind = 'plan' as const;
    readonly outputSubdirectory = 'plans';
    readonly modeModelKey = 'plan';
    readonly injectorClass = PlanModeInjector as unknown as new (agent: Agent) => SessionModeInjector;
    readonly handoffTarget = 'normal' as const;
  }
  ```

  `packages/agent-core/src/agent/session-mode/behaviors/design.ts`：

  ```ts
  import type { Agent } from '../../..';
  import { DesignModeInjector } from '../../injection/design-mode';
  import { BaseSessionModeBehavior } from './base';
  import type { ModeEnterContext, ModeExitContext, SessionModeInjector } from './types';

  export class DesignModeBehavior extends BaseSessionModeBehavior<'design'> {
    readonly kind = 'design' as const;
    readonly outputSubdirectory = 'designs';
    readonly modeModelKey = 'design';
    readonly injectorClass = DesignModeInjector as unknown as new (agent: Agent) => SessionModeInjector;
    readonly handoffTarget = 'plan' as const;
    readonly supportsDesignSessions = true;

    override async onEnter(ctx: ModeEnterContext): Promise<void> {
      await super.onEnter(ctx);
      ctx.agent.sessionMode.startDesignSession(ctx.id);
    }

    override async onExit(ctx: ModeExitContext): Promise<void> {
      await super.onExit(ctx);
      ctx.agent.sessionMode.closeCurrentDesignSession(ctx.sessionModeFilePath ?? undefined);
      if (ctx.sessionModeFilePath !== null) {
        ctx.agent.sessionMode.setLastCompletedDesignFilePath(ctx.sessionModeFilePath);
      }
    }

    override async onCancel(ctx: ModeExitContext): Promise<void> {
      await super.onCancel(ctx);
      ctx.agent.sessionMode.closeCurrentDesignSession();
    }
  }
  ```

  `packages/agent-core/src/agent/session-mode/behaviors/office-hours.ts`：

  ```ts
  import type { Agent } from '../../..';
  import { OfficeHoursInjector } from '../../injection/office-hours';
  import { BaseSessionModeBehavior } from './base';
  import type { SessionModeInjector } from './types';

  export class OfficeHoursModeBehavior extends BaseSessionModeBehavior<'office-hours'> {
    readonly kind = 'office-hours' as const;
    readonly outputSubdirectory = 'products';
    readonly modeModelKey = 'officeHours';
    readonly injectorClass = OfficeHoursInjector as unknown as new (agent: Agent) => SessionModeInjector;
  }
  ```

  `packages/agent-core/src/agent/session-mode/behaviors/game-design.ts`：

  ```ts
  import type { Agent } from '../../..';
  import { GameDesignInjector } from '../../injection/game-design';
  import { BaseSessionModeBehavior } from './base';
  import type { SessionModeInjector } from './types';

  export class GameDesignModeBehavior extends BaseSessionModeBehavior<'game-design'> {
    readonly kind = 'game-design' as const;
    readonly outputSubdirectory = 'game-design';
    readonly modeModelKey = 'gameDesign';
    readonly injectorClass = GameDesignInjector as unknown as new (agent: Agent) => SessionModeInjector;
  }
  ```

  注意：`PlanModeInjector` 等继承 `BaseSessionModeInjector`（而 `BaseSessionModeInjector` 继承 `DynamicInjector`），但 `SessionModeInjector` 接口是独立接口；TypeScript 结构类型兼容，但直接赋值需要 `as unknown as` 绕过构造函数签名差异。`BaseSessionModeInjector` 的子类构造函数签名为 `new (agent: Agent)`，接口要求 `new (agent: Agent) => SessionModeInjector`，结构兼容；使用 `as unknown as` 是为了避免 TypeScript 将类类型与接口类型做严格比较。

- [ ] **Run it and verify it PASSES**

  ```bash
  pnpm test packages/agent-core/src/agent/session-mode/__tests__/behaviors.test.ts
  pnpm -r typecheck
  ```

  预期：behavior 测试通过，全仓库类型检查无循环导入错误。

- [ ] **Commit**

  ```bash
  git add packages/agent-core/src/agent/session-mode/behaviors
  git commit -m "refactor(agent-core): split behavior module to avoid ESM circular imports"
  ```

## Task 5.1：重构 `SessionMode.enter()` 委托给 behavior

**Depends on:** Task 5.0

**Files:**
- Modify: `packages/agent-core/src/agent/session-mode/index.ts`（`enter()` 方法与相关导入/导出）
- Test: `packages/agent-core/src/agent/session-mode/__tests__/session-mode.test.ts`

### 步骤

- [ ] **Write the failing test**

  在 `packages/agent-core/src/agent/session-mode/__tests__/session-mode.test.ts` 写入（如文件不存在则新建）：

  ```ts
  import { describe, it, expect, vi } from 'vitest';
  import type { Agent } from '../../..';
  import { SessionMode } from '../index';
  import { ModeBehaviorRegistry } from '../behaviors/registry';
  import type { ModeEnterContext, ModeExitContext, SessionModeBehavior, SessionModeInjector } from '../behaviors/types';
  import type { SessionModeFilePath, SessionModeKind } from '../types';

  class FakePlanBehavior implements SessionModeBehavior<'plan'> {
    readonly kind = 'plan' as const;
    readonly outputSubdirectory = 'plans';
    readonly modeModelKey = 'plan';
    readonly injectorClass = class implements SessionModeInjector {
      readonly injectionVariant = 'fake_plan';
      onContextClear(): void {}
      onContextCompacted(): void {}
      onContextMessageRemoved(): void {}
      async inject(): Promise<void> {}
      getInjection(): undefined { return undefined; }
    };
    entered = false;
    async onEnter(ctx: ModeEnterContext): Promise<void> {
      this.entered = true;
    }
    async onExit(_ctx: ModeExitContext): Promise<void> {}
    async onCancel(_ctx: ModeExitContext): Promise<void> {}
  }

  function makeAgent(modelAlias = 'normal-model'): Agent {
    return {
      config: { modelAlias, update: vi.fn((patch) => { modelAlias = patch.modelAlias ?? modelAlias; }) },
      kaos: { mkdir: vi.fn().mockResolvedValue(undefined) },
      log: { debug: vi.fn(), warn: vi.fn() },
      records: { logRecord: vi.fn() },
      setContextMode: vi.fn(),
      emitStatusUpdated: vi.fn(),
      refreshLlm: vi.fn(),
      context: { history: [] },
    } as unknown as Agent;
  }

  describe('SessionMode.enter delegation', () => {
    it('calls behavior.onEnter and captures restore model alias', async () => {
      const behavior = new FakePlanBehavior();
      const registry = new ModeBehaviorRegistry();
      registry.register(behavior);
      const agent = makeAgent('normal-model');
      const sessionMode = new SessionMode(agent, registry);
      await sessionMode.enter('id-1', false, true, 'plan');
      expect(behavior.entered).toBe(true);
      expect(sessionMode.isActive).toBe(true);
      expect(sessionMode.kind).toBe('plan');
      sessionMode.exit();
      expect(agent.config.modelAlias).toBe('normal-model');
    });

    it('rolls back state when behavior.onEnter throws', async () => {
      const behavior = new FakePlanBehavior();
      behavior.onEnter = async () => { throw new Error('boom'); };
      const registry = new ModeBehaviorRegistry();
      registry.register(behavior);
      const agent = makeAgent();
      const sessionMode = new SessionMode(agent, registry);
      await expect(sessionMode.enter('id-1', false, true, 'plan')).rejects.toThrow('boom');
      expect(sessionMode.isActive).toBe(false);
      expect(agent.setContextMode).toHaveBeenCalledWith('normal');
    });

    it('logs session_mode.enter and sets context mode after behavior succeeds', async () => {
      const behavior = new FakePlanBehavior();
      const registry = new ModeBehaviorRegistry();
      registry.register(behavior);
      const agent = makeAgent();
      const sessionMode = new SessionMode(agent, registry);
      await sessionMode.enter('id-1', false, true, 'plan');
      expect(agent.records.logRecord).toHaveBeenCalledWith({ type: 'session_mode.enter', id: 'id-1', kind: 'plan' });
      expect(agent.setContextMode).toHaveBeenCalledWith('plan');
    });
  });
  ```

  注意：测试使用 `SessionMode(agent, registry)` 构造函数；实现需支持可选 registry 参数。

- [ ] **Run it and verify it FAILS**

  ```bash
  pnpm test packages/agent-core/src/agent/session-mode/__tests__/session-mode.test.ts
  ```

  预期失败：构造函数不接受 registry，或 `enter()` 未调用 `behavior.onEnter`。

- [ ] **Write the minimal implementation**

  修改 `packages/agent-core/src/agent/session-mode/index.ts`：

  1. 顶部导入调整：

  ```ts
  import { randomUUID } from 'node:crypto';
  import { basename, dirname, join, normalize } from 'pathe';

  import type { Agent } from '..';
  import type { DesignSessionCheckpoint } from '../../session/checkpoint/checkpoint';
  import {
    createDefaultModeBehaviorRegistry,
    type ModeBehaviorRegistry,
  } from './behaviors/registry';
  import { resolveSessionModeDirectory } from './directory';
  import {
    extractFirstHeading,
    extractTopicFromMessage,
    formatDatePrefix,
    slugifyTitle,
    stripDatePrefix,
    stripLocators,
    buildTitlePrompt,
  } from './topic-generator';

  export {
    SESSION_MODE_KINDS,
    RUNTIME_MODES,
    isSessionModeKind,
    isRuntimeMode,
    normalizeRuntimeMode,
    type SessionModeKind,
    type RuntimeMode,
  } from './types';
  ```

  删除 `ensureGitignore` 导入、`ResolvedRuntimeProvider` 导入、以及本地 `SessionModeKind` 类型定义。

  2. `SessionMode` 构造函数增加 registry 参数：

  ```ts
  constructor(
    protected readonly agent: Agent,
    private readonly registry: ModeBehaviorRegistry = createDefaultModeBehaviorRegistry(),
  ) {}
  ```

  3. `enter()` 替换为 behavior 委托版本：

  ```ts
  async enter(
    id = this.createSessionModeId(),
    _createFile = false,
    emitStatus = true,
    kind: SessionModeKind = 'plan',
  ): Promise<void> {
    const enterModelAlias = this.agent.config.modelAlias;
    this.agent.log?.debug('sessionMode.enter start', {
      kind,
      fromModelAlias: enterModelAlias,
      isActive: this._isActive,
      currentKind: this._kind,
    });
    if (this._isActive) {
      if (this._kind === kind) {
        this.agent.log?.debug('sessionMode.enter already in kind', { kind });
        return;
      }
      // Switching directly between plan and design: exit current first.
      this.exit();
    }

    // The model to restore when leaving modes entirely. Read AFTER the exit()
    // above: a direct plan↔design switch restores the normal model there, so the
    // entry-time alias (captured before exit) would be the PREVIOUS mode's model
    // and would leak back into normal on the final exit.
    const restoreTargetAlias = this.agent.config.modelAlias;

    const behavior = this.registry.resolve(kind);

    this._isActive = true;
    this._sessionModeId = id;
    this._kind = kind;
    this._sessionModeFilePath = null;

    try {
      await behavior.onEnter({ agent: this.agent, id, restoreTargetAlias });
      // Capture restore alias after behavior runs. If behavior did not switch the
      // model, restoring it on exit/cancel is a no-op.
      this._preModeModelAlias = { value: restoreTargetAlias };

      this.agent.records.logRecord({
        type: 'session_mode.enter',
        id,
        kind,
      });
      this.agent.setContextMode(kind);
    } catch (error) {
      this.agent.setContextMode('normal');
      if (this._preModeModelAlias !== null) {
        this.agent.config.update({ modelAlias: this._preModeModelAlias.value });
        this.agent.refreshLlm();
        this._preModeModelAlias = null;
      }
      this._isActive = false;
      this._sessionModeId = null;
      this._sessionModeFilePath = null;
      this._kind = 'plan';
      throw error;
    }

    this.agent.log?.debug('sessionMode.enter end', {
      kind,
      modelAlias: this.agent.config.modelAlias,
      preModeModelAlias: this._preModeModelAlias?.value,
    });

    if (emitStatus) this.agent.emitStatusUpdated();
  }
  ```

  4. 删除 `SessionMode` 中以下已迁移到 `BaseSessionModeBehavior` 的私有方法：
     - `resolveSessionModeDirectory`
     - `ensureGitignore`
     - `modelAliasHasUsableAuth`

  保留 `findUniqueStemInDir`（仍用于文件路径解析），并继续从 `./directory` 导入 `resolveSessionModeDirectory`。

- [ ] **Run it and verify it PASSES**

  ```bash
  pnpm test packages/agent-core/src/agent/session-mode/__tests__/session-mode.test.ts
  ```

  预期：enter 相关测试通过。

- [ ] **Commit**

  ```bash
  git add packages/agent-core/src/agent/session-mode/index.ts packages/agent-core/src/agent/session-mode/__tests__/session-mode.test.ts
  git commit -m "refactor(agent-core): delegate SessionMode.enter to behavior"
  ```

## Task 5.2：重构 `SessionMode.exit()` / `cancel()` 并迁移 design session 追踪

**Depends on:** Task 5.1

**Files:**
- Modify: `packages/agent-core/src/agent/session-mode/index.ts`（`exit()` / `cancel()` / design session 方法可见性）
- Modify: `packages/agent-core/src/agent/session-mode/behaviors/design.ts`
- Test: `packages/agent-core/src/agent/session-mode/__tests__/session-mode.test.ts`

### 步骤

- [ ] **Write the failing test**

  在 `packages/agent-core/src/agent/session-mode/__tests__/session-mode.test.ts` 追加：

  ```ts
  import { createDefaultModeBehaviorRegistry } from '../behaviors/registry';

  describe('SessionMode.exit and cancel delegation', () => {
    it('delegates exit to behavior and restores model', async () => {
      const agent = makeAgent('normal-model');
      const sessionMode = new SessionMode(agent, createDefaultModeBehaviorRegistry());
      await sessionMode.enter('id-1', false, true, 'plan');
      sessionMode.exit();
      expect(sessionMode.isActive).toBe(false);
      expect(agent.setContextMode).toHaveBeenLastCalledWith('normal');
      expect(agent.config.modelAlias).toBe('normal-model');
    });

    it('delegates cancel to behavior and restores model', async () => {
      const agent = makeAgent('normal-model');
      const sessionMode = new SessionMode(agent, createDefaultModeBehaviorRegistry());
      await sessionMode.enter('id-1', false, true, 'plan');
      sessionMode.cancel();
      expect(sessionMode.isActive).toBe(false);
      expect(agent.setContextMode).toHaveBeenLastCalledWith('normal');
      expect(agent.config.modelAlias).toBe('normal-model');
    });

    it('tracks design sessions only for design mode', async () => {
      const agent = makeAgent('normal-model');
      const sessionMode = new SessionMode(agent, createDefaultModeBehaviorRegistry());
      await sessionMode.enter('id-1', false, true, 'design');
      expect(sessionMode.designSessions.length).toBe(1);
      sessionMode.exit();
      expect(sessionMode.designSessions[0].exitedAtMsg).toBeDefined();
    });

    it('is idempotent: exit twice logs only one session_mode.exit', async () => {
      const agent = makeAgent('normal-model');
      const sessionMode = new SessionMode(agent, createDefaultModeBehaviorRegistry());
      await sessionMode.enter('id-1', false, true, 'plan');
      sessionMode.exit();
      sessionMode.exit();
      const exitRecords = agent.records.logRecord.mock.calls.filter(
        (call) => call[0].type === 'session_mode.exit',
      );
      expect(exitRecords).toHaveLength(1);
    });
  });
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  pnpm test packages/agent-core/src/agent/session-mode/__tests__/session-mode.test.ts
  ```

  预期失败：`exit()` / `cancel()` 尚未委托 behavior；design session 追踪未迁移。

- [ ] **Write the minimal implementation**

  1. 在 `packages/agent-core/src/agent/session-mode/index.ts` 中：
     - 将 `startDesignSession`、`closeCurrentDesignSession` 的 `private` 改为 `public`。
     - 新增公共 setter `setLastCompletedDesignFilePath(path: string | null): void`。
     - 将 `exit()` 和 `cancel()` 替换为 behavior 委托版本。

  ```ts
  public startDesignSession(id: string): void {
    this._designSessions.push({
      designSessionID: id,
      startedAtMsg: this.currentMessageCount(),
    });
  }

  public closeCurrentDesignSession(approvedPath?: string): void {
    const session = this._designSessions[this._designSessions.length - 1];
    if (session === undefined || session.exitedAtMsg !== undefined) return;
    const count = this.currentMessageCount();
    if (count < session.startedAtMsg) return;
    session.exitedAtMsg = count;
    if (approvedPath !== undefined && approvedPath.length > 0) {
      session.approvedPath = approvedPath;
    }
  }

  public setLastCompletedDesignFilePath(path: string | null): void {
    this._lastCompletedDesignFilePath = path;
  }
  ```

  2. `cancel()` 替换为：

  ```ts
  cancel(id?: string): void {
    if (!this._isActive) return;

    if (this._preModeModelAlias !== null) {
      this.agent.config.update({ modelAlias: this._preModeModelAlias.value });
      this.agent.refreshLlm();
      this._preModeModelAlias = null;
    }

    const behavior = this.registry.resolve(this._kind);
    behavior.onCancel({ agent: this.agent, id, sessionModeFilePath: this._sessionModeFilePath });

    this.agent.records.logRecord({ type: 'session_mode.cancel', id });
    this.agent.setContextMode('normal');
    this.agent.replayBuilder.push({
      type: 'session_mode_updated',
      enabled: false,
      kind: this._kind,
    });
    this._isActive = false;
    this._sessionModeId = null;
    this._sessionModeFilePath = null;
    this._kind = 'plan';
    this.agent.emitStatusUpdated();
  }
  ```

  3. `exit()` 替换为：

  ```ts
  exit(id?: string): void {
    if (!this._isActive) return;

    const exitModelAlias = this.agent.config.modelAlias;
    const restoreModelAlias = this._preModeModelAlias?.value;
    this.agent.log?.debug('sessionMode.exit start', {
      kind: this._kind,
      currentModelAlias: exitModelAlias,
      restoreModelAlias,
    });

    if (this._preModeModelAlias !== null) {
      this.agent.config.update({ modelAlias: this._preModeModelAlias.value });
      this.agent.refreshLlm();
      this._preModeModelAlias = null;
    }

    const behavior = this.registry.resolve(this._kind);
    behavior.onExit({ agent: this.agent, id, sessionModeFilePath: this._sessionModeFilePath });

    this.agent.records.logRecord({ type: 'session_mode.exit', id });
    this.agent.log?.debug('sessionMode.exit end', {
      kind: this._kind,
      modelAlias: this.agent.config.modelAlias,
    });
    this.agent.setContextMode('normal');
    this.agent.replayBuilder.push({
      type: 'session_mode_updated',
      enabled: false,
      kind: this._kind,
    });
    this._isActive = false;
    this._sessionModeId = null;
    this._sessionModeFilePath = null;
    this._kind = 'plan';
    this.agent.emitStatusUpdated();
  }
  ```

  4. 确认 Task 5.0 中 `DesignModeBehavior` 已覆盖 `onEnter` / `onExit` / `onCancel` 以调用 `startDesignSession` / `closeCurrentDesignSession` / `setLastCompletedDesignFilePath`。

- [ ] **Run it and verify it PASSES**

  ```bash
  pnpm test packages/agent-core/src/agent/session-mode/__tests__/session-mode.test.ts
  ```

  预期：exit/cancel/design session 测试通过。

- [ ] **Commit**

  ```bash
  git add packages/agent-core/src/agent/session-mode/index.ts packages/agent-core/src/agent/session-mode/behaviors/design.ts packages/agent-core/src/agent/session-mode/__tests__/session-mode.test.ts
  git commit -m "refactor(agent-core): delegate SessionMode.exit/cancel to behavior and move design sessions"
  ```

## Task 5.3：`InjectionManager` 从 behavior registry 实例化 mode injectors

**Depends on:** Task 5.0, Task 5.2

**Files:**
- Modify: `packages/agent-core/src/agent/injection/manager.ts`

### 步骤

- [ ] **Write the failing test**

  新建 `packages/agent-core/src/agent/injection/__tests__/injection-manager.test.ts`：

  ```ts
  import { describe, it, expect, vi } from 'vitest';
  import type { Agent } from '../../..';
  import { InjectionManager } from '../manager';

  function makeAgent(): Agent {
    return {
      sessionMode: { isActive: false, kind: 'plan', sessionModeFilePath: null, data: vi.fn() },
      context: { history: [], appendSystemReminder: vi.fn() },
      type: 'main',
    } as unknown as Agent;
  }

  describe('InjectionManager mode injector registration', () => {
    it('registers all four mode injectors', () => {
      const agent = makeAgent();
      const manager = new InjectionManager(agent);
      // Access private field for verification; in production this is internal.
      const injectors = (manager as unknown as { injectors: { injectionVariant: string }[] }).injectors;
      const variants = injectors.map((i) => i.injectionVariant);
      expect(variants).toContain('plan_mode');
      expect(variants).toContain('design_mode');
      expect(variants).toContain('office_hours');
      expect(variants).toContain('game_design');
    });
  });
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  pnpm test packages/agent-core/src/agent/injection/__tests__/injection-manager.test.ts
  ```

  预期失败：`InjectionManager` 尚未从 registry 实例化。

- [ ] **Write the minimal implementation**

  修改 `packages/agent-core/src/agent/injection/manager.ts`：

  ```ts
  import type { Agent } from '..';
  import { flags } from '../../flags';
  import { createDefaultModeBehaviorRegistry } from '../session-mode/behaviors/registry';
  import { DesignModeInjector } from './design-mode';
  import { GoalInjector } from './goal';
  import type { DynamicInjector } from './injector';
  import { OfficeHoursInjector } from './office-hours';
  import { GameDesignInjector } from './game-design';
  import { PermissionModeInjector } from './permission-mode';
  import { PluginSessionStartInjector } from './plugin-session-start';
  import { PlanModeInjector } from './plan-mode';
  import { TodoListReminderInjector } from './todo-list';
  import { KnowledgeMicroagentInjector } from './knowledge-microagent';

  export class InjectionManager {
    private readonly injectors: DynamicInjector[];
    private readonly goalInjector: GoalInjector | null;

    constructor(protected readonly agent: Agent) {
      const registry = createDefaultModeBehaviorRegistry();
      const modeInjectors: DynamicInjector[] = registry.kinds.map((kind) => {
        const behavior = registry.resolve(kind);
        return new behavior.injectorClass(agent);
      });

      this.injectors = [
        new PluginSessionStartInjector(agent),
        new TodoListReminderInjector(agent),
        ...modeInjectors,
        new PermissionModeInjector(agent),
        ...(flags.enabled('repo-knowledge') ? [new KnowledgeMicroagentInjector(agent)] : []),
      ];
      this.goalInjector =
        flags.enabled('goal-command') && agent.type === 'main' ? new GoalInjector(agent) : null;
    }

    // ... inject(), injectGoal(), onContextClear(), onContextCompacted(),
    // onContextMessageRemoved(), lifecycleInjectors() 保持不变 ...
  }
  ```

  说明：
  - 保留 `PlanModeInjector` / `DesignModeInjector` / `OfficeHoursInjector` / `GameDesignInjector` 的显式导入，以便 TypeScript 能解析 `behavior.injectorClass` 返回的具体类型。若后续希望完全去硬编码，可将这些导入也移除，但当前保持导入可在 registry 初始化失败时提供清晰错误堆栈。
  - `modeInjectors` 类型为 `DynamicInjector[]`，因为 `behavior.injectorClass` 的接口返回 `SessionModeInjector`，而所有 mode injector 都继承 `BaseSessionModeInjector extends DynamicInjector`。

- [ ] **Run it and verify it PASSES**

  ```bash
  pnpm test packages/agent-core/src/agent/injection/__tests__/injection-manager.test.ts
  ```

  预期：测试通过。

- [ ] **Commit**

  ```bash
  git add packages/agent-core/src/agent/injection/manager.ts packages/agent-core/src/agent/injection/__tests__/injection-manager.test.ts
  git commit -m "refactor(agent-core): instantiate mode injectors from behavior registry"
  ```

## Task 5.4：全仓库类型检查与相关测试

**Depends on:** Task 5.1, Task 5.2, Task 5.3

**Files:**
- 无新文件；验证修改后的全树编译。

### 步骤

- [ ] **Run whole-tree typecheck**

  ```bash
  pnpm -r typecheck
  ```

  预期：全仓库通过。

- [ ] **Run session-mode and injection tests**

  ```bash
  pnpm test packages/agent-core/src/agent/session-mode/__tests__
  pnpm test packages/agent-core/src/agent/injection/__tests__
  ```

  预期：Part 3–5 相关测试全部通过。

- [ ] **Search for leftover mode-specific branches in SessionMode**

  ```bash
  rg -n "kind === 'design'|kind === 'plan'|modeModelKey|resolveSessionModeDirectory|modelAliasHasUsableAuth" packages/agent-core/src/agent/session-mode/index.ts
  ```

  预期：仅剩 `resolveSessionModeDirectory` 在文件路径解析函数（`resolveFilePathFromContent`、`resolveFilePathFromModelRequest`、`setWritingPlanSource`）中的合法使用，以及 `kind` 字段读写与 log 记录。不应再有 `modeModelKey` 分支或 `modelAliasHasUsableAuth`。

- [ ] **Commit**

  ```bash
  git add -u
  git commit -m "chore(agent-core): verify SessionMode refactor with typecheck and tests"
  ```

## Local Self-Review

- [ ] 1. Spec-coverage：覆盖 Scope In #4（`SessionModeBehavior` 策略对象）与 #5（`SessionMode` 退化为调度器）以及 Architecture 中“SessionMode → behavior.onEnter/onExit/onCancel”数据流。
- [ ] 2. Placeholder scan：无 TODO；所有 reminder/contract 文本仍由 injector 文件调用既有 contract 函数；behavior 拆分后代码完整。
- [ ] 3. No phantom tasks：Task 5.0 的 ESM 拆分是真实必要的编译修复；Task 5.4 的验证是真实产出。
- [ ] 4. Dependency soundness：Task 5.0 → Task 5.1 → Task 5.2 → Task 5.3 → Task 5.4；无向后引用。
- [ ] 5. Caller & build soundness：
  - `SessionMode` 构造函数新增可选 `registry` 参数，保持向后兼容；测试与 `Agent` 调用点无需修改。
  - `startDesignSession` / `closeCurrentDesignSession` 从 `private` 改为 `public`，唯一调用者为 `DesignModeBehavior`（本 part 修改）。
  - `InjectionManager` 的公共 API 不变。
  - Task 5.4 以 `pnpm -r typecheck` 覆盖全仓库。
- [ ] 6. Test-the-risk：
  - enter 回滚：behavior 抛错后状态复位、`setContextMode('normal')`。
  - model 恢复：enter 后 exit/cancel 恢复进入前的 model alias。
  - design session：仅 design mode 产生 session 记录，exit 后 `exitedAtMsg` 设置。
  - 幂等性：exit 两次只产生一条 `session_mode.exit` 记录。
  - InjectionManager：验证 4 个 mode injector variant 均注册。
- [ ] 7. Type consistency：
  - `SessionModeKind` 现在从 `./types` 导入并 re-export，与 Part 1 一致。
  - `ModeEnterContext` / `ModeExitContext` 从 `behaviors/types.ts` 导出，与 Task 5.0 一致。
  - `BaseSessionModeInjector` 子类仍兼容 `SessionModeInjector` 接口签名。
