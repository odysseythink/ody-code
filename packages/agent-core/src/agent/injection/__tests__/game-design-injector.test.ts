import { describe, it, expect } from 'vitest';
import type { Agent } from '../../..';
import { GameDesignInjector } from '../game-design';

interface GameDesignStub {
  isActive: boolean;
  filePath?: string | null;
  content?: string;
}

function makeAgent(stub: GameDesignStub): Agent {
  const history: unknown[] = [];
  return {
    sessionMode: {
      get isActive() { return stub.isActive; },
      get kind() { return 'game-design'; },
      get sessionModeFilePath() { return stub.filePath ?? null; },
      data: async () =>
        stub.content !== undefined
          ? { id: 'gd1', content: stub.content, path: stub.filePath ?? '', kind: 'game-design' }
          : null,
    },
    context: {
      history,
      appendSystemReminder: (content: string) => {
        history.push({ role: 'user', content: [{ type: 'text', text: content }] });
      },
    },
  } as unknown as Agent;
}

function lastReminder(agent: Agent): string {
  const msgs = agent.context.history as ReadonlyArray<{ role: string; content?: ReadonlyArray<{ text?: string }> }>;
  const last = msgs.findLast((m) => m.role === 'user');
  return last?.content?.map((p) => p.text ?? '').join('') ?? '';
}

describe('GameDesignInjector', () => {
  it('injects entry reminder when mode becomes active with empty content', async () => {
    const stub: GameDesignStub = { isActive: true, filePath: '/tmp/game-design.md' };
    const agent = makeAgent(stub);
    const injector = new GameDesignInjector(agent);
    await injector.inject();
    expect(lastReminder(agent)).toContain('game-design mode is now active');
  });

  it('injects reentry reminder when prior content exists', async () => {
    const stub: GameDesignStub = { isActive: true, filePath: '/tmp/game-design.md', content: '# prior design' };
    const agent = makeAgent(stub);
    const injector = new GameDesignInjector(agent);
    await injector.inject();
    expect(lastReminder(agent)).toContain('game-design resumed');
  });

  it('injects exit reminder when mode turns off', async () => {
    const stub: GameDesignStub = { isActive: true, filePath: '/tmp/game-design.md' };
    const agent = makeAgent(stub);
    const injector = new GameDesignInjector(agent);
    await injector.inject();
    stub.isActive = false;
    await injector.inject();
    expect(lastReminder(agent)).toContain('game-design session complete');
  });
});
