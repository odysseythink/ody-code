import { describe, it, expect, vi } from 'vitest';
import type { Agent } from '../../..';
import { SessionMode } from '../index';
import { ModeBehaviorRegistry } from '../behaviors/registry';
import { createDefaultModeBehaviorRegistry } from '../behaviors/registry';
import type { ModeEnterContext, ModeExitContext, SessionModeBehavior, SessionModeInjector } from '../behaviors/types';
import type { SessionModeKind } from '../types';

class FakePlanBehavior implements SessionModeBehavior<'plan'> {
  readonly kind = 'plan' as const;
  readonly outputSubdirectory = 'plans';
  readonly modeModelKey = 'plan';
  readonly injectorClass = class implements SessionModeInjector {
    readonly injectionVariant = 'fake_plan';
    onContextClear(): void {}
    onContextCompacted(): void {}
    onContextMessageRemoved(): void {}
    async inject(): Promise<void> {}
    getInjection(): undefined { return undefined; }
  };
  entered = false;
  async onEnter(ctx: ModeEnterContext): Promise<void> {
    this.entered = true;
  }
  async onExit(_ctx: ModeExitContext): Promise<void> {}
  async onCancel(_ctx: ModeExitContext): Promise<void> {}
}

function makeAgent(modelAlias = 'normal-model'): Agent {
  return {
    config: { modelAlias, update: vi.fn(), cwd: '/test', providerConfig: undefined },
    kaos: { mkdir: vi.fn().mockResolvedValue(undefined) },
    log: { debug: vi.fn(), warn: vi.fn(), info: vi.fn(), error: vi.fn() },
    records: { logRecord: vi.fn() },
    setContextMode: vi.fn(),
    emitStatusUpdated: vi.fn(),
    refreshLlm: vi.fn(),
    context: { history: [] },
    contexts: {},
    replayBuilder: { push: vi.fn() },
  } as unknown as Agent;
}

describe('SessionMode.enter delegation', () => {
  it('calls behavior.onEnter and sets active state', async () => {
    const behavior = new FakePlanBehavior();
    const registry = new ModeBehaviorRegistry();
    registry.register(behavior);
    const agent = makeAgent('normal-model');
    const sessionMode = new SessionMode(agent, registry);
    await sessionMode.enter('id-1', false, true, 'plan');
    expect(behavior.entered).toBe(true);
    expect(sessionMode.isActive).toBe(true);
    expect(sessionMode.kind).toBe('plan');
  });

  it('rolls back state when behavior.onEnter throws', async () => {
    const behavior = new FakePlanBehavior();
    behavior.onEnter = async () => { throw new Error('boom'); };
    const registry = new ModeBehaviorRegistry();
    registry.register(behavior);
    const agent = makeAgent();
    const sessionMode = new SessionMode(agent, registry);
    await expect(sessionMode.enter('id-1', false, true, 'plan')).rejects.toThrow('boom');
    expect(sessionMode.isActive).toBe(false);
  });

  it('logs session_mode.enter and sets context mode after behavior succeeds', async () => {
    const behavior = new FakePlanBehavior();
    const registry = new ModeBehaviorRegistry();
    registry.register(behavior);
    const agent = makeAgent();
    const sessionMode = new SessionMode(agent, registry);
    await sessionMode.enter('id-1', false, true, 'plan');
    expect(vi.mocked(agent.records.logRecord)).toHaveBeenCalledWith({ type: 'session_mode.enter', id: 'id-1', kind: 'plan' });
    expect(vi.mocked(agent.setContextMode)).toHaveBeenCalledWith('plan');
  });
});

describe('SessionMode.exit and cancel delegation', () => {
  it('delegates exit to behavior and restores state', async () => {
    const agent = makeAgent('normal-model');
    const sessionMode = new SessionMode(agent, createDefaultModeBehaviorRegistry());
    await sessionMode.enter('id-1', false, true, 'plan');
    sessionMode.exit();
    expect(sessionMode.isActive).toBe(false);
    expect(vi.mocked(agent.setContextMode)).toHaveBeenLastCalledWith('normal');
  });

  it('delegates cancel to behavior and restores state', async () => {
    const agent = makeAgent('normal-model');
    const sessionMode = new SessionMode(agent, createDefaultModeBehaviorRegistry());
    await sessionMode.enter('id-1', false, true, 'plan');
    sessionMode.cancel();
    expect(sessionMode.isActive).toBe(false);
    expect(vi.mocked(agent.setContextMode)).toHaveBeenLastCalledWith('normal');
  });

  it('exit is idempotent: exit twice logs only one session_mode.exit', async () => {
    const agent = makeAgent('normal-model');
    const sessionMode = new SessionMode(agent, createDefaultModeBehaviorRegistry());
    await sessionMode.enter('id-1', false, true, 'plan');
    sessionMode.exit();
    sessionMode.exit();
    const exitRecords = vi.mocked(agent.records.logRecord).mock.calls.filter(
      (call) => call[0].type === 'session_mode.exit',
    );
    expect(exitRecords).toHaveLength(1);
  });
});

describe('Design session tracking', () => {
  it('tracks design sessions only for design mode', async () => {
    const agent = makeAgent('normal-model');
    const sessionMode = new SessionMode(agent, createDefaultModeBehaviorRegistry());
    // Wire sessionMode onto the agent so DesignModeBehavior.onEnter can call startDesignSession
    (agent as unknown as { sessionMode: SessionMode }).sessionMode = sessionMode;
    await sessionMode.enter('id-1', false, true, 'design');
    expect(sessionMode.designSessions.length).toBe(1);
    sessionMode.exit();
    expect(sessionMode.designSessions[0]?.exitedAtMsg).toBeDefined();
  });
});
