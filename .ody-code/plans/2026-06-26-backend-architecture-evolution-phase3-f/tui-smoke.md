# Part 2 — TUI Smoke Mode

> Scope: `--smoke-test` CLI flag, non-interactive smoke path for the Rust host, and `OdyTUI` smoke support.

---

### Task B1: Add `smokeTest` to `CLIOptions` and register `--smoke-test`

**Depends on:** none

**Files:**
- Modify: `apps/ody-code/src/cli/options.ts` line 23 area
- Modify: `apps/ody-code/src/cli/commands.ts` lines 100–153 area
- Modify: `apps/ody-code/test/cli/options.test.ts` line 33 area

**Steps:**

- [ ] Write the failing test.

Append to `apps/ody-code/test/cli/options.test.ts` inside `describe('CLI options parsing')`:

```ts
describe('--smoke-test', () => {
  it('--smoke-test sets smokeTest to true', () => {
    expect(parse(['--smoke-test']).smokeTest).toBe(true);
  });

  it('--smoke-test defaults to false', () => {
    expect(parse([]).smokeTest).toBe(false);
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
cd apps/ody-code && pnpm vitest run test/cli/options.test.ts --reporter=verbose
```

Expected failure: `Property 'smokeTest' does not exist on type 'CLIOptions'` or `Expected false to be true`.

- [ ] Write the minimal implementation.

In `apps/ody-code/src/cli/options.ts`, add to `CLIOptions`:

```ts
export interface CLIOptions {
  // ... existing fields ...
  hostBinary: string | undefined;
  /** Non-interactive smoke test: create a session and exit without rendering the TUI. */
  smokeTest?: boolean;
}
```

In `apps/ody-code/src/cli/commands.ts`, add the option after `--host-binary`:

```ts
.addOption(new Option('--host-binary <path>', 'Path to the Rust host executable (defaults to ody-host on PATH).'))
.option('--smoke-test', 'Non-interactive smoke test: create a session and exit.', false);
```

And parse it into `CLIOptions`:

```ts
const opts: CLIOptions = {
  // ... existing fields ...
  hostBinary: raw['hostBinary'] as string | undefined,
  smokeTest: (raw['smokeTest'] as boolean) ?? false,
};
```

For consistency, add `smokeTest: false` to the `base()` helper in `apps/ody-code/test/cli/options.test.ts`:

```ts
function base(): CLIOptions {
  return {
    // ... existing fields ...
    hostBinary: undefined,
    smokeTest: false,
  };
}
```

- [ ] Verify callers and whole-tree typecheck.

Search for all `CLIOptions` construction sites:

```bash
rg -n "hostBinary:" apps/ody-code/test
rg -n "CLIOptions\s*{" apps/ody-code/src apps/ody-code/test
```

Because `smokeTest` is optional, existing callers remain valid. Run the workspace typecheck to confirm:

```bash
pnpm -r typecheck
```

Expected: typecheck succeeds (or fails only on pre-existing errors that Phase C will fix).

- [ ] Run the new tests and verify they PASS.

```bash
cd apps/ody-code && pnpm vitest run test/cli/options.test.ts --reporter=verbose
```

Expected: `--smoke-test` parsing tests pass.

- [ ] Commit.

```bash
git add apps/ody-code/src/cli/options.ts apps/ody-code/src/cli/commands.ts apps/ody-code/test/cli/options.test.ts
git commit -m "feat(cli): add --smoke-test flag and CLIOptions field"
```

---

### Task B2: Validate `--smoke-test` requires `--host=rust`

**Depends on:** Task B1

**Files:**
- Modify: `apps/ody-code/src/cli/options.ts` lines 121–145 area
- Modify: `apps/ody-code/test/cli/options.test.ts` line 441 area

**Steps:**

- [ ] Write the failing test.

Append inside `describe('rust host options')` in `apps/ody-code/test/cli/options.test.ts`:

