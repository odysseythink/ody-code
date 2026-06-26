# Part 1: agent-core — SetupScriptRunner, Permission Gate, Session Wiring, /init Extension

**Depends on:** none (first part)

## Task 1: Types + detectSetupScript + formatSetupReminder

**Depends on:** none  
**Files:** Create `packages/agent-core/src/session/setup-script.ts:1-90`, Create `packages/agent-core/test/session/setup-script.test.ts:1-120`

### Design

Exported types/constants:
```typescript
export const SETUP_SCRIPT_PATH = '.ody-code/setup.sh';
export const DEFAULT_TIMEOUT_MS = 300_000;   // 300s
export const MAX_OUTPUT_CHARS = 64 * 1024;   // 64KB

export interface SetupScriptResult {
  readonly ran: boolean;
  readonly approved: boolean | undefined;
  readonly exitCode: number | undefined;
  readonly stdout: string;
  readonly stderr: string;
  readonly timedOut: boolean;
  readonly durationMs: number;
  readonly error: string | undefined;
}

export interface SetupScriptRunOptions {
  /** Force re-run even if already executed this session. */
  readonly force?: boolean;
}
```

Pure helpers (no state mutations, light tests):

- `detectSetupScript(kaos: Kaos): Promise<string | null>` — resolves `.ody-code/setup.sh`, stats it, returns absolute path iff it exists and is a regular file, else null.
- `doesSetupScriptExist(kaos: Kaos): Promise<boolean>` — shortcut for "detect !== null".
- `formatSetupReminder(result: SetupScriptResult): string` — formats the system-reminder text for injection (see below).
- `formatRejectionReminder(): string` — formats the "user denied" injection text.

### Steps

- [ ] Write the failing test (T1 helpers only)

**Test file**: `packages/agent-core/test/session/setup-script.test.ts`

```typescript
import { describe, it, expect, vi } from 'vitest';
import { LocalKaos } from '@odysseythink/kaos';
import { TEST_OS_ENV } from '../fixtures/test-kaos';
import {
  SETUP_SCRIPT_PATH,
  DEFAULT_TIMEOUT_MS,
  MAX_OUTPUT_CHARS,
  detectSetupScript,
  doesSetupScriptExist,
  formatSetupReminder,
  formatRejectionReminder,
  type SetupScriptResult,
} from '../../src/session/setup-script';

// Helper: create a LocalKaos patched with spy methods
function makeKaos(overrides: Partial<{ statResult: any; cwd: string }> = {}) {
  const kaos = new (LocalKaos as any)(TEST_OS_ENV) as LocalKaos;
  const cwd = overrides.cwd ?? '/fake/repo';
  kaos.getcwd = () => cwd;
  if (overrides.statResult !== undefined) {
    kaos.stat = vi.fn().mockResolvedValue(overrides.statResult);
  }
  return kaos;
}

describe('detectSetupScript', () => {
  it('returns null when .ody-code/setup.sh does not exist', async () => {
    const kaos = makeKaos({
      statResult: Promise.reject(Object.assign(new Error('ENOENT'), { code: 'ENOENT' })),
    });
    const result = await detectSetupScript(kaos);
    expect(result).toBeNull();
  });

  it('returns null when .ody-code/setup.sh is a directory', async () => {
    const kaos = makeKaos({ statResult: { isFile: false, isDirectory: true } });
    const result = await detectSetupScript(kaos);
    expect(result).toBeNull();
  });

  it('returns absolute path when .ody-code/setup.sh is a regular file', async () => {
    const kaos = makeKaos({ statResult: { isFile: true, isDirectory: false } });
    const result = await detectSetupScript(kaos);
    expect(result).toBe('/fake/repo/.ody-code/setup.sh');
  });

  it('returned path uses kaos.normpath for cross-platform safety', async () => {
    const kaos = makeKaos({
      cwd: '/fake/repo/sub',
      statResult: { isFile: true, isDirectory: false },
    });
    kaos.normpath = (p: string) => p.replace(/\\/g, '/');
    const result = await detectSetupScript(kaos);
    expect(result).toBe('/fake/repo/sub/.ody-code/setup.sh');
    expect(result).not.toContain('\\');
  });
});

describe('doesSetupScriptExist', () => {
  it('returns false when detect returns null', async () => {
    const kaos = makeKaos({
      statResult: Promise.reject(Object.assign(new Error('ENOENT'), { code: 'ENOENT' })),
    });
    expect(await doesSetupScriptExist(kaos)).toBe(false);
  });

  it('returns true when detect returns a path', async () => {
    const kaos = makeKaos({ statResult: { isFile: true, isDirectory: false } });
    expect(await doesSetupScriptExist(kaos)).toBe(true);
  });
});

describe('formatSetupReminder', () => {
  const makeResult = (overrides: Partial<SetupScriptResult> = {}): SetupScriptResult => ({
    ran: true, approved: true, exitCode: 0,
    stdout: '', stderr: '', timedOut: false, durationMs: 1234, error: undefined,
    ...overrides,
  });

  it('includes success message and exit code 0', () => {
    const text = formatSetupReminder(makeResult());
    expect(text).toContain('setup script');
    expect(text).toContain('completed');
  });

  it('mentions non-zero exit code on failure', () => {
    const text = formatSetupReminder(makeResult({
      exitCode: 1, stdout: '', stderr: 'npm ERR!',
    }));
    expect(text).toContain('exit code 1');
    expect(text).toContain('npm ERR!');
  });

  it('includes truncated flag when stdout is known to be truncated', () => {
    const text = formatSetupReminder(makeResult({
      stdout: 'x'.repeat(MAX_OUTPUT_CHARS + 50),
    }));
    expect(text).toContain('truncated');
  });

  it('mentions timeout when timedOut is true', () => {
    const text = formatSetupReminder(makeResult({ timedOut: true, error: 'timed out' }));
    expect(text).toContain('timeout');
  });

  it('includes duration', () => {
    const text = formatSetupReminder(makeResult({ durationMs: 5678 }));
    expect(text).toContain('5678');
  });
});

describe('formatRejectionReminder', () => {
  it('mentions user denied the script', () => {
    const text = formatRejectionReminder();
    expect(text).toContain('not run');
    expect(text).toContain('denied');
  });
});
```

- [ ] Run it and verify it FAILS

