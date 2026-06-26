# Part 3 — SessionMode 引擎、OfficeHoursInjector 与工具注册

**Phase:** C — 依赖 Task 1（类型扩展），可与 Phase B 并行。

## Task 5: SessionMode.enter/exit('office-hours') + 目录解析

**Depends on:** Task 1

**Files:**
- Modify: `packages/agent-core/src/agent/session-mode/index.ts:593-606`
- Test: `packages/agent-core/test/agent/session-mode-office-hours.test.ts`（新建）

### Steps

- [ ] 创建测试文件 `packages/agent-core/test/agent/session-mode-office-hours.test.ts`：
  ```typescript
  import { describe, expect, it, vi } from 'vitest';
  import { join } from 'pathe';
  import { SessionMode } from '#/agent/session-mode';
  import type { Agent } from '#/agent';

  function mockAgent(overrides: Partial<Agent> = {}): Agent {
    const kaos = {
      mkdir: vi.fn().mockResolvedValue(undefined),
      readText: vi.fn().mockRejectedValue({ code: 'ENOENT' }),
      writeText: vi.fn().mockResolvedValue(undefined),
      stat: vi.fn().mockRejectedValue({ code: 'ENOENT' }),
    };
    const config = {
      cwd: '/fake/project',
      modelAlias: 'default',
      update: vi.fn(),
    };
    const records = {
      logRecord: vi.fn(),
    };
    return {
      kaos,
      config,
      records,
      homedir: '/fake/home/.ody-code/sessions/s1',
      kimiConfig: undefined,
      modelProvider: undefined,
      log: undefined,
      replayBuilder: { push: vi.fn() },
      emitStatusUpdated: vi.fn(),
      setContextMode: vi.fn(),
      ...overrides,
    } as unknown as Agent;
  }

  describe('SessionMode office-hours', () => {
    it('enter sets kind to office-hours', async () => {
      const agent = mockAgent();
      const mode = new SessionMode(agent);
      await mode.enter('id-1', false, false, 'office-hours');
      expect(mode.kind).toBe('office-hours');
      expect(mode.isActive).toBe(true);
    });

    it('exit clears active state', async () => {
      const agent = mockAgent();
      const mode = new SessionMode(agent);
      await mode.enter('id-1', false, false, 'office-hours');
      mode.exit();
      expect(mode.isActive).toBe(false);
    });

    it('resolveSessionModeDirectory uses office-hours subdirectory', async () => {
      const mkdirSpy = vi.fn().mockResolvedValue(undefined);
      const agent = mockAgent({ kaos: { ...mockAgent().kaos, mkdir: mkdirSpy } });
      const mode = new SessionMode(agent);
      await mode.enter('id-1', false, false, 'office-hours');
      const calls = mkdirSpy.mock.calls;
      const officeHoursCall = calls.find((c: unknown[]) =>
        typeof c[0] === 'string' && c[0].includes('office-hours'),
      );
      expect(officeHoursCall).toBeDefined();
      expect(officeHoursCall![0]).toContain(join('.ody-code', 'office-hours'));
    });
  });
  ```

- [ ] 运行测试验证 FAIL：
  ```bash
  pnpm -F @odysseythink/agent-core test -- test/agent/session-mode-office-hours
  ```
  **预期：** 测试失败 — 模块可能还需要 mock 更多 deps，或 `mkdir` 调用不匹配。

- [ ] 修改 `packages/agent-core/src/agent/session-mode/index.ts:593-606` 的 `resolveSessionModeDirectory`：
  ```typescript
  private async resolveSessionModeDirectory(kind: SessionModeKind): Promise<{ dir: string; isProjectScoped: boolean }> {
    const subdir = kind === 'office-hours' ? 'office-hours' : kind === 'design' ? 'designs' : 'plans';
    const projectDir = join(this.agent.config.cwd, '.ody-code', subdir);
    try {
      await this.agent.kaos.mkdir(projectDir, { parents: true, existOk: true });
      return { dir: projectDir, isProjectScoped: true };
    } catch (error) {
      if (isPermissionError(error) && this.agent.homedir !== undefined) {
        const sessionDir = join(this.agent.homedir, subdir);
        await this.agent.kaos.mkdir(sessionDir, { parents: true, existOk: true });
        return { dir: sessionDir, isProjectScoped: false };
      }
      throw error;
    }
  }
  ```

