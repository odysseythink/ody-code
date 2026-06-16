import { describe, expect, it, vi } from 'vitest';
import { join } from 'pathe';
import { SessionMode } from '../../src/agent/session-mode';
import type { Agent } from '../../src/agent';

function mockAgent(overrides: Partial<Agent> = {}): Agent {
  const kaos = {
    mkdir: vi.fn().mockResolvedValue(undefined),
    readText: vi.fn().mockRejectedValue({ code: 'ENOENT' }),
    writeText: vi.fn().mockResolvedValue(undefined),
    stat: vi.fn().mockRejectedValue({ code: 'ENOENT' }),
  };
  const config = {
    cwd: '/fake/project',
    modelAlias: 'default',
    update: vi.fn(),
  };
  const records = {
    logRecord: vi.fn(),
  };
  return {
    kaos,
    config,
    records,
    homedir: '/fake/home/.ody-code/sessions/s1',
    kimiConfig: undefined,
    modelProvider: undefined,
    log: undefined,
    replayBuilder: { push: vi.fn() },
    emitStatusUpdated: vi.fn(),
    setContextMode: vi.fn(),
    ...overrides,
  } as unknown as Agent;
}

describe('SessionMode office-hours', () => {
  it('enter sets kind to office-hours', async () => {
    const agent = mockAgent();
    const mode = new SessionMode(agent);
    await mode.enter('id-1', false, false, 'office-hours');
    expect(mode.kind).toBe('office-hours');
    expect(mode.isActive).toBe(true);
  });

  it('exit clears active state', async () => {
    const agent = mockAgent();
    const mode = new SessionMode(agent);
    await mode.enter('id-1', false, false, 'office-hours');
    mode.exit();
    expect(mode.isActive).toBe(false);
  });

  it('resolveSessionModeDirectory uses office-hours subdirectory', async () => {
    const mkdirSpy = vi.fn().mockResolvedValue(undefined);
    const agent = mockAgent({ kaos: { ...mockAgent().kaos, mkdir: mkdirSpy } });
    const mode = new SessionMode(agent);
    await mode.enter('id-1', false, false, 'office-hours');
    const calls = mkdirSpy.mock.calls;
    const officeHoursCall = calls.find((c: unknown[]) =>
      typeof c[0] === 'string' && c[0].includes('office-hours'),
    );
    expect(officeHoursCall).toBeDefined();
    expect(officeHoursCall![0]).toContain(join('.ody-code', 'office-hours'));
  });
});