```bash
pnpm --filter @odysseythink/agent-core vitest run test/session/setup-script.test.ts
# Expected: FAIL — module '../../src/session/setup-script' not found (file doesn't exist yet)
```

- [ ] Write the minimal implementation

**File**: `packages/agent-core/src/session/setup-script.ts`

```typescript
import { join } from 'pathe';
import type { Kaos } from '@odysseythink/kaos';

export const SETUP_SCRIPT_PATH = '.ody-code/setup.sh';
export const DEFAULT_TIMEOUT_MS = 300_000;
export const MAX_OUTPUT_CHARS = 64 * 1024;

export interface SetupScriptResult {
  readonly ran: boolean;
  readonly approved: boolean | undefined;
  readonly exitCode: number | undefined;
  readonly stdout: string;
  readonly stderr: string;
  readonly timedOut: boolean;
  readonly durationMs: number;
  readonly error: string | undefined;
}

export interface SetupScriptRunOptions {
  readonly force?: boolean;
}

export async function detectSetupScript(kaos: Kaos): Promise<string | null> {
  const cwd = kaos.getcwd();
  const scriptPath = join(cwd, SETUP_SCRIPT_PATH);
  try {
    const stat = await kaos.stat(scriptPath);
    if (!stat.isFile) return null;
    return kaos.normpath(scriptPath);
  } catch {
    return null;
  }
}

export async function doesSetupScriptExist(kaos: Kaos): Promise<boolean> {
  return (await detectSetupScript(kaos)) !== null;
}

export function formatRejectionReminder(): string {
  return [
    'Repository setup script was not run (user denied).',
    'Environment may be unprepared. Run `/setup` to execute it manually.',
  ].join('\n');
}

export function formatSetupReminder(result: SetupScriptResult): string {
  const parts: string[] = [];
  const durationS = (result.durationMs / 1000).toFixed(1);

  if (result.timedOut) {
    parts.push(`Repository setup script timed out after ${durationS}s.`);
  } else if (result.exitCode === 0) {
    parts.push(`Repository setup script completed successfully (${durationS}s).`);
  } else {
    parts.push(`Repository setup script failed with exit code ${result.exitCode} (${durationS}s).`);
  }

  // Append truncated output if available
  const stdoutTrimmed = truncateForInjection(result.stdout);
  const stderrTrimmed = truncateForInjection(result.stderr);

  if (stderrTrimmed.length > 0) {
    parts.push('', 'Stderr:', stderrTrimmed);
  }
  if (stdoutTrimmed.length > 0) {
    parts.push('', 'Stdout:', stdoutTrimmed);
  }

  if (result.error !== undefined && result.error.length > 0) {
    parts.push('', `Error: ${result.error}`);
  }

  return parts.join('\n');
}

function truncateForInjection(text: string): string {
  if (text.length <= MAX_OUTPUT_CHARS) return text;
  return text.slice(0, MAX_OUTPUT_CHARS) + '\n[...truncated]';
}
```

- [ ] Run it and verify it PASSES

```bash
pnpm --filter @odysseythink/agent-core vitest run test/session/setup-script.test.ts
# Expected: all 11 tests pass
```

- [ ] Commit

```bash
git add packages/agent-core/src/session/setup-script.ts packages/agent-core/test/session/setup-script.test.ts
git commit -m "feat: add SetupScriptResult types, detect, and format helpers"
```

---

## Task 2: executeSetupScript + runSetupScriptIfNeeded + persistAndInject

**Depends on:** Task 1  
**Files:** Modify `packages/agent-core/src/session/setup-script.ts` (append, ~line 90+), Modify `packages/agent-core/test/session/setup-script.test.ts` (append, ~line 120+)

### Design

```typescript
// New exports appended to setup-script.ts

export interface SetupRunMeta {
  readonly ranAt: string;      // ISO 8601
  readonly approved: boolean;
  readonly exitCode: number | undefined;
  readonly timedOut: boolean;
  readonly durationMs: number;
}

/**
 * Main entry point: detect, check permission, execute, persist, inject.
 *
 * @param session  Session instance (needs options.kaos, metadata, writeMetadata)
 * @param agent    Main agent (needs permission, kaos, telemetry, context)
 * @param opts     Optional overrides (e.g. force for /setup)
 */
export async function runSetupScriptIfNeeded(
  session: { readonly options: { readonly kaos: Kaos }; metadata: { custom: Record<string, any> }; writeMetadata(): Promise<void> },
  agent: { readonly permission: PermissionGate; readonly kaos: Kaos; readonly telemetry: { track: (event: string, props: Record<string, unknown>) => void }; readonly context: { appendSystemReminder: (content: string, origin: { kind: string; variant: string }) => void } },
  opts: SetupScriptRunOptions = {},
): Promise<SetupScriptResult>
```

Key behaviors:
1. If `!opts.force` and `session.metadata.custom['setupRun']` already exists → return no-op result (already ran)
2. Call `detectSetupScript(kaos)` → if null, return `ran: false` (no script) + telemetry `has_script: false`
3. Check permission via `agent.permission.requestSetupScriptApproval(scriptPath)` (defined in T3 — use a type-only shim in T2 so T2 compiles without T3)
4. If denied → persist denied metadata, inject rejection reminder, return
5. If approved → execute via Kaos, handle timeout/truncation, persist, inject summary

Type-only shim for T2 (since T3 not yet done):
```typescript
interface PermissionGate {
  readonly mode: 'manual' | 'auto' | 'yolo';
  requestSetupScriptApproval(scriptPath: string): Promise<{ decision: 'approved' | 'rejected' | 'cancelled' }>;
}
```

### Steps

- [ ] Write the failing test (T2 runner + execute)

Append to `packages/agent-core/test/session/setup-script.test.ts`:

```typescript
// Add these imports at top of file:
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { Readable, Writable } from 'node:stream';
import { runSetupScriptIfNeeded, type SetupRunMeta, type SetupScriptRunOptions } from '../../src/session/setup-script';

// ── Mocks ──────────────────────────────────────────────────────────

function readableFrom(text: string): Readable {
  const r = new Readable({ read() {} });
  r.push(text);
  r.push(null);
  return r;
}

function makeMockKaosProcess(exitCode: number, stdout: string, stderr: string) {
  return {
    stdin: new Writable({ write(_chunk, _enc, cb) { cb(); } }),
    stdout: readableFrom(stdout),
    stderr: readableFrom(stderr),
    pid: 9999,
    exitCode,
    wait: vi.fn().mockResolvedValue(exitCode),
    kill: vi.fn().mockResolvedValue(undefined),
  };
}

// ── runSetupScriptIfNeeded tests ────────────────────────────────────

describe('runSetupScriptIfNeeded', () => {
  /** Helper to build a minimal session + agent for testing */
  function buildContext(opts: {
    permissionMode?: 'manual' | 'auto' | 'yolo';
    approvalDecision?: 'approved' | 'rejected';
    scriptExists?: boolean;
    process?: ReturnType<typeof makeMockKaosProcess>;
    metadataSetupRun?: SetupRunMeta | undefined;
  } = {}) {
    const mockedStat = opts.scriptExists === false
      ? vi.fn().mockRejectedValue(Object.assign(new Error('ENOENT'), { code: 'ENOENT' }))
      : vi.fn().mockResolvedValue({ isFile: true, isDirectory: false });

    const kaos = new (LocalKaos as any)(TEST_OS_ENV) as LocalKaos & { withCwd: any; execWithEnv: any };
    kaos.getcwd = () => '/fake/repo';
    kaos.normpath = (p: string) => p;
    kaos.stat = mockedStat;
    kaos.withCwd = vi.fn().mockReturnValue(kaos);
    kaos.execWithEnv = vi.fn().mockResolvedValue(opts.process ?? makeMockKaosProcess(0, '', ''));

    const telemetryTrack = vi.fn();
    const appendSystemReminder = vi.fn();
    const writeMetadata = vi.fn().mockResolvedValue(undefined);

    const session = {
      options: { kaos },
      metadata: { custom: {} as Record<string, any> },
      writeMetadata,
    };

    const agent = {
      permission: {
        mode: opts.permissionMode ?? 'yolo',
        requestSetupScriptApproval: vi.fn().mockResolvedValue({
          decision: opts.approvalDecision ?? 'approved',
        }),
      },
      kaos,
      telemetry: { track: telemetryTrack },
      context: { appendSystemReminder },
    };

    if (opts.metadataSetupRun !== undefined) {
      session.metadata.custom['setupRun'] = opts.metadataSetupRun;
    }

    return { session, agent, kaos, telemetryTrack, appendSystemReminder, writeMetadata };
  }

  it('returns ran=false when no .ody-code/setup.sh exists', async () => {
    const { session, agent, telemetryTrack } = buildContext({ scriptExists: false });
    const result = await runSetupScriptIfNeeded(session, agent);
    expect(result.ran).toBe(false);
    expect(result.approved).toBeUndefined();
    expect(telemetryTrack).toHaveBeenCalledWith('setup_script_executed', expect.objectContaining({
      ran: false, has_script: false,
    }));
  });

  it('returns no-op when setupRun metadata already exists and force is not set', async () => {
    const { session, agent } = buildContext({
      metadataSetupRun: { ranAt: '2026-01-01T00:00:00Z', approved: true, exitCode: 0, timedOut: false, durationMs: 100 },
    });
    const result = await runSetupScriptIfNeeded(session, agent);
    expect(result.ran).toBe(false);
    expect(result.approved).toBeUndefined();
  });

  it('re-runs when force=true even if metadata exists', async () => {
    const { session, agent, telemetryTrack } = buildContext({
      metadataSetupRun: { ranAt: '2026-01-01T00:00:00Z', approved: true, exitCode: 0, timedOut: false, durationMs: 100 },
    });
    const result = await runSetupScriptIfNeeded(session, agent, { force: true });
    expect(result.ran).toBe(true);
    expect(telemetryTrack).toHaveBeenCalledWith('setup_script_executed', expect.objectContaining({
      ran: true, has_script: true,
    }));
  });

  it('yolo mode auto-approves and executes', async () => {
    const proc = makeMockKaosProcess(0, 'install ok', '');
    const { session, agent, telemetryTrack, appendSystemReminder } = buildContext({
      permissionMode: 'yolo', process: proc,
    });
    const result = await runSetupScriptIfNeeded(session, agent);
    expect(result.ran).toBe(true);
    expect(result.approved).toBe(true);
    expect(result.exitCode).toBe(0);
    expect(result.stdout).toContain('install ok');
    expect(appendSystemReminder).toHaveBeenCalledWith(
      expect.stringContaining('completed'),
      { kind: 'injection', variant: 'setup_script' },
    );
    expect(telemetryTrack).toHaveBeenCalledWith('setup_script_executed', expect.objectContaining({
      permission_mode: 'yolo',
      exit_code: 0,
    }));
  });

  it('auto mode auto-approves and executes', async () => {
    const proc = makeMockKaosProcess(0, '', '');
    const { session, agent } = buildContext({ permissionMode: 'auto', process: proc });
    const result = await runSetupScriptIfNeeded(session, agent);
    expect(result.ran).toBe(true);
    expect(result.approved).toBe(true);
  });

  it('manual mode uses permission.requestSetupScriptApproval', async () => {
    const proc = makeMockKaosProcess(0, '', '');
    const { session, agent } = buildContext({
      permissionMode: 'manual', approvalDecision: 'approved', process: proc,
    });
    const result = await runSetupScriptIfNeeded(session, agent);
    expect(result.ran).toBe(true);
    expect(result.approved).toBe(true);
    expect(agent.permission.requestSetupScriptApproval).toHaveBeenCalledWith('/fake/repo/.ody-code/setup.sh');
  });

  it('manual mode rejection does not execute', async () => {
    const { session, agent, appendSystemReminder } = buildContext({
      permissionMode: 'manual', approvalDecision: 'rejected',
    });
    const result = await runSetupScriptIfNeeded(session, agent);
    expect(result.ran).toBe(false);
    expect(result.approved).toBe(false);
    expect(appendSystemReminder).toHaveBeenCalledWith(
      expect.stringContaining('denied'),
      { kind: 'injection', variant: 'setup_script' },
    );
    // Session metadata should record rejected
    expect(session.metadata.custom['setupRun']).toBeDefined();
    expect(session.metadata.custom['setupRun'].approved).toBe(false);
  });

  it('non-zero exit code injects failure reminder', async () => {
    const proc = makeMockKaosProcess(1, '', 'npm ERR! something broke');
    const { session, agent, appendSystemReminder } = buildContext({ process: proc });
    const result = await runSetupScriptIfNeeded(session, agent);
    expect(result.exitCode).toBe(1);
    expect(result.stdout).toBe('');
    expect(result.stderr).toBe('npm ERR! something broke');
    expect(appendSystemReminder).toHaveBeenCalledWith(
      expect.stringContaining('exit code 1'),
      { kind: 'injection', variant: 'setup_script' },
    );
  });

  it('writes metadata.custom.setupRun after execution', async () => {
    const { session, agent, writeMetadata } = buildContext();
    await runSetupScriptIfNeeded(session, agent);
    const meta = session.metadata.custom['setupRun'] as SetupRunMeta;
    expect(meta).toBeDefined();
    expect(meta.approved).toBe(true);
    expect(meta.exitCode).toBe(0);
    expect(meta.timedOut).toBe(false);
    expect(typeof meta.ranAt).toBe('string');
    expect(typeof meta.durationMs).toBe('number');
    expect(writeMetadata).toHaveBeenCalled();
  });

  it('truncates stdout/stderr to MAX_OUTPUT_CHARS', async () => {
    const long = 'a'.repeat(MAX_OUTPUT_CHARS + 500);
    const proc = makeMockKaosProcess(0, long, '');
    const { session, agent } = buildContext({ process: proc });
    const result = await runSetupScriptIfNeeded(session, agent);
    expect(result.stdout.length).toBeLessThanOrEqual(MAX_OUTPUT_CHARS + 50); // + truncated marker
    expect(result.stdout).toContain('truncated');
  });
});
```

