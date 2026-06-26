# Part 3：ModeBehaviorRegistry 与具体 Behavior

本 part 建立 `SessionModeBehavior` 策略接口、`ModeBehaviorRegistry` 注册表、`BaseSessionModeBehavior` 基类以及 4 个具体 behavior。同时把目录解析、model 可用性判断从 `SessionMode` 提取为可复用工具函数，供 behavior 使用。

## Task 3.1：定义 `SessionModeBehavior` / `ModeBehaviorRegistry` / `SessionModeInjector` 接口

**Depends on:** Task 1.1, Task 1.2

**Files:**
- Create: `packages/agent-core/src/agent/session-mode/behaviors/index.ts`
- Test: `packages/agent-core/src/agent/session-mode/__tests__/behaviors.test.ts`

### 步骤

- [ ] **Write the failing test**

  新建 `packages/agent-core/src/agent/session-mode/__tests__/behaviors.test.ts`：

  ```ts
  import { describe, it, expect } from 'vitest';
  import { ModeBehaviorRegistry } from '../behaviors';
  import { PlanModeBehavior } from '../behaviors/plan';
  import { DesignModeBehavior } from '../behaviors/design';
  import { OfficeHoursModeBehavior } from '../behaviors/office-hours';
  import { GameDesignModeBehavior } from '../behaviors/game-design';

  describe('ModeBehaviorRegistry', () => {
    it('resolves registered behaviors by kind', () => {
      const registry = new ModeBehaviorRegistry();
      registry.register(new PlanModeBehavior());
      registry.register(new DesignModeBehavior());
      expect(registry.resolve('plan')).toBeInstanceOf(PlanModeBehavior);
      expect(registry.resolve('design')).toBeInstanceOf(DesignModeBehavior);
    });

    it('throws INTERNAL_ERROR for unregistered kinds', () => {
      const registry = new ModeBehaviorRegistry();
      expect(() => registry.resolve('plan')).toThrow('Unknown session mode kind: plan');
    });

    it('lists registered kinds', () => {
      const registry = new ModeBehaviorRegistry();
      registry.register(new PlanModeBehavior());
      registry.register(new DesignModeBehavior());
      expect(registry.kinds).toContain('plan');
      expect(registry.kinds).toContain('design');
      expect(registry.kinds).not.toContain('normal');
    });
  });
  ```

  此时 `PlanModeBehavior` 等尚未创建，测试会失败；本任务先完成 registry 与接口，具体类在 Task 3.3 实现。为了让测试可编译，Task 3.3 会同时创建这些类。若执行者希望逐步验证，可临时把测试文件留到 Task 3.3 再创建；但计划要求测试-first，因此本任务先写出测试，后续任务提供实现。

- [ ] **Run it and verify it FAILS**

  ```bash
  pnpm test packages/agent-core/src/agent/session-mode/__tests__/behaviors.test.ts
  ```

  预期失败：找不到 `../behaviors` 模块或具体类不存在。

- [ ] **Write the minimal implementation**

  新建 `packages/agent-core/src/agent/session-mode/behaviors/index.ts`：

  ```ts
  import type { Agent } from '../../..';
  import type { SessionModeKind } from '../types';
  import { ErrorCodes, OdyError } from '@odysseythink/agent-core-shared';

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
    inject(): Promise<void>;
    getInjection(): string | Promise<string | undefined> | undefined;
  }

  export interface SessionModeInjectorOptions {
    fullRefreshTurns: number;
    dedupMinTurns: number;
  }

  export interface SessionModeBehavior<TKind extends SessionModeKind> {
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
    // Import lazily to avoid circular imports with concrete behaviors.
    const { PlanModeBehavior } = require('./plan') as typeof import('./plan');
    const { DesignModeBehavior } = require('./design') as typeof import('./design');
    const { OfficeHoursModeBehavior } = require('./office-hours') as typeof import('./office-hours');
    const { GameDesignModeBehavior } = require('./game-design') as typeof import('./game-design');

    const registry = new ModeBehaviorRegistry();
    registry.register(new PlanModeBehavior());
    registry.register(new DesignModeBehavior());
    registry.register(new OfficeHoursModeBehavior());
    registry.register(new GameDesignModeBehavior());
    return registry;
  }
  ```

  注意：使用 `ErrorCodes.INTERNAL`（当前代码中 `INTERNAL` 与 `INTERNAL_ERROR` 都有？需确认 codes.ts 中是否有 `INTERNAL_ERROR`）。根据前文读取的 `packages/agent-core-shared/src/errors/codes.ts`，存在的是 `INTERNAL: 'internal'`。设计文档写的是 `ErrorCodes.INTERNAL_ERROR`，但实际代码是 `INTERNAL`。本计划以代码为准，使用 `ErrorCodes.INTERNAL`。

