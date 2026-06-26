# Game Design Mode — CLI 层

## Scope

本部分覆盖从命令行到 TUI 启动入口的改动：新增 `--game-design` 标志、专用 runner、选项校验、telemetry 命名。

## 数据流

```
process.argv
  → createProgram() 注册 --game-design Option
  → program.action() 解析出 opts.gameDesign
  → validateOptions() 校验冲突（与 office-hours 互斥）
  → handleMainCommand() 识别 opts.gameDesign
  → runGameDesign(opts, version)
  → OdyTUI(harness, { cliOptions: {...opts, sessionMode: 'game-design'}, officeHours: false })
```

## 类型与接口

### `CLIOptions` 扩展

**文件**：`apps/ody-code/src/cli/options.ts:4-17`

```ts
export interface CLIOptions {
  session: string | undefined;
  continue: boolean;
  yolo: boolean;
  auto: boolean;
  sessionMode: 'normal' | 'plan' | 'design' | 'office-hours';
  officeHours: boolean;
  gameDesign: boolean;              // [C:INFERRED] 与 officeHours 并列的布尔标志
  model: string | undefined;
  outputFormat: PromptOutputFormat | undefined;
  prompt: string | undefined;
  skillsDirs: string[];
  loginProvider: string | undefined;
  logoutProvider: string | undefined;
}
```

- `gameDesign` 为 `true` 当且仅当用户显式传入 `--game-design`。
- 与 `officeHours` 互斥：两个标志不能同时为 `true` [C:INFERRED]。

### `ValidatedOptions` 不变

```ts
export interface ValidatedOptions {
  options: CLIOptions;
  uiMode: UIMode;  // 'shell'，game-design 只支持交互式 shell
}
```

## 调用点

### 1. 注册 CLI 选项

**文件**：`apps/ody-code/src/cli/commands.ts:75-80`（当前 `--office-hours` 位置）

在 `--office-hours` 选项之后追加：

```ts
.addOption(
  new Option(
    '--game-design',
    'Start Ody Code in Game Design mode.',
  ).conflicts(['prompt', 'session', 'continue', 'sessionMode', 'yolo', 'auto', 'officeHours']),
)
```

- `conflicts` 包含 `officeHours` [C:INFERRED]，防止同时进入两种专用模式。

### 2. 解析为 `CLIOptions`

**文件**：`apps/ody-code/src/cli/commands.ts:121-134`

在 `opts` 对象中新增：

```ts
gameDesign: (raw['gameDesign'] as boolean) ?? false,
```

### 3. 校验逻辑

**文件**：`apps/ody-code/src/cli/options.ts:73-90`（当前 `officeHours` 校验位置）

在 `officeHours` 分支后追加对称的 `gameDesign` 分支：

```ts
if (opts.gameDesign) {
  if (opts.prompt !== undefined) {
    throw new OptionConflictError('Cannot combine --game-design with --prompt.');
  }
  if (opts.session !== undefined) {
    throw new OptionConflictError('Cannot combine --game-design with --session.');
  }
  if (opts.continue) {
    throw new OptionConflictError('Cannot combine --game-design with --continue.');
  }
  if (opts.sessionMode !== 'normal') {
    throw new OptionConflictError('Cannot combine --game-design with --session-mode.');
  }
  if (opts.yolo || opts.auto) {
    throw new OptionConflictError('Permission mode is fixed to manual in game-design mode.');
  }
  if (opts.officeHours) {
    throw new OptionConflictError('Cannot combine --game-design with --office-hours.');
  }
  return { options: opts, uiMode: 'shell' };
}
```

### 4. 主命令路由

**文件**：`apps/ody-code/src/main.ts:80-85`

```ts
if (validated.options.gameDesign) {
  await runGameDesign(validated.options, version);
  return;
}
if (validated.options.officeHours) {
  await runOfficeHours(validated.options, version);
  return;
}
```

### 5. 专用 runner

**文件**：新建 `apps/ody-code/src/cli/run-game-design.ts`

整体结构复制 `run-office-hours.ts:23-107`，关键差异：

