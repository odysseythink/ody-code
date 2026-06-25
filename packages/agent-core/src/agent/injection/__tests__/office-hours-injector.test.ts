import { describe, it, expect } from 'vitest';
import type { Agent } from '../../..';
import { OfficeHoursInjector } from '../office-hours';

interface OfficeHoursStub {
  isActive: boolean;
  filePath?: string | null;
  content?: string;
}

function makeAgent(stub: OfficeHoursStub): Agent {
  const history: unknown[] = [];
  return {
    sessionMode: {
      get isActive() { return stub.isActive; },
      get kind() { return 'office-hours'; },
      get sessionModeFilePath() { return stub.filePath ?? null; },
      data: async () =>
        stub.content !== undefined
          ? { id: 'oh1', content: stub.content, path: stub.filePath ?? '', kind: 'office-hours' }
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

describe('OfficeHoursInjector', () => {
  it('injects entry reminder when mode becomes active with empty content', async () => {
    const stub: OfficeHoursStub = { isActive: true, filePath: '/tmp/office-hours.md' };
    const agent = makeAgent(stub);
    const injector = new OfficeHoursInjector(agent);
    await injector.inject();
    expect(lastReminder(agent)).toContain('Office hours is now active');
  });

  it('injects reentry reminder when prior content exists', async () => {
    const stub: OfficeHoursStub = { isActive: true, filePath: '/tmp/office-hours.md', content: '# prior session' };
    const agent = makeAgent(stub);
    const injector = new OfficeHoursInjector(agent);
    await injector.inject();
    const text = lastReminder(agent);
    expect(text).toContain('Office hours');
    expect(text).not.toContain('now active');
  });

  it('injects exit reminder when mode turns off', async () => {
    const stub: OfficeHoursStub = { isActive: true, filePath: '/tmp/office-hours.md' };
    const agent = makeAgent(stub);
    const injector = new OfficeHoursInjector(agent);
    await injector.inject();
    stub.isActive = false;
    await injector.inject();
    expect(lastReminder(agent)).toContain('Office hours session complete');
  });
});