- [ ] 运行测试验证 PASS：
  ```bash
  pnpm -F @odysseythink/agent-core test -- test/agent/session-mode-office-hours
  ```
  **预期：** 测试通过。

- [ ] 运行 `pnpm -F @odysseythink/agent-core typecheck` 确认编译通过。

- [ ] Commit: `feat: add office-hours directory resolution in SessionMode`

---

## Task 6: OfficeHoursInjector

**Depends on:** Task 5, Task 8（office-hours-contract.ts 中的 entry/full/sparse/exit 函数需要先存在）

> **注：** 本任务依赖 Task 8（contract prompt fragments）中定义的 `officeHoursEntryReminder`、`officeHoursFullReminder`、`officeHoursSparseReminder`、`officeHoursReentryReminder`、`officeHoursExitReminder` 函数。Task 8 属于 Phase D，与 Phase C 部分顺序可调整。为保持依赖清晰，contract 文件中的函数签名在本任务中声明为导入目标，实现代码在 Task 8 中完成。或者，可在本任务中先定义这些函数的最小桩实现（返回占位字符串），Task 8 再替换为完整内容。

**Files:**
- Create: `packages/agent-core/src/agent/injection/office-hours.ts`
- Modify: `packages/agent-core/src/agent/injection/manager.ts:20-30`

### Steps

- [ ] 创建 `packages/agent-core/src/agent/injection/office-hours.ts`：
  ```typescript
  import type { SessionModeFilePath } from '../session-mode';
  import { DynamicInjector } from './injector';
  import {
    officeHoursEntryReminder,
    officeHoursExitReminder,
    officeHoursFullReminder,
    officeHoursReentryReminder,
    officeHoursSparseReminder,
  } from './office-hours-contract';

  const OFFICE_HOURS_DEDUP_MIN_TURNS = 2;
  const OFFICE_HOURS_FULL_REFRESH_TURNS = 5;

  export class OfficeHoursInjector extends DynamicInjector {
    protected override readonly injectionVariant = 'office_hours';
    private wasActive = false;

    override onContextClear(): void {
      super.onContextClear();
      this.wasActive =
        this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'office-hours';
    }

    override async getInjection(): Promise<string | undefined> {
      const isActive =
        this.agent.sessionMode.isActive && this.agent.sessionMode.kind === 'office-hours';
      const { sessionModeFilePath } = this.agent.sessionMode;

      if (!isActive) {
        if (!this.wasActive) return undefined;
        this.wasActive = false;
        this.injectedAt = null;
        return officeHoursExitReminder(sessionModeFilePath);
      }

      if (!this.wasActive) {
        this.injectedAt = null;
        this.wasActive = true;
        return officeHoursEntryReminder(sessionModeFilePath);
      }

      const variant = this.getVariant();
      if (variant === null) return undefined;
      if (variant === 'reentry') return officeHoursReentryReminder(sessionModeFilePath);
      return variant === 'full'
        ? officeHoursFullReminder(sessionModeFilePath)
        : officeHoursSparseReminder(sessionModeFilePath);
    }

    protected getVariant(): 'full' | 'sparse' | 'reentry' | null {
      if (this.injectedAt === null) return 'full';
      const history = this.agent.context.history;
      let assistantTurnsSince = 0;
      for (let i = this.injectedAt + 1; i < history.length; i++) {
        const msg = history[i];
        if (msg === undefined) continue;
        if (msg.role === 'assistant') {
          assistantTurnsSince += 1;
          continue;
        }
        if (msg.role === 'user') return 'full';
      }
      if (assistantTurnsSince >= OFFICE_HOURS_FULL_REFRESH_TURNS) return 'full';
      if (assistantTurnsSince >= OFFICE_HOURS_DEDUP_MIN_TURNS) return 'sparse';
      return null;
    }
  }
  ```

