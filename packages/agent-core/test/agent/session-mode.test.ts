import { describe, it, expect, vi, beforeEach } from 'vitest';
import { join } from 'pathe';
import { SessionMode } from '../../src/agent/session-mode';
import { stripDatePrefix } from '../../src/agent/session-mode/topic-generator';
import type { Agent } from '../../src/agent';

const CWD = '/workspace/project';

function makeAgent(overrides: {
  existingPaths?: Set<string>;
  modelAlias?: string;
  kimiConfig?: { defaultModel?: string; modeModels?: { plan?: string; design?: string } };
  modelProvider?: { resolveProviderConfig: ReturnType<typeof vi.fn>; resolveAuth?: ReturnType<typeof vi.fn> };
} = {}): Agent {
  const existing = overrides.existingPaths ?? new Set<string>();
  return {
    homedir: '/home/user',
    config: { cwd: CWD, modelAlias: overrides.modelAlias ?? 'test', provider: 'test', update: vi.fn() },
    kimiConfig: overrides.kimiConfig,
    modelProvider: overrides.modelProvider,
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
    log: { warn: vi.fn(), error: vi.fn(), info: vi.fn(), debug: vi.fn() },
    refreshLlm: vi.fn(),
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

    it('switches to the modeModels model when it has an API key', async () => {
      const agent = makeAgent({
        modelAlias: 'design-model',
        kimiConfig: {
          modeModels: { plan: 'plan-model' },
        },
        modelProvider: {
          resolveProviderConfig: vi.fn().mockReturnValue({
            providerName: 'test-p',
            provider: { type: 'openai', apiKey: 'sk-test' },
            modelCapabilities: {},
          }),
        },
      });
      const sm = new SessionMode(agent);
      await sm.enter('id-1', undefined, false, 'plan');
      expect(agent.config.update).toHaveBeenCalledWith({ modelAlias: 'plan-model' });
    });

    it('switches to the modeModels model when it uses OAuth', async () => {
      const agent = makeAgent({
        modelAlias: 'design-model',
        kimiConfig: {
          modeModels: { plan: 'plan-model' },
        },
        modelProvider: {
          resolveProviderConfig: vi.fn().mockReturnValue({
            providerName: 'managed:ody-code',
            provider: { type: 'kimi', apiKey: '' },
            modelCapabilities: {},
          }),
          resolveAuth: vi.fn().mockReturnValue(vi.fn()),
        },
      });
      const sm = new SessionMode(agent);
      await sm.enter('id-1', undefined, false, 'plan');
      expect(agent.config.update).toHaveBeenCalledWith({ modelAlias: 'plan-model' });
    });

    it('keeps the current model when modeModels model lacks API key and OAuth', async () => {
      const agent = makeAgent({
        modelAlias: 'design-model',
        kimiConfig: {
          modeModels: { plan: 'plan-model' },
        },
        modelProvider: {
          resolveProviderConfig: vi.fn().mockReturnValue({
            providerName: 'test-p',
            provider: { type: 'openai', apiKey: '' },
            modelCapabilities: {},
          }),
        },
      });
      const sm = new SessionMode(agent);
      await sm.enter('id-1', undefined, false, 'plan');
      expect(agent.config.update).not.toHaveBeenCalled();
      expect(agent.log.warn).toHaveBeenCalledWith(
        expect.stringContaining('no configured API key or OAuth login'),
      );
    });

    it('keeps the current model when modeModels model is not found', async () => {
      const agent = makeAgent({
        modelAlias: 'design-model',
        kimiConfig: {
          modeModels: { plan: 'unknown-model' },
        },
        modelProvider: {
          resolveProviderConfig: vi.fn().mockImplementation(() => {
            throw new Error('not found');
          }),
        },
      });
      const sm = new SessionMode(agent);
      await sm.enter('id-1', undefined, false, 'plan');
      expect(agent.config.update).not.toHaveBeenCalled();
      expect(agent.log.warn).toHaveBeenCalledWith(expect.stringContaining('not found'));
    });

    it('invalidates the cached LLM when switching to a modeModels model', async () => {
      const agent = makeAgent({
        modelAlias: 'design-model',
        kimiConfig: { modeModels: { plan: 'plan-model' } },
        modelProvider: {
          resolveProviderConfig: vi.fn().mockReturnValue({
            providerName: 'test-p',
            provider: { type: 'openai', apiKey: 'sk-test' },
            modelCapabilities: {},
          }),
        },
      });
      const sm = new SessionMode(agent);
      await sm.enter('id-1', undefined, false, 'plan');
      expect(agent.refreshLlm).toHaveBeenCalled();
    });

    it('does not invalidate the cached LLM when the modeModels model equals the current model', async () => {
      const agent = makeAgent({
        modelAlias: 'same-model',
        kimiConfig: { modeModels: { plan: 'same-model' } },
        modelProvider: {
          resolveProviderConfig: vi.fn().mockReturnValue({
            providerName: 'test-p',
            provider: { type: 'openai', apiKey: 'sk-test' },
            modelCapabilities: {},
          }),
        },
      });
      const sm = new SessionMode(agent);
      await sm.enter('id-1', undefined, false, 'plan');
      expect(agent.refreshLlm).not.toHaveBeenCalled();
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

    it('invalidates the cached LLM when exit restores the previous model', () => {
      const agent = makeAgent({
        modelAlias: 'design-model',
        kimiConfig: { defaultModel: 'normal-model' },
      });
      const sm = new SessionMode(agent);
      sm.restoreEnter({ id: 'id', kind: 'design' });
      sm.exit();
      expect(agent.refreshLlm).toHaveBeenCalled();
    });

    it('invalidates the cached LLM when cancel restores the previous model', () => {
      const agent = makeAgent({
        modelAlias: 'plan-model',
        kimiConfig: { defaultModel: 'normal-model' },
      });
      const sm = new SessionMode(agent);
      sm.restoreEnter({ id: 'id', kind: 'plan' });
      sm.cancel();
      expect(agent.refreshLlm).toHaveBeenCalled();
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
    it('handoffTo("plan") exits design, enters plan, stores path/filename artifact', async () => {
      const agent = makeAgent();
      vi.mocked(agent.kaos.readText).mockResolvedValue('# My Design\n\nSome content');
      const sm = new SessionMode(agent);
      await sm.enter('design-id', undefined, false, 'design');
      await sm.resolveFilePathFromModelRequest('.ody-code/designs/my-feature.md', '# My Design\nSome content');

      vi.mocked(agent.records.logRecord).mockClear();

      await sm.handoffTo('plan');

      expect(sm.isActive).toBe(true);
      expect(sm.kind).toBe('plan');

      const handoff = sm.consumePendingHandoffForPlan();
      expect(handoff).not.toBeNull();
      expect(handoff).not.toHaveProperty('content');
      expect(handoff?.path).toMatch(/my-feature\.md$/);
      expect(handoff?.filename).toMatch(/my-feature\.md$/);

      expect(sm.consumePendingHandoffForPlan()).toBeNull();
    });

    it('handoffTo("plan") stores selectedLabel when provided', async () => {
      const agent = makeAgent();
      vi.mocked(agent.kaos.readText).mockResolvedValue('# My Design');
      const sm = new SessionMode(agent);
      await sm.enter('design-id', undefined, false, 'design');
      await sm.resolveFilePathFromModelRequest('.ody-code/designs/my-feature.md', '# My Design');

      await sm.handoffTo('plan', { selectedLabel: 'Approach A' });

      const handoff = sm.consumePendingHandoffForPlan();
      expect(handoff?.selectedLabel).toBe('Approach A');
    });

    it('handoffTo("plan") stores artifact when content is empty but path exists', async () => {
      const agent = makeAgent();
      vi.mocked(agent.kaos.readText).mockResolvedValue('');
      const sm = new SessionMode(agent);
      await sm.enter('design-id', undefined, false, 'design');
      await sm.resolveFilePathFromModelRequest('.ody-code/designs/my-feature.md', '');

      await sm.handoffTo('plan');

      const handoff = sm.consumePendingHandoffForPlan();
      expect(handoff).not.toBeNull();
      expect(handoff?.path).toMatch(/my-feature\.md$/);
      expect(handoff?.filename).toMatch(/my-feature\.md$/);
    });

    it('handoffTo("plan") stores null artifact when no file path is set', async () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      await sm.enter('design-id', undefined, false, 'design');

      await sm.handoffTo('plan');

      expect(sm.consumePendingHandoffForPlan()).toBeNull();
    });

    it('handoffTo("normal") exits plan, stores content/path artifact unchanged', async () => {
      const agent = makeAgent();
      vi.mocked(agent.kaos.readText).mockResolvedValue('## Step 1\n\nDo this');
      const sm = new SessionMode(agent);
      await sm.enter('plan-id', undefined, false, 'plan');
      await sm.resolveFilePathFromModelRequest('.ody-code/plans/my-plan.md', '## Step 1\nDo this');

      vi.mocked(agent.records.logRecord).mockClear();

      await sm.handoffTo('normal');

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
      vi.mocked(agent.kaos.mkdir).mockResolvedValue(undefined);
      await sm.enter('design-id', undefined, false, 'design');
      await sm.resolveFilePathFromModelRequest('.ody-code/designs/foo.md', '# Design');
      vi.mocked(agent.kaos.mkdir).mockRejectedValue(new Error('disk full'));

      await expect(sm.handoffTo('plan')).rejects.toThrow('disk full');

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

  describe('design session tracking', () => {
    it('records a design session on enter and closes it on exit', async () => {
      const agent = { ...makeAgent(), context: { history: [{}, {}, {}] } } as unknown as Agent;
      const sm = new SessionMode(agent);
      await sm.enter('design-id', undefined, false, 'design');

      expect(sm.designSessions).toHaveLength(1);
      expect(sm.designSessions[0]!.designSessionID).toBe('design-id');
      expect(sm.designSessions[0]!.startedAtMsg).toBe(3);

      await sm.resolveFilePathFromModelRequest('.ody-code/designs/foo.md', '# Design');
      sm.exit();

      expect(sm.designSessions[0]!.exitedAtMsg).toBe(3);
      expect(sm.designSessions[0]!.approvedPath).toMatch(/foo\.md$/);
    });

    it('records a cancelled design session without an approved path', async () => {
      const agent = { ...makeAgent(), context: { history: [{}, {}] } } as unknown as Agent;
      const sm = new SessionMode(agent);
      await sm.enter('design-id', undefined, false, 'design');
      sm.cancel();

      expect(sm.designSessions[0]!.exitedAtMsg).toBe(2);
      expect(sm.designSessions[0]!.approvedPath).toBeUndefined();
    });

    it('restores design sessions from a checkpoint', async () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      sm.restoreDesignSessions([
        {
          designSessionID: 'd1',
          startedAtMsg: 0,
          exitedAtMsg: 5,
          approvedPath: '/design.md',
        },
      ]);

      expect(sm.designSessions).toHaveLength(1);
      expect(sm.designSessions[0]!.designSessionID).toBe('d1');
    });

    it('leaves exitedAtMsg undefined when context was cleared before exit', async () => {
      const agent = { ...makeAgent(), context: { history: [{}, {}, {}] } } as unknown as Agent;
      const sm = new SessionMode(agent);
      await sm.enter('design-id', undefined, false, 'design');

      expect(sm.designSessions[0]!.startedAtMsg).toBe(3);

      // Simulate context clear (e.g. from replay of a `context.clear` WAL record).
      (agent as unknown as { context: { history: unknown[] } }).context.history = [];

      sm.exit();

      // exitedAtMsg must stay undefined — setting it to 0 would corrupt checkpoints.
      expect(sm.designSessions[0]!.exitedAtMsg).toBeUndefined();
    });

    it('does not track sessions for plan mode', async () => {
      const agent = makeAgent();
      const sm = new SessionMode(agent);
      await sm.enter('plan-id', undefined, false, 'plan');
      sm.exit();

      expect(sm.designSessions).toHaveLength(0);
    });
  });
});
