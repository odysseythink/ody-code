import { describe, it, expect, vi, beforeEach } from 'vitest';
import { join } from 'pathe';
import { SessionMode } from '../../src/agent/session-mode';
import type { Agent } from '../../src/agent';

const CWD = '/workspace/project';

function makeAgent(overrides: {
  existingPaths?: Set<string>;
} = {}): Agent {
  const existing = overrides.existingPaths ?? new Set<string>();
  return {
    homedir: '/home/user',
    config: { cwd: CWD, modelAlias: 'test', provider: 'test', update: vi.fn() },
    emitStatusUpdated: vi.fn(),
    replayBuilder: { push: vi.fn() },
    records: { logRecord: vi.fn() },
    context: { history: undefined },
    kaos: {
      mkdir: vi.fn().mockResolvedValue(undefined),
      stat: vi.fn(async (p: string) => {
        if (existing.has(p)) return { stMode: 0o100644 };
        throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
      }),
      readText: vi.fn().mockRejectedValue(Object.assign(new Error('ENOENT'), { code: 'ENOENT' })),
      writeText: vi.fn().mockResolvedValue(undefined),
    },
    log: { warn: vi.fn() },
  } as unknown as Agent;
}

describe('SessionMode', () => {
  describe('enter', () => {
    it('does NOT set _sessionModeFilePath after enter (was eagerly resolved before)', async () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      await sm.enter('id-1', undefined, false, 'design');
      expect(sm.sessionModeFilePath).toBeNull();
    });
  });

  describe('resolveFilePathFromModelRequest', () => {
    it('extracts slug from model-requested path basename', async () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      await sm.enter('id-1', undefined, false, 'design');
      const path = await sm.resolveFilePathFromModelRequest(
        '.ody-code/designs/my-cool-feature.md',
        '# My Design\n\nSome content',
      );
      // Should contain date prefix + slug
      expect(path).toMatch(/^\S+\d{4}-\d{2}-\d{2}-my-cool-feature\.md$/);
      expect(sm.sessionModeFilePath).toBe(path);
    });

    it('uses only the basename, ignoring directory structure', async () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      await sm.enter('id-1', undefined, false, 'design');
      const path = await sm.resolveFilePathFromModelRequest(
        'deeply/nested/subdir/my-topic.md',
        '# Content',
      );
      expect(path).toMatch(/my-topic\.md$/);
      // The directory prefix should be .ody-code/designs/, not deeply/nested/
      expect(path).toMatch(/\.ody-code\/designs\//);
    });

    it('falls back to content heading when basename yields unusable slug', async () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      await sm.enter('id-1', undefined, false, 'design');
      const path = await sm.resolveFilePathFromModelRequest(
        '.ody-code/designs/---.md',
        '# Actual Heading\n\nContent',
      );
      // After sanitization "---" becomes empty → falls back to heading
      expect(path).toMatch(/actual-heading\.md$/);
    });

    it('falls back to untitled when both basename and content heading are unusable', async () => {
      const agent = makeAgent({
        // Mock kaos.stat for findUniqueStemInDir — first call for untitled.md throws ENOENT (unique)
      });
      const sm = new SessionMode(agent);
      await sm.enter('id-1', undefined, false, 'design');
      const path = await sm.resolveFilePathFromModelRequest(
        '.ody-code/designs/---.md',
        'No heading here.',
      );
      expect(path).toMatch(/untitled\.md$/);
    });

    it('returns existing path if already resolved', async () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      await sm.enter('id-1', undefined, false, 'design');
      const first = await sm.resolveFilePathFromModelRequest(
        '.ody-code/designs/topic-a.md',
        '# Content',
      );
      const second = await sm.resolveFilePathFromModelRequest(
        '.ody-code/designs/topic-b.md',
        '# Different',
      );
      expect(second).toBe(first);
    });

    it('deduplicates against existing files on disk', async () => {
      const existing = new Set<string>();
      const agent = makeAgent({ existingPaths: existing });
      const sm = new SessionMode(agent);
      await sm.enter('id-1', undefined, false, 'design');

      const path = await sm.resolveFilePathFromModelRequest(
        '.ody-code/designs/my-topic.md',
        '# Content',
      );
      expect(path).toMatch(/my-topic\.md$/);
      expect(path).not.toMatch(/my-topic-1/);
    });

    it('logs session_mode.enter record with path after resolution', async () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      await sm.enter('id-1', undefined, false, 'design');
      // Clear the logRecord call from enter()
      vi.mocked(agent.records.logRecord).mockClear();

      await sm.resolveFilePathFromModelRequest(
        '.ody-code/designs/my-topic.md',
        '# Content',
      );
      expect(agent.records.logRecord).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'session_mode.enter',
          id: 'id-1',
          kind: 'design',
          path: expect.any(String),
        }),
      );
    });
  });

  describe('restoreEnter with path', () => {
    it('restores _sessionModeFilePath when path is provided', () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      sm.restoreEnter({
        id: 'restored-id',
        kind: 'design',
        path: '/workspace/.ody-code/designs/2026-06-10-my-topic.md',
      });
      expect(sm.sessionModeFilePath).toBe('/workspace/.ody-code/designs/2026-06-10-my-topic.md');
    });

    it('leaves _sessionModeFilePath null when path is not provided (legacy)', () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      sm.restoreEnter({ id: 'legacy-id', kind: 'plan' });
      expect(sm.sessionModeFilePath).toBeNull();
    });

    it('leaves _sessionModeFilePath null when path is empty string', () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      sm.restoreEnter({ id: 'id', kind: 'design', path: '' });
      expect(sm.sessionModeFilePath).toBeNull();
    });
  });
});
