import { describe, expect, it, vi } from 'vitest';

import type { CLIOptions } from '#cli/options';
import { OdyTUI } from '#tui/ody-tui';
import type { OdyHarness } from '#tui/types';

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