```ts
it('rejects --smoke-test without --host=rust', () => {
  expect(() => validateOptions({ ...base(), smokeTest: true })).toThrow(OptionConflictError);
  expect(() => validateOptions({ ...base(), smokeTest: true })).toThrow('--smoke-test requires --host=rust.');
});

it('allows --smoke-test with --host=rust', () => {
  const result = validateOptions({ ...base(), host: 'rust', hostStdio: true, smokeTest: true });
  expect(result.uiMode).toBe('shell');
});
```

- [ ] Run it and verify it FAILS.

```bash
cd apps/ody-code && pnpm vitest run test/cli/options.test.ts --reporter=verbose
```

Expected failure: validation does not throw for `--smoke-test` without `--host=rust`.

- [ ] Write the minimal implementation.

In `apps/ody-code/src/cli/options.ts`, inside the `if (opts.host === 'rust') { ... }` block, add at the top:

```ts
if (opts.smokeTest && opts.host !== 'rust') {
  throw new OptionConflictError('--smoke-test requires --host=rust.');
}
```

Wait — the validation must run even when `host !== 'rust'`. Place it just before the `if (opts.host === 'rust')` block, after the host-validity check:

```ts
if (!['inproc', 'rust'].includes(opts.host)) {
  throw new OptionConflictError(`Invalid --host: ${opts.host}. Must be inproc or rust.`);
}
if (opts.smokeTest && opts.host !== 'rust') {
  throw new OptionConflictError('--smoke-test requires --host=rust.');
}
if (opts.host === 'rust') {
  // ... existing rust validations ...
}
```

- [ ] Run it and verify it PASSES.

```bash
cd apps/ody-code && pnpm vitest run test/cli/options.test.ts --reporter=verbose
```

Expected: the new validation tests pass.

- [ ] Commit.

```bash
git add apps/ody-code/src/cli/options.ts apps/ody-code/test/cli/options.test.ts
git commit -m "feat(cli): require --host=rust when --smoke-test is used"
```

---

### Task B3: Add smoke mode types and `OdyTUI.runSmokeTest()`

**Depends on:** Task B2

**Files:**
- Modify: `apps/ody-code/src/tui/types.ts` lines 192–211
- Modify: `apps/ody-code/src/tui/ody-tui.ts` lines 137–148, 252–304, 374–392

**Steps:**

- [ ] Write the failing test.

Create `apps/ody-code/test/tui/smoke-mode.test.ts`:

