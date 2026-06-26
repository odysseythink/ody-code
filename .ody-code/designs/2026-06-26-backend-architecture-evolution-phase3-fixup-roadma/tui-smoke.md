# Part 2 — TUI Smoke Mode

## 1. Scope

This part designs the `--smoke-test` CLI flag and the non-interactive TUI path it triggers. It covers:

- Adding `smokeTest` to `CLIOptions` and the commander program. [C:USER]
- Wiring `--smoke-test` through `runShell()` to `OdyTUI`. [C:USER]
- A smoke branch inside `OdyTUI` that creates a session and exits cleanly without rendering the interactive UI. [C:USER]
- Smoke verification for stdio, Unix-socket, and TCP transports. [C:USER]

Out of scope:
- Actual LLM chat round-trip (no provider required). [C:USER]
- Smoke for `--host=inproc` (the flag may be ignored or rejected when not `rust`). [C:INFERRED]

---

## 2. Data Models

### 2.1 `CLIOptions` extension

```ts
export interface CLIOptions {
  // ... existing fields ...
  readonly smokeTest: boolean;
}
```

### 2.2 `OdyTUIOptions` extension

```ts
export interface OdyTUIOptions {
  // ... existing fields ...
  readonly smokeTest: boolean;
}
```

### 2.3 `SmokeTestResult`

```ts
interface SmokeTestResult {
  readonly success: boolean;
  readonly sessionId: string | undefined;
  readonly transport: 'stdio' | 'socket' | 'tcp';
  readonly error?: string;
}
```

---

## 3. Algorithms

### 3.1 `runShell()` smoke branch

File: `apps/ody-code/src/cli/run-shell.ts`, lines 97-223. [C:INFERRED]

```
function runShell(opts: CLIOptions, version: string): Promise<void>
  // existing setup: loadTuiConfig, detect theme, telemetry bootstrap
  harness := opts.host === 'rust'
    ? await createRustHarness(opts, telemetryClient, telemetryBootstrap.homeDir)
    : createKimiHarness(...)

  if opts.smokeTest
    return runSmokeTest(harness, opts, version)

  // existing interactive TUI path follows
  ...
```

### 3.2 `runSmokeTest()`

```
function runSmokeTest(
  harness: OdyHarness,
  opts: CLIOptions,
  version: string,
): Promise<void>
  tui := new OdyTUI(harness, {
    cliOptions: opts,
    tuiConfig,
    version,
    workDir,
    resolvedTheme,
    officeHours: false,
    gameDesign: false,
  })
  result := await tui.runSmokeTest()
  await harness.close()
  if result.success
    stdout.write(`SMOKE_OK ${result.transport} ${result.sessionId}\n`)
    exit 0
  else
    stderr.write(`SMOKE_FAIL ${result.transport}: ${result.error}\n`)
    exit 1
```

### 3.3 `OdyTUI.runSmokeTest()`

```
async function runSmokeTest(): Promise<SmokeTestResult>
  try
    await this.harness.ensureConfigFile()
    flags := await this.harness.getExperimentalFlags()
    setExperimentalFlags(flags)

    workDir := this.state.appState.workDir
    session := await this.harness.createSession({ workDir })

    if session.id === undefined || session.id.length === 0
      throw Error('createSession returned empty session id')

    // Verify the session is observable from the host's perspective.
    sessions := await this.harness.listSessions({ workDir })
    if !sessions.some(s => s.id === session.id)
      throw Error('created session not found in listSessions')

    return { success: true, sessionId: session.id, transport: resolveTransport(this.options.cliOptions) }
  catch error
    return { success: false, sessionId: undefined, transport: resolveTransport(this.options.cliOptions), error: error.message }
```

### 3.4 `resolveTransport(opts)`

```
function resolveTransport(opts: CLIOptions): 'stdio' | 'socket' | 'tcp'
  if opts.hostSocket !== undefined return 'socket'
  if opts.hostTcp !== undefined return 'tcp'
  return 'stdio'
```

### 3.5 `OdyTUI.start()` guarded branch

File: `apps/ody-code/src/tui/ody-tui.ts`, line 374. [C:INFERRED]

```
async start(): Promise<void>
  if this.options.smokeTest
    // Smoke path bypasses the terminal UI entirely.
    return

  // existing interactive start
  this.registerSignalHandlers()
  ...
```

---

## 4. Call-Site Integration

