import { describe, it, expect } from 'vitest';
import type { Agent } from '../../..';
import { ProductInjector } from '../product';

interface ProductStub {
  isActive: boolean;
  filePath?: string | null;
  content?: string;
}

function makeAgent(stub: ProductStub): Agent {
  const history: unknown[] = [];
  return {
    sessionMode: {
      get isActive() { return stub.isActive; },
      get kind() { return 'product'; },
      get sessionModeFilePath() { return stub.filePath ?? null; },
      data: async () =>
        stub.content !== undefined
          ? { id: 'oh1', content: stub.content, path: stub.filePath ?? '', kind: 'product' }
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

describe('ProductInjector', () => {
  it('injects entry reminder when mode becomes active with empty content', async () => {
    const stub: ProductStub = { isActive: true, filePath: '/tmp/product.md' };
    const agent = makeAgent(stub);
    const injector = new ProductInjector(agent);
    await injector.inject();
    expect(lastReminder(agent)).toContain('Office hours is now active');
  });

  it('injects reentry reminder when prior content exists', async () => {
    const stub: ProductStub = { isActive: true, filePath: '/tmp/product.md', content: '# prior session' };
    const agent = makeAgent(stub);
    const injector = new ProductInjector(agent);
    await injector.inject();
    const text = lastReminder(agent);
    expect(text).toContain('Office hours');
    expect(text).not.toContain('now active');
  });

  it('injects exit reminder when mode turns off', async () => {
    const stub: ProductStub = { isActive: true, filePath: '/tmp/product.md' };
    const agent = makeAgent(stub);
    const injector = new ProductInjector(agent);
    await injector.inject();
    stub.isActive = false;
    await injector.inject();
    expect(lastReminder(agent)).toContain('Office hours session complete');
  });
});