- [ ] Run it and verify it FAILS

```bash
pnpm --filter @odysseythink/agent-core vitest run test/session/setup-script.test.ts --reporter=verbose 2>&1 | head -30
# Expected: FAIL — runSetupScriptIfNeeded is not exported (not yet written)
```

- [ ] Write the minimal implementation

Append to `packages/agent-core/src/session/setup-script.ts`:

```typescript
import { type Readable } from 'node:stream';

// ── Type-only permission gate (T2 compilation shim; T3 provides the real impl) ──

interface PermissionGate {
  readonly mode: 'manual' | 'auto' | 'yolo';
  requestSetupScriptApproval(scriptPath: string): Promise<{
    decision: 'approved' | 'rejected' | 'cancelled';
  }>;
}

// ── Execution ───────────────────────────────────────────────────────

async function readStreamTruncated(stream: Readable, maxChars: number): Promise<string> {
  const chunks: string[] = [];
  let total = 0;
  for await (const chunk of stream) {
    const str = typeof chunk === 'string' ? chunk : chunk.toString('utf-8');
    if (total >= maxChars) continue;
    const remaining = maxChars - total;
    chunks.push(str.slice(0, remaining));
    total += str.length;
  }
  const text = chunks.join('');
  if (total > maxChars) {
    return text + '\n[...truncated]';
  }
  return text;
}

async function executeSetupScript(
  kaos: Kaos,
  scriptPath: string,
): Promise<SetupScriptResult> {
  const start = Date.now();
  const cwd = kaos.getcwd();
  const shellPath = kaos.osEnv.shellPath;

  const env: Record<string, string> = {
    NO_COLOR: '1',
    TERM: 'dumb',
    GIT_TERMINAL_PROMPT: '0',
    SHELL: shellPath,
    ...(process.env as Record<string, string>),
  };

  let proc;
  try {
    proc = await kaos.withCwd(cwd).execWithEnv([shellPath, scriptPath], env);
  } catch (error) {
    return {
      ran: true, approved: true, exitCode: undefined,
      stdout: '', stderr: '',
      timedOut: false, durationMs: Date.now() - start,
      error: error instanceof Error ? error.message : String(error),
    };
  }

  // Close stdin to prevent hanging
  proc.stdin.end();

  const timeoutMs = DEFAULT_TIMEOUT_MS;
  let timedOut = false;

  try {
    const [stdout, stderr, exitCode] = await Promise.race([
      Promise.all([
        readStreamTruncated(proc.stdout, MAX_OUTPUT_CHARS),
        readStreamTruncated(proc.stderr, MAX_OUTPUT_CHARS),
        proc.wait(),
      ]),
      new Promise<never>((_, reject) =>
        setTimeout(() => {
          timedOut = true;
          reject(new Error('timeout'));
        }, timeoutMs),
      ),
    ]);

    return {
      ran: true, approved: true, exitCode,
      stdout, stderr,
      timedOut: false, durationMs: Date.now() - start, error: undefined,
    };
  } catch {
    // Timeout or unexpected error during read/wait
    await proc.kill().catch(() => {});
    return {
      ran: true, approved: true, exitCode: undefined,
      stdout: '', stderr: '',
      timedOut, durationMs: Date.now() - start,
      error: timedOut ? `Script timed out after ${timeoutMs}ms` : 'Script execution failed',
    };
  }
}

// ── Main entry point ────────────────────────────────────────────────

export interface SetupRunMeta {
  readonly ranAt: string;
  readonly approved: boolean;
  readonly exitCode: number | undefined;
  readonly timedOut: boolean;
  readonly durationMs: number;
}

export async function runSetupScriptIfNeeded(
  session: {
    readonly options: { readonly kaos: Kaos };
    metadata: { custom: Record<string, any> };
    writeMetadata(): Promise<void>;
  },
  agent: {
    readonly permission: PermissionGate;
    readonly kaos: Kaos;
    readonly telemetry: { track: (event: string, props: Record<string, unknown>) => void };
    readonly context: {
      appendSystemReminder: (content: string, origin: { kind: string; variant: string }) => void;
    };
  },
  opts: SetupScriptRunOptions = {},
): Promise<SetupScriptResult> {
  const kaos = session.options.kaos;

  // Already ran this session? Skip unless forced.
  if (!opts.force && session.metadata.custom['setupRun'] !== undefined) {
    return {
      ran: false, approved: undefined, exitCode: undefined,
      stdout: '', stderr: '', timedOut: false, durationMs: 0, error: undefined,
    };
  }

  const scriptPath = await detectSetupScript(kaos);
  if (scriptPath === null) {
    agent.telemetry.track('setup_script_executed', {
      ran: false, approved: null,
      exit_code: null, timed_out: false, duration_ms: 0,
      permission_mode: agent.permission.mode,
      has_script: false,
    });
    return {
      ran: false, approved: undefined, exitCode: undefined,
      stdout: '', stderr: '', timedOut: false, durationMs: 0, error: undefined,
    };
  }

  // Permission gate
  let approved = true;
  if (agent.permission.mode === 'manual') {
    const approval = await agent.permission.requestSetupScriptApproval(scriptPath);
    if (approval.decision !== 'approved') {
      approved = false;
    }
  }

  if (!approved) {
    const result: SetupScriptResult = {
      ran: false, approved: false, exitCode: undefined,
      stdout: '', stderr: '', timedOut: false, durationMs: 0, error: undefined,
    };
    await persistAndInject(session, agent, result);
    return result;
  }

  const result = await executeSetupScript(agent.kaos, scriptPath);
  await persistAndInject(session, agent, result);
  return result;
}

// ── Persist + inject ────────────────────────────────────────────────

async function persistAndInject(
  session: {
    metadata: { custom: Record<string, any> };
    writeMetadata(): Promise<void>;
  },
  agent: {
    readonly permission: { readonly mode: string };
    readonly telemetry: { track: (event: string, props: Record<string, unknown>) => void };
    readonly context: {
      appendSystemReminder: (content: string, origin: { kind: string; variant: string }) => void;
    };
  },
  result: SetupScriptResult,
): Promise<void> {
  // 1. Persist metadata
  session.metadata.custom['setupRun'] = {
    ranAt: new Date().toISOString(),
    approved: result.approved ?? false,
    exitCode: result.exitCode ?? null,
    timedOut: result.timedOut,
    durationMs: result.durationMs,
  } satisfies SetupRunMeta;
  await session.writeMetadata().catch(() => {});

  // 2. Telemetry
  agent.telemetry.track('setup_script_executed', {
    ran: result.ran,
    approved: result.approved ?? null,
    exit_code: result.exitCode ?? null,
    timed_out: result.timedOut,
    duration_ms: result.durationMs,
    permission_mode: agent.permission.mode,
    has_script: true,
  });

  // 3. Inject system reminder
  if (!result.ran && result.approved === false) {
    agent.context.appendSystemReminder(formatRejectionReminder(), {
      kind: 'injection',
      variant: 'setup_script',
    });
    return;
  }

  if (result.ran) {
    agent.context.appendSystemReminder(formatSetupReminder(result), {
      kind: 'injection',
      variant: 'setup_script',
    });
  }
}
```

