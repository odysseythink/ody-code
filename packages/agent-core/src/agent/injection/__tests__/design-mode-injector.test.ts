import { describe, it, expect } from 'vitest';
import type { Agent } from '../../..';
import { DesignModeInjector } from '../design-mode';

interface DesignModeStub {
  isActive: boolean;
  filePath?: string | null;
  content?: string;
  skillsReminder?: string;
  mockupAvailable?: boolean;
  handoff?: { path: string; filename: string; selectedLabel?: string } | null;
}

function designAgent(stub: DesignModeStub): Agent {
  const history: unknown[] = [];
  let pendingHandoff = stub.handoff ?? null;
  return {
    tools: {
      isToolActive: (name: string) =>
        stub.mockupAvailable === true && name === 'ShowDesignMockup',
    },
    sessionMode: {
      get isActive() { return stub.isActive; },
      get kind() { return 'design'; },
      get sessionModeFilePath() { return stub.filePath ?? null; },
      data: async () =>
        stub.content !== undefined
          ? { id: 'd1', content: stub.content, path: stub.filePath ?? '', kind: 'design' }
          : null,
      consumePendingHandoffForPlan: () => {
        const p = pendingHandoff;
        pendingHandoff = null;
        return p;
      },
    },
    context: {
      history,
      appendSystemReminder: (content: string) => {
        history.push({ role: 'user', content: [{ type: 'text', text: content }] });
      },
    },
    skills: {
      registry: {
        getUnavailableSkillsReminder: (_mode: string) => stub.skillsReminder ?? '',
      },
    },
  } as unknown as Agent;
}

function lastReminder(agent: Agent): string {
  const msgs = agent.context.history as Array<{ role: string; content?: Array<{ text?: string }> }>;
  const last = msgs.findLast((m) => m.role === 'user');
  return last?.content?.map((p) => p.text ?? '').join('') ?? '';
}

describe('DesignModeInjector', () => {
  it('injects reentry reminder containing "Re-entering Design Mode" when prior content exists', async () => {
    const stub: DesignModeStub = { isActive: true, filePath: '/tmp/design.md', content: '# existing design' };
    const agent = designAgent(stub);
    const injector = new DesignModeInjector(agent);
    await injector.inject();
    const text = lastReminder(agent);
    expect(text).toContain('Re-entering Design Mode');
  });

  it('shows visual companion when mockup is available', async () => {
    const stub: DesignModeStub = { isActive: true, filePath: '/tmp/design.md', mockupAvailable: true };
    const agent = designAgent(stub);
    const injector = new DesignModeInjector(agent);
    await injector.inject();
    const text = lastReminder(agent);
    expect(text).toContain('ShowDesignMockup IS available');
    expect(text).not.toContain('ShowDesignMockup is NOT available');
  });

  it('appends skills reminder when unavailable skills exist', async () => {
    const stub: DesignModeStub = { isActive: true, filePath: '/tmp/design.md', skillsReminder: 'Some design skills are unavailable.' };
    const agent = designAgent(stub);
    const injector = new DesignModeInjector(agent);
    await injector.inject();
    expect(lastReminder(agent)).toContain('Some design skills are unavailable.');
  });

  it('injects handoff to plan reminder on exit when handoff is pending', async () => {
    const stub: DesignModeStub = { isActive: true, filePath: '/tmp/design.md', handoff: { path: '/tmp/design.md', filename: 'design.md', selectedLabel: 'Approach B' } };
    const agent = designAgent(stub);
    const injector = new DesignModeInjector(agent);
    await injector.inject();
    stub.isActive = false;
    await injector.inject();
    const text = lastReminder(agent);
    expect(text).toContain('Design mode completed');
    expect(text).toContain('plan mode');
    expect(text).toContain('Selected approach: Approach B');
  });

  it('injects exit reminder on cancel when no handoff is pending', async () => {
    const stub: DesignModeStub = { isActive: true, filePath: '/tmp/design.md' };
    const agent = designAgent(stub);
    const injector = new DesignModeInjector(agent);
    await injector.inject();
    stub.isActive = false;
    await injector.inject();
    expect(lastReminder(agent)).toContain('Design mode was cancelled');
  });
});
