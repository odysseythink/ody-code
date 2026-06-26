# Part 4：SessionModeInjector 基类与注入器重构

本 part 创建 `BaseSessionModeInjector` 抽象基类，统一 `full/sparse/reentry` 变体调度、`onContextClear` 状态记忆与 `inject()` 模板方法；随后将 `PlanModeInjector`、`DesignModeInjector`、`OfficeHoursInjector`、`GameDesignInjector` 改为继承基类，仅保留各 mode 的 reminder 文本与少量扩展上下文（如 design 的 `mockupAvailable`、plan/design 的 handoff 与 skills reminder）。

## Task 4.1：创建 `BaseSessionModeInjector` 抽象基类

**Depends on:** Task 3.1, Task 3.3

**Files:**
- Create: `packages/agent-core/src/agent/injection/session-mode-injector.ts`
- Modify: `packages/agent-core/src/agent/injection/injector.ts`（可选：保持 `DynamicInjector` 不变；基类通过继承复用）
- Test: `packages/agent-core/src/agent/injection/__tests__/session-mode-injector.test.ts`

### 步骤

- [ ] **Write the failing test**

  新建 `packages/agent-core/src/agent/injection/__tests__/session-mode-injector.test.ts`：

  ```ts
  import { describe, it, expect, vi } from 'vitest';
  import type { Agent } from '../../..';
  import type { SessionModeFilePath } from '../../session-mode';
  import type { SessionModeInjectorOptions } from '../../session-mode/behaviors';
  import { BaseSessionModeInjector } from '../session-mode-injector';

  class TestInjector extends BaseSessionModeInjector {
    readonly injectionVariant = 'test_mode';
    readonly options: SessionModeInjectorOptions = { fullRefreshTurns: 5, dedupMinTurns: 2 };
    active = false;
    injectedAtValue(): number | null {
      return this.injectedAt;
    }

    isModeActive(): boolean {
      return this.active;
    }

    computeVariantPublic(
      injectedAt: number | null,
      history: { role: string }[],
      options: SessionModeInjectorOptions,
    ): 'full' | 'sparse' | null {
      return this.computeVariant(injectedAt, history, options);
    }

    protected getEntryReminder(): string {
      return 'entry';
    }

    protected getReentryReminder(): string {
      return 'reentry';
    }

    protected getFullReminder(): string {
      return 'full';
    }

    protected getSparseReminder(): string {
      return 'sparse';
    }

    protected getExitReminder(): string {
      return 'exit';
    }
  }

  function makeAgent(overrides: {
    isActive?: boolean;
    kind?: 'plan';
    filePath?: SessionModeFilePath;
    content?: string;
    history?: { role: string }[];
  } = {}): Agent {
    return {
      sessionMode: {
        isActive: overrides.isActive ?? false,
        kind: overrides.kind ?? 'plan',
        sessionModeFilePath: overrides.filePath ?? null,
        data: vi.fn().mockResolvedValue(overrides.content ? { content: overrides.content } : null),
      },
      context: {
        history: overrides.history ?? [],
        appendSystemReminder: vi.fn(),
      },
    } as unknown as Agent;
  }

  describe('BaseSessionModeInjector', () => {
    it('computeVariant returns full when injectedAt is null', () => {
      const injector = new TestInjector(makeAgent());
      expect(injector.computeVariantPublic(null, [], { fullRefreshTurns: 5, dedupMinTurns: 2 })).toBe('full');
    });

    it('computeVariant returns null with only one assistant turn', () => {
      const injector = new TestInjector(makeAgent());
      const history = [{ role: 'assistant' }];
      expect(injector.computeVariantPublic(0, history, { fullRefreshTurns: 5, dedupMinTurns: 2 })).toBeNull();
    });

    it('computeVariant returns sparse at dedup threshold', () => {
      const injector = new TestInjector(makeAgent());
      const history = [{ role: 'assistant' }, { role: 'assistant' }];
      expect(injector.computeVariantPublic(0, history, { fullRefreshTurns: 5, dedupMinTurns: 2 })).toBe('sparse');
    });

    it('computeVariant returns full at refresh threshold', () => {
      const injector = new TestInjector(makeAgent());
      const history = Array.from({ length: 5 }, () => ({ role: 'assistant' }));
      expect(injector.computeVariantPublic(0, history, { fullRefreshTurns: 5, dedupMinTurns: 2 })).toBe('full');
    });

    it('computeVariant returns full when user message appears after injection', () => {
      const injector = new TestInjector(makeAgent());
      const history = [{ role: 'assistant' }, { role: 'user' }];
      expect(injector.computeVariantPublic(0, history, { fullRefreshTurns: 5, dedupMinTurns: 2 })).toBe('full');
    });

    it('onContextClear resets injectedAt and remembers wasActive', async () => {
      const agent = makeAgent({ isActive: true });
      const injector = new TestInjector(agent);
      injector.active = true;
      await injector.inject();
      expect(injector.injectedAtValue()).toBe(0);
      injector.onContextClear();
      expect(injector.injectedAtValue()).toBeNull();
      // After clear, a second inject should see wasActive=true so it returns exit reminder.
      injector.active = false;
      await injector.inject();
      const reminder = agent.context.appendSystemReminder.mock.calls[1][0];
      expect(reminder).toContain('exit');
    });

    it('inject appends system reminder with injection origin', async () => {
      const agent = makeAgent({ isActive: true });
      const injector = new TestInjector(agent);
      injector.active = true;
      await injector.inject();
      expect(agent.context.appendSystemReminder).toHaveBeenCalledWith('entry', {
        kind: 'injection',
        variant: 'test_mode',
      });
    });
  });
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  pnpm test packages/agent-core/src/agent/injection/__tests__/session-mode-injector.test.ts
  ```

  预期失败：`../session-mode-injector` 模块不存在。

