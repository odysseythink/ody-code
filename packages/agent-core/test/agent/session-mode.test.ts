import { describe, it, expect, vi, beforeEach } from 'vitest';
import { join } from 'pathe';
import { SessionMode } from '../../src/agent/session-mode';
import { stripDatePrefix } from '../../src/agent/session-mode/topic-generator';
import type { Agent } from '../../src/agent';

const CWD = '/workspace/project';

function makeAgent(overrides: {
  existingPaths?: Set<string>;
  modelAlias?: string;
  kimiConfig?: { defaultModel?: string };
} = {}): Agent {
  const existing = overrides.existingPaths ?? new Set<string>();
  return {
    homedir: '/home/user',
    config: { cwd: CWD, modelAlias: overrides.modelAlias ?? 'test', provider: 'test', update: vi.fn() },
    kimiConfig: overrides.kimiConfig,
    emitStatusUpdated: vi.fn(),
    replayBuilder: { push: vi.fn() },
    records: { logRecord: vi.fn() },
    context: { history: undefined },
    contexts: { normal: { history: [] }, plan: { history: [] }, design: { history: [] } },
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
    setContextMode: vi.fn(),
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

    it('does not double the date prefix when the request already has one', async () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      await sm.enter('id-1', undefined, false, 'design');
      const path = await sm.resolveFilePathFromModelRequest(
        '.ody-code/designs/2026-06-10-resume-session-model-from-config.md',
        '# Content',
      );
      // Exactly one date prefix, then the slug — no doubled date.
      expect(path).toMatch(/\/\d{4}-\d{2}-\d{2}-resume-session-model-from-config\.md$/);
      expect(path).not.toMatch(/\d{4}-\d{2}-\d{2}-\d{4}-\d{2}-\d{2}-/);
    });

    it('collapses multiple existing date prefixes into a single one', async () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      await sm.enter('id-1', undefined, false, 'design');
      const path = await sm.resolveFilePathFromModelRequest(
        '.ody-code/designs/2026-06-10-2026-06-10-foo.md',
        '# Content',
      );
      expect(path).toMatch(/\/\d{4}-\d{2}-\d{2}-foo\.md$/);
      expect(path).not.toMatch(/\d{4}-\d{2}-\d{2}-\d{4}-\d{2}-\d{2}-/);
    });

    it('falls back to content heading when the basename is only a date', async () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      await sm.enter('id-1', undefined, false, 'design');
      const path = await sm.resolveFilePathFromModelRequest(
        '.ody-code/designs/2026-06-10.md',
        '# Actual Heading\n\nContent',
      );
      // Stripping the date leaves an empty slug → content heading fallback.
      expect(path).toMatch(/actual-heading\.md$/);
      expect(path).not.toMatch(/\d{4}-\d{2}-\d{2}-\d{4}-\d{2}-\d{2}-/);
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

    it('restores the normal-mode model (defaultModel) on exit after a resumed enter', () => {
      // Resume boots straight into design mode on the resumed mode's model; a later
      // exit must revert to the normal-mode model, not stay on the design model.
      const agent = makeAgent({
        modelAlias: 'design-model',
        kimiConfig: { defaultModel: 'normal-model' },
      });
      const sm = new SessionMode(agent);
      sm.restoreEnter({ id: 'id', kind: 'design' });
      sm.exit();
      expect(agent.config.update).toHaveBeenCalledWith({ modelAlias: 'normal-model' });
    });

    it('restores the normal-mode model (defaultModel) on cancel after a resumed enter', () => {
      const agent = makeAgent({
        modelAlias: 'plan-model',
        kimiConfig: { defaultModel: 'normal-model' },
      });
      const sm = new SessionMode(agent);
      sm.restoreEnter({ id: 'id', kind: 'plan' });
      sm.cancel();
      expect(agent.config.update).toHaveBeenCalledWith({ modelAlias: 'normal-model' });
    });

    it('does not restore (or throw) when no defaultModel is configured', () => {
      const agent = makeAgent({ modelAlias: 'design-model' }); // kimiConfig undefined
      const sm = new SessionMode(agent);
      sm.restoreEnter({ id: 'id', kind: 'design' });
      expect(() => sm.exit()).not.toThrow();
      // No pre-mode alias was seeded → exit must not push a model restore.
      expect(agent.config.update).not.toHaveBeenCalled();
    });
  });

  describe('stripDatePrefix', () => {
    it('removes a single leading date prefix', () => {
      expect(stripDatePrefix('2026-06-10-foo')).toBe('foo');
    });

    it('removes repeated leading date prefixes', () => {
      expect(stripDatePrefix('2026-06-10-2026-06-10-foo')).toBe('foo');
    });

    it('leaves a slug without a date prefix untouched', () => {
      expect(stripDatePrefix('foo')).toBe('foo');
    });

    it('strips a bare date to an empty string', () => {
      expect(stripDatePrefix('2026-06-10')).toBe('');
      expect(stripDatePrefix('2026-06-10-2026-06-10')).toBe('');
    });

    it('only strips prefixes, not dates embedded later', () => {
      expect(stripDatePrefix('foo-2026-06-10')).toBe('foo-2026-06-10');
    });
  });

  describe('handoffTo', () => {
    it('handoffTo("plan") exits design, enters plan, stores artifact in _pendingHandoffForPlan', async () => {
      const agent = makeAgent();
      vi.mocked(agent.kaos.readText).mockResolvedValue('# My Design\n\nSome content');
      const sm = new SessionMode(agent);
      await sm.enter('design-id', undefined, false, 'design');
      // Resolve a file path so data() has something to return
      await sm.resolveFilePathFromModelRequest('.ody-code/designs/my-feature.md', '# My Design\nSome content');

      vi.mocked(agent.records.logRecord).mockClear();

      await sm.handoffTo('plan');

      // Should have exited design and entered plan
      expect(sm.isActive).toBe(true);
      expect(sm.kind).toBe('plan');

      // Pending handoff for plan is set
      const handoff = sm.consumePendingHandoffForPlan();
      expect(handoff).not.toBeNull();
      expect(handoff?.content).toBe('# My Design\n\nSome content');
      expect(handoff?.path).toMatch(/my-feature\.md$/);

      // Consumed — second call returns null
      expect(sm.consumePendingHandoffForPlan()).toBeNull();
    });

    it('handoffTo("plan") stores null artifact when design file is empty', async () => {
      const agent = makeAgent();
      // readText returns empty content
      vi.mocked(agent.kaos.readText).mockResolvedValue('');
      const sm = new SessionMode(agent);
      await sm.enter('design-id', undefined, false, 'design');
      await sm.resolveFilePathFromModelRequest('.ody-code/designs/my-feature.md', '');

      await sm.handoffTo('plan');

      expect(sm.consumePendingHandoffForPlan()).toBeNull();
    });

    it('handoffTo("plan") stores null artifact when no file path is set', async () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      await sm.enter('design-id', undefined, false, 'design');
      // No resolveFilePathFromModelRequest called — _sessionModeFilePath is null

      await sm.handoffTo('plan');

      expect(sm.consumePendingHandoffForPlan()).toBeNull();
    });

    it('handoffTo("normal") exits plan, stores artifact in _pendingHandoffForNormal', async () => {
      const agent = makeAgent();
      vi.mocked(agent.kaos.readText).mockResolvedValue('## Step 1\n\nDo this');
      const sm = new SessionMode(agent);
      await sm.enter('plan-id', undefined, false, 'plan');
      await sm.resolveFilePathFromModelRequest('.ody-code/plans/my-plan.md', '## Step 1\nDo this');

      vi.mocked(agent.records.logRecord).mockClear();

      await sm.handoffTo('normal');

      // Should have exited plan — not active anymore
      expect(sm.isActive).toBe(false);

      const handoff = sm.consumePendingHandoffForNormal();
      expect(handoff).not.toBeNull();
      expect(handoff?.content).toBe('## Step 1\n\nDo this');
      expect(handoff?.path).toMatch(/my-plan\.md$/);

      expect(sm.consumePendingHandoffForNormal()).toBeNull();
    });

    it('handoffTo("normal") stores null artifact when plan file is empty', async () => {
      const agent = makeAgent();
      vi.mocked(agent.kaos.readText).mockResolvedValue('');
      const sm = new SessionMode(agent);
      await sm.enter('plan-id', undefined, false, 'plan');
      await sm.resolveFilePathFromModelRequest('.ody-code/plans/my-plan.md', '');

      await sm.handoffTo('normal');

      expect(sm.consumePendingHandoffForNormal()).toBeNull();
    });

    it('handoffTo("plan") clears _pendingHandoffForPlan when enter throws', async () => {
      const agent = makeAgent();
      vi.mocked(agent.kaos.readText).mockResolvedValue('# Design');
      const sm = new SessionMode(agent);
      // enter('design') and resolveFilePathFromModelRequest each call mkdir once.
      // The third mkdir call is from enter('plan') inside handoffTo — make it throw
      // with a non-permission error so the homedir fallback is not attempted.
      vi.mocked(agent.kaos.mkdir)
        .mockResolvedValue(undefined)  // default: succeed
      await sm.enter('design-id', undefined, false, 'design');
      await sm.resolveFilePathFromModelRequest('.ody-code/designs/foo.md', '# Design');
      vi.mocked(agent.kaos.mkdir).mockRejectedValue(new Error('disk full'));

      await expect(sm.handoffTo('plan')).rejects.toThrow('disk full');

      // Pending must be cleared — no ghost injection on next turn
      expect(sm.consumePendingHandoffForPlan()).toBeNull();
    });

    it('cancel() does NOT store a pending handoff', async () => {
      const agent = makeAgent();
      vi.mocked(agent.kaos.readText).mockResolvedValue('## Plan content');
      const sm = new SessionMode(agent);
      await sm.enter('plan-id', undefined, false, 'plan');
      await sm.resolveFilePathFromModelRequest('.ody-code/plans/my-plan.md', '## Plan content');

      sm.cancel();

      expect(sm.consumePendingHandoffForNormal()).toBeNull();
      expect(sm.consumePendingHandoffForPlan()).toBeNull();
    });
  });
});
