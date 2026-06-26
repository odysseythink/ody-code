# Part 1 — CLI 参数解析与 TUI 入口

## Scope

定义 `ody --office-hours` 启动参数如何贯穿 CLI 解析、选项校验、TUI bootstrap，最终创建一个锁定在 office-hours mode 的 Session。

## Interfaces

```typescript
// apps/ody-code/src/cli/options.ts
export interface CLIOptions {
  session: string | undefined;
  continue: boolean;
  yolo: boolean;
  auto: boolean;
  sessionMode: 'normal' | 'plan' | 'design';
  officeHours: boolean;                       // [C:USER] 新增
  model: string | undefined;
  outputFormat: PromptOutputFormat | undefined;
  prompt: string | undefined;
  skillsDirs: string[];
  loginProvider: string | undefined;
  logoutProvider: string | undefined;
}

// apps/ody-code/src/tui/types.ts
export interface TUIStartupOptions {
  readonly sessionFlag?: string;
  readonly continueLast: boolean;
  readonly yolo: boolean;
  readonly auto: boolean;
  readonly sessionMode: 'normal' | 'plan' | 'design' | 'office-hours'; // [C:USER]
  readonly officeHours: boolean;                                        // [C:USER]
  readonly model?: string;
  readonly startupNotice?: string;
  readonly authIntent?: { readonly kind: 'login' | 'logout'; readonly providerType: string };
}

export interface AppState {
  // ... existing fields ...
  sessionMode: 'normal' | 'plan' | 'design' | 'office-hours'; // [C:USER]
  // ...
}

// apps/ody-code/src/tui/ody-tui.ts
export interface KimiTUIStartupInput {
  readonly cliOptions: CLIOptions;
  readonly tuiConfig: TuiConfig;
  readonly version: string;
  readonly workDir: string;
  readonly startupNotice?: string;
  readonly resolvedTheme?: ResolvedTheme;
  readonly authIntent?: { readonly kind: 'login' | 'logout'; readonly providerType: string };
  readonly officeHours: boolean; // [C:USER] 新增，决定是否进入专用流程
}
```

## Data Flow

```
process.argv
  │
  ▼
commands.ts createProgram() 解析 --office-hours
  │
  ▼
CLIOptions.officeHours = true
  │
  ▼
options.ts validateOptions()：
  - 与 --prompt、--session、--continue、--session-mode 冲突 [C:INFERRED]
  - officeHours 为 true 时 uiMode 强制为 'shell'
  │
  ▼
main.ts handleMainCommand()
  - if opts.officeHours → runOfficeHours(opts, version) [C:USER]
  │
  ▼
run-office-hours.ts (new file)
  - 复用 run-shell.ts 的 harness / telemetry 初始化
  - 构造 KimiTUI(harness, { ..., officeHours: true })
  - await tui.start()
  - 若 tui 未自动退出，调用 tui.exitWhenDone()
  │
  ▼
KimiTUI.start()
  - 创建 Session 时传入 mode = 'office-hours'
  - 禁用普通 slash 命令（或仅保留 help/exit）
  - 自动发送 office-hours entry prompt
```

## Algorithms

### CLI 参数冲突校验

```
function validateOptions(opts: CLIOptions): ValidatedOptions
  if opts.officeHours then
    if opts.prompt !== undefined           → throw OptionConflictError('Cannot combine --office-hours with --prompt.')
    if opts.session !== undefined          → throw OptionConflictError('Cannot combine --office-hours with --session.')
    if opts.continue                       → throw OptionConflictError('Cannot combine --office-hours with --continue.')
    if opts.sessionMode !== 'normal'       → throw OptionConflictError('Cannot combine --office-hours with --session-mode.')
    if opts.yolo or opts.auto              → throw OptionConflictError('Permission mode is fixed to manual in office-hours.')
  return { options: opts, uiMode: promptMode ? 'print' : 'shell' }
```

### runOfficeHours 启动流程