- [ ] **Write the minimal implementation**

  新建 `packages/agent-core/src/agent/injection/session-mode-injector.ts`：

  ```ts
  import type { Message } from '@odysseythink/kosong';

  import type { Agent } from '..';
  import type { SessionModeFilePath } from '../session-mode';
  import type { SessionModeInjectorOptions } from '../session-mode/behaviors';
  import { DynamicInjector } from './injector';

  export abstract class BaseSessionModeInjector extends DynamicInjector {
    protected wasActive = false;
    protected currentContent = '';

    abstract override readonly injectionVariant: string;
    abstract readonly options: SessionModeInjectorOptions;

    abstract isModeActive(): boolean;

    override onContextClear(): void {
      super.onContextClear();
      this.wasActive = this.isModeActive();
    }

    override async getInjection(): Promise<string | undefined> {
      const active = this.isModeActive();
      const path = this.agent.sessionMode.sessionModeFilePath;

      if (!active) {
        if (!this.wasActive) {
          return undefined;
        }
        this.wasActive = false;
        this.injectedAt = null;
        return this.decorateReminder(this.getExitInjection(path));
      }

      this.currentContent = await this.readModeContent();

      if (!this.wasActive) {
        this.injectedAt = null;
        this.wasActive = true;
        return this.decorateReminder(this.getEntryInjection(this.currentContent, path));
      }

      const variant = this.computeVariant(this.injectedAt, this.agent.context.history, this.options);
      if (variant === null) {
        return undefined;
      }
      if (variant === 'full') {
        return this.decorateReminder(this.getFullReminder(path));
      }
      return this.decorateReminder(this.getSparseReminder(path));
    }

    protected readModeContent(): Promise<string> {
      try {
        return this.agent.sessionMode.data().then((data) => data?.content ?? '');
      } catch {
        return Promise.resolve('');
      }
    }

    protected computeVariant(
      injectedAt: number | null,
      history: readonly Message[],
      options: SessionModeInjectorOptions,
    ): 'full' | 'sparse' | null {
      if (injectedAt === null) {
        return 'full';
      }
      let assistantTurnsSince = 0;
      for (let i = injectedAt + 1; i < history.length; i++) {
        const msg = history[i];
        if (msg === undefined) {
          continue;
        }
        if (msg.role === 'assistant') {
          assistantTurnsSince += 1;
          continue;
        }
        if (msg.role === 'user') {
          return 'full';
        }
      }
      if (assistantTurnsSince >= options.fullRefreshTurns) {
        return 'full';
      }
      if (assistantTurnsSince >= options.dedupMinTurns) {
        return 'sparse';
      }
      return null;
    }

    protected getEntryInjection(content: string, path: SessionModeFilePath): string | undefined {
      if (content.trim().length > 0) {
        return this.getReentryReminder(path);
      }
      return this.getEntryReminder(path);
    }

    protected getExitInjection(path: SessionModeFilePath): string | undefined {
      return this.getExitReminder(path);
    }

    protected decorateReminder(body: string): string {
      return body;
    }

    protected abstract getEntryReminder(path: SessionModeFilePath): string;
    protected abstract getReentryReminder(path: SessionModeFilePath): string;
    protected abstract getFullReminder(path: SessionModeFilePath): string;
    protected abstract getSparseReminder(path: SessionModeFilePath): string;
    protected abstract getExitReminder(path: SessionModeFilePath): string;
  }
  ```

  注意：`BaseSessionModeInjector` 隐式实现 `behaviors/index.ts` 中的 `SessionModeInjector` 接口（继承 `DynamicInjector` 的 `onContextCompacted` / `onContextMessageRemoved`，自身实现 `onContextClear` / `inject` / `getInjection`）。

- [ ] **Run it and verify it PASSES**

  ```bash
  pnpm test packages/agent-core/src/agent/injection/__tests__/session-mode-injector.test.ts
  ```

  预期：所有测试通过。

- [ ] **Commit**

  ```bash
  git add packages/agent-core/src/agent/injection/session-mode-injector.ts packages/agent-core/src/agent/injection/__tests__/session-mode-injector.test.ts
  git commit -m "feat(agent-core): add BaseSessionModeInjector abstract base class"
  ```

## Task 4.2：重构 `PlanModeInjector` 继承基类

**Depends on:** Task 4.1

**Files:**
- Modify: `packages/agent-core/src/agent/injection/plan-mode.ts`
- Test: `packages/agent-core/src/agent/injection/__tests__/plan-mode-injector.test.ts`

### 步骤

