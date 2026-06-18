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
    expect(text).toContain('timed out');
  });

  it('includes duration in seconds', () => {
    const text = formatSetupReminder(makeResult({ durationMs: 5678 }));
    expect(text).toContain('5.7s');
    expect(text).toContain('completed');
  });
});

describe('formatRejectionReminder', () => {
  it('mentions user denied the script', () => {
    const text = formatRejectionReminder();
    expect(text).toContain('not run');
    expect(text).toContain('denied');
  });
});
