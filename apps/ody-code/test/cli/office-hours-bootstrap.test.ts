import { describe, expect, it, vi } from 'vitest';

import { runOfficeHours } from '#/cli/run-office-hours';
import type { CLIOptions } from '#/cli/options';

const mockWithTelemetryContext = vi.fn<(ctx: { sessionId?: string }) => { track: () => void }>(() => ({
  track: vi.fn(),
}));

// Mock external modules used by runOfficeHours
vi.mock('@odysseythink/ody-telemetry', () => ({
  setCrashPhase: vi.fn(),
  setTelemetryContext: vi.fn(),
  shutdownTelemetry: vi.fn(),
  track: vi.fn(),
  withTelemetryContext: (ctx: { sessionId?: string }) => mockWithTelemetryContext(ctx),
}));

vi.mock('@odysseythink/ody-code-sdk', async () => {
  const actual = await vi.importActual('@odysseythink/ody-code-sdk');
  return {
    ...actual,
    KimiHarness: vi.fn().mockImplementation(function () {
      return {
        ensureConfigFile: vi.fn().mockResolvedValue(undefined),
        getConfig: vi.fn().mockResolvedValue({}),
        close: vi.fn().mockResolvedValue(undefined),
      };
    }),
  };
});

let lastTuiInstance: { onExit?: (exitCode?: number) => Promise<void>; getCurrentSessionId: () => string } | undefined;

vi.mock('#/tui/index', () => ({
  OdyTUI: vi.fn().mockImplementation(function () {
    const instance = {
      onExit: undefined as unknown as ((exitCode?: number) => Promise<void>) | undefined,
      start: vi.fn().mockResolvedValue(undefined),
      stop: vi.fn().mockResolvedValue(undefined),
      getCurrentSessionId: vi.fn().mockReturnValue('ses-office-hours'),
    };
    lastTuiInstance = instance;
    return instance;
  }),
}));

vi.mock('#/tui/config', () => ({
  loadTuiConfig: vi.fn().mockResolvedValue({ theme: 'dark', editorCommand: null, notifications: {}, upgrade: {} }),
  TuiConfigParseError: class extends Error {},
}));

vi.mock('#/tui/theme/detect', () => ({
  detectTerminalTheme: vi.fn().mockResolvedValue('dark'),
}));

vi.mock('#/cli/telemetry', () => ({
  createCliTelemetryBootstrap: vi.fn().mockReturnValue({
    homeDir: '/tmp/.ody-code-test',
  }),
  initializeCliTelemetry: vi.fn(),
}));

vi.mock('#/cli/version', () => ({
  createKimiCodeHostIdentity: vi.fn().mockReturnValue({}),
}));

vi.mock('node:child_process', () => ({
  execSync: vi.fn(),
}));

describe('runOfficeHours', () => {
  it('creates harness and calls tui.start', async () => {
    const opts: CLIOptions = {
      session: undefined,
      continue: false,
      yolo: false,
      auto: false,
      sessionMode: 'normal',
      officeHours: true,
      gameDesign: false,
      model: undefined,
      outputFormat: undefined,
      prompt: undefined,
      skillsDirs: [],
      loginProvider: undefined,
    host: 'inproc',
    hostStdio: false,
    hostSocket: undefined,
    hostTcp: undefined,
    hostBinary: undefined,
      logoutProvider: undefined,
    };

    // Should not throw
    await expect(runOfficeHours(opts, '0.0.0-test')).resolves.toBeUndefined();
  });

  it('uses tui.getCurrentSessionId for office_hours_completed telemetry on exit', async () => {
    const opts: CLIOptions = {
      session: undefined,
      continue: false,
      yolo: false,
      auto: false,
      sessionMode: 'normal',
      officeHours: true,
      gameDesign: false,
      model: undefined,
      outputFormat: undefined,
      prompt: undefined,
      skillsDirs: [],
      loginProvider: undefined,
    host: 'inproc',
    hostStdio: false,
    hostSocket: undefined,
    hostTcp: undefined,
    hostBinary: undefined,
      logoutProvider: undefined,
    };

    await runOfficeHours(opts, '0.0.0-test');
    expect(lastTuiInstance).toBeDefined();
    expect(lastTuiInstance?.onExit).toBeDefined();

    mockWithTelemetryContext.mockClear();
    const exitSpy = vi.spyOn(process, 'exit').mockImplementation(() => undefined as never);
    try {
      await lastTuiInstance!.onExit!(0);
    } finally {
      exitSpy.mockRestore();
    }

    expect(mockWithTelemetryContext).toHaveBeenCalledWith({ sessionId: 'ses-office-hours' });
  });
});