- [ ] 修改 `packages/agent-core/src/agent/injection/manager.ts:20-30`，在 `DesignModeInjector` 之后注册 `OfficeHoursInjector`：
  ```typescript
  import { OfficeHoursInjector } from './office-hours';

  // ... in constructor:
  this.injectors = [
    new PluginSessionStartInjector(agent),
    new TodoListReminderInjector(agent),
    new PlanModeInjector(agent),
    new DesignModeInjector(agent),
    new OfficeHoursInjector(agent),
    new PermissionModeInjector(agent),
  ];
  ```

- [ ] 先创建 contract 函数的桩实现（在 `packages/agent-core/src/agent/injection/office-hours-contract.ts`）以确保编译通过：
  ```typescript
  import type { SessionModeFilePath } from '../session-mode';

  export function officeHoursEntryReminder(path: SessionModeFilePath): string {
    return `Office hours mode is active. Design file: ${path ?? '(not yet assigned)'}.`;
  }

  export function officeHoursFullReminder(path: SessionModeFilePath): string {
    return `[FULL] Office hours workflow. Design file: ${path ?? '(not yet assigned)'}.`;
  }

  export function officeHoursSparseReminder(path: SessionModeFilePath): string {
    return `[SPARSE] Continue office hours workflow.`;
  }

  export function officeHoursReentryReminder(path: SessionModeFilePath): string {
    return `[REENTRY] Resume office hours workflow.`;
  }

  export function officeHoursExitReminder(path: SessionModeFilePath | null): string {
    return `Office hours mode has ended.`;
  }
  ```

  > 注：Task 8 将把这些桩替换为完整的 YC Office Hours Phase 1-6 prompt。

- [ ] 运行 `pnpm -F @odysseythink/agent-core typecheck` 确认编译通过。

- [ ] 运行 injector 测试（可在 Task 8 后补充完整）：
  ```bash
  pnpm -F @odysseythink/agent-core test
  ```
  **预期：** 现有测试全部通过，无回归。

- [ ] Commit: `feat: add OfficeHoursInjector with variant cadence`

---

## Task 7: OfficeHours Entry/Exit Tools + ToolManager 注册 + Permission Guard

**Depends on:** Task 5

**Files:**
- Create: `packages/agent-core/src/tools/builtin/office-hours/enter-office-hours.ts`
- Create: `packages/agent-core/src/tools/builtin/office-hours/exit-office-hours.ts`
- Create: `packages/agent-core/src/tools/builtin/office-hours/enter-office-hours.md`
- Create: `packages/agent-core/src/tools/builtin/office-hours/exit-office-hours.md`
- Modify: `packages/agent-core/src/tools/builtin/index.ts:29-30`（追加 export）
- Modify: `packages/agent-core/src/agent/tool/index.ts:419-465`（注册 tools）
- Modify: `packages/agent-core/src/agent/permission/policies/plan-mode-guard-deny.ts:1-82`
- Test: `packages/agent-core/test/agent/tool/enter-office-hours.test.ts`（新建）

### Steps