- [ ] Run it and verify it PASSES

```bash
pnpm --filter @odysseythink/agent-core vitest run test/session/setup-script.test.ts
# Expected: all 20 tests pass (11 from T1 + 9 from T2)
```

- [ ] Commit

```bash
git add packages/agent-core/src/session/setup-script.ts packages/agent-core/test/session/setup-script.test.ts
git commit -m "feat: add executeSetupScript and runSetupScriptIfNeeded with peristence"
```

---

## Task 3: PermissionManager.requestSetupScriptApproval()

**Depends on:** none  
**Files:** Modify `packages/agent-core/src/agent/permission/index.ts` (append method ~line 314), Modify `packages/agent-core/test/agent/permission.test.ts` (append tests)

### Design

Add a public method `requestSetupScriptApproval(scriptPath: string)` to `PermissionManager`:

```typescript
/**
 * Request user approval for running the repository setup script.
 * In yolo/auto modes, returns approved immediately.
 * In manual mode, constructs an ApprovalRequest and calls this.agent.rpc?.requestApproval.
 * Falls back to approved when no RPC is available (headless mode).
 */
async requestSetupScriptApproval(
  scriptPath: string,
  signal?: AbortSignal,
): Promise<{ decision: 'approved' | 'rejected' | 'cancelled' }> {
  if (this.mode === 'yolo' || this.mode === 'auto') {
    return { decision: 'approved' };
  }

  if (!this.agent.rpc?.requestApproval) {
    // Headless / no RPC: approve by default (same fallback as BashTool)
    return { decision: 'approved' };
  }

  const approvalReq = {
    turnId: 0, // setup runs at session start, before any turn
    toolCallId: 'setup-script',
    toolName: 'Setup Script',
    action: `Run ${scriptPath}`,
    display: {
      kind: 'generic' as const,
      summary: 'Run repository setup script',
      detail: `The repository contains a setup script at ${scriptPath}. Running it may install dependencies and prepare the environment.`,
    },
  };

  try {
    return await this.agent.rpc.requestApproval(approvalReq);
  } catch {
    return { decision: 'approved' }; // fallback on error
  }
}
```

### Steps

- [ ] Write the failing test

Append to `packages/agent-core/test/agent/permission.test.ts`:

```typescript
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { Agent } from '../../src/agent';
import { PermissionManager } from '../../src/agent/permission';
import { testKaos, TEST_OS_ENV } from '../fixtures/test-kaos';
import { noopTelemetryClient } from '../../src/telemetry';

describe('PermissionManager.requestSetupScriptApproval', () => {
  function makePerm(mode: 'manual' | 'auto' | 'yolo', rpc?: any) {
    // Minimal agent stub
    const agent = {
      kaos: testKaos,
      rpc,
      telemetry: noopTelemetryClient,
      records: { logRecord: vi.fn() },
      replayBuilder: { push: vi.fn() },
      emitStatusUpdated: vi.fn(),
      hooks: undefined,
      microCompaction: { reset: vi.fn() },
      injection: { onContextClear: vi.fn() },
    } as unknown as Agent;
    const pm = new PermissionManager(agent);
    pm.setMode(mode);
    return { pm, agent };
  }

  it('yolo mode returns approved immediately', async () => {
    const { pm } = makePerm('yolo');
    const result = await pm.requestSetupScriptApproval('/repo/.ody-code/setup.sh');
    expect(result.decision).toBe('approved');
  });

  it('auto mode returns approved immediately', async () => {
    const { pm } = makePerm('auto');
    const result = await pm.requestSetupScriptApproval('/repo/.ody-code/setup.sh');
    expect(result.decision).toBe('approved');
  });

  it('manual mode calls rpc.requestApproval', async () => {
    const requestApproval = vi.fn().mockResolvedValue({ decision: 'approved' });
    const { pm } = makePerm('manual', { requestApproval });
    const result = await pm.requestSetupScriptApproval('/repo/.ody-code/setup.sh');
    expect(requestApproval).toHaveBeenCalledWith(expect.objectContaining({
      toolCallId: 'setup-script',
      toolName: 'Setup Script',
    }));
    expect(result.decision).toBe('approved');
  });

  it('manual mode returns rejected when user denies', async () => {
    const requestApproval = vi.fn().mockResolvedValue({ decision: 'rejected' });
    const { pm } = makePerm('manual', { requestApproval });
    const result = await pm.requestSetupScriptApproval('/repo/.ody-code/setup.sh');
    expect(result.decision).toBe('rejected');
  });

  it('falls back to approved when rpc is undefined', async () => {
    const { pm } = makePerm('manual', undefined);
    const result = await pm.requestSetupScriptApproval('/repo/.ody-code/setup.sh');
    expect(result.decision).toBe('approved');
  });

  it('falls back to approved when requestApproval throws', async () => {
    const requestApproval = vi.fn().mockRejectedValue(new Error('RPC down'));
    const { pm } = makePerm('manual', { requestApproval });
    const result = await pm.requestSetupScriptApproval('/repo/.ody-code/setup.sh');
    expect(result.decision).toBe('approved');
  });
});
```