```ts
import { describe, expect, it, vi } from 'vitest';

import type { CLIOptions } from '#/cli/options';
import { OdyTUI } from '#/tui/ody-tui';
import type { OdyHarness } from '#/tui/types';

function makeHarness(overrides: Partial<OdyHarness> = {}): OdyHarness {
  return {
    homeDir: '/tmp/home',
    configPath: '/tmp/home/config.toml',
    interactiveAgentId: 'agent-1',
    track: vi.fn(),
    setTelemetryContext: vi.fn(),
    ensureConfigFile: vi.fn().mockResolvedValue(undefined),
    getConfig: vi.fn(),
    setConfig: vi.fn(),
    removeProvider: vi.fn(),
    getExperimentalFlags: vi.fn().mockResolvedValue({}),
    createSession: vi.fn(),
    resumeSession: vi.fn(),
    listSessions: vi.fn(),
    closeSession: vi.fn(),
    renameSession: vi.fn(),
    forkSession: vi.fn(),
    exportSession: vi.fn(),
    requestCodeReview: vi.fn(),
    close: vi.fn().mockResolvedValue(undefined),
    auth: {} as any,
    ...overrides,
  } as OdyHarness;
}

describe('OdyTUI.runSmokeTest', () => {
  it('returns success when session is created and listed', async () => {
    const session = { id: 'sess-123' } as any;
    const harness = makeHarness({
      createSession: vi.fn().mockResolvedValue(session),
      listSessions: vi.fn().mockResolvedValue([{ id: 'sess-123' }]),
    });
    const opts: CLIOptions = {
      host: 'rust',
      hostStdio: true,
      smokeTest: true,
    } as CLIOptions;

    const result = await OdyTUI.runSmokeTest(harness, opts);

    expect(result.success).toBe(true);
    expect(result.sessionId).toBe('sess-123');
    expect(result.transport).toBe('stdio');
  });

  it('returns failure when session id is empty', async () => {
    const harness = makeHarness({
      createSession: vi.fn().mockResolvedValue({ id: '' }),
    });
    const result = await OdyTUI.runSmokeTest(harness, { host: 'rust', hostSocket: '/tmp/x.sock' } as CLIOptions);
    expect(result.success).toBe(false);
    expect(result.transport).toBe('socket');
    expect(result.error).toContain('empty session id');
  });

  it('returns failure when created session is not listed', async () => {
    const harness = makeHarness({
      createSession: vi.fn().mockResolvedValue({ id: 'sess-123' }),
      listSessions: vi.fn().mockResolvedValue([]),
    });
    const result = await OdyTUI.runSmokeTest(harness, { host: 'rust', hostTcp: '127.0.0.1:9000' } as CLIOptions);
    expect(result.success).toBe(false);
    expect(result.transport).toBe('tcp');
    expect(result.error).toContain('not found in listSessions');
  });

  it('guards start() when smokeTest is set', async () => {
    // start() should be a no-op; the test simply verifies it does not throw.
    const harness = makeHarness();
    const tui = new OdyTUI(harness, {
      cliOptions: { host: 'rust', smokeTest: true } as CLIOptions,
      tuiConfig: { theme: 'dark', editorCommand: null, notifications: { enabled: false }, upgrade: { check: false } } as any,
      version: '0.0.0',
      workDir: '/tmp',
      officeHours: false,
      gameDesign: false,
      smokeTest: true,
    });
    await expect(tui.start()).resolves.toBeUndefined();
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
cd apps/ody-code && pnpm vitest run test/tui/smoke-mode.test.ts --reporter=verbose
```

Expected failure: `runSmokeTest` is not a property of `OdyTUI` or `smokeTest` is not accepted by `OdyTUIStartupInput`.

- [ ] Write the minimal implementation.

In `apps/ody-code/src/tui/types.ts`, add `smokeTest` to the startup/options types:

```ts
export interface OdyTUIOptions {
  initialAppState: AppState;
  startup: TUIStartupOptions;
  resolvedTheme?: ResolvedTheme;
  /** If true, bypass the interactive terminal UI (smoke-test mode). */
  smokeTest?: boolean;
}
```

In `apps/ody-code/src/tui/ody-tui.ts`, add `smokeTest` to `OdyTUIStartupInput`:

```ts
export interface OdyTUIStartupInput {
  readonly cliOptions: CLIOptions;
  readonly tuiConfig: TuiConfig;
  readonly version: string;
  readonly workDir: string;
  readonly startupNotice?: string;
  readonly resolvedTheme?: ResolvedTheme;
  readonly authIntent?: { readonly kind: 'login' | 'logout'; readonly providerType: string };
  readonly officeHours: boolean;
  readonly gameDesign: boolean;
  /** If true, bypass the interactive terminal UI (smoke-test mode). */
  readonly smokeTest?: boolean;
}
```

In the constructor, propagate the flag into `OdyTUIOptions`:

```ts
const tuiOptions: OdyTUIOptions = {
  initialAppState: createInitialAppState(startupInput),
  startup: {
    sessionFlag: startupInput.cliOptions.session,
    continueLast: startupInput.cliOptions.continue,
    yolo: startupInput.cliOptions.yolo,
    auto: startupInput.cliOptions.auto,
    sessionMode: startupInput.cliOptions.sessionMode,
    officeHours: startupInput.officeHours,
    gameDesign: startupInput.gameDesign,
    model: startupInput.cliOptions.model,
    startupNotice: startupInput.startupNotice,
    authIntent: startupInput.authIntent,
  },
  resolvedTheme: startupInput.resolvedTheme,
  smokeTest: startupInput.smokeTest,
};
```