- [ ] 创建测试文件 `packages/agent-core/test/agent/tool/enter-office-hours.test.ts`：
  ```typescript
  import { describe, expect, it, vi } from 'vitest';
  import { EnterOfficeHoursModeTool } from '#/tools/builtin/office-hours/enter-office-hours';
  import { ExitOfficeHoursModeTool } from '#/tools/builtin/office-hours/exit-office-hours';
  import type { Agent } from '#/agent';
  import { SessionMode } from '#/agent/session-mode';

  function mockAgent(sessionMode?: SessionMode): Agent {
    const kaos = {
      mkdir: vi.fn().mockResolvedValue(undefined),
      readText: vi.fn().mockRejectedValue({ code: 'ENOENT' }),
      writeText: vi.fn().mockResolvedValue(undefined),
      stat: vi.fn().mockRejectedValue({ code: 'ENOENT' }),
    };
    const config = {
      cwd: '/fake/project',
      modelAlias: 'default',
      update: vi.fn(),
    };
    const records = { logRecord: vi.fn() };
    const mode = sessionMode ?? new SessionMode({
      kaos, config, records, homedir: '/x',
      kimiConfig: undefined, modelProvider: undefined,
      replayBuilder: { push: vi.fn() },
      emitStatusUpdated: vi.fn(),
      setContextMode: vi.fn(),
      log: undefined, rpc: undefined,
      telemetry: { track: vi.fn() },
    } as unknown as Agent);
    return {
      kaos, config, records, sessionMode: mode, homedir: '/x',
      kimiConfig: undefined, modelProvider: undefined,
      replayBuilder: { push: vi.fn() },
      emitStatusUpdated: vi.fn(),
      setContextMode: vi.fn(),
      log: undefined, rpc: undefined,
      telemetry: { track: vi.fn() },
    } as unknown as Agent;
  }

  describe('EnterOfficeHoursModeTool', () => {
    it('enters office-hours mode when not active', async () => {
      const agent = mockAgent();
      const tool = new EnterOfficeHoursModeTool(agent);
      const result = tool.resolveExecution({});
      const output = await result.execute!({} as any);
      expect(output.isError).toBeFalsy();
      expect(agent.sessionMode.kind).toBe('office-hours');
    });

    it('returns isError when office-hours is already active', async () => {
      const agent = mockAgent();
      await agent.sessionMode.enter('id', false, false, 'office-hours');
      const tool = new EnterOfficeHoursModeTool(agent);
      const result = tool.resolveExecution({});
      const output = await result.execute!({} as any);
      expect(output.isError).toBe(true);
    });
  });

  describe('ExitOfficeHoursModeTool', () => {
    it('exits office-hours mode and returns completion message', async () => {
      const agent = mockAgent();
      await agent.sessionMode.enter('id', false, false, 'office-hours');
      const tool = new ExitOfficeHoursModeTool(agent);
      const result = tool.resolveExecution({ approved: true });
      const output = await result.execute!({} as any);
      expect(output.isError).toBeFalsy();
      expect(agent.sessionMode.isActive).toBe(false);
    });

    it('returns isError when office-hours is not active', async () => {
      const agent = mockAgent();
      const tool = new ExitOfficeHoursModeTool(agent);
      const result = tool.resolveExecution({ approved: true });
      const output = await result.execute!({} as any);
      expect(output.isError).toBe(true);
    });
  });
  ```

- [ ] 运行测试验证 FAIL（文件不存在）：
  ```bash
  pnpm -F @odysseythink/agent-core test -- test/agent/tool/enter-office-hours
  ```
  **预期：** 模块未找到错误。

- [ ] 创建 `packages/agent-core/src/tools/builtin/office-hours/enter-office-hours.md`（tool description）：
  ```markdown
  Use this tool when the user explicitly asks to start office hours mode. Office hours mode provides structured YC-style startup/builder diagnostic workflow. It should only be used as the very first action in a session — once active, it locks the session into the diagnostic flow and exits after producing a design document.
  ```

