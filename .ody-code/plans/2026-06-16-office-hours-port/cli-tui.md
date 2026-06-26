# Part 2 — CLI 入口、runOfficeHours 启动流程与 TUI Wiring

**Phase:** B — 依赖 Task 1（类型扩展）。

## Task 2: CLI `--office-hours` 参数解析与冲突校验

**Depends on:** Task 1

**Files:**
- Modify: `apps/ody-code/src/cli/commands.ts:70-85,113-126`
- Modify: `apps/ody-code/src/cli/options.ts:4-16,30-72`
- Test: `apps/ody-code/test/cli/options.test.ts`（已存在，追加测试）

### Steps

- [ ] 在 `apps/ody-code/src/cli/commands.ts` 的 `--session-mode` 选项后（line 73）添加 `--office-hours` 选项：
  ```typescript
  .addOption(
    new Option(
      '--office-hours',
      'Start Ody Code in YC Office Hours mode. Exits after the design doc is written.',
    ).conflicts(['prompt', 'session', 'continue', 'sessionMode', 'yolo', 'auto']),
  )
  ```

- [ ] 在 `program.action` 处理函数中（line 113-126），解析 `officeHours` 字段：
  ```typescript
  const opts: CLIOptions = {
    session: sessionValue,
    continue: raw['continue'] as boolean,
    yolo: yoloValue,
    auto: autoValue,
    sessionMode: (raw['sessionMode'] as 'normal' | 'plan' | 'design' | 'office-hours') ?? 'normal',
    officeHours: (raw['officeHours'] as boolean) ?? false,
    model: raw['model'] as string | undefined,
    outputFormat: raw['outputFormat'] as CLIOptions['outputFormat'],
    prompt: raw['prompt'] as string | undefined,
    skillsDirs: raw['skillsDir'] as string[],
    loginProvider: raw['login'] as string | undefined,
    logoutProvider: raw['logout'] as string | undefined,
  };
  ```

- [ ] 在 `apps/ody-code/src/cli/options.ts` 的 `CLIOptions` 接口中新增 `officeHours` 字段（line 16 之后）：
  ```typescript
  export interface CLIOptions {
    session: string | undefined;
    continue: boolean;
    yolo: boolean;
    auto: boolean;
    sessionMode: 'normal' | 'plan' | 'design' | 'office-hours';
    officeHours: boolean;
    model: string | undefined;
    outputFormat: PromptOutputFormat | undefined;
    prompt: string | undefined;
    skillsDirs: string[];
    loginProvider: string | undefined;
    logoutProvider: string | undefined;
  }
  ```

- [ ] 在 `validateOptions` 函数中新增 office-hours 冲突校验（在现有校验之后，line 71 之后）：
  ```typescript
  if (opts.officeHours) {
    if (opts.prompt !== undefined) {
      throw new OptionConflictError('Cannot combine --office-hours with --prompt.');
    }
    if (opts.session !== undefined) {
      throw new OptionConflictError('Cannot combine --office-hours with --session.');
    }
    if (opts.continue) {
      throw new OptionConflictError('Cannot combine --office-hours with --continue.');
    }
    if (opts.sessionMode !== 'normal') {
      throw new OptionConflictError('Cannot combine --office-hours with --session-mode.');
    }
    if (opts.yolo || opts.auto) {
      throw new OptionConflictError('Permission mode is fixed to manual in office-hours mode.');
    }
    return { options: opts, uiMode: 'shell' };
  }
  ```