- [ ] **Write the failing test**

  新建 `packages/agent-core/src/agent/injection/__tests__/plan-mode-injector.test.ts`：

  ```ts
  import { describe, it, expect, vi } from 'vitest';
  import type { Agent } from '../../..';
  import type { SessionModeFilePath } from '../../session-mode';
  import { PlanModeInjector } from '../plan-mode';

  function makeAgent(overrides: {
    isActive?: boolean;
    kind?: 'plan';
    filePath?: SessionModeFilePath;
    content?: string;
    history?: { role: string }[];
  } = {}): Agent {
    return {
      sessionMode: {
        isActive: overrides.isActive ?? false,
        kind: overrides.kind ?? 'plan',
        sessionModeFilePath: overrides.filePath ?? null,
        data: vi.fn().mockResolvedValue(overrides.content ? { content: overrides.content } : null),
      },
      context: {
        history: overrides.history ?? [],
        appendSystemReminder: vi.fn(),
      },
      skills: { registry: { getUnavailableSkillsReminder: vi.fn().mockReturnValue('') } },
    } as unknown as Agent;
  }

  describe('PlanModeInjector', () => {
    it('injects reentry reminder when plan is active and content exists', async () => {
      const agent = makeAgent({ isActive: true, kind: 'plan', filePath: '/plan.md', content: '# Plan' });
      const injector = new PlanModeInjector(agent);
      await injector.inject();
      const reminder = agent.context.appendSystemReminder.mock.calls[0][0];
      expect(reminder).toContain('Re-entering Plan Mode');
    });

    it('injects full reminder when plan is active but content is empty', async () => {
      const agent = makeAgent({ isActive: true, kind: 'plan', filePath: '/plan.md' });
      const injector = new PlanModeInjector(agent);
      await injector.inject();
      const reminder = agent.context.appendSystemReminder.mock.calls[0][0];
      expect(reminder).toContain('implementation-planning session');
    });

    it('appends skills reminder when registry returns one', async () => {
      const agent = makeAgent({ isActive: true, kind: 'plan', filePath: '/plan.md', content: '# Plan' });
      agent.skills.registry.getUnavailableSkillsReminder = vi.fn().mockReturnValue('Skill X is unavailable');
      const injector = new PlanModeInjector(agent);
      await injector.inject();
      const reminder = agent.context.appendSystemReminder.mock.calls[0][0];
      expect(reminder).toContain('Skill X is unavailable');
    });

    it('injects handoff reminder on exit when a normal handoff is pending', async () => {
      const agent = makeAgent({ isActive: false, filePath: '/plan.md' });
      agent.sessionMode.consumePendingHandoffForNormal = vi.fn().mockReturnValue({
        content: 'approved plan',
        path: '/plan.md',
        selectedLabel: 'Option A',
      });
      const injector = new PlanModeInjector(agent);
      injector.onContextClear();
      await injector.inject();
      const reminder = agent.context.appendSystemReminder.mock.calls[0][0];
      expect(reminder).toContain('Plan mode is no longer active');
      expect(reminder).toContain('Option A');
    });
  });
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  pnpm test packages/agent-core/src/agent/injection/__tests__/plan-mode-injector.test.ts
  ```

  预期失败：`PlanModeInjector` 尚未继承基类，或基类方法不存在。

