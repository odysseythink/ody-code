import { describe, expect, it, vi } from 'vitest';

import { OdyTUI, type OdyTUIStartupInput, type TUIState } from '#tui/ody-tui';
import type { RuntimeMode, SkillSummary } from '@odysseythink/ody-code-sdk';

interface SkillCommandsDriver {
  state: TUIState;
  refreshSkillCommands(session: unknown): Promise<void>;
}

function makeSession(overrides: Record<string, unknown> = {}) {
  return {
    id: 'ses-1',
    summary: { title: null },
    listSkills: vi.fn(async () => []),
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

type EffectiveMode = RuntimeMode;

function makeStartupInput(mode: EffectiveMode = 'normal'): OdyTUIStartupInput {
  const product = mode === 'product';
  const gameDesign = mode === 'game-design';
  return {
    cliOptions: {
      session: undefined,
      continue: false,
      yolo: false,
      auto: false,
      sessionMode: product || gameDesign ? 'normal' : mode,
      product,
      gameDesign,
      model: undefined,
      outputFormat: undefined,
      prompt: undefined,
      skillsDirs: [],
      loginProvider: undefined,
    host: 'ts',
    hostStdio: false,
    hostSocket: undefined,
    hostTcp: undefined,
    hostBinary: undefined,
      logoutProvider: undefined,
      smokeTest: false,
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
    product,
    gameDesign,
  };
}

function makeDriver(session = makeSession(), mode: EffectiveMode = 'normal') {
  const harness = makeHarness(session);
  const driver = new OdyTUI(harness as never, makeStartupInput(mode)) as unknown as SkillCommandsDriver;
  vi.spyOn(driver.state.ui, 'requestRender').mockImplementation(() => {});
  vi.spyOn(driver.state.terminal, 'setProgress').mockImplementation(() => {});
  return { driver, session, harness };
}

describe('OdyTUI skill slash command mode filtering', () => {
  it('passes sessionMode normal to listSkills so hidden skills are filtered out', async () => {
    const session = makeSession({
      listSkills: vi.fn(async () => []),
    });
    const { driver } = makeDriver(session, 'normal');

    await driver.refreshSkillCommands(session as never);

    expect(session.listSkills).toHaveBeenCalledWith({ sessionMode: 'normal' });
  });

  it('passes sessionMode plan to listSkills', async () => {
    const session = makeSession({
      listSkills: vi.fn(async () => []),
    });
    const { driver } = makeDriver(session, 'plan');

    await driver.refreshSkillCommands(session as never);

    expect(session.listSkills).toHaveBeenCalledWith({ sessionMode: 'plan' });
  });

  it('passes sessionMode design to listSkills', async () => {
    const session = makeSession({
      listSkills: vi.fn(async () => []),
    });
    const { driver } = makeDriver(session, 'design');

    await driver.refreshSkillCommands(session as never);

    expect(session.listSkills).toHaveBeenCalledWith({ sessionMode: 'design' });
  });

  it('passes sessionMode product to listSkills', async () => {
    const session = makeSession({
      listSkills: vi.fn(async () => []),
    });
    const { driver } = makeDriver(session, 'product');

    await driver.refreshSkillCommands(session as never);

    expect(session.listSkills).toHaveBeenCalledWith({ sessionMode: 'product' });
  });

  it('passes sessionMode game-design to listSkills', async () => {
    const session = makeSession({
      listSkills: vi.fn(async () => []),
    });
    const { driver } = makeDriver(session, 'game-design');

    await driver.refreshSkillCommands(session as never);

    expect(session.listSkills).toHaveBeenCalledWith({ sessionMode: 'game-design' });
  });

  it('uses the skills returned by listSkills for slash commands', async () => {
    const skills: SkillSummary[] = [
      { name: 'debt-ledger', description: 'debt', path: 'builtin://debt-ledger', source: 'builtin' },
      { name: 'review', description: 'review', path: 'builtin://review', source: 'builtin', type: 'flow' },
    ];
    const session = makeSession({
      listSkills: vi.fn(async () => skills),
    });
    const { driver } = makeDriver(session, 'normal');

    await driver.refreshSkillCommands(session as never);

    const slashCommands = (driver as unknown as { getSlashCommands(): readonly { name: string }[] }).getSlashCommands();
    const names = slashCommands.map((c) => c.name);
    expect(names).toContain('skill:debt-ledger');
    expect(names).toContain('skill:review');
  });
});