- [ ] 创建 `packages/agent-core/src/tools/builtin/office-hours/enter-office-hours.ts`：
  ```typescript
  import type { Agent } from '#/agent';
  import { z } from 'zod';

  import { officeHoursEntryReminder } from '../../../agent/injection/office-hours-contract';
  import type { BuiltinTool } from '../../../agent/tool';
  import type { ToolExecution } from '../../../loop/types';
  import { toInputJsonSchema } from '../../support/input-schema';
  import DESCRIPTION from './enter-office-hours.md';

  export const EnterOfficeHoursModeInputSchema = z.object({}).strict();
  export type EnterOfficeHoursModeInput = z.infer<typeof EnterOfficeHoursModeInputSchema>;

  export class EnterOfficeHoursModeTool implements BuiltinTool<EnterOfficeHoursModeInput> {
    readonly name = 'EnterOfficeHoursMode' as const;
    readonly description: string = DESCRIPTION;
    readonly parameters: Record<string, unknown> = toInputJsonSchema(EnterOfficeHoursModeInputSchema);

    constructor(private readonly agent: Agent) {}

    resolveExecution(_args: EnterOfficeHoursModeInput): ToolExecution {
      return {
        description: 'Requesting to enter office hours mode',
        approvalRule: this.name,
        execute: async () => {
          if (this.agent.sessionMode.isActive) {
            if (this.agent.sessionMode.kind === 'office-hours') {
              return {
                isError: true,
                output: 'Office hours mode is already active. Use ExitOfficeHoursMode when the session is complete.',
              };
            }
            return {
              isError: true,
              output: 'Another session mode is already active. Exit it first before entering office hours mode.',
            };
          }

          try {
            await this.agent.sessionMode.enter(undefined, undefined, undefined, 'office-hours');
          } catch (error) {
            const message = error instanceof Error ? error.message : 'Failed to enter office hours mode.';
            return { isError: true, output: `Failed to enter office hours mode: ${message}` };
          }

          return {
            output: officeHoursEntryReminder(this.agent.sessionMode.sessionModeFilePath),
          };
        },
      };
    }
  }
  ```

- [ ] 创建 `packages/agent-core/src/tools/builtin/office-hours/exit-office-hours.md`（tool description）：
  ```markdown
  Exit office hours mode after the design document has been approved and written. This ends the office hours session, flushes telemetry and profile data, and shuts down the application.
  ```

- [ ] 创建 `packages/agent-core/src/tools/builtin/office-hours/exit-office-hours.ts`：
  ```typescript
  import type { Agent } from '#/agent';
  import { z } from 'zod';

  import type { BuiltinTool } from '../../../agent/tool';
  import type { ToolExecution } from '../../../loop/types';
  import { toInputJsonSchema } from '../../support/input-schema';
  import DESCRIPTION from './exit-office-hours.md';

  export const ExitOfficeHoursModeInputSchema = z.object({
    approved: z.boolean().describe('Whether the design document has been approved.'),
  }).strict();
  export type ExitOfficeHoursModeInput = z.infer<typeof ExitOfficeHoursModeInputSchema>;

  export class ExitOfficeHoursModeTool implements BuiltinTool<ExitOfficeHoursModeInput> {
    readonly name = 'ExitOfficeHoursMode' as const;
    readonly description: string = DESCRIPTION;
    readonly parameters: Record<string, unknown> = toInputJsonSchema(ExitOfficeHoursModeInputSchema);

    constructor(private readonly agent: Agent) {}

    resolveExecution(args: ExitOfficeHoursModeInput): ToolExecution {
      return {
        description: 'Requesting to exit office hours mode',
        approvalRule: this.name,
        execute: async () => {
          if (!this.agent.sessionMode.isActive || this.agent.sessionMode.kind !== 'office-hours') {
            return {
              isError: true,
              output: 'Office hours mode is not active.',
            };
          }

          const path = this.agent.sessionMode.sessionModeFilePath;
          this.agent.sessionMode.exit();

          return {
            output: [
              'Office hours session complete.',
              path ? `Design document saved to: ${path}` : '',
              'The application will now exit.',
            ].filter(Boolean).join('\n'),
          };
        },
      };
    }
  }
  ```

- [ ] 在 `packages/agent-core/src/tools/builtin/index.ts` 末尾追加：
  ```typescript
  export * from './office-hours/enter-office-hours';
  export * from './office-hours/exit-office-hours';
  ```

- [ ] 在 `packages/agent-core/src/agent/tool/index.ts:419-465` 的 `builtinTools` Map 中注册 office-hours tools（始终注册，内部检查 mode）：
  ```typescript
  // Insert after the planning tools (line 422):
  new b.EnterOfficeHoursModeTool(this.agent),
  new b.ExitOfficeHoursModeTool(this.agent),
  ```