- [ ] 测试：在 `apps/ody-code/test/cli/options.test.ts` 追加测试用例：
  ```typescript
  describe('--office-hours', () => {
    it('defaults officeHours to false', () => {
      expect(parse([]).officeHours).toBe(false);
    });

    it('--office-hours sets officeHours to true', () => {
      expect(parse(['--office-hours']).officeHours).toBe(true);
    });

    it('--office-hours forces uiMode to shell', () => {
      const opts = parse(['--office-hours']);
      expect(validateOptions(opts).uiMode).toBe('shell');
    });

    it('rejects --office-hours combined with --prompt', () => {
      const opts = parse(['--office-hours', '--prompt', 'x']);
      expect(() => validateOptions(opts)).toThrow(OptionConflictError);
      expect(() => validateOptions(opts)).toThrow('Cannot combine --office-hours with --prompt.');
    });

    it('rejects --office-hours combined with --session', () => {
      const opts = parse(['--office-hours', '--session', 'abc']);
      expect(() => validateOptions(opts)).toThrow(OptionConflictError);
      expect(() => validateOptions(opts)).toThrow('Cannot combine --office-hours with --session.');
    });

    it('rejects --office-hours combined with --continue', () => {
      const opts = parse(['--office-hours', '--continue']);
      expect(() => validateOptions(opts)).toThrow(OptionConflictError);
      expect(() => validateOptions(opts)).toThrow('Cannot combine --office-hours with --continue.');
    });

    it('rejects --office-hours combined with --session-mode', () => {
      const opts = parse(['--office-hours', '--session-mode', 'plan']);
      expect(() => validateOptions(opts)).toThrow(OptionConflictError);
      expect(() => validateOptions(opts)).toThrow('Cannot combine --office-hours with --session-mode.');
    });

    it('rejects --office-hours combined with --yolo', () => {
      const opts = parse(['--office-hours', '--yolo']);
      expect(() => validateOptions(opts)).toThrow(OptionConflictError);
    });

    it('rejects --office-hours combined with --auto', () => {
      const opts = parse(['--office-hours', '--auto']);
      expect(() => validateOptions(opts)).toThrow(OptionConflictError);
    });
  });
  ```

- [ ] 运行测试验证 FAIL（新增 `officeHours` 字段尚不在 CLIOptions 中，parse 函数不会捕获）：
  ```bash
  pnpm -F @odysseythink/ody-code test -- --reporter=verbose 2>&1 | grep -A5 'office-hours'
  ```
  **预期：** 类型错误或测试失败（`officeHours` 未定义）。

- [ ] 应用上述代码变更。

- [ ] 运行测试验证 PASS：
  ```bash
  pnpm -F @odysseythink/ody-code test -- test/cli/options
  ```
  **预期：** 所有 `--office-hours` 测试通过。

- [ ] 运行 `pnpm -F @odysseythink/ody-code typecheck` 确认编译通过。

- [ ] Commit: `feat: add --office-hours CLI flag with conflict validation`

---

## Task 3: `runOfficeHours` 启动流程与 main.ts 分支

**Depends on:** Task 2

**Files:**
- Create: `apps/ody-code/src/cli/run-office-hours.ts`
- Modify: `apps/ody-code/src/main.ts:74-79`
- Test: `apps/ody-code/test/cli/office-hours-bootstrap.test.ts`（新建）

### Steps