```
async function runOfficeHours(opts: CLIOptions, version: string): Promise<void>
  tuiConfig ← loadTuiConfig()
  workDir ← process.cwd()
  telemetryBootstrap ← createCliTelemetryBootstrap()
  harness ← new KimiHarness({ homeDir, identity, telemetry })
  await harness.ensureConfigFile()
  config ← await harness.getConfig()
  initializeCliTelemetry(...)

  tui ← new KimiTUI(harness, {
    cliOptions: { ...opts, sessionMode: 'office-hours', officeHours: true },
    tuiConfig,
    version,
    workDir,
    resolvedTheme,
  })

  tui.onExit ← async (exitCode = 0) => {
    await shutdownTelemetry(...)
    process.exit(exitCode)
  }

  try
    execSync('stty -ixon', { stdio: 'ignore' })
  catch ignore

  await tui.start()
```

### KimiTUI 启动时进入 office-hours mode

```
KimiTUI.start()
  await this.bootstrapSession()
  if this.options.initialAppState.sessionMode === 'office-hours' then
    await this.session.setSessionMode('office-hours')
    this.sendOfficeHoursEntryPrompt()
```

## Call-Site Integration

### 1. apps/ody-code/src/cli/commands.ts:70-85 [C:USER]

在 `.option('--session-mode <mode>', ...)` 之后添加：

```typescript
.addOption(
  new Option('--office-hours', 'Start Ody Code in YC Office Hours mode. Exits after the design doc is written.')
    .conflicts(['prompt', 'session', 'continue', 'sessionMode', 'yolo', 'auto']),
)
```

`program.action` 中解析 raw opts：

```typescript
const opts: CLIOptions = {
  // ... existing fields ...
  officeHours: (raw['officeHours'] as boolean) ?? false,
};
```

### 2. apps/ody-code/src/cli/options.ts:48-53 [C:USER]

在 `!['normal', 'plan', 'design'].includes(opts.sessionMode)` 校验之后添加 office-hours 冲突校验（见 Algorithms）。

### 3. apps/ody-code/src/main.ts:74-79 [C:USER]

```typescript
if (validated.uiMode === 'print') {
  await runPrompt(validated.options, version);
  return;
}

if (opts.officeHours) {
  await runOfficeHours(validated.options, version);
  return;
}

await runShell(validated.options, version);
```

### 4. apps/ody-code/src/tui/ody-tui.ts:149-180 [C:USER]

`createInitialAppState` 接受 `officeHours` 并设置 `sessionMode: 'office-hours'`：

```typescript
function createInitialAppState(input: KimiTUIStartupInput): AppState {
  // ...
  return {
    // ...
    sessionMode: input.officeHours ? 'office-hours' : input.cliOptions.sessionMode,
    // ...
  };
}
```

### 5. apps/ody-code/src/tui/ody-tui.ts:KimiTUI.start() [C:INFERRED]

在 Session 创建后，若 `this.state.appState.sessionMode === 'office-hours'`，调用 `this.enterOfficeHoursMode()`。

## Error & Degradation

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| `OptionConflictError` for `--office-hours` combos | 打印 usage 到 stderr，exit 1 | N/A | 用户重新启动并使用合法参数 |
| `KimiHarness` config load failure | 复用现有 run-shell 错误处理 | 启动失败 | 修复 config.toml |
| `Session.setSessionMode('office-hours')` rejected | 显示错误通知，调用 `host.stop(1)` | 退出应用 | 后端实现正确后恢复 |
| TUI 渲染失败 | run-office-hours 的 try/catch 触发 shutdownTelemetry + exit 1 | N/A | 修复终端环境 |

## Test Plan

1. **CLI 解析测试**（`apps/ody-code/test/cli/options.test.ts` 新增）：
   - `expect(() => validateOptions({ ...base, officeHours: true, prompt: 'x' })).toThrow('Cannot combine --office-hours with --prompt.')`
   - `expect(validateOptions({ ...base, officeHours: true }).uiMode).toBe('shell')`

2. **TUI 启动状态测试**：
   - 构造 `KimiTUI` 时 `officeHours: true`，断言 `state.appState.sessionMode === 'office-hours'`。

3. **runOfficeHours 集成测试**（mock harness / tui）：
   - 验证 `runOfficeHours` 创建 harness、初始化 telemetry、调用 `tui.start()`。

## Done Criteria

- `pnpm -F @odysseythink/ody-code typecheck` passes.
- `pnpm -F @odysseythink/ody-code test` passes.
- 手动验证：`ody --office-hours --prompt x` 立即报错并退出。
