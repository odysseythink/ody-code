import { describe, it, expect, vi } from 'vitest';
import { GameDesignInjector } from '../../../src/agent/injection/game-design';
import {
  gameDesignEntryReminder,
  gameDesignExitReminder,
} from '../../../src/agent/injection/game-design-contract';

function mockAgent(overrides: Record<string, unknown> = {}) {
  return {
    sessionMode: {
      isActive: false,
      kind: 'game-design',
      sessionModeFilePath: '/fake/.ody-code/game-design/game-design.md',
      data: vi.fn().mockResolvedValue({ content: '' }),
    },
    context: {
      history: [],
    },
    ...overrides,
  } as any;
}

describe('GameDesignInjector', () => {
  it('returns entry reminder when mode becomes active with empty doc', async () => {
    const agent = mockAgent({
      sessionMode: {
        isActive: true,
        kind: 'game-design',
        sessionModeFilePath: '/fake/.ody-code/game-design/game-design.md',
        data: vi.fn().mockResolvedValue({ content: '' }),
      },
    });
    const injector = new GameDesignInjector(agent);
    const result = await injector.getInjection();
    expect(result).toContain('game-design mode is now active');
    expect(result).toContain('Phase 1: 概念定义');
  });

  it('returns exit reminder when mode deactivated after being active', async () => {
    const agent = mockAgent({
      sessionMode: {
        isActive: false,
        kind: 'game-design',
        sessionModeFilePath: '/fake/.ody-code/game-design/game-design.md',
      },
    });
    const injector = new GameDesignInjector(agent);
    // Mark wasActive internally
    (injector as any).wasActive = true;
    const result = await injector.getInjection();
    expect(result).toContain('game-design session complete');
  });

  it('returns undefined when mode never active', async () => {
    const agent = mockAgent();
    const injector = new GameDesignInjector(agent);
    const result = await injector.getInjection();
    expect(result).toBeUndefined();
  });
});

describe('gameDesignEntryReminder', () => {
  it('contains LANG_INSTRUCTION and Phase 1 heading', () => {
    const path = '/fake/.ody-code/game-design/game-design.md';
    const msg = gameDesignEntryReminder(path);
    expect(msg).toContain('**Language:**');
    expect(msg).toContain('Phase 1: 概念定义');
    expect(msg).toContain(path);
  });
});

describe('gameDesignExitReminder', () => {
  it('reports completion with path', () => {
    const path = '/fake/.ody-code/game-design/game-design.md';
    const msg = gameDesignExitReminder(path);
    expect(msg).toContain('game-design session complete');
    expect(msg).toContain(path);
  });

  it('reports no document when path is null', () => {
    const msg = gameDesignExitReminder(null);
    expect(msg).toContain('no design document');
  });
});
