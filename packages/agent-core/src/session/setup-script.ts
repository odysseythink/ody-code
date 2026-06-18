import { join } from 'pathe';
import { type Readable } from 'node:stream';
import type { Kaos } from '@odysseythink/kaos';

const S_IFMT = 0o170000;
const S_IFREG = 0o100000;

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
    if ((stat.stMode & S_IFMT) !== S_IFREG) return null;
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

// ── Persist + inject ────────────────────────────────────────────────

export interface SetupRunMeta {
  readonly ranAt: string;
  readonly approved: boolean;
  readonly exitCode: number | undefined;
  readonly timedOut: boolean;
  readonly durationMs: number;
}

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
    exitCode: result.exitCode ?? undefined,
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

// ── Main entry point ────────────────────────────────────────────────

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