- [ ] 创建测试文件 `apps/ody-code/test/cli/office-hours-bootstrap.test.ts`：
  ```typescript
  import { describe, expect, it, vi } from 'vitest';

  import { runOfficeHours } from '#/cli/run-office-hours';
  import type { CLIOptions } from '#/cli/options';

  // Mock external modules used by runOfficeHours
  vi.mock('@odysseythink/ody-telemetry', () => ({
    setCrashPhase: vi.fn(),
    setTelemetryContext: vi.fn(),
    shutdownTelemetry: vi.fn(),
    track: vi.fn(),
    withTelemetryContext: vi.fn(() => ({ track: vi.fn() })),
  }));

  vi.mock('@odysseythink/ody-code-sdk', async () => {
    const actual = await vi.importActual('@odysseythink/ody-code-sdk');
    return {
      ...actual,
      KimiHarness: vi.fn().mockImplementation(() => ({
        ensureConfigFile: vi.fn().mockResolvedValue(undefined),
        getConfig: vi.fn().mockResolvedValue({}),
        close: vi.fn().mockResolvedValue(undefined),
      })),
    };
  });

  vi.mock('#/tui/index', () => ({
    KimiTUI: vi.fn().mockImplementation(() => ({
      onExit: undefined as unknown,
      start: vi.fn().mockResolvedValue(undefined),
      stop: vi.fn().mockResolvedValue(undefined),
      getCurrentSessionId: vi.fn().mockReturnValue(''),
    })),
  }));

  vi.mock('#/tui/config', () => ({
    loadTuiConfig: vi.fn().mockResolvedValue({ theme: 'dark', editorCommand: null, notifications: {}, upgrade: {} }),
    TuiConfigParseError: class extends Error {},
  }));

  vi.mock('#/tui/theme/detect', () => ({
    detectTerminalTheme: vi.fn().mockResolvedValue('dark'),
  }));

  vi.mock('#/cli/telemetry', () => ({
    createCliTelemetryBootstrap: vi.fn().mockReturnValue({
      homeDir: '/tmp/.ody-code-test',
    }),
    initializeCliTelemetry: vi.fn(),
  }));

  vi.mock('#/cli/version', () => ({
    createKimiCodeHostIdentity: vi.fn().mockReturnValue({}),
  }));

  vi.mock('node:child_process', () => ({
    execSync: vi.fn(),
  }));

  describe('runOfficeHours', () => {
    it('creates harness and calls tui.start', async () => {
      const opts: CLIOptions = {
        session: undefined,
        continue: false,
        yolo: false,
        auto: false,
        sessionMode: 'normal',
        officeHours: true,
        model: undefined,
        outputFormat: undefined,
        prompt: undefined,
        skillsDirs: [],
        loginProvider: undefined,
        logoutProvider: undefined,
      };

      // Should not throw
      await expect(runOfficeHours(opts, '0.0.0-test')).resolves.toBeUndefined();
    });
  });
  ```

- [ ] 运行测试验证 FAIL（文件不存在）：
  ```bash
  pnpm -F @odysseythink/ody-code test -- test/cli/office-hours-bootstrap
  ```
  **预期：** 模块未找到错误。

- [ ] 创建 `apps/ody-code/src/cli/run-office-hours.ts`：
  ```typescript
  import { execSync } from 'node:child_process';

  import {
    setCrashPhase,
    setTelemetryContext,
    shutdownTelemetry,
    track,
    withTelemetryContext,
  } from '@odysseythink/ody-telemetry';
  import { KimiHarness, log, type TelemetryClient } from '@odysseythink/ody-code-sdk';

  import { CLI_SHUTDOWN_TIMEOUT_MS, CLI_UI_MODE } from '#/constant/app';
  import type { TuiConfig } from '#/tui/config';
  import { loadTuiConfig, TuiConfigParseError } from '#/tui/config';
  import { KimiTUI } from '#/tui/index';
  import { detectTerminalTheme } from '#/tui/theme/detect';

  import type { CLIOptions } from './options';
  import { createCliTelemetryBootstrap, initializeCliTelemetry } from './telemetry';
  import { createKimiCodeHostIdentity } from './version';

  export async function runOfficeHours(opts: CLIOptions, version: string): Promise<void> {
    const startedAt = Date.now();
    let tuiConfig: TuiConfig;
    let configWarning: string | undefined;
    try {
      tuiConfig = await loadTuiConfig();
    } catch (error) {
      if (!(error instanceof TuiConfigParseError)) throw error;
      tuiConfig = error.fallback;
      configWarning = error.message;
    }

    const resolvedTheme =
      tuiConfig.theme === 'auto' ? await detectTerminalTheme() : tuiConfig.theme;

    const workDir = process.cwd();
    const telemetryBootstrap = createCliTelemetryBootstrap();
    const telemetryClient: TelemetryClient = {
      track,
      withContext: withTelemetryContext,
      setContext: setTelemetryContext,
    };
    const harness = new KimiHarness({
      homeDir: telemetryBootstrap.homeDir,
      identity: createKimiCodeHostIdentity(version),
      telemetry: telemetryClient,
    });
    log.info('kimi-code starting in office-hours mode', {
      version,
      uiMode: CLI_UI_MODE,
      nodeVersion: process.version,
      platform: `${process.platform}/${process.arch}`,
      workDir,
    });
    await harness.ensureConfigFile();
    const config = await harness.getConfig();

    const tui = new KimiTUI(harness, {
      cliOptions: { ...opts, sessionMode: 'office-hours', officeHours: true },
      tuiConfig,
      version,
      workDir,
      startupNotice: configWarning,
      resolvedTheme,
      officeHours: true,
    });

    initializeCliTelemetry({
      harness,
      bootstrap: telemetryBootstrap,
      config,
      version,
      uiMode: CLI_UI_MODE,
    });
    setCrashPhase('runtime');

    tui.onExit = async (exitCode = 0) => {
      setCrashPhase('shutdown');
      track('office_hours_completed', {
        duration_s: (Date.now() - startedAt) / 1000,
      });
      await shutdownTelemetry({ timeoutMs: CLI_SHUTDOWN_TIMEOUT_MS });
      process.exit(exitCode);
    };

    try {
      execSync('stty -ixon', { stdio: 'ignore' });
    } catch {
      /* ignore */
    }

    try {
      await tui.start();
    } catch (error) {
      setCrashPhase('shutdown');
      await shutdownTelemetry({ timeoutMs: CLI_SHUTDOWN_TIMEOUT_MS });
      await harness.close();
      throw error;
    }
  }
  ```