- [ ] **Run it and verify it PASSES（部分）**

  由于具体类尚未实现，registry 接口测试可通过直接实例化 `ModeBehaviorRegistry` 完成；引用 `PlanModeBehavior` 的用例仍会失败。执行者可保留测试到 Task 3.3 再运行完整测试：

  ```bash
  pnpm test packages/agent-core/src/agent/session-mode/__tests__/behaviors.test.ts
  ```

  预期：registry 基础测试（不引用具体类）通过；涉及具体类的测试失败。

- [ ] **Commit**

  ```bash
  git add packages/agent-core/src/agent/session-mode/behaviors/index.ts
  git commit -m "feat(agent-core): add ModeBehaviorRegistry and SessionModeBehavior interfaces"
  ```

## Task 3.2：提取目录解析与 model 可用性工具函数

**Depends on:** Task 3.1

**Files:**
- Create: `packages/agent-core/src/agent/session-mode/directory.ts`
- Create: `packages/agent-core/src/agent/session-mode/model-auth.ts`
- Test: `packages/agent-core/src/agent/session-mode/__tests__/directory.test.ts`

### 步骤

- [ ] **Write the failing test**

  新建 `packages/agent-core/src/agent/session-mode/__tests__/directory.test.ts`：

  ```ts
  import { describe, it, expect, vi } from 'vitest';
  import { resolveSessionModeDirectory, getModeOutputSubdirectory } from '../directory';

  const CWD = '/workspace/project';

  function makeAgent(overrides: { homedir?: string; existing?: Set<string> } = {}) {
    const existing = overrides.existing ?? new Set<string>();
    return {
      config: { cwd: CWD },
      homedir: overrides.homedir,
      kaos: {
        mkdir: vi.fn().mockResolvedValue(undefined),
        stat: vi.fn(async (p: string) => {
          if (existing.has(p)) return { stMode: 0o100644 };
          throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
        }),
      },
    } as unknown as import('../../../src/agent').Agent;
  }

  describe('directory utilities', () => {
    it('returns the correct subdirectory for each kind', () => {
      expect(getModeOutputSubdirectory('plan')).toBe('plans');
      expect(getModeOutputSubdirectory('design')).toBe('designs');
      expect(getModeOutputSubdirectory('office-hours')).toBe('products');
      expect(getModeOutputSubdirectory('game-design')).toBe('game-design');
    });

    it('resolves project-scoped directory when mkdir succeeds', async () => {
      const agent = makeAgent();
      const result = await resolveSessionModeDirectory(agent, 'plan');
      expect(result.dir).toBe('/workspace/project/.ody-code/plans');
      expect(result.isProjectScoped).toBe(true);
    });

    it('falls back to homedir on permission error', async () => {
      const agent = makeAgent({ homedir: '/home/user' });
      agent.kaos.mkdir = vi.fn().mockRejectedValue(Object.assign(new Error('EACCES'), { code: 'EACCES' }));
      const result = await resolveSessionModeDirectory(agent, 'design');
      expect(result.dir).toBe('/home/user/designs');
      expect(result.isProjectScoped).toBe(false);
    });
  });
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  pnpm test packages/agent-core/src/agent/session-mode/__tests__/directory.test.ts
  ```

  预期失败：模块 `../directory` 不存在。