Guard `start()`:

```ts
async start(): Promise<void> {
  if (this.options.smokeTest) {
    return;
  }
  // ... existing start implementation ...
}
```

Add the static smoke method near the `OdyTUI` class definition (after the constructor is fine):

```ts
export interface SmokeTestResult {
  readonly success: boolean;
  readonly sessionId: string | undefined;
  readonly transport: 'stdio' | 'socket' | 'tcp';
  readonly error?: string;
}

function resolveSmokeTransport(opts: CLIOptions): 'stdio' | 'socket' | 'tcp' {
  if (opts.hostSocket !== undefined) return 'socket';
  if (opts.hostTcp !== undefined) return 'tcp';
  return 'stdio';
}

export class OdyTUI {
  // ... existing class body ...

  static async runSmokeTest(harness: OdyHarness, opts: CLIOptions): Promise<SmokeTestResult> {
    try {
      await harness.ensureConfigFile();
      const flags = await harness.getExperimentalFlags();
      setExperimentalFlags(flags);

      const workDir = process.cwd();
      const session = await harness.createSession({ workDir });

      if (session.id === undefined || session.id.length === 0) {
        throw new Error('createSession returned empty session id');
      }

      const sessions = await harness.listSessions({ workDir });
      if (!sessions.some((s) => s.id === session.id)) {
        throw new Error('created session not found in listSessions');
      }

      return {
        success: true,
        sessionId: session.id,
        transport: resolveSmokeTransport(opts),
      };
    } catch (error) {
      return {
        success: false,
        sessionId: undefined,
        transport: resolveSmokeTransport(opts),
        error: error instanceof Error ? error.message : String(error),
      };
    }
  }
}
```

- [ ] Verify callers and whole-tree typecheck.

Search for `OdyTUIStartupInput` construction sites:

```bash
rg -n "new OdyTUI\(" apps/ody-code/src apps/ody-code/test
rg -n "makeStartupInput" apps/ody-code/test/tui
```

Because `smokeTest` is optional, existing callers remain valid. Run:

```bash
pnpm -r typecheck
```

Expected: typecheck succeeds (or only pre-existing errors remain).

- [ ] Run it and verify it PASSES.

```bash
cd apps/ody-code && pnpm vitest run test/tui/smoke-mode.test.ts --reporter=verbose
```

Expected: all smoke-mode unit tests pass.

- [ ] Commit.

```bash
git add apps/ody-code/src/tui/types.ts apps/ody-code/src/tui/ody-tui.ts apps/ody-code/test/tui/smoke-mode.test.ts
git commit -m "feat(tui): add smoke-mode support and OdyTUI.runSmokeTest"
```

---

### Task B4: Wire smoke branch in `run-shell-rust.ts`

**Depends on:** Task B3

**Files:**
- Modify: `apps/ody-code/src/cli/run-shell-rust.ts` lines 91–109 area

**Steps:**

- [ ] Write the failing test.

Append to `apps/ody-code/test/cli/run-shell-rust.test.ts`:

```ts
import { runSmokeTestBranch } from '#/cli/run-shell-rust';

describe('runSmokeTestBranch', () => {
  it('prints SMOKE_OK and exits 0 on success', async () => {
    const harness = {
      ensureConfigFile: vi.fn().mockResolvedValue(undefined),
      getExperimentalFlags: vi.fn().mockResolvedValue({}),
      createSession: vi.fn().mockResolvedValue({ id: 'sess-ok' }),
      listSessions: vi.fn().mockResolvedValue([{ id: 'sess-ok' }]),
      close: vi.fn().mockResolvedValue(undefined),
    } as any;
    const stdout = vi.spyOn(process.stdout, 'write').mockImplementation(() => true);
    const exit = vi.spyOn(process, 'exit').mockImplementation(() => undefined as never);

    await runSmokeTestBranch(harness, { host: 'rust', hostStdio: true } as any);

    expect(stdout).toHaveBeenCalledWith('SMOKE_OK stdio sess-ok\n');
    expect(exit).toHaveBeenCalledWith(0);

    stdout.mockRestore();
    exit.mockRestore();
  });

  it('prints SMOKE_FAIL and exits 1 on failure', async () => {
    const harness = {
      ensureConfigFile: vi.fn().mockRejectedValue(new Error('boom')),
      close: vi.fn().mockResolvedValue(undefined),
    } as any;
    const stderr = vi.spyOn(process.stderr, 'write').mockImplementation(() => true);
    const exit = vi.spyOn(process, 'exit').mockImplementation(() => undefined as never);

    await runSmokeTestBranch(harness, { host: 'rust', hostStdio: true } as any);

    expect(stderr).toHaveBeenCalledWith(expect.stringMatching(/^SMOKE_FAIL stdio: boom/));
    expect(exit).toHaveBeenCalledWith(1);

    stderr.mockRestore();
    exit.mockRestore();
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
cd apps/ody-code && pnpm vitest run test/cli/run-shell-rust.test.ts --reporter=verbose
```

Expected failure: `runSmokeTestBranch` is not exported.

- [ ] Write the minimal implementation.

In `apps/ody-code/src/cli/run-shell-rust.ts`, add the import and helper:

```ts
import { OdyTUI } from '#/tui/index';

export async function runSmokeTestBranch(harness: any, opts: CLIOptions): Promise<void> {
  const result = await OdyTUI.runSmokeTest(harness, opts);
  await harness.close();
  if (result.success) {
    process.stdout.write(`SMOKE_OK ${result.transport} ${result.sessionId}\n`);
    process.exit(0);
  } else {
    process.stderr.write(`SMOKE_FAIL ${result.transport}: ${result.error}\n`);
    process.exit(1);
  }
}
```

Then insert the branch after the harness is created (after line 94 in the current file):

```ts
const harness = new RustHostHarness({ client, telemetry: telemetryClient });

if (opts.smokeTest) {
  await runSmokeTestBranch(harness, opts);
  return;
}

await harness.ensureConfigFile();
```

- [ ] Run it and verify it PASSES.

```bash
cd apps/ody-code && pnpm vitest run test/cli/run-shell-rust.test.ts --reporter=verbose
```

Expected: the smoke branch unit tests pass.

- [ ] Commit.

```bash
git add apps/ody-code/src/cli/run-shell-rust.ts apps/ody-code/test/cli/run-shell-rust.test.ts
git commit -m "feat(cli): wire --smoke-test branch in run-shell-rust"
```

---

### Task B5: Smoke tests for stdio, socket, and tcp transports

**Depends on:** Task B4

**Files:**
- Create: `apps/ody-code/test/cli/smoke-test.integration.test.ts`

**Steps:**

- [ ] Write the failing test.

Create `apps/ody-code/test/cli/smoke-test.integration.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { execa } from 'execa';

const ODY_HOST_BINARY_PATH = process.env.ODY_HOST_BINARY_PATH ?? '../../rust-ody/target/release/ody-host';
const RUN = 'pnpm --filter ody-code run dev:cli-only --';

function runSmoke(args: string[]): Promise<{ exitCode: number; stdout: string; stderr: string }> {
  return new Promise((resolve) => {
    const child = execa.command(`${RUN} ${args.join(' ')}`, {
      env: { ODY_HOST_BINARY_PATH },
      reject: false,
    });
    void child.then((result) =>
      resolve({
        exitCode: result.exitCode ?? 1,
        stdout: result.stdout,
        stderr: result.stderr,
      }),
    );
  });
}

describe('TUI smoke mode integration', () => {
  it('stdio transport exits 0 with SMOKE_OK', async () => {
    const { exitCode, stdout } = await runSmoke(['--host=rust', '--host-stdio', '--smoke-test']);
    expect(exitCode).toBe(0);
    expect(stdout).toMatch(/^SMOKE_OK stdio /);
  }, 60_000);

  it('socket transport exits 0 with SMOKE_OK', async () => {
    const { exitCode, stdout } = await runSmoke(['--host=rust', '--host-socket', '/tmp/ody-smoke-test.sock', '--smoke-test']);
    expect(exitCode).toBe(0);
    expect(stdout).toMatch(/^SMOKE_OK socket /);
  }, 60_000);

  it('tcp transport exits 0 with SMOKE_OK', async () => {
    const { exitCode, stdout } = await runSmoke(['--host=rust', '--host-tcp', '127.0.0.1:19095', '--smoke-test']);
    expect(exitCode).toBe(0);
    expect(stdout).toMatch(/^SMOKE_OK tcp /);
  }, 60_000);

  it('bad binary path exits 1 with SMOKE_FAIL', async () => {
    const child = execa.command(`${RUN} --host=rust --host-stdio --smoke-test`, {
      env: { ODY_HOST_BINARY_PATH: '/nonexistent/ody-host' },
      reject: false,
    });
    const result = await child;
    expect(result.exitCode).not.toBe(0);
    expect(result.stderr).toMatch(/^SMOKE_FAIL stdio:/);
  }, 60_000);
});
```