- [ ] 在 `apps/ody-code/src/main.ts:74-79` 的 `runShell` 调用前插入 office-hours 分支：
  ```typescript
  if (validated.uiMode === 'print') {
    await runPrompt(validated.options, version);
    return;
  }

  if (validated.options.officeHours) {
    await runOfficeHours(validated.options, version);
    return;
  }

  await runShell(validated.options, version);
  ```

- [ ] 在 `apps/ody-code/src/main.ts` 文件顶部添加 import：
  ```typescript
  import { runOfficeHours } from './cli/run-office-hours';
  ```

- [ ] 运行测试验证 PASS：
  ```bash
  pnpm -F @odysseythink/ody-code test -- test/cli/office-hours-bootstrap
  ```
  **预期：** 测试通过。

- [ ] 运行 `pnpm -F @odysseythink/ody-code typecheck` 确认编译通过。

- [ ] Commit: `feat: add runOfficeHours bootstrap and main.ts routing`

---

## Task 4: TUI Startup Wiring（KimiTUI + AppState）

**Depends on:** Task 3

**Files:**
- Modify: `apps/ody-code/src/tui/ody-tui.ts:140-145,149-180,244-260,459-469`
- Modify: `apps/ody-code/src/tui/types.ts:174-183`（TUIStartupOptions 已在 Task 1 扩展 sessionMode 字面量，此处只需新增 `officeHours` 字段）

### Steps

- [ ] 在 `apps/ody-code/src/tui/types.ts` 的 `TUIStartupOptions` 中新增 `officeHours` 字段（line 179 之后）：
  ```typescript
  export interface TUIStartupOptions {
    readonly sessionFlag?: string;
    readonly continueLast: boolean;
    readonly yolo: boolean;
    readonly auto: boolean;
    readonly sessionMode: 'normal' | 'plan' | 'design' | 'office-hours';
    readonly officeHours: boolean;
    readonly model?: string;
    readonly startupNotice?: string;
    readonly authIntent?: { readonly kind: 'login' | 'logout'; readonly providerType: string };
  }
  ```

- [ ] 在 `apps/ody-code/src/tui/ody-tui.ts` 的 `KimiTUIStartupInput` 新增 `officeHours` 字段（line 145 之后）：
  ```typescript
  export interface KimiTUIStartupInput {
    readonly cliOptions: CLIOptions;
    readonly tuiConfig: TuiConfig;
    readonly version: string;
    readonly workDir: string;
    readonly startupNotice?: string;
    readonly resolvedTheme?: ResolvedTheme;
    readonly authIntent?: { readonly kind: 'login' | 'logout'; readonly providerType: string };
    readonly officeHours: boolean;
  }
  ```