- [ ] 修改 `packages/agent-core/src/agent/permission/policies/plan-mode-guard-deny.ts` 扩展 office-hours 模式处理：

  当前 `evaluate` 方法（line 12-52）只区分 `isDesign` vs plan。需要扩展为支持 office-hours：

  ```typescript
  evaluate(context: PermissionPolicyContext): PermissionPolicyResult | undefined {
    if (!this.agent.sessionMode.isActive) return;

    const kind = this.agent.sessionMode.kind;
    const isOfficeHours = kind === 'office-hours';
    const isDesign = kind === 'design';
    const modeLabel = isOfficeHours ? 'office-hours' : isDesign ? 'design' : 'plan';
    const exitTool = isOfficeHours
      ? 'ExitOfficeHoursMode'
      : isDesign
        ? 'ExitDesignMode'
        : 'ExitPlanMode';
    const toolName = context.toolCall.name;

    if (toolName === 'Write' || toolName === 'Edit') {
      const sessionModeFilePath = this.agent.sessionMode.sessionModeFilePath;
      if (sessionModeFilePath === null) {
        return {
          kind: 'deny',
          message: modeWriteDeniedMessage(modeLabel, sessionModeFilePath),
        };
      }
      if (writesOnlyPlanFileset(context, this.agent)) {
        return;
      }
      return {
        kind: 'deny',
        message: modeWriteDeniedMessage(modeLabel, sessionModeFilePath),
      };
    }

    if (toolName === 'TaskStop') {
      return {
        kind: 'deny',
        message: `TaskStop is not available in ${modeLabel} mode. Call ${exitTool} to exit ${modeLabel} mode before stopping a background task.`,
      };
    }

    if (toolName === 'CronCreate' || toolName === 'CronDelete') {
      return {
        kind: 'deny',
        message: `${toolName} is not available in ${modeLabel} mode because it would mutate scheduled work that runs after ${modeLabel} exit. Call ${exitTool} first.`,
      };
    }

    return;
  }
  ```

- [ ] 运行测试验证 PASS：
  ```bash
  pnpm -F @odysseythink/agent-core test -- test/agent/tool/enter-office-hours
  ```
  **预期：** 测试通过。

- [ ] 运行 `pnpm -F @odysseythink/agent-core typecheck` 确认编译通过。

- [ ] Commit: `feat: add EnterOfficeHoursMode and ExitOfficeHoursMode tools with permission guard`

## Self-Review

- [ ] 1. Spec-coverage: Tasks 5-7 cover spec items 2 (Session Mode), 4 (design doc output path), 5 (app lifecycle — auto-exit).
- [ ] 2. Placeholder scan: no TODO/TBD. Contract stubs are explicitly noted as replaced by Task 8.
- [ ] 3. No phantom tasks: Task 5 produces directory resolution + tests; Task 6 produces injector; Task 7 produces 2 tools + permission guard + tests.
- [ ] 4. Dependency soundness: Task 5 dep on Task 1; Task 6 dep on Task 5 + Task 8 (contract stubs); Task 7 dep on Task 5. All satisfied.
- [ ] 5. Caller & build soundness: `OfficeHoursInjector` imports from `office-hours-contract.ts` (created as stubs in this phase). `EnterOfficeHoursModeTool` calls `sessionMode.enter('office-hours')` which is validated in Task 5. Permission guard's `isWritableSessionModePath` is already office-hours-aware since `SessionModeKind` includes `'office-hours'`. Tool registration is unconditional — no mode-based conditional. Ends with `pnpm -F @odysseythink/agent-core typecheck`.
- [ ] 6. Test-the-risk: Task 5 test verifies `kind === 'office-hours'` after enter and `isActive === false` after exit. Task 7 test verifies tool error on double-enter and exit-when-inactive.
- [ ] 7. Type consistency: `EnterOfficeHoursModeTool` and `ExitOfficeHoursModeTool` use `BuiltinTool<>` pattern consistent with existing `EnterDesignModeTool`.