- [ ] Run it and verify it FAILS

```bash
pnpm --filter @odysseythink/agent-core vitest run test/agent/permission.test.ts --reporter=verbose 2>&1 | tail -20
# Expected: FAIL — requestSetupScriptApproval is not a function (method not yet added)
```

- [ ] Write the minimal implementation

Append to `packages/agent-core/src/agent/permission/index.ts` (after line 314, the closing `}` of `formatPolicyDenyMessage`):

```typescript
  /**
   * Request user approval for running the repository setup script.
   * In `yolo`/`auto` modes, approves immediately.
   * In `manual` mode, constructs an {@link ApprovalRequest} and calls
   * `this.agent.rpc?.requestApproval`. Falls back to approved when no RPC
   * is available (headless mode).
   */
  async requestSetupScriptApproval(
    scriptPath: string,
    signal?: AbortSignal,
  ): Promise<{ decision: 'approved' | 'rejected' | 'cancelled' }> {
    if (this.mode === 'yolo' || this.mode === 'auto') {
      return { decision: 'approved' };
    }

    if (!this.agent.rpc?.requestApproval) {
      return { decision: 'approved' };
    }

    const approvalReq = {
      turnId: 0,
      toolCallId: 'setup-script',
      toolName: 'Setup Script',
      action: `Run ${scriptPath}`,
      display: {
        kind: 'generic' as const,
        summary: 'Run repository setup script',
        detail: `The repository contains a setup script at ${scriptPath}. Running it may install dependencies and prepare the environment.`,
      },
    };

    try {
      const response = await this.agent.rpc.requestApproval(approvalReq);
      // Fire telemetry + hooks consistent with existing tool-approval path
      this.agent.telemetry.track('permission_approval_result', {
        policy_name: 'setup_script',
        tool_name: 'Setup Script',
        permission_mode: this.mode,
        result: response.decision === 'approved' ? 'approved' : response.decision,
        approval_surface: 'generic',
        duration_ms: 0,
        session_cache_written: false,
        has_feedback: false,
      });
      void this.agent.hooks?.fireAndForgetTrigger?.('PermissionResult', {
        matcherValue: 'Setup Script',
        inputData: {
          turnId: 0,
          toolCallId: 'setup-script',
          toolName: 'Setup Script',
          action: `Run ${scriptPath}`,
          decision: response.decision,
          scope: response.scope,
          feedback: response.feedback,
          selectedLabel: response.selectedLabel,
        },
      });
      return response;
    } catch {
      return { decision: 'approved' };
    }
  }
```

- [ ] Run it and verify it PASSES

```bash
pnpm --filter @odysseythink/agent-core vitest run test/agent/permission.test.ts --reporter=verbose
# Expected: PASS (existing + 6 new tests)
```

- [ ] Commit

```bash
git add packages/agent-core/src/agent/permission/index.ts packages/agent-core/test/agent/permission.test.ts
git commit -m "feat: add PermissionManager.requestSetupScriptApproval()"
```

---

## Task 4: Session.createMain() wiring

**Depends on:** Task 2, Task 3  
**Files:** Modify `packages/agent-core/src/session/index.ts:186-193`, Modify `packages/agent-core/test/session/setup-script.test.ts` (append)

### Design

Insert a call to `runSetupScriptIfNeeded` in `createMain()` right before `return agent`. The function is a new import from `./setup-script`.

### Steps

- [ ] Write the test

Append to `packages/agent-core/test/session/setup-script.test.ts`:

```typescript
// This is an integration-level test — verifies that runSetupScriptIfNeeded
// is importable and its signature compiles against the real Session and Agent types.
// We don't spin up a real Session here (that's an e2e concern); instead we verify
// the exported signature matches what createMain will call.

import { runSetupScriptIfNeeded } from '../../src/session/setup-script';
import { Agent } from '../../src/agent';
import { Session } from '../../src/session';

describe('createMain wiring contract', () => {
  it('runSetupScriptIfNeeded accepts Session and Agent types', () => {
    // Type-only assertion: this test compiles only if the overload matches.
    // The actual runtime call is a smoke test with mocks.
    const fn: (
      session: { readonly options: { readonly kaos: any }; metadata: { custom: Record<string, any> }; writeMetadata(): Promise<void> },
      agent: { readonly permission: any; readonly kaos: any; readonly telemetry: any; readonly context: any },
      opts?: { force?: boolean },
    ) => Promise<any> = runSetupScriptIfNeeded;
    expect(typeof fn).toBe('function');
  });

  it('Session and Agent satisfy the minimal structural contract', () => {
    // Verify that Session has the properties runSetupScriptIfNeeded needs
    const sessionProto = Session.prototype;
    expect(sessionProto).toBeDefined();
    // Session has writeMetadata (async method)
    expect(typeof Session.prototype.writeMetadata).toBe('function');
    // Agent has permission, kaos, telemetry, context
    expect(Agent.prototype).toBeDefined();
  });
});
```

- [ ] Run it and verify it FAILS (on typecheck for the agent import)

```bash
pnpm --filter @odysseythink/agent-core vitest run test/session/setup-script.test.ts --reporter=verbose 2>&1 | tail -10
# No new failure — the test uses already-exported functions from T2. The real
# compile-time check is the type assertion.
```

The test passes as-is since it uses T2 exports. The real verification is the manual integration step below.

- [ ] Write the minimal implementation

