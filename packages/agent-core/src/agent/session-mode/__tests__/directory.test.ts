import { describe, it, expect, vi } from 'vitest';
import { resolveSessionModeDirectory, getModeOutputSubdirectory } from '../directory';

const CWD = '/workspace/project';

function makeAgent(overrides: { homedir?: string; existing?: Set<string> } = {}) {
  const existing = overrides.existing ?? new Set<string>();
  return {
    config: { cwd: CWD },
    homedir: overrides.homedir,
    kaos: {
      mkdir: vi.fn().mockResolvedValue(undefined),
      stat: vi.fn(async (p: string) => {
        if (existing.has(p)) return { stMode: 0o100644 };
        throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
      }),
    },
  } as unknown as import('../../agent').Agent;
}

describe('directory utilities', () => {
  it('returns the correct subdirectory for each kind', () => {
    expect(getModeOutputSubdirectory('plan')).toBe('plans');
    expect(getModeOutputSubdirectory('design')).toBe('designs');
    expect(getModeOutputSubdirectory('office-hours')).toBe('products');
    expect(getModeOutputSubdirectory('game-design')).toBe('game-design');
  });

  it('resolves project-scoped directory when mkdir succeeds', async () => {
    const agent = makeAgent();
    const result = await resolveSessionModeDirectory(agent, 'plan');
    expect(result.dir).toBe('/workspace/project/.ody-code/plans');
    expect(result.isProjectScoped).toBe(true);
  });

  it('falls back to homedir on permission error', async () => {
    const agent = makeAgent({ homedir: '/home/user' });
    agent.kaos.mkdir = vi.fn()
      .mockRejectedValueOnce(Object.assign(new Error('EACCES'), { code: 'EACCES' }))
      .mockResolvedValueOnce(undefined);
    const result = await resolveSessionModeDirectory(agent, 'design');
    expect(result.dir).toBe('/home/user/designs');
    expect(result.isProjectScoped).toBe(false);
  });
});
