import { describe, it, expect, vi } from 'vitest';
import type { Agent } from '../../..';
import { BaseSessionModeInjector } from '../session-mode-injector';
import type { SessionModeInjectorOptions } from '../../session-mode/behaviors';

class TestInjector extends BaseSessionModeInjector {
  readonly injectionVariant = 'test_mode';
  readonly options: SessionModeInjectorOptions = { fullRefreshTurns: 5, dedupMinTurns: 2 };
  active = false;
  protected wasActive = false;

  isModeActive(): boolean {
    return this.active;
  }

  computeVariantPublic(
    injectedAt: number | null,
    history: { role: string }[],
    options: SessionModeInjectorOptions,
  ): 'full' | 'sparse' | null {
    return this.computeVariant(injectedAt, history, options);
  }

  protected getEntryReminder(_path: string | null): string {
    return 'entry';
  }
  protected getReentryReminder(_path: string | null): string {
    return 'reentry';
  }
  protected getFullReminder(_path: string | null): string {
    return 'full';
  }
  protected getSparseReminder(_path: string | null): string {
    return 'sparse';
  }
  protected getExitReminder(_path: string | null): string {
    return 'exit';
  }

  // No getInjection override — uses BaseSessionModeInjector.getInjection()
}

function makeAgent(overrides: {
  isActive?: boolean;
  kind?: string;
  filePath?: string | null;
  content?: string;
  history?: { role: string }[];
} = {}): Agent {
  return {
    sessionMode: {
      isActive: overrides.isActive ?? false,
      kind: overrides.kind ?? 'plan',
      sessionModeFilePath: overrides.filePath ?? null,
      data: vi.fn().mockResolvedValue(
        overrides.content ? { content: overrides.content } : null,
      ),
    },
    context: {
      history: overrides.history ?? [],
      appendSystemReminder: vi.fn(),
    },
  } as unknown as Agent;
}

describe('BaseSessionModeInjector', () => {
  it('computeVariant returns full when injectedAt is null', () => {
    const injector = new TestInjector(makeAgent());
    expect(
      injector.computeVariantPublic(null, [], { fullRefreshTurns: 5, dedupMinTurns: 2 }),
    ).toBe('full');
  });

  it('computeVariant returns null with only one assistant turn', () => {
    const injector = new TestInjector(makeAgent());
    const history = [{ role: 'assistant' }];
    expect(
      injector.computeVariantPublic(0, history, { fullRefreshTurns: 5, dedupMinTurns: 2 }),
    ).toBeNull();
  });

  // injectedAt = 0 means the injection sits at index 0; assistant turns are
  // counted from index 1 onward. So 3 assistant entries → 2 counted → 'sparse'.
  it('computeVariant returns sparse at dedup threshold', () => {
    const injector = new TestInjector(makeAgent());
    const history = Array.from({ length: 3 }, () => ({ role: 'assistant' }));
    expect(
      injector.computeVariantPublic(0, history, { fullRefreshTurns: 5, dedupMinTurns: 2 }),
    ).toBe('sparse');
  });

  // 6 assistant entries → 5 counted → 'full'
  it('computeVariant returns full at refresh threshold', () => {
    const injector = new TestInjector(makeAgent());
    const history = Array.from({ length: 6 }, () => ({ role: 'assistant' }));
    expect(
      injector.computeVariantPublic(0, history, { fullRefreshTurns: 5, dedupMinTurns: 2 }),
    ).toBe('full');
  });

  it('computeVariant returns full when user message appears after injection', () => {
    const injector = new TestInjector(makeAgent());
    const history = [{ role: 'assistant' }, { role: 'user' }];
    expect(
      injector.computeVariantPublic(0, history, { fullRefreshTurns: 5, dedupMinTurns: 2 }),
    ).toBe('full');
  });

  it('inject appends system reminder with injection origin', async () => {
    const agent = makeAgent({ isActive: true });
    const injector = new TestInjector(agent);
    injector.active = true;
    // Override getInjection to return a value
    injector.getInjection = vi.fn().mockResolvedValue('test injection');
    await injector.inject();
    expect(agent.context.appendSystemReminder).toHaveBeenCalledWith('test injection', {
      kind: 'injection',
      variant: 'test_mode',
    });
  });

  it('injects entry reminder when mode becomes active', async () => {
    const agent = makeAgent({ isActive: true, filePath: '/tmp/test.md' });
    const injector = new TestInjector(agent);
    injector.active = true;
    await injector.inject();
    expect(agent.context.appendSystemReminder).toHaveBeenCalledWith(
      'entry',
      expect.objectContaining({ kind: 'injection' }),
    );
  });

  it('injects reentry reminder when mode has prior content', async () => {
    const agent = makeAgent({
      isActive: true,
      filePath: '/tmp/test.md',
      content: '# existing content',
    });
    const injector = new TestInjector(agent);
    injector.active = true;
    await injector.inject();
    expect(agent.context.appendSystemReminder).toHaveBeenCalledWith(
      'reentry',
      expect.objectContaining({ kind: 'injection' }),
    );
  });

  it('injects exit reminder when mode turns off after being active', async () => {
    const agent = makeAgent({ isActive: true, filePath: '/tmp/test.md' });
    const injector = new TestInjector(agent);
    injector.active = true;
    await injector.inject();
    injector.active = false;
    await injector.inject();
    expect(agent.context.appendSystemReminder).toHaveBeenCalledWith(
      'exit',
      expect.objectContaining({ kind: 'injection' }),
    );
  });

  it('does not inject anything when mode is inactive from the start', async () => {
    const agent = makeAgent({ isActive: false });
    const injector = new TestInjector(agent);
    await injector.inject();
    expect(agent.context.appendSystemReminder).not.toHaveBeenCalled();
  });

  it('injects full reminder on cadence refresh', async () => {
    const agent = makeAgent({ isActive: true, filePath: '/tmp/test.md' });
    const injector = new TestInjector(agent);
    injector.active = true;
    // First injection sets wasActive; injectedAt = 0 (empty history)
    await injector.inject();
    vi.mocked(agent.context.appendSystemReminder).mockClear();
    // Push enough assistant turns (6) so 5 are counted after index 0 → 'full'
    const history = agent.context.history as { role: string }[];
    for (let i = 0; i < 6; i++) {
      history.push({ role: 'assistant' });
    }
    await injector.inject();
    expect(agent.context.appendSystemReminder).toHaveBeenCalledWith(
      'full',
      expect.objectContaining({ kind: 'injection' }),
    );
  });

  it('injects sparse reminder at dedup threshold', async () => {
    const agent = makeAgent({ isActive: true, filePath: '/tmp/test.md' });
    const injector = new TestInjector(agent);
    injector.active = true;
    // First injection sets wasActive; injectedAt = 0
    await injector.inject();
    vi.mocked(agent.context.appendSystemReminder).mockClear();
    // Push 3 assistants → 2 counted after index 0 → 'sparse'
    const history = agent.context.history as { role: string }[];
    history.push({ role: 'assistant' }, { role: 'assistant' }, { role: 'assistant' });
    await injector.inject();
    expect(agent.context.appendSystemReminder).toHaveBeenCalledWith(
      'sparse',
      expect.objectContaining({ kind: 'injection' }),
    );
  });
});