- [ ] **Write the minimal implementation**

  新建 `packages/agent-core/src/agent/session-mode/directory.ts`：

  ```ts
  import { join } from 'pathe';
  import type { Agent } from '..';
  import type { SessionModeKind } from './types';

  export function getModeOutputSubdirectory(kind: SessionModeKind): string {
    if (kind === 'office-hours') return 'products';
    if (kind === 'game-design') return 'game-design';
    if (kind === 'design') return 'designs';
    return 'plans';
  }

  export async function resolveSessionModeDirectory(
    agent: Agent,
    kind: SessionModeKind,
  ): Promise<{ dir: string; isProjectScoped: boolean }> {
    const subdir = getModeOutputSubdirectory(kind);
    const projectDir = join(agent.config.cwd, '.ody-code', subdir);
    try {
      await agent.kaos.mkdir(projectDir, { parents: true, existOk: true });
      return { dir: projectDir, isProjectScoped: true };
    } catch (error) {
      if (isPermissionError(error) && agent.homedir !== undefined) {
        const sessionDir = join(agent.homedir, subdir);
        await agent.kaos.mkdir(sessionDir, { parents: true, existOk: true });
        return { dir: sessionDir, isProjectScoped: false };
      }
      throw error;
    }
  }

  function isPermissionError(error: unknown): boolean {
    if (error === null || typeof error !== 'object') return false;
    const code = (error as { readonly code?: unknown }).code;
    return code === 'EACCES' || code === 'EPERM';
  }
  ```

  新建 `packages/agent-core/src/agent/session-mode/model-auth.ts`：

  ```ts
  import type { Agent } from '..';
  import type { ResolvedRuntimeProvider } from '../../session/provider-manager';

  export function modelAliasHasUsableAuth(
    agent: Agent,
    modelAlias: string,
    resolved: ResolvedRuntimeProvider,
  ): boolean {
    const withAuth = agent.modelProvider?.resolveAuth?.(modelAlias, { log: agent.log });
    if (withAuth !== undefined) return true;
    const apiKey = (resolved.provider as { apiKey?: string }).apiKey;
    return apiKey !== undefined && apiKey.length > 0;
  }
  ```

- [ ] **Run it and verify it PASSES**

  ```bash
  pnpm test packages/agent-core/src/agent/session-mode/__tests__/directory.test.ts
  ```

  预期：测试通过。

- [ ] **Commit**

  ```bash
  git add packages/agent-core/src/agent/session-mode/directory.ts packages/agent-core/src/agent/session-mode/model-auth.ts packages/agent-core/src/agent/session-mode/__tests__/directory.test.ts
  git commit -m "feat(agent-core): extract session-mode directory and model-auth utilities"
  ```

## Task 3.3：实现 `BaseSessionModeBehavior` 与 4 个具体 Behavior

**Depends on:** Task 3.1, Task 3.2

**Files:**
- Create: `packages/agent-core/src/agent/session-mode/behaviors/plan.ts`
- Create: `packages/agent-core/src/agent/session-mode/behaviors/design.ts`
- Create: `packages/agent-core/src/agent/session-mode/behaviors/office-hours.ts`
- Create: `packages/agent-core/src/agent/session-mode/behaviors/game-design.ts`
- Modify: `packages/agent-core/src/agent/session-mode/behaviors/index.ts`（添加 `BaseSessionModeBehavior`）
- Test: `packages/agent-core/src/agent/session-mode/__tests__/behaviors.test.ts`

### 步骤