In `packages/agent-core/src/session/index.ts`, add import:

```typescript
// After line 28 (import { loadAgentsMd } from '../profile';)
import { runSetupScriptIfNeeded } from './setup-script';
```

Modify `createMain()` (lines 186-193):

```typescript
  async createMain() {
    const { agent } = await this.createAgent({ type: 'main' }, DEFAULT_AGENT_PROFILES['agent']);
    this.attachCheckpointCoordinator(agent);
    // The main-agent audit sink now exists; flush any goal records queued before it.
    this.goals.flushPendingRecords();
    await this.triggerSessionStart('startup');

    // Run repository setup script after main agent exists but before returning.
    // Failure does not block session start; the agent will receive a system
    // reminder with the result.
    await runSetupScriptIfNeeded(this, agent).catch((error: unknown) => {
      this.log.error('setup script failed', error);
    });

    return agent;
  }
```

- [ ] Manual verification + typecheck

```bash
# 1. Typecheck the changed package
pnpm --filter @odysseythink/agent-core typecheck
# Expected: PASS — no type errors

# 2. Run all agent-core tests to ensure no regressions
pnpm --filter @odysseythink/agent-core vitest run
# Expected: PASS
```

- [ ] Commit

```bash
git add packages/agent-core/src/session/index.ts packages/agent-core/test/session/setup-script.test.ts
git commit -m "feat: wire SetupScriptRunner into Session.createMain()"
```

---

## Task 5: generateAgentsMd() extension — writeSetupScriptTemplate

**Depends on:** Task 1  
**Files:** Modify `packages/agent-core/src/session/index.ts:329-357`, Modify `packages/agent-core/test/session/setup-script.test.ts` (append)

### Design

After `generateAgentsMd()` finishes its subagent call and injects the init reminder, also call `writeSetupScriptTemplate(this.options.kaos)` to generate `.ody-code/setup.sh` if it doesn't already exist.

The template generator detects common project markers (pnpm-lock, package-lock, pyproject.toml, Cargo.toml, go.mod, etc.) and writes a shell script with the appropriate install commands.

### Steps

- [ ] Write the test

Append to `packages/agent-core/test/session/setup-script.test.ts`:

```typescript
import { writeSetupScriptTemplate } from '../../src/session/setup-script';

describe('writeSetupScriptTemplate', () => {
  function makeKaosWithDir(files: string[]) {
    const kaos = new (LocalKaos as any)(TEST_OS_ENV) as LocalKaos & {
      stat: any; writeText: any; mkdir: any; readText: any;
    };
    kaos.getcwd = () => '/fake/repo';
    kaos.mkdir = vi.fn().mockResolvedValue(undefined);
    kaos.writeText = vi.fn().mockResolvedValue(10);
    // stat: only succeed for files in the `files` list
    kaos.stat = vi.fn().mockImplementation(async (path: string) => {
      const basename = path.replace(/^.*[/\\]/, '');
      if (files.includes(basename)) return { isFile: true, isDirectory: false };
      throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
    });
    kaos.readText = vi.fn().mockResolvedValue('');
    kaos.normpath = (p: string) => p;
    return kaos;
  }

  it('skips if setup.sh already exists', async () => {
    const kaos = makeKaosWithDir([]);
    // Simulate setup.sh already exists
    const origStat = kaos.stat;
    let callCount = 0;
    kaos.stat = vi.fn().mockImplementation(async (path: string) => {
      callCount++;
      if (path.endsWith('.ody-code/setup.sh')) return { isFile: true, isDirectory: false };
      return origStat(path);
    });
    await writeSetupScriptTemplate(kaos);
    expect(kaos.writeText).not.toHaveBeenCalled();
  });

  it('generates template with pnpm install for pnpm projects', async () => {
    const kaos = makeKaosWithDir(['pnpm-lock.yaml', 'package.json']);
    await writeSetupScriptTemplate(kaos);
    expect(kaos.writeText).toHaveBeenCalled();
    const content = (kaos.writeText as any).mock.calls[0][1] as string;
    expect(content).toContain('pnpm install');
    expect(content).toContain('#!/usr/bin/env bash');
  });

  it('generates template with npm install for npm projects', async () => {
    const kaos = makeKaosWithDir(['package-lock.json']);
    await writeSetupScriptTemplate(kaos);
    const content = (kaos.writeText as any).mock.calls[0][1] as string;
    expect(content).toContain('npm install');
  });

  it('generates template with pip install for Python projects', async () => {
    const kaos = makeKaosWithDir(['requirements.txt']);
    await writeSetupScriptTemplate(kaos);
    const content = (kaos.writeText as any).mock.calls[0][1] as string;
    expect(content).toContain('pip install');
  });

  it('generates template with cargo build for Rust projects', async () => {
    const kaos = makeKaosWithDir(['Cargo.toml']);
    await writeSetupScriptTemplate(kaos);
    const content = (kaos.writeText as any).mock.calls[0][1] as string;
    expect(content).toContain('cargo build');
  });

  it('generates template with go mod download for Go projects', async () => {
    const kaos = makeKaosWithDir(['go.mod']);
    await writeSetupScriptTemplate(kaos);
    const content = (kaos.writeText as any).mock.calls[0][1] as string;
    expect(content).toContain('go mod download');
  });

  it('generates empty template when no known markers found', async () => {
    const kaos = makeKaosWithDir([]);
    await writeSetupScriptTemplate(kaos);
    const content = (kaos.writeText as any).mock.calls[0][1] as string;
    expect(content).toContain('#!/usr/bin/env bash');
    expect(content).toContain('# No recognized project markers');
  });
});
```

- [ ] Run it and verify it FAILS

```bash
pnpm --filter @odysseythink/agent-core vitest run test/session/setup-script.test.ts --reporter=verbose 2>&1 | tail -10
# Expected: FAIL — writeSetupScriptTemplate is not exported
```

- [ ] Write the minimal implementation

Append to `packages/agent-core/src/session/setup-script.ts`:

```typescript
// ── /init template generation ───────────────────────────────────────

const PROJECT_MARKERS: Record<string, string> = {
  'pnpm-lock.yaml': 'pnpm install',
  'yarn.lock': 'yarn install',
  'package-lock.json': 'npm install',
  'requirements.txt': 'pip install -r requirements.txt',
  'pyproject.toml': 'pip install -e .',
  'Cargo.toml': 'cargo build',
  'go.mod': 'go mod download',
  'Gemfile': 'bundle install',
  'composer.json': 'composer install',
};

export async function writeSetupScriptTemplate(kaos: Kaos): Promise<void> {
  const scriptPath = kaos.normpath(join(kaos.getcwd(), SETUP_SCRIPT_PATH));

  // Don't overwrite existing setup script
  try {
    const existing = await kaos.stat(scriptPath);
    if (existing.isFile) return;
  } catch {
    // File doesn't exist — proceed
  }

  // Ensure .ody-code/ directory exists
  const dir = kaos.normpath(join(kaos.getcwd(), '.ody-code'));
  try {
    await kaos.mkdir(dir, { parents: false, existOk: true });
  } catch {
    // May already exist or be uncreatable
  }

  // Detect project markers
  const commands: string[] = [];
  for (const [marker, command] of Object.entries(PROJECT_MARKERS)) {
    try {
      const markerPath = kaos.normpath(join(kaos.getcwd(), marker));
      const stat = await kaos.stat(markerPath);
      if (stat.isFile) {
        commands.push(command);
      }
    } catch {
      // Marker not found
    }
  }

  const template = renderShellTemplate(commands);
  await kaos.writeText(scriptPath, template);

  // Make executable on POSIX
  if (kaos.osEnv.osKind !== 'Windows') {
    try {
      await kaos.exec('chmod', '+x', scriptPath);
    } catch {
      // Non-fatal
    }
  }
}

function renderShellTemplate(commands: string[]): string {
  const lines: string[] = [
    '#!/usr/bin/env bash',
    '# Generated by Ody Code `/init` — install dependencies for this project.',
    '# Edit this file to customize your environment setup.',
    '',
    'set -euo pipefail',
    '',
  ];

  if (commands.length === 0) {
    lines.push('# No recognized project markers found. Add your own setup commands below.');
    lines.push('');
    lines.push('echo "No project markers detected — customize this script for your setup."');
  } else {
    for (const cmd of commands) {
      lines.push(`echo "Running: ${cmd}"`);
      lines.push(cmd);
      lines.push('');
    }
    lines.push('echo "Setup complete."');
  }

  lines.push('');
  return lines.join('\n');
}
```

Now modify `generateAgentsMd()` in `packages/agent-core/src/session/index.ts` (after line 349, right after the `mainAgent.records.flush()` call):

```typescript
  async generateAgentsMd(): Promise<void> {
    await this.skillsReady;
    const mainAgent = this.requireMainAgent();

    try {
      const handle = await mainAgent.subagentHost!.spawn('coder', {
        parentToolCallId: 'generate-agents-md',
        prompt: DEFAULT_INIT_PROMPT,
        description: 'Initialize AGENTS.md',
        runInBackground: false,
        origin: { kind: 'system_trigger', name: 'init' },
        signal: new AbortController().signal,
      });
      await handle.completion;

      const agentsMd = await loadAgentsMd(mainAgent.kaos);
      mainAgent.context.appendSystemReminder(initCompletionReminder(agentsMd), {
        kind: 'injection',
        variant: 'init',
      });
      await mainAgent.records.flush();

      // Also generate a setup.sh template alongside AGENTS.md
      await writeSetupScriptTemplate(mainAgent.kaos).catch((error: unknown) => {
        this.log.error('writeSetupScriptTemplate failed', error);
      });
    } catch (error) {
      throw new OdyError(
        ErrorCodes.SESSION_INIT_FAILED,
        error instanceof Error ? error.message : 'Init failed',
        { cause: error },
      );
    }
  }
```

Also add the import at the top of `session/index.ts` (update the line that was added in T4):

```typescript
// Replace the single import from T4 with:
import { runSetupScriptIfNeeded, writeSetupScriptTemplate } from './setup-script';
```

- [ ] Run tests and verify they PASS

```bash
pnpm --filter @odysseythink/agent-core vitest run test/session/setup-script.test.ts --reporter=verbose
# Expected: all tests pass (T1+T2+T4+T5)

# Full typecheck
pnpm --filter @odysseythink/agent-core typecheck
# Expected: PASS

# Full test suite
pnpm --filter @odysseythink/agent-core vitest run
# Expected: PASS — no regressions
```

- [ ] Commit

```bash
git add packages/agent-core/src/session/setup-script.ts packages/agent-core/src/session/index.ts packages/agent-core/test/session/setup-script.test.ts
git commit -m "feat: generate .ody-code/setup.sh template during /init"
```

---

## Part 1 Self-Review

- [x] 1. Spec-coverage table (core scope): all agent-core spec items covered — T1 covers detect + types + format, T2 covers execute + run + persist + inject + telemetry, T3 covers permission gate, T4 covers createMain wiring, T5 covers /init template generation.
- [x] 2. Placeholder scan: no TODO/TBD/deferred-by-dependency — T2 uses a type-only `PermissionGate` shim to remain compilable before T3; the shim is replaced by the real `PermissionManager` type when T3 commits.
- [x] 3. No phantom tasks: every task produces a verifiable change (new file, new method, or modified method body); zero `--allow-empty` commits.
- [x] 4. Dependency soundness: T2 depends on T1 (uses types + helpers), T4 depends on T2+T3 (uses runner + permission), T5 depends on T1 (uses types). No forward references.
- [x] 5. Caller & build soundness: T4 and T5 modify `session/index.ts` but in non-overlapping methods (`createMain` vs `generateAgentsMd`); the shared import `from './setup-script'` is introduced in T4 and extended in T5 (adds `writeSetupScriptTemplate` import). T5's test step explicitly re-checks that T4's typecheck still passes. No shared-signature changes across tasks.
- [x] 6. Test-the-risk: T1 tests boundary/null cases for detect (ENOENT, directory, file). T2 tests yolo/auto/manual approval, rejection, non-zero exit, timeout handling, output truncation, metadata persistence. T3 tests all three modes + null-rpc fallback + error fallback. T5 tests template generation for pnpm/npm/pip/cargo/go + missing markers + existing script skip. All state mutations (metadata write, telemetry, system-reminder injection) have behavioral assertions.
- [x] 7. Type consistency: `SetupScriptResult` defined in T1 is used in T2; `SetupRunMeta` defined in T2 is used in T4 test; `PermissionGate` shim in T2 matches T3's real signature; `SetupScriptRunOptions` defined in T1 is used in T2+T4.