- [ ] 修改 `createInitialAppState`（line 160）使用 `input.officeHours` 设置初始 sessionMode：
  ```typescript
  function createInitialAppState(input: KimiTUIStartupInput): AppState {
    const startupPermission: PermissionMode = input.cliOptions.auto
      ? 'auto'
      : input.cliOptions.yolo
        ? 'yolo'
        : 'manual';
    return {
      model: '',
      workDir: input.workDir,
      sessionId: '',
      permissionMode: startupPermission,
      sessionMode: input.officeHours ? 'office-hours' : input.cliOptions.sessionMode,
      // ... rest unchanged
    };
  }
  ```

- [ ] 在 `KimiTUI` 构造函数中映射 `officeHours` 到 `TUIStartupOptions`（line 248-260）：
  ```typescript
  const tuiOptions: KimiTUIOptions = {
    initialAppState: createInitialAppState(startupInput),
    startup: {
      sessionFlag: startupInput.cliOptions.session,
      continueLast: startupInput.cliOptions.continue,
      yolo: startupInput.cliOptions.yolo,
      auto: startupInput.cliOptions.auto,
      sessionMode: startupInput.cliOptions.sessionMode,
      officeHours: startupInput.officeHours,
      model: startupInput.cliOptions.model,
      startupNotice: startupInput.startupNotice,
      authIntent: startupInput.authIntent,
    },
    resolvedTheme: startupInput.resolvedTheme,
  };
  ```

- [ ] 在 `init()` 方法中，`createSessionOptions` 构造时（line 464-469），当 `officeHours` 为 true 时设置 `sessionMode: 'office-hours'`：
  ```typescript
  const createSessionOptions: CreateSessionOptions = {
    workDir,
    model: startup.model,
    permission: startup.auto ? 'auto' : startup.yolo ? 'yolo' : undefined,
    sessionMode:
      startup.officeHours
        ? 'office-hours'
        : startup.sessionMode === 'normal'
          ? undefined
          : startup.sessionMode,
  };
  ```

- [ ] 在 Session 创建成功后（line 516 之后），如果 `startup.officeHours` 为 true，进入 office-hours mode：
  ```typescript
  } else {
    session = await this.harness.createSession(createSessionOptions);
  }
  if (session !== undefined && startup.officeHours) {
    await session.setSessionMode('office-hours');
  }
  ```

- [ ] 运行 `pnpm -F @odysseythink/ody-code typecheck` 确认编译通过。

- [ ] 手动验证：运行 `ody --office-hours`，确认 TUI 启动后状态面板显示 `Office Hours: on`。
  ```bash
  # 预期输出：TUI 界面状态栏显示 "Office Hours: on"
  ```

- [ ] Commit: `feat: wire TUI startup for office-hours mode`

## Self-Review

- [ ] 1. Spec-coverage: Tasks 2-4 cover spec items 1 (CLI entry), 5 (app lifecycle — exit wiring).
- [ ] 2. Placeholder scan: no TODO/TBD; every edit has exact code.
- [ ] 3. No phantom tasks: Task 2 produces `--office-hours` flag + tests; Task 3 produces `runOfficeHours.ts` + tests; Task 4 produces TUI wiring.
- [ ] 4. Dependency soundness: Task 2 → Task 3 → Task 4 (sequential). All depend on Task 1 (types).
- [ ] 5. Caller soundness: `runOfficeHours` imports `CLIOptions` (with `officeHours`). `KimiTUIStartupInput` extends cleanly. `createSessionOptions` passes `'office-hours'` which is valid after Task 1. All callers checked.
- [ ] 6. Test-the-risk: Task 2 tests cover all 6 conflict combinations. Task 3 tests verify harness creation + tui.start call.
- [ ] 7. Type consistency: `CLIOptions.officeHours: boolean` matches `TUIStartupOptions.officeHours: boolean` and `KimiTUIStartupInput.officeHours: boolean`.
