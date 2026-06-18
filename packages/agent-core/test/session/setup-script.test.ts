import { describe, it, expect, vi, beforeEach } from 'vitest';
import { Readable, Writable } from 'node:stream';
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
  runSetupScriptIfNeeded,
  type SetupScriptResult,
  type SetupRunMeta,
  type SetupScriptRunOptions,
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
    const kaos = makeKaos({ statResult: { stMode: 0o040000 } }); // directory
    const result = await detectSetupScript(kaos);
    expect(result).toBeNull();
  });

  it('returns absolute path when .ody-code/setup.sh is a regular file', async () => {
    const kaos = makeKaos({ statResult: { stMode: 0o100000 } }); // regular file
    const result = await detectSetupScript(kaos);
    expect(result).toBe('/fake/repo/.ody-code/setup.sh');
  });

  it('returned path uses kaos.normpath for cross-platform safety', async () => {
    const kaos = makeKaos({
      cwd: '/fake/repo/sub',
      statResult: { stMode: 0o100000 }, // regular file
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
    const kaos = makeKaos({ statResult: { stMode: 0o100000 } }); // regular file
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

// ── T2: runSetupScriptIfNeeded tests ────────────────────────────────────

function readableFrom(text: string): Readable {
  const r = new Readable({ read() {} });
  r.push(text);
  r.push(null);
  return r;
}

function makeMockKaosProcess(exitCode: number, stdout: string, stderr: string) {
  return {
    stdin: new Writable({ write(_chunk: any, _enc: any, cb: any) { cb(); } }),
    stdout: readableFrom(stdout),
    stderr: readableFrom(stderr),
    pid: 9999,
    exitCode,
    wait: vi.fn().mockResolvedValue(exitCode),
    kill: vi.fn().mockResolvedValue(undefined),
  };
}

describe('runSetupScriptIfNeeded', () => {
  function buildContext(opts: {
    permissionMode?: 'manual' | 'auto' | 'yolo';
    approvalDecision?: 'approved' | 'rejected';
    scriptExists?: boolean;
    process?: ReturnType<typeof makeMockKaosProcess>;
    metadataSetupRun?: SetupRunMeta | undefined;
  } = {}) {
    const mockedStat = opts.scriptExists === false
      ? vi.fn().mockRejectedValue(Object.assign(new Error('ENOENT'), { code: 'ENOENT' }))
      : vi.fn().mockResolvedValue({ stMode: 0o100000 });

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

// ── T4: createMain wiring contract tests ────────────────────────────────

import { runSetupScriptIfNeeded as rsni } from '../../src/session/setup-script';
import { Agent } from '../../src/agent';
import { Session } from '../../src/session';

describe('createMain wiring contract', () => {
  it('runSetupScriptIfNeeded accepts Session and Agent types', () => {
    const fn: (
      session: { readonly options: { readonly kaos: any }; metadata: { custom: Record<string, any> }; writeMetadata(): Promise<void> },
      agent: { readonly permission: any; readonly kaos: any; readonly telemetry: any; readonly context: any },
      opts?: { force?: boolean },
    ) => Promise<any> = rsni;
    expect(typeof fn).toBe('function');
  });

  it('Session and Agent satisfy the minimal structural contract', () => {
    const sessionProto = Session.prototype;
    expect(sessionProto).toBeDefined();
    expect(typeof Session.prototype.writeMetadata).toBe('function');
    expect(Agent.prototype).toBeDefined();
  });
});

// ── T5: writeSetupScriptTemplate tests ──────────────────────────────────

import { writeSetupScriptTemplate } from '../../src/session/setup-script';

describe('writeSetupScriptTemplate', () => {
  function makeKaosWithDir(files: string[]) {
    const kaos = new (LocalKaos as any)(TEST_OS_ENV) as LocalKaos & {
      stat: any; writeText: any; mkdir: any; readText: any;
    };
    kaos.getcwd = () => '/fake/repo';
    kaos.mkdir = vi.fn().mockResolvedValue(undefined);
    kaos.writeText = vi.fn().mockResolvedValue(10);
    kaos.normpath = (p: string) => p;
    // stat: only succeed for files in the `files` list
    kaos.stat = vi.fn().mockImplementation(async (path: string) => {
      const basename = path.replace(/^.*[/\\]/, '');
      if (files.includes(basename)) return { stMode: 0o100000 }; // regular file
      throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
    });
    kaos.readText = vi.fn().mockResolvedValue('');
    kaos.exec = vi.fn().mockResolvedValue(undefined);
    return kaos;
  }

  it('skips if setup.sh already exists', async () => {
    const kaos = makeKaosWithDir([]);
    let callCount = 0;
    kaos.stat = vi.fn().mockImplementation(async (path: string) => {
      callCount++;
      if (path.endsWith('.ody-code/setup.sh')) return { stMode: 0o100000 };
      throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
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
    expect(content).toContain('No recognized project markers');
  });
});