```ts
export async function runGameDesign(opts: CLIOptions, version: string): Promise<void> {
  // ... telemetry / config / harness 初始化与 runOfficeHours 相同 ...

  log.info('kimi-code starting in game-design mode', {
    version,
    uiMode: CLI_UI_MODE,
    nodeVersion: process.version,
    platform: `${process.platform}/${process.arch}`,
    workDir,
  });

  const tui = new OdyTUI(harness, {
    cliOptions: { ...opts, sessionMode: 'game-design' },
    tuiConfig,
    version,
    workDir,
    startupNotice: configWarning,
    resolvedTheme,
    officeHours: false,
  });

  initializeCliTelemetry({
    harness,
    bootstrap: telemetryBootstrap,
    config,
    version,
    uiMode: CLI_UI_MODE,
  });
  setCrashPhase('runtime');

  track('game_design_started', { project_slug: basename(workDir) });

  tui.onExit = async (exitCode = 0) => {
    const sessionId = tui.getCurrentSessionId();
    setCrashPhase('shutdown');
    withTelemetryContext({ sessionId }).track('game_design_completed', {
      duration_s: (Date.now() - startedAt) / 1000,
      project_slug: basename(workDir),
      outcome: exitCode === 0 ? 'success' : 'abort',
    });
    await shutdownTelemetry({ timeoutMs: CLI_SHUTDOWN_TIMEOUT_MS });
    process.exit(exitCode);
  };

  // ... 其余与 runOfficeHours 相同 ...
}
```

- 不新增 `gameDesign: true` 字段传入 TUI [C:INFERRED]；`cliOptions.sessionMode = 'game-design'` 是单一事实来源，TUI 内部通过 `sessionMode` 判断。

## 算法

### 选项冲突检测算法

输入：`raw: Record<string, unknown>`
输出：`CLIOptions` 或抛出 `OptionConflictError`

```
1. 解析 session / resume 得到 sessionValue。
2. 解析 yolo / auto / continue / sessionMode / officeHours / gameDesign 等布尔/枚举值。
3. 若 gameDesign === true：
   a. prompt !== undefined → 错误
   b. sessionValue !== undefined → 错误
   c. continue === true → 错误
   d. sessionMode !== 'normal' → 错误
   e. yolo === true 或 auto === true → 错误
   f. officeHours === true → 错误
   g. 返回 { options, uiMode: 'shell' }
4. 否则沿用现有校验链。
```

## 错误处理

| 错误类 | 立即处理 | 降级路径 | 恢复条件 |
|--------|---------|---------|---------|
| `OptionConflictError` | `process.stderr.write(error.message); process.exit(1)` | 无 | 用户重新输入不冲突的命令 |
| `TuiConfigParseError` | 使用 fallback config，展示 `startupNotice` | 无 | 用户修复 `~/.ody-code/config.toml` 后重试 |
| runner 启动异常 | `logStartupFailure` → 输出日志路径 → `process.exit(1)` | 无 | 修复环境/配置后重试 |

## 测试断言

1. `apps/ody-code/test/cli/options.test.ts` 新增：
   - 输入 `{ gameDesign: true, prompt: 'x' }` → 抛出 `OptionConflictError`。
   - 输入 `{ gameDesign: true, officeHours: true }` → 抛出 `OptionConflictError`。
   - 输入 `{ gameDesign: true, yolo: true }` → 抛出 `OptionConflictError`。
   - 输入 `{ gameDesign: true }` → 返回 `{ uiMode: 'shell' }`。
   - 输入 `{ gameDesign: true, sessionMode: 'design' }` → 抛出 `OptionConflictError`。

2. `apps/ody-code/test/cli/main.test.ts` 新增（如已有类似 office-hours 分支测试）：
   - `handleMainCommand({ gameDesign: true, ...defaultOpts }, version)` 调用 `runGameDesign` 并提前返回。

3. `apps/ody-code/test/cli/game-design-bootstrap.test.ts`（可选新建）：
   - 验证 `runGameDesign` 构造的 `OdyTUI` 选项中 `cliOptions.sessionMode === 'game-design'` 且 `officeHours === false`。
   - 验证 `track('game_design_started', ...)` 被调用。
   - 验证退出时 `track('game_design_completed', ...)` 被调用。

## 本地说明

- `game-design` 与 `office-hours` 在 CLI 层完全平行：专用标志 → 校验 → 专用 runner → TUI 启动。
- 不新增 `gameDesign` 布尔字段传入 TUI，避免与 `cliOptions.sessionMode` 形成双重来源 [C:INFERRED]。
- telemetry 事件使用独立命名 `game_design_*`，不与 office-hours 指标混合 [C:USER]。
