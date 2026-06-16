import { describe, expect, it, vi } from 'vitest';
import { join } from 'pathe';
import { Session } from '../../src/session';
import { testKaos } from '../fixtures/test-kaos';

async function makeTempDir(): Promise<string> {
  const dir = testKaos.pathClass() === 'posix'
    ? `/tmp/ody-test-${Math.random().toString(36).slice(2)}`
    : join(process.cwd(), 'test-tmp', Math.random().toString(36).slice(2));
  await testKaos.mkdir(dir, { parents: true, existOk: true });
  return dir;
}

describe('Session userLanguage persistence', () => {
  it('restores userLanguage from metadata.custom when creating agent', async () => {
    const sessionDir = await makeTempDir();
    const workDir = await makeTempDir();
    const session = new Session({
      id: 'test-lang-persist',
      kaos: testKaos.withCwd(workDir),
      homedir: sessionDir,
      rpc: { emitEvent: vi.fn() } as any,
      skills: { explicitDirs: [join(workDir, 'missing-skills')] },
    });
    // simulate a previously set language
    session.metadata.custom = { userLanguage: 'zh' };
    const main = await session.createMain();
    expect(main.userLanguage).toBe('zh');
  });

  it('writes userLanguage to metadata and persists when setUserLanguage is called', async () => {
    const sessionDir = await makeTempDir();
    const workDir = await makeTempDir();
    const session = new Session({
      id: 'test-lang-persist-2',
      kaos: testKaos.withCwd(workDir),
      homedir: sessionDir,
      rpc: { emitEvent: vi.fn() } as any,
      skills: { explicitDirs: [join(workDir, 'missing-skills')] },
    });
    const main = await session.createMain();
    const writeSpy = vi.spyOn(session, 'writeMetadata');
    main.setUserLanguage('zh');
    expect(session.metadata.custom['userLanguage']).toBe('zh');
    expect(writeSpy).toHaveBeenCalled();
  });

  it('defaults userLanguage to undefined when metadata has no entry', async () => {
    const sessionDir = await makeTempDir();
    const workDir = await makeTempDir();
    const session = new Session({
      id: 'test-lang-persist-3',
      kaos: testKaos.withCwd(workDir),
      homedir: sessionDir,
      rpc: { emitEvent: vi.fn() } as any,
      skills: { explicitDirs: [join(workDir, 'missing-skills')] },
    });
    const main = await session.createMain();
    expect(main.userLanguage).toBeUndefined();
  });
});