### 4.1 Commander option registration

File: `apps/ody-code/src/cli/commands.ts`, lines 100-104 area. [C:USER]

```ts
program
  .option('--host-stdio', 'Launch Rust host in stdio mode.', false)
  .addOption(new Option('--host-socket <path>', 'Launch Rust host listening on a Unix socket.'))
  .addOption(new Option('--host-tcp <host:port>', 'Launch Rust host listening on TCP.'))
  .addOption(new Option('--host-binary <path>', 'Path to the Rust host executable (defaults to ody-host on PATH).'))
  .option('--smoke-test', 'Non-interactive smoke test: create a session and exit.', false);
```

### 4.2 Option parsing into `CLIOptions`

File: `apps/ody-code/src/cli/commands.ts`, lines 134-153. [C:INFERRED]

```ts
const opts: CLIOptions = {
  // ... existing fields ...
  hostBinary: raw['hostBinary'] as string | undefined,
  smokeTest: (raw['smokeTest'] as boolean) ?? false,
};
```

### 4.3 Option validation

File: `apps/ody-code/src/cli/options.ts`, lines 121-145. [C:INFERRED]

```ts
if (opts.host === 'rust') {
  // ... existing rust validations ...
  if (opts.smokeTest && opts.host !== 'rust') {
    throw new OptionConflictError('--smoke-test requires --host=rust.');
  }
  // default to stdio remains unchanged
}
```

### 4.4 `OdyTUI` construction in `runShell()`

File: `apps/ody-code/src/cli/run-shell.ts`, lines 140-150. [C:INFERRED]

```ts
const tui = new OdyTUI(harness, {
  cliOptions: opts,
  tuiConfig,
  version,
  workDir,
  startupNotice: configWarning,
  resolvedTheme,
  authIntent: runOptions.authIntent,
  officeHours: false,
  gameDesign: false,
  // smokeTest is already in cliOptions, no new constructor field needed
});
```

---

## 5. Error Handling

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| `--smoke-test` without `--host=rust` | `validateOptions` throws `OptionConflictError`; CLI exits 1. | None. | Rerun with `--host=rust`. |
| Rust host fails to spawn | `createRustHarness` rejects; caught in `runSmokeTest`; exits 1. | None. | Fix host binary path or transport config. |
| `createSession` returns empty id | `runSmokeTest` returns `success: false`; exits 1. | None. | Debug Rust host session creation. |
| `listSessions` does not contain created session | Returns `success: false`; exits 1. | None. | Debug session persistence/listing. |
| Terminal TTY errors in smoke path | Not reached because UI is bypassed. | N/A | N/A |

---

## 6. Test Plan

| Test | Assertion |
|---|---|
| `--smoke-test` rejected with `--host=inproc` | `validateOptions` throws `OptionConflictError`. |
| `--smoke-test --host=rust --host-stdio` exits 0 | stdout contains `SMOKE_OK stdio <uuid>`. |
| `--smoke-test --host=rust --host-socket /tmp/ody-smoke.sock` exits 0 | stdout contains `SMOKE_OK socket <uuid>`. |
| `--smoke-test --host=rust --host-tcp 127.0.0.1:<port>` exits 0 | stdout contains `SMOKE_OK tcp <uuid>`. |
| Smoke with bad binary path exits 1 | stderr contains `SMOKE_FAIL stdio:` and a spawn error. |
| No interactive UI starts in smoke mode | Process does not wait for stdin input; exits within timeout. |

Done criteria [C:USER]:
- `pnpm --filter ody-code run dev:cli-only -- --host=rust --host-stdio --smoke-test` exits 0.
- Same for `--host-socket` and `--host-tcp` variants.
- The smoke output contains a parseable `SMOKE_OK <transport> <sessionId>` line.

---

## 7. Local Notes

- `runSmokeTest()` must **not** call `tui.start()`; it directly uses the `harness` without initializing `pi-tui`. [C:INFERRED]
- The `OdyTUI` constructor currently performs some state setup; ensure smoke mode does not open `/dev/tty` or install signal handlers. If it does, move those into `start()`. [C:INFERRED]
- For CI stability, the smoke process should complete within 30 seconds; set a 60-second timeout in the verification script. [C:INFERRED]
- TCP smoke should use the same port-retry logic as `rust-host-connect.test.ts` (base port 19090, 10 attempts) to avoid `EADDRINUSE`. [C:INFERRED]