- [ ] **Write the minimal implementation**

  将 `packages/agent-core/src/agent/injection/plan-mode.ts` 完整替换为：

  ```ts
  import { basename } from 'pathe';

  import type { SessionModeFilePath } from '../session-mode';
  import { BaseSessionModeInjector } from './session-mode-injector';
  import {
    type ManifestPart,
    parsePartsManifest,
    planModeFullReminder,
    planModeReentryReminder,
    planModeSparseReminder,
    splitContinuationDirective,
    splitFinalReviewDirective,
  } from './plan-mode-contract';

  const PLAN_MODE_DEDUP_MIN_TURNS = 2;
  const PLAN_MODE_FULL_REFRESH_TURNS = 5;

  /**
   * Plan-mode reminder variants.
   *
   * `reentry` is used once when a restored planning session already has plan
   * content. `full` is used for the first reminder and periodic refreshes.
   * `sparse` keeps the read-only invariant visible between full reminders.
   */
  export type PlanModeVariant = 'full' | 'sparse' | 'reentry';

  export class PlanModeInjector extends BaseSessionModeInjector {
    protected override readonly injectionVariant = 'plan_mode';
    protected override readonly options = {
      fullRefreshTurns: PLAN_MODE_FULL_REFRESH_TURNS,
      dedupMinTurns: PLAN_MODE_DEDUP_MIN_TURNS,
    };

    isModeActive(): boolean {
      return this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'plan';
    }

    protected override getEntryInjection(content: string, path: SessionModeFilePath): string | undefined {
      if (content.trim().length > 0) {
        return planModeReentryReminder(path);
      }
      return this.getFullReminder(path);
    }

    protected override getExitInjection(path: SessionModeFilePath): string | undefined {
      const handoff = this.agent.sessionMode.consumePendingHandoffForNormal();
      if (handoff !== null) {
        return planToNormalHandoffReminder(handoff.content, handoff.path, handoff.selectedLabel);
      }
      return exitReminder();
    }

    protected override decorateReminder(body: string): string {
      const skillsReminder = this.agent.skills?.registry.getUnavailableSkillsReminder('plan') ?? '';
      return appendSkillsReminder(body, skillsReminder);
    }

    protected getEntryReminder(_path: SessionModeFilePath): string {
      // Plan mode does not use a dedicated entry reminder; empty content falls through to full.
      return this.getFullReminder(_path);
    }

    protected getReentryReminder(path: SessionModeFilePath): string {
      return planModeReentryReminder(path);
    }

    protected getFullReminder(path: SessionModeFilePath): string {
      const directive = splitDirectiveFor(this.currentContent, path);
      return planModeFullReminder(path, directive);
    }

    protected getSparseReminder(path: SessionModeFilePath): string {
      const directive = splitDirectiveFor(this.currentContent, path);
      return planModeSparseReminder(path, directive);
    }

    protected getExitReminder(_path: SessionModeFilePath): string {
      return exitReminder();
    }
  }

  /**
   * When the current plan file is a split index, derive the directive that steers
   * the model to the next pending part (or the cross-file final review once every
   * part is done). Returns undefined for single-file plans (no manifest).
   */
  function splitDirectiveFor(content: string, sessionModeFilePath: SessionModeFilePath): string | undefined {
    const manifest = parsePartsManifest(content);
    if (manifest === null) return undefined;
    if (manifest.next !== null) {
      const next: ManifestPart = manifest.next;
      return splitContinuationDirective(next, indexStemFor(sessionModeFilePath));
    }
    if (manifest.allDone) return splitFinalReviewDirective();
    return undefined;
  }

  /** The index file's stem (filename without the `.md`), used as the split subdirectory name. */
  function indexStemFor(sessionModeFilePath: SessionModeFilePath): string {
    if (sessionModeFilePath === null || sessionModeFilePath.length === 0) return '';
    return basename(sessionModeFilePath).replace(/\.md$/, '');
  }

  function exitReminder(): string {
    return `Plan mode was cancelled — no plan was approved or handed off. The read-only and plan-file-only restrictions no longer apply. Continue with normal operation.`;
  }

  function planToNormalHandoffReminder(content: string, path: string, selectedLabel?: string): string {
    const savedTo = path ? `Plan saved to: ${path}\n\n` : '';
    const optionPrefix =
      selectedLabel !== undefined && selectedLabel.length > 0
        ? `Selected approach: ${selectedLabel}. Implement ONLY this approach; do not execute any unselected alternatives.\n\n`
        : '';
    return `Plan mode is no longer active. The approved plan has been handed off to this context.\n\n${optionPrefix}${savedTo}## Approved Plan\n\n${content}\n\nProceed with implementing the plan above using the normal tool and permission rules.`;
  }

  function appendSkillsReminder(body: string, reminder: string): string {
    return reminder.length > 0 ? `${body}\n\n${reminder}` : body;
  }
  ```

- [ ] **Run it and verify it PASSES**

  ```bash
  pnpm test packages/agent-core/src/agent/injection/__tests__/plan-mode-injector.test.ts
  ```

  预期：所有测试通过。

- [ ] **Commit**

  ```bash
  git add packages/agent-core/src/agent/injection/plan-mode.ts packages/agent-core/src/agent/injection/__tests__/plan-mode-injector.test.ts
  git commit -m "refactor(agent-core): PlanModeInjector extends BaseSessionModeInjector"
  ```

## Task 4.3：重构 `DesignModeInjector` 继承基类

**Depends on:** Task 4.1, Task 4.2

**Files:**
- Modify: `packages/agent-core/src/agent/injection/design-mode.ts`
- Test: `packages/agent-core/src/agent/injection/__tests__/design-mode-injector.test.ts`

### 步骤

- [ ] **Write the failing test**

  新建 `packages/agent-core/src/agent/injection/__tests__/design-mode-injector.test.ts`：

  ```ts
  import { describe, it, expect, vi } from 'vitest';
  import type { Agent } from '../../..';
  import type { SessionModeFilePath } from '../../session-mode';
  import { DesignModeInjector } from '../design-mode';

  function makeAgent(overrides: {
    isActive?: boolean;
    kind?: 'design';
    filePath?: SessionModeFilePath;
    content?: string;
    history?: { role: string }[];
    mockupActive?: boolean;
  } = {}): Agent {
    return {
      sessionMode: {
        isActive: overrides.isActive ?? false,
        kind: overrides.kind ?? 'design',
        sessionModeFilePath: overrides.filePath ?? null,
        data: vi.fn().mockResolvedValue(overrides.content ? { content: overrides.content } : null),
      },
      context: {
        history: overrides.history ?? [],
        appendSystemReminder: vi.fn(),
      },
      skills: { registry: { getUnavailableSkillsReminder: vi.fn().mockReturnValue('') } },
      tools: { isToolActive: vi.fn().mockReturnValue(overrides.mockupActive ?? false) },
    } as unknown as Agent;
  }

  describe('DesignModeInjector', () => {
    it('injects reentry reminder when design is active and content exists', async () => {
      const agent = makeAgent({ isActive: true, kind: 'design', filePath: '/design.md', content: '# Design' });
      const injector = new DesignModeInjector(agent);
      await injector.inject();
      const reminder = agent.context.appendSystemReminder.mock.calls[0][0];
      expect(reminder).toContain('Re-entering Design Mode');
    });

    it('includes visual companion when ShowDesignMockup is active', async () => {
      const agent = makeAgent({
        isActive: true,
        kind: 'design',
        filePath: '/design.md',
        content: '# Design',
        mockupActive: true,
      });
      const injector = new DesignModeInjector(agent);
      await injector.inject();
      const reminder = agent.context.appendSystemReminder.mock.calls[0][0];
      expect(reminder).toContain('Visual companion');
    });

    it('injects plan handoff reminder on exit when a plan handoff is pending', async () => {
      const agent = makeAgent({ isActive: false, filePath: '/design.md' });
      agent.sessionMode.consumePendingHandoffForPlan = vi.fn().mockReturnValue({
        path: '/design.md',
        filename: 'design.md',
        selectedLabel: 'Approach B',
      });
      const injector = new DesignModeInjector(agent);
      injector.onContextClear();
      await injector.inject();
      const reminder = agent.context.appendSystemReminder.mock.calls[0][0];
      expect(reminder).toContain('Design mode completed');
      expect(reminder).toContain('Approach B');
    });
  });
  ```

- [ ] **Run it and verify it FAILS**

  ```bash
  pnpm test packages/agent-core/src/agent/injection/__tests__/design-mode-injector.test.ts
  ```

  预期失败：`DesignModeInjector` 尚未继承基类。

- [ ] **Write the minimal implementation**

  将 `packages/agent-core/src/agent/injection/design-mode.ts` 完整替换为：

  ```ts
  import { basename } from 'pathe';

  import type { SessionModeFilePath } from '../session-mode';
  import { BaseSessionModeInjector } from './session-mode-injector';
  import {
    designModeFullReminder,
    designModeReentryReminder,
    designModeSparseReminder,
    designSplitContinuationDirective,
    designSplitFinalReviewDirective,
  } from './design-mode-contract';
  import { type ManifestPart, parsePartsManifest } from './parts-manifest';

  const DESIGN_MODE_DEDUP_MIN_TURNS = 2;
  const DESIGN_MODE_FULL_REFRESH_TURNS = 5;

  export type DesignModeVariant = 'full' | 'sparse' | 'reentry';

  export class DesignModeInjector extends BaseSessionModeInjector {
    protected override readonly injectionVariant = 'design_mode';
    protected override readonly options = {
      fullRefreshTurns: DESIGN_MODE_FULL_REFRESH_TURNS,
      dedupMinTurns: DESIGN_MODE_DEDUP_MIN_TURNS,
    };

    isModeActive(): boolean {
      return this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'design';
    }

    protected override getEntryInjection(content: string, path: SessionModeFilePath): string | undefined {
      if (content.trim().length > 0) {
        const directive = splitDirectiveFor(content, path);
        return designModeReentryReminder(path, this.mockupAvailable(), directive);
      }
      return this.getFullReminder(path);
    }

    protected override getExitInjection(path: SessionModeFilePath): string | undefined {
      const handoff = this.agent.sessionMode.consumePendingHandoffForPlan();
      if (handoff !== null) {
        return designToPlanHandoffReminder(handoff.path, handoff.filename, handoff.selectedLabel);
      }
      return exitReminder();
    }

    protected override decorateReminder(body: string): string {
      const skillsReminder = this.agent.skills?.registry.getUnavailableSkillsReminder('design') ?? '';
      return appendSkillsReminder(body, skillsReminder);
    }

    protected getEntryReminder(_path: SessionModeFilePath): string {
      return this.getFullReminder(_path);
    }

    protected getReentryReminder(path: SessionModeFilePath): string {
      return designModeReentryReminder(path, this.mockupAvailable());
    }

    protected getFullReminder(path: SessionModeFilePath): string {
      const directive = splitDirectiveFor(this.currentContent, path);
      return designModeFullReminder(path, this.mockupAvailable(), directive);
    }

    protected getSparseReminder(path: SessionModeFilePath): string {
      const directive = splitDirectiveFor(this.currentContent, path);
      return designModeSparseReminder(path, this.mockupAvailable(), directive);
    }

    protected getExitReminder(_path: SessionModeFilePath): string {
      return exitReminder();
    }

    private mockupAvailable(): boolean {
      return this.agent.tools.isToolActive('ShowDesignMockup');
    }
  }

  /**
   * When the current design file is a split index, derive the directive that steers
   * the model to the next pending part (or the cross-file final review once every
   * part is done). Returns undefined for single-file designs (no manifest).
   */
  function splitDirectiveFor(content: string, sessionModeFilePath: SessionModeFilePath): string | undefined {
    const manifest = parsePartsManifest(content);
    if (manifest === null) return undefined;
    if (manifest.next !== null) {
      const next: ManifestPart = manifest.next;
      return designSplitContinuationDirective(next, indexStemFor(sessionModeFilePath));
    }
    if (manifest.allDone) return designSplitFinalReviewDirective();
    return undefined;
  }

  /** The index file's stem (filename without the `.md`), used as the split subdirectory name. */
  function indexStemFor(sessionModeFilePath: SessionModeFilePath): string {
    if (sessionModeFilePath === null || sessionModeFilePath.length === 0) return '';
    return basename(sessionModeFilePath).replace(/\.md$/, '');
  }

  function exitReminder(): string {
    return `Design mode was cancelled — no design was approved or handed off. Continue with normal operation.`;
  }

  function designToPlanHandoffReminder(
    path: string,
    filename: string,
    selectedLabel?: string,
  ): string {
    const savedTo = path ? `Design saved to: ${path}\n\n` : '';
    const selectedLabelPrefix =
      selectedLabel !== undefined && selectedLabel.length > 0
        ? `Selected approach: ${selectedLabel}. Execute ONLY the selected approach; do not execute any unselected alternatives.\n\n`
        : '';
    return `Design mode completed. The approved design has been handed off — you are now in plan mode.\n\n${savedTo}${selectedLabelPrefix}Create a concrete, step-by-step implementation plan based on the approved design in \`${filename}\`. Do not implement anything yet.`;
  }

  function appendSkillsReminder(body: string, reminder: string): string {
    return reminder.length > 0 ? `${body}\n\n${reminder}` : body;
  }
  ```

- [ ] **Run it and verify it PASSES**

  ```bash
  pnpm test packages/agent-core/src/agent/injection/__tests__/design-mode-injector.test.ts
  ```

  预期：所有测试通过。

- [ ] **Commit**

  ```bash
  git add packages/agent-core/src/agent/injection/design-mode.ts packages/agent-core/src/agent/injection/__tests__/design-mode-injector.test.ts
  git commit -m "refactor(agent-core): DesignModeInjector extends BaseSessionModeInjector"
  ```

## Task 4.4：重构 `OfficeHoursInjector` 继承基类

**Depends on:** Task 4.1

**Files:**
- Modify: `packages/agent-core/src/agent/injection/office-hours.ts`
- Test: `packages/agent-core/src/agent/injection/__tests__/office-hours-injector.test.ts`

### 步骤

- [ ] **Write the failing test**

  新建 `packages/agent-core/src/agent/injection/__tests__/office-hours-injector.test.ts`：

  ```ts
  import { describe, it, expect, vi } from 'vitest';
  import type { Agent } from '../../..';
  import type { SessionModeFilePath } from '../../session-mode';
  import { OfficeHoursInjector } from '../office-hours';

  function makeAgent(overrides: {
    isActive?: boolean;
    kind?: 'office-hours';
    filePath?: SessionModeFilePath;
    content?: string;
    history?: { role: string }[];
  } = {}): Agent {
    return {
      sessionMode: {
        isActive: overrides.isActive ?? false,
        kind: overrides.kind ?? 'office-hours',
        sessionModeFilePath: overrides.filePath ?? null,
        data: vi.fn().mockResolvedValue(overrides.content ? { content: overrides.content } : null),
      },
      context: {
        history: overrides.history ?? [],
        appendSystemReminder: vi.fn(),
      },
    } as unknown as Agent;
  }

  describe('OfficeHoursInjector', () => {
    it('injects entry reminder when office-hours becomes active with no content', async () => {
      const agent = makeAgent({ isActive: true, kind: 'office-hours', filePath: '/oh.md' });
      const injector = new OfficeHoursInjector(agent);
      await injector.inject();
      const reminder = agent.context.appendSystemReminder.mock.calls[0][0];
      expect(reminder).toContain('Office hours is now active');
    });

    it('injects reentry reminder when office-hours becomes active with existing content', async () => {
      const agent = makeAgent({ isActive: true, kind: 'office-hours', filePath: '/oh.md', content: '# Product' });
      const injector = new OfficeHoursInjector(agent);
      await injector.inject();
      const reminder = agent.context.appendSystemReminder.mock.calls[0][0];
      expect(reminder).toContain('Office hours is active');
      expect(reminder).not.toContain('Office hours is now active');
    });

    it('injects exit reminder when office-hours ends', async () => {
      const agent = makeAgent({ isActive: false, filePath: '/oh.md' });
      const injector = new OfficeHoursInjector(agent);
      injector.onContextClear();
      await injector.inject();
      const reminder = agent.context.appendSystemReminder.mock.calls[0][0];
      expect(reminder).toContain('Office hours has ended');
    });
  });
  ```

  注意：若 `officeHoursReentryReminder` / `officeHoursExitReminder` 的实际文本与断言不同，执行者应根据 `office-hours-contract.ts` 中的实际文本调整断言关键词（如 `'re-entering'` / `'ended'`）。

- [ ] **Run it and verify it FAILS**

  ```bash
  pnpm test packages/agent-core/src/agent/injection/__tests__/office-hours-injector.test.ts
  ```

  预期失败：`OfficeHoursInjector` 尚未继承基类。

- [ ] **Write the minimal implementation**

  将 `packages/agent-core/src/agent/injection/office-hours.ts` 完整替换为：

  ```ts
  import type { SessionModeFilePath } from '../session-mode';
  import { BaseSessionModeInjector } from './session-mode-injector';
  import {
    officeHoursEntryReminder,
    officeHoursExitReminder,
    officeHoursFullReminder,
    officeHoursReentryReminder,
    officeHoursSparseReminder,
  } from './office-hours-contract';

  const OFFICE_HOURS_DEDUP_MIN_TURNS = 2;
  const OFFICE_HOURS_FULL_REFRESH_TURNS = 5;

  export class OfficeHoursInjector extends BaseSessionModeInjector {
    protected override readonly injectionVariant = 'office_hours';
    protected override readonly options = {
      fullRefreshTurns: OFFICE_HOURS_FULL_REFRESH_TURNS,
      dedupMinTurns: OFFICE_HOURS_DEDUP_MIN_TURNS,
    };

    isModeActive(): boolean {
      return this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'office-hours';
    }

    protected getEntryReminder(path: SessionModeFilePath): string {
      return officeHoursEntryReminder(path);
    }

    protected getReentryReminder(path: SessionModeFilePath): string {
      return officeHoursReentryReminder(path);
    }

    protected getFullReminder(path: SessionModeFilePath): string {
      return officeHoursFullReminder(path);
    }

    protected getSparseReminder(path: SessionModeFilePath): string {
      return officeHoursSparseReminder(path);
    }

    protected getExitReminder(path: SessionModeFilePath): string {
      return officeHoursExitReminder(path);
    }
  }
  ```

- [ ] **Run it and verify it PASSES**

  ```bash
  pnpm test packages/agent-core/src/agent/injection/__tests__/office-hours-injector.test.ts
  ```

  预期：所有测试通过（如断言文本不匹配则按实际 contract 文本调整）。

- [ ] **Commit**

  ```bash
  git add packages/agent-core/src/agent/injection/office-hours.ts packages/agent-core/src/agent/injection/__tests__/office-hours-injector.test.ts
  git commit -m "refactor(agent-core): OfficeHoursInjector extends BaseSessionModeInjector"
  ```

## Task 4.5：重构 `GameDesignInjector` 继承基类

**Depends on:** Task 4.1, Task 4.4

**Files:**
- Modify: `packages/agent-core/src/agent/injection/game-design.ts`
- Test: `packages/agent-core/src/agent/injection/__tests__/game-design-injector.test.ts`

### 步骤

- [ ] **Write the failing test**

  新建 `packages/agent-core/src/agent/injection/__tests__/game-design-injector.test.ts`：

  ```ts
  import { describe, it, expect, vi } from 'vitest';
  import type { Agent } from '../../..';
  import type { SessionModeFilePath } from '../../session-mode';
  import { GameDesignInjector } from '../game-design';

  function makeAgent(overrides: {
    isActive?: boolean;
    kind?: 'game-design';
    filePath?: SessionModeFilePath;
    content?: string;
    history?: { role: string }[];
  } = {}): Agent {
    return {
      sessionMode: {
        isActive: overrides.isActive ?? false,
        kind: overrides.kind ?? 'game-design',
        sessionModeFilePath: overrides.filePath ?? null,
        data: vi.fn().mockResolvedValue(overrides.content ? { content: overrides.content } : null),
      },
      context: {
        history: overrides.history ?? [],
        appendSystemReminder: vi.fn(),
      },
    } as unknown as Agent;
  }

  describe('GameDesignInjector', () => {
    it('injects entry reminder when game-design becomes active with no content', async () => {
      const agent = makeAgent({ isActive: true, kind: 'game-design', filePath: '/gd.md' });
      const injector = new GameDesignInjector(agent);
      await injector.inject();
      const reminder = agent.context.appendSystemReminder.mock.calls[0][0];
      expect(reminder).toContain('game-design mode is now active');
    });

    it('injects reentry reminder when game-design becomes active with existing content', async () => {
      const agent = makeAgent({ isActive: true, kind: 'game-design', filePath: '/gd.md', content: '# Game' });
      const injector = new GameDesignInjector(agent);
      await injector.inject();
      const reminder = agent.context.appendSystemReminder.mock.calls[0][0];
      expect(reminder).toContain('game-design mode is active');
      expect(reminder).not.toContain('game-design mode is now active');
    });

    it('injects exit reminder when game-design ends', async () => {
      const agent = makeAgent({ isActive: false, filePath: '/gd.md' });
      const injector = new GameDesignInjector(agent);
      injector.onContextClear();
      await injector.inject();
      const reminder = agent.context.appendSystemReminder.mock.calls[0][0];
      expect(reminder).toContain('game-design mode has ended');
    });
  });
  ```

  注意：若 `gameDesignReentryReminder` / `gameDesignExitReminder` 的实际文本与断言不同，执行者应根据 `game-design-contract.ts` 中的实际文本调整断言关键词。

- [ ] **Run it and verify it FAILS**

  ```bash
  pnpm test packages/agent-core/src/agent/injection/__tests__/game-design-injector.test.ts
  ```

  预期失败：`GameDesignInjector` 尚未继承基类。

- [ ] **Write the minimal implementation**

  将 `packages/agent-core/src/agent/injection/game-design.ts` 完整替换为：

  ```ts
  import type { SessionModeFilePath } from '../session-mode';
  import { BaseSessionModeInjector } from './session-mode-injector';
  import {
    gameDesignEntryReminder,
    gameDesignExitReminder,
    gameDesignFullReminder,
    gameDesignReentryReminder,
    gameDesignSparseReminder,
  } from './game-design-contract';

  const GAME_DESIGN_DEDUP_MIN_TURNS = 2;
  const GAME_DESIGN_FULL_REFRESH_TURNS = 5;

  export class GameDesignInjector extends BaseSessionModeInjector {
    protected override readonly injectionVariant = 'game_design';
    protected override readonly options = {
      fullRefreshTurns: GAME_DESIGN_FULL_REFRESH_TURNS,
      dedupMinTurns: GAME_DESIGN_DEDUP_MIN_TURNS,
    };

    isModeActive(): boolean {
      return this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'game-design';
    }

    protected getEntryReminder(path: SessionModeFilePath): string {
      return gameDesignEntryReminder(path);
    }

    protected getReentryReminder(path: SessionModeFilePath): string {
      return gameDesignReentryReminder(path);
    }

    protected getFullReminder(path: SessionModeFilePath): string {
      return gameDesignFullReminder(path);
    }

    protected getSparseReminder(path: SessionModeFilePath): string {
      return gameDesignSparseReminder(path);
    }

    protected getExitReminder(path: SessionModeFilePath): string {
      return gameDesignExitReminder(path);
    }
  }
  ```

- [ ] **Run it and verify it PASSES**

  ```bash
  pnpm test packages/agent-core/src/agent/injection/__tests__/game-design-injector.test.ts
  ```

  预期：所有测试通过（如断言文本不匹配则按实际 contract 文本调整）。

- [ ] **Commit**

  ```bash
  git add packages/agent-core/src/agent/injection/game-design.ts packages/agent-core/src/agent/injection/__tests__/game-design-injector.test.ts
  git commit -m "refactor(agent-core): GameDesignInjector extends BaseSessionModeInjector"
  ```

## Task 4.6：更新 `InjectionManager` 并做全仓库类型检查

**Depends on:** Task 4.2, Task 4.3, Task 4.4, Task 4.5

**Files:**
- Modify: `packages/agent-core/src/agent/injection/manager.ts`（仅类型注释/导入，使模式注入器列表语义更清晰）

### 步骤

- [ ] **确认无需修改共享签名**

  四个 mode injector 仍继承 `DynamicInjector`（通过 `BaseSessionModeInjector`），因此 `InjectionManager` 的 `DynamicInjector[]` 数组类型与生命周期调用（`inject()`、`onContextClear()`、`onContextCompacted()`、`onContextMessageRemoved()`）无需改动。但为增强可读性，可导入 `BaseSessionModeInjector` 并在注释中标注 mode injectors：

  ```ts
  import type { DynamicInjector } from './injector';
  import { BaseSessionModeInjector } from './session-mode-injector';
  // ...
  ```

  实际代码改动：无（若保留原样）。本任务的核心是验证重构后的注入器类与 behavior 注册表兼容。

- [ ] **检查 behavior 注册表的 injectorClass 兼容性**

  搜索所有 `injectorClass` 引用：

  ```bash
  rg -n "injectorClass" packages/agent-core/src/agent/session-mode/behaviors/
  ```

  预期输出显示 `PlanModeInjector` / `DesignModeInjector` / `OfficeHoursInjector` / `GameDesignInjector` 四个赋值，且这些类均继承 `BaseSessionModeInjector`，结构兼容 `SessionModeInjector` 接口。

- [ ] **Run whole-tree typecheck**

  ```bash
  pnpm -r typecheck
  ```

  预期：全仓库类型检查通过。

- [ ] **Run all injection tests**

  ```bash
  pnpm test packages/agent-core/src/agent/injection/__tests__
  ```

  预期：所有新增与既有 injection 测试通过。

- [ ] **Commit**

  ```bash
  git add packages/agent-core/src/agent/injection/manager.ts
  git commit -m "chore(agent-core): verify mode injectors with BaseSessionModeInjector"
  ```

  若 `manager.ts` 无改动，则此 commit 不成立；此时应在 Task 4.5 的 commit 之后直接运行全仓库 typecheck，并将本任务的“commit”步骤替换为：

  ```bash
  pnpm -r typecheck
  # 不生成新 commit；类型检查通过即完成本 part
  ```

## Local Self-Review

- [ ] 1. Spec-coverage：覆盖 Scope In #5（`SessionModeInjector` 抽象基类、变体调度、生命周期钩子）与 Scope Out #3（不改 contract 文本内容）。
- [ ] 2. Placeholder scan：无 TODO；每个 injector 的 reminder 文本直接调用既有 contract 函数，无占位符。
- [ ] 3. No phantom tasks：Task 4.6 的验证步骤是真实必须；若 `manager.ts` 无改动，则显式以 typecheck 作为产出，不生成空 commit。
- [ ] 4. Dependency soundness：Task 4.1 → Task 4.2/4.3/4.4/4.5 → Task 4.6；无向后引用。
- [ ] 5. Caller & build soundness：注入器类的公共 API（`inject()`、`onContextClear()` 等）保持不变，`InjectionManager` 无需更新；Task 4.6 以 `pnpm -r typecheck` 覆盖全仓库。
- [ ] 6. Test-the-risk：
  - `computeVariant` 边界：injectedAt=null → full；1 assistant → null；dedup 阈值 → sparse；refresh 阈值 → full；user 打断 → full。
  - `onContextClear` 状态记忆：通过二次 `inject` 验证 `wasActive`。
  - plan/design handoff 与 skills reminder 行为保留。
  - design 的 `mockupAvailable` 通过 `isToolActive('ShowDesignMockup')` 保留。
- [ ] 7. Type consistency：
  - `BaseSessionModeInjector` 的 `options` 类型为 `SessionModeInjectorOptions`（来自 Task 3.1）。
  - `SessionModeFilePath` 类型来自 `session-mode/index.ts`。
  - 四个 mode injector 均实现 `isModeActive()` 返回布尔值， reminder 方法签名一致。
