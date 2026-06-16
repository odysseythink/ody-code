import { describe, expect, it, vi } from 'vitest';

import { runOfficeHours } from '#/cli/run-office-hours';
import type { CLIOptions } from '#/cli/options';

// Mock external modules used by runOfficeHours
vi.mock('@odysseythink/ody-telemetry', () => ({
  setCrashPhase: vi.fn(),
  setTelemetryContext: vi.fn(),
  shutdownTelemetry: vi.fn(),
  track: vi.fn(),
  withTelemetryContext: vi.fn(() => ({ track: vi.fn() })),
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

vi.mock('#/tui/index', () => ({
  KimiTUI: vi.fn().mockImplementation(function () {
    return {
      onExit: undefined as unknown,
      start: vi.fn().mockResolvedValue(undefined),
      stop: vi.fn().mockResolvedValue(undefined),
      getCurrentSessionId: vi.fn().mockReturnValue(''),
    };
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
      model: undefined,
      outputFormat: undefined,
      prompt: undefined,
      skillsDirs: [],
      loginProvider: undefined,
      logoutProvider: undefined,
    };

    // Should not throw
    await expect(runOfficeHours(opts, '0.0.0-test')).resolves.toBeUndefined();
  });
});