- [ ] Run it and verify it FAILS.

```bash
cd apps/ody-code && pnpm vitest run test/cli/smoke-test.integration.test.ts --reporter=verbose
```

Expected failure: file does not exist yet.

- [ ] Write the minimal implementation.

Task B4 wired the smoke branch in `run-shell-rust.ts`. This task only adds the integration test file; no new production code is required. Ensure `execa` is available in the workspace; if not, use `node:child_process` instead:

```ts
import { exec } from 'node:child_process';

function runSmoke(args: string[]): Promise<{ exitCode: number; stdout: string; stderr: string }> {
  return new Promise((resolve, reject) => {
    const cmd = `${RUN} ${args.join(' ')}`;
    exec(cmd, { env: { ...process.env, ODY_HOST_BINARY_PATH } }, (error, stdout, stderr) => {
      resolve({ exitCode: error?.code ?? 0, stdout, stderr });
    });
  });
}
```

Use whichever execution helper the project already depends on. If `execa` is not a dependency, replace with `node:child_process` in the plan before running.

- [ ] Run it and verify it PASSES.

First ensure the host binary exists:

```bash
pnpm run build:host
```

Then run the integration tests:

```bash
cd apps/ody-code && pnpm vitest run test/cli/smoke-test.integration.test.ts --reporter=verbose
```

Expected: all four integration tests pass.

- [ ] Commit.

```bash
git add apps/ody-code/test/cli/smoke-test.integration.test.ts
git commit -m "test(cli): add integration tests for --smoke-test over stdio/socket/tcp"
```

---

## Local Self-Review

- [ ] 1. Spec-coverage table (Part 2): `--smoke-test` flag (B1), non-interactive smoke path (B3–B4), stdio/socket/tcp coverage (B4–B5), validation (B2) — all covered.
- [ ] 2. Placeholder scan: no TODO/TBD; every function is fully implemented in the steps above.
- [ ] 3. No phantom tasks: each task creates/modifies real files and ends with a passing test + commit.
- [ ] 4. Dependency soundness: B2 depends on B1, B3 on B2, B4 on B3, B5 on B4; no forward references.
- [ ] 5. Caller & build soundness: B1 changes `CLIOptions` and B3 changes `OdyTUIStartupInput` / `OdyTUIOptions`. Both fields are optional (`?`), so existing callers remain type-correct; the search commands and whole-tree `pnpm -r typecheck` confirm this.
- [ ] 6. Test-the-risk: validation test asserts `--smoke-test` is rejected without `--host=rust`; `runSmokeTest` unit tests assert session creation/listing behavior; integration tests assert `SMOKE_OK`/`SMOKE_FAIL` output and exit codes.
- [ ] 7. Type consistency: `smokeTest` is optional boolean in all three types; `OdyTUI.runSmokeTest` returns the `SmokeTestResult` shape from the design.
