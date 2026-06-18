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