- [ ] **Write the failing test**

  在 `packages/agent-core/src/agent/session-mode/__tests__/behaviors.test.ts` 追加具体 behavior 断言（完整文件如下，覆盖 Task 3.1 与 Task 3.3）：

  ```ts
  import { describe, it, expect } from 'vitest';
  import { ModeBehaviorRegistry, createDefaultModeBehaviorRegistry } from '../behaviors';
  import { PlanModeBehavior } from '../behaviors/plan';
  import { DesignModeBehavior } from '../behaviors/design';
  import { OfficeHoursModeBehavior } from '../behaviors/office-hours';
  import { GameDesignModeBehavior } from '../behaviors/game-design';

  describe('ModeBehaviorRegistry', () => {
    it('resolves registered behaviors by kind', () => {
      const registry = new ModeBehaviorRegistry();
      registry.register(new PlanModeBehavior());
      registry.register(new DesignModeBehavior());
      expect(registry.resolve('plan')).toBeInstanceOf(PlanModeBehavior);
      expect(registry.resolve('design')).toBeInstanceOf(DesignModeBehavior);
    });

    it('throws INTERNAL for unregistered kinds', () => {
      const registry = new ModeBehaviorRegistry();
      expect(() => registry.resolve('plan')).toThrow('Unknown session mode kind: plan');
    });

    it('lists registered kinds', () => {
      const registry = createDefaultModeBehaviorRegistry();
      expect(registry.kinds).toEqual(['plan', 'design', 'office-hours', 'game-design']);
    });
  });

  describe('concrete behaviors', () => {
    it('has correct outputSubdirectory and modeModelKey for each kind', () => {
      expect(new PlanModeBehavior()).toMatchObject({ kind: 'plan', outputSubdirectory: 'plans', modeModelKey: 'plan' });
      expect(new DesignModeBehavior()).toMatchObject({ kind: 'design', outputSubdirectory: 'designs', modeModelKey: 'design' });
      expect(new OfficeHoursModeBehavior()).toMatchObject({ kind: 'office-hours', outputSubdirectory: 'products', modeModelKey: 'officeHours' });
      expect(new GameDesignModeBehavior()).toMatchObject({ kind: 'game-design', outputSubdirectory: 'game-design', modeModelKey: 'gameDesign' });
    });

    it('has correct handoff targets', () => {
      expect(new DesignModeBehavior().handoffTarget).toBe('plan');
      expect(new PlanModeBehavior().handoffTarget).toBe('normal');
      expect(new OfficeHoursModeBehavior().handoffTarget).toBeUndefined();
      expect(new GameDesignModeBehavior().handoffTarget).toBeUndefined();
    });

    it('tracks design sessions only for design', () => {
      expect(new DesignModeBehavior().supportsDesignSessions).toBe(true);
      expect(new PlanModeBehavior().supportsDesignSessions).toBeUndefined();
      expect(new OfficeHoursModeBehavior().supportsDesignSessions).toBeUndefined();
      expect(new GameDesignModeBehavior().supportsDesignSessions).toBeUndefined();
    });
  });
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  pnpm test packages/agent-core/src/agent/session-mode/__tests__/behaviors.test.ts
  ```

  预期失败：具体 behavior 类不存在。

