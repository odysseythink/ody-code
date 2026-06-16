import { describe, expect, it, vi } from 'vitest';

import { KimiTUI, type KimiTUIStartupInput, type TUIState } from '#/tui/ody-tui';

vi.mock('node:fs/promises', async (importOriginal) => {
  const actual = await importOriginal<typeof import('node:fs/promises')>();
  return {
    ...actual,
    access: vi.fn(),
  };
});

import { access } from 'node:fs/promises';

interface SyncDriver {
  state: TUIState;
  syncRuntimeState(session: unknown): Promise<void>;
}

function makeSession(overrides: Record<string, unknown> = {}) {
  return {
    id: 'ses-1',
    summary: { title: null },
    getStatus: vi.fn(async () => ({
      model: 'k2',
      thinkingLevel: 'off',
      permission: 'manual',
      sessionMode: 'plan',
      sessionModeFilePath: '/tmp/plans/test-plan.md',
      contextTokens: 10,
      maxContextTokens: 100,
      contextUsage: 0.1,
    })),
    getGoal: vi.fn(async () => ({ goal: null })),
    ...overrides,
  };
}

function makeHarness(session = makeSession()) {
  return {
    getConfig: vi.fn(async () => ({ models: {} })),
    createSession: vi.fn(async () => session),
    resumeSession: vi.fn(async () => session),
    listSessions: vi.fn(async () => []),
    close: vi.fn(async () => {}),
    track: vi.fn(),
    setTelemetryContext: vi.fn(),
    getExperimentalFlags: vi.fn(async () => ({})),
    auth: {
      status: vi.fn(async () => ({ providers: [] })),
      login: vi.fn(async () => {}),
      logout: vi.fn(),
      getManagedUsage: vi.fn(),
    },
  };
}

function makeStartupInput(): KimiTUIStartupInput {
  return {
    cliOptions: {
      session: undefined,
      continue: false,
      yolo: false,
      auto: false,
      sessionMode: 'normal',
      officeHours: false,
      model: undefined,
      outputFormat: undefined,
      prompt: undefined,
      skillsDirs: [],
      loginProvider: undefined,
      logoutProvider: undefined,
    },
    tuiConfig: {
      theme: 'dark',
      editorCommand: null,
      notifications: { enabled: true, condition: 'unfocused' },
      upgrade: { autoInstall: true },
    },
    version: '0.0.0-test',
    workDir: '/tmp/proj-a',
    resolvedTheme: 'dark',
    officeHours: false,
  };
}

describe('KimiTUI syncRuntimeState', () => {
  it('propagates sessionModeFilePath when the file exists', async () => {
    const session = makeSession();
    const harness = makeHarness(session);
    const driver = new KimiTUI(harness as never, makeStartupInput()) as unknown as SyncDriver;
    vi.spyOn(driver.state.ui, 'requestRender').mockImplementation(() => {});
    vi.spyOn(driver.state.terminal, 'setProgress').mockImplementation(() => {});

    vi.mocked(access).mockResolvedValue(undefined);

    await driver.syncRuntimeState(session as never);

    expect(driver.state.appState.sessionModeFilePath).toBe('/tmp/plans/test-plan.md');
  });

  it('falls back to null when the plan file does not exist', async () => {
    const session = makeSession();
    const harness = makeHarness(session);
    const driver = new KimiTUI(harness as never, makeStartupInput()) as unknown as SyncDriver;
    vi.spyOn(driver.state.ui, 'requestRender').mockImplementation(() => {});
    vi.spyOn(driver.state.terminal, 'setProgress').mockImplementation(() => {});

    vi.mocked(access).mockRejectedValue(new Error('ENOENT'));

    await driver.syncRuntimeState(session as never);

    expect(driver.state.appState.sessionModeFilePath).toBeNull();
  });
});
