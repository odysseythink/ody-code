import { describe, it, expect } from 'vitest';
import type { Agent } from '../../..';
import { PlanModeInjector } from '../plan-mode';

interface PlanModeStub {
  isActive: boolean;
  filePath?: string | null;
  content?: string;
  skillsReminder?: string;
  handoff?: { content: string; path: string; selectedLabel?: string } | null;
}

function planAgent(stub: PlanModeStub): Agent {
  const history: unknown[] = [];
  let pendingHandoff = stub.handoff ?? null;
  return {
    sessionMode: {
      get isActive() { return stub.isActive; },
      get kind() { return 'plan'; },
      get sessionModeFilePath() { return stub.filePath ?? null; },
      data: async () =>
        stub.content !== undefined
          ? { id: 'p1', content: stub.content, path: stub.filePath ?? '', kind: 'plan' }
          : null,
      consumePendingHandoffForNormal: () => {
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
  const msgs = agent.context.history as ReadonlyArray<{ role: string; content?: ReadonlyArray<{ text?: string }> }>;
  const last = msgs.findLast((m) => m.role === 'user');
  return last?.content?.map((p) => p.text ?? '').join('') ?? '';
}

describe('PlanModeInjector', () => {
  it('injects reentry reminder containing "Re-entering Plan Mode" when prior content exists', async () => {
    const stub: PlanModeStub = { isActive: true, filePath: '/tmp/plan.md', content: '# existing plan' };
    const agent = planAgent(stub);
    const injector = new PlanModeInjector(agent);
    await injector.inject();
    expect(lastReminder(agent)).toContain('Re-entering Plan Mode');
  });

  it('injects full reminder with rubric when content is empty', async () => {
    const stub: PlanModeStub = { isActive: true, filePath: '/tmp/plan.md' };
    const agent = planAgent(stub);
    const injector = new PlanModeInjector(agent);
    await injector.inject();
    const text = lastReminder(agent);
    expect(text).toContain('Plan mode is active');
    expect(text).toContain('Plan file: /tmp/plan.md');
  });

  it('appends skills reminder when unavailable skills exist', async () => {
    const stub: PlanModeStub = { isActive: true, filePath: '/tmp/plan.md', skillsReminder: 'Some skills are unavailable in plan mode.' };
    const agent = planAgent(stub);
    const injector = new PlanModeInjector(agent);
    await injector.inject();
    const text = lastReminder(agent);
    expect(text).toContain('Some skills are unavailable in plan mode.');
  });

  it('injects handoff reminder on exit when handoff is pending', async () => {
    const stub: PlanModeStub = { isActive: true, filePath: '/tmp/plan.md', handoff: { content: '## Approved Plan\n\nDo this.', path: '/tmp/plan.md', selectedLabel: 'Approach A' } };
    const agent = planAgent(stub);
    const injector = new PlanModeInjector(agent);
    // First inject activates mode
    await injector.inject();
    // Deactivate and inject again → should produce handoff
    stub.isActive = false;
    await injector.inject();
    const text = lastReminder(agent);
    expect(text).toContain('Plan mode is no longer active');
    expect(text).toContain('## Approved Plan');
    expect(text).toContain('Selected approach: Approach A');
  });

  it('injects exit reminder on cancel when no handoff is pending', async () => {
    const stub: PlanModeStub = { isActive: true, filePath: '/tmp/plan.md' };
    const agent = planAgent(stub);
    const injector = new PlanModeInjector(agent);
    await injector.inject();
    stub.isActive = false;
    await injector.inject();
    expect(lastReminder(agent)).toContain('Plan mode was cancelled');
  });
});