- [ ] **Write the minimal implementation**

  在 `packages/agent-core/src/agent/session-mode/behaviors/index.ts` 追加 `BaseSessionModeBehavior`：

  ```ts
  import { ensureGitignore } from '../../../utils/gitignore';
  import { resolveSessionModeDirectory } from '../directory';
  import { modelAliasHasUsableAuth } from '../model-auth';

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

  创建 `packages/agent-core/src/agent/session-mode/behaviors/plan.ts`：

  ```ts
  import type { Agent } from '../../..';
  import { PlanModeInjector } from '../../injection/plan-mode';
  import { BaseSessionModeBehavior } from '.';

  export class PlanModeBehavior extends BaseSessionModeBehavior<'plan'> {
    readonly kind = 'plan' as const;
    readonly outputSubdirectory = 'plans';
    readonly modeModelKey = 'plan';
    readonly injectorClass = PlanModeInjector;
    readonly handoffTarget = 'normal' as const;
  }
  ```

  创建 `packages/agent-core/src/agent/session-mode/behaviors/design.ts`：

  ```ts
  import type { Agent } from '../../..';
  import { DesignModeInjector } from '../../injection/design-mode';
  import { BaseSessionModeBehavior } from '.';

  export class DesignModeBehavior extends BaseSessionModeBehavior<'design'> {
    readonly kind = 'design' as const;
    readonly outputSubdirectory = 'designs';
    readonly modeModelKey = 'design';
    readonly injectorClass = DesignModeInjector;
    readonly handoffTarget = 'plan' as const;
    readonly supportsDesignSessions = true;
  }
  ```

  创建 `packages/agent-core/src/agent/session-mode/behaviors/office-hours.ts`：

  ```ts
  import type { Agent } from '../../..';
  import { OfficeHoursInjector } from '../../injection/office-hours';
  import { BaseSessionModeBehavior } from '.';

  export class OfficeHoursModeBehavior extends BaseSessionModeBehavior<'office-hours'> {
    readonly kind = 'office-hours' as const;
    readonly outputSubdirectory = 'products';
    readonly modeModelKey = 'officeHours';
    readonly injectorClass = OfficeHoursInjector;
  }
  ```

  创建 `packages/agent-core/src/agent/session-mode/behaviors/game-design.ts`：

  ```ts
  import type { Agent } from '../../..';
  import { GameDesignInjector } from '../../injection/game-design';
  import { BaseSessionModeBehavior } from '.';

  export class GameDesignModeBehavior extends BaseSessionModeBehavior<'game-design'> {
    readonly kind = 'game-design' as const;
    readonly outputSubdirectory = 'game-design';
    readonly modeModelKey = 'gameDesign';
    readonly injectorClass = GameDesignInjector;
  }
  ```

  注意：当前 `PlanModeInjector` / `DesignModeInjector` 等仍是 `DynamicInjector` 子类，尚未改为 `SessionModeInjector` 接口实现。TypeScript 会把符合结构类型的类当作兼容，但 `injectorClass` 要求构造函数返回 `SessionModeInjector`；由于 `DynamicInjector` 与 `SessionModeInjector` 结构不同（`DynamicInjector` 有 `onContextCompacted` / `onContextMessageRemoved`），直接赋值会产生类型错误。为避免此问题，可暂时将 `BaseSessionModeBehavior` 的 `injectorClass` 类型放宽为 `new (agent: Agent) => SessionModeInjector | DynamicInjector`，或在 Part 4 完成后再运行本任务测试。更稳妥的做法是：将 `SessionModeInjector` 接口扩展以包含 `onContextCompacted` 和 `onContextMessageRemoved`（设计文档中未列出但实际生命周期需要），使现有 `DynamicInjector` 自动兼容。

  修改 `SessionModeInjector` 接口：

  ```ts
  export interface SessionModeInjector {
    readonly injectionVariant: string;
    onContextClear(): void;
    onContextCompacted(compactedCount: number): void;
    onContextMessageRemoved(index: number): void;
    inject(): Promise<void>;
    getInjection(): string | Promise<string | undefined> | undefined;
  }
  ```

- [ ] **Run it and verify it PASSES**

  ```bash
  pnpm test packages/agent-core/src/agent/session-mode/__tests__/behaviors.test.ts
  ```

  预期：所有测试通过。

- [ ] **Commit**

  ```bash
  git add packages/agent-core/src/agent/session-mode/behaviors packages/agent-core/src/agent/session-mode/__tests__/behaviors.test.ts
  git commit -m "feat(agent-core): add BaseSessionModeBehavior and four concrete behaviors"
  ```

## Local Self-Review

- [ ] 1. Spec-coverage：覆盖 Scope In #4（`SessionModeBehavior` 策略接口与注册表）。
- [ ] 2. Placeholder scan：无 TODO；接口、基类、具体类均完整给出。
- [ ] 3. No phantom tasks：Task 3.2 提取的 utilities 是 Task 3.3 的实际依赖。
- [ ] 4. Dependency soundness：Task 3.1 → Task 3.2 → Task 3.3；Task 3.3 使用 Task 3.2 的 `directory.ts` / `model-auth.ts`。
- [ ] 5. Caller & build soundness：本 part 新增文件未修改共享签名；`SessionModeInjector` 接口扩展以兼容现有 `DynamicInjector`。
- [ ] 6. Test-the-risk：behavior 测试断言 `outputSubdirectory`、`modeModelKey`、`handoffTarget`、`supportsDesignSessions` 与实现常量一一对应；非法 kind 抛错路径已测。
- [ ] 7. Type consistency：`BaseSessionModeBehavior` 的 `kind` / `outputSubdirectory` / `modeModelKey` 与子类常量一致；`SessionModeInjector` 接口将在 Part 4 实现。
