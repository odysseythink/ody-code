import { describe, expect, it } from 'vitest';

import type { Agent } from '../../../src/agent';
import { DesignModeInjector } from '../../../src/agent/injection/design-mode';
import { designModeEntryMessage } from '../../../src/agent/injection/design-mode-contract';

interface DesignModeStub {
  isActive: boolean;
  planFilePath?: string | null;
  /** Content returned by planMode.data(); when set, triggers the reentry variant. */
  content?: string;
  /** When true, the host advertises openExternal so ShowDesignMockup is registered. */
  mockupAvailable?: boolean;
}

function designAgent(stub: DesignModeStub): Agent {
  const history: unknown[] = [];
  const rpc =
    stub.mockupAvailable === true
      ? { openExternal: async () => ({ opened: true }) }
      : undefined;
  return {
    type: 'main',
    rpc,
    // The injector reads tool visibility (enabled + registered), not raw rpc.
    tools: {
      isToolActive: (name: string) =>
        stub.mockupAvailable === true && name === 'ShowDesignMockup',
    },
    planMode: {
      get isActive() {
        return stub.isActive;
      },
      get kind() {
        return 'design';
      },
      get planFilePath() {
        return stub.planFilePath ?? null;
      },
      data: async () =>
        stub.content === undefined
          ? null
          : { id: 'd1', content: stub.content, path: stub.planFilePath ?? '', kind: 'design' },
    },
    context: {
      history,
      appendSystemReminder: (content: string) => {
        history.push({ role: 'user', content: [{ type: 'text', text: content }] });
      },
    },
  } as unknown as Agent;
}

function history(agent: Agent): Array<{ role: string; content?: ReadonlyArray<{ text?: string }> }> {
  return agent.context.history as unknown as Array<{
    role: string;
    content?: ReadonlyArray<{ text?: string }>;
  }>;
}

function lastReminder(agent: Agent): string {
  const last = history(agent).findLast((message) => message.role === 'user');
  return last?.content?.map((part) => part.text ?? '').join('') ?? '';
}

describe('DesignModeInjector content', () => {
  it('injects the full reminder with the brainstorming contract and design file footer', async () => {
    const agent = designAgent({ isActive: true, planFilePath: '/tmp/design.md' });
    const injector = new DesignModeInjector(agent);

    await injector.inject();
    const text = lastReminder(agent);

    expect(text).toContain('Design mode is active');
    // Audit gate + seven dimensions.
    expect(text).toContain('Step 0 — Audit strategy gate');
    expect(text).toContain('Observability');
    expect(text).toContain('Operations');
    // Strengthened anti-premature-design guard.
    expect(text).toContain('HARD STOP before Step 2');
    expect(text).toContain('Verify it in the code');
    // Conditional upstream-inventory branch + the fourth tag.
    expect(text).toContain('Step 0.5 — Upstream inventory');
    expect(text).toContain('[C:UPSTREAM]');
    // Document-fidelity rubric.
    expect(text).toContain('Call-site integration');
    expect(text).toContain('Risk register');
    // Self-review + consolidated audit gate.
    expect(text).toContain('Step 4.5');
    // Visual companion section.
    expect(text).toContain('Visual companion');
    expect(text).toContain('ShowDesignMockup');
    expect(text).toContain('Design file: /tmp/design.md');
  });

  it('keeps the entry message and the full reminder in sync (shared contract)', async () => {
    const agent = designAgent({ isActive: true, planFilePath: '/tmp/design.md' });
    const injector = new DesignModeInjector(agent);
    await injector.inject();
    const full = lastReminder(agent);
    const entry = designModeEntryMessage('/tmp/design.md', false);

    // Both compose from the same contract fragments, so the fidelity rubric,
    // the upstream branch, and the audit gate must appear in both.
    for (const marker of [
      'Step 0 — Audit strategy gate',
      'Step 0.5 — Upstream inventory',
      'Call-site integration',
      'Step 4.5',
      '[C:UPSTREAM]',
    ]) {
      expect(full).toContain(marker);
      expect(entry).toContain(marker);
    }
    expect(entry).toContain('Design mode is now active');
  });

  it('tells the model ShowDesignMockup IS available when the host advertises openExternal', async () => {
    const agent = designAgent({
      isActive: true,
      planFilePath: '/tmp/design.md',
      mockupAvailable: true,
    });
    const injector = new DesignModeInjector(agent);

    await injector.inject();
    const text = lastReminder(agent);

    expect(text).toContain('ShowDesignMockup IS available');
    expect(text).toContain('ONLY use ShowDesignMockup when');
    expect(text).toContain('DO NOT use ShowDesignMockup for non-visual content');
    // Multi-scheme guidance: render candidates side by side, do not describe them.
    expect(text).toContain('side by side in a single HTML document');
    expect(text).not.toContain('ShowDesignMockup is NOT available');
  });

  it('tells the model ShowDesignMockup is NOT available when the host lacks openExternal', async () => {
    const agent = designAgent({ isActive: true, planFilePath: '/tmp/design.md' });
    const injector = new DesignModeInjector(agent);

    await injector.inject();
    const text = lastReminder(agent);

    expect(text).toContain('ShowDesignMockup is NOT available');
    expect(text).not.toContain('ShowDesignMockup IS available');
  });

  it('resolves the turn-discipline / visual-companion contradiction in the full reminder', async () => {
    const agent = designAgent({
      isActive: true,
      planFilePath: '/tmp/design.md',
      mockupAvailable: true,
    });
    const injector = new DesignModeInjector(agent);

    await injector.inject();
    const text = lastReminder(agent);

    // Turn must still end with a question/exit, AND rendering is explicitly a
    // within-turn tool call that does not end it — the two no longer conflict.
    expect(text).toContain('Your turn must end with either AskUserQuestion or ExitDesignMode');
    expect(text).toContain('do not count as ending it');
    expect(text).toContain('it does NOT end the turn');
  });

  it('keeps the available visual-companion guidance identical between entry and full reminder', async () => {
    const agent = designAgent({
      isActive: true,
      planFilePath: '/tmp/design.md',
      mockupAvailable: true,
    });
    const injector = new DesignModeInjector(agent);
    await injector.inject();
    const full = lastReminder(agent);
    const entry = designModeEntryMessage('/tmp/design.md', true);

    for (const marker of [
      'ShowDesignMockup IS available',
      'ONLY use ShowDesignMockup when',
      'lead with ShowDesignMockup',
    ]) {
      expect(full).toContain(marker);
      expect(entry).toContain(marker);
    }
  });

  it('injects the sparse reminder with the quality pointer after the short threshold', async () => {
    const agent = designAgent({ isActive: true, planFilePath: '/tmp/design.md' });
    const injector = new DesignModeInjector(agent);

    await injector.inject();
    const messages = history(agent);
    messages.push({ role: 'assistant' }, { role: 'assistant' });
    await injector.inject();

    const text = lastReminder(agent);
    expect(text).toContain('Design mode still active');
    expect(text).toContain('fidelity rubric');
    expect(text).toContain('consolidated audit gate');
    expect(text).toContain('Design file: /tmp/design.md');
  });

  it('injects the reentry reminder when prior design content exists', async () => {
    const agent = designAgent({
      isActive: true,
      planFilePath: '/tmp/design.md',
      content: '# Previous design',
    });
    const injector = new DesignModeInjector(agent);

    await injector.inject();

    const text = lastReminder(agent);
    expect(text).toContain('Re-entering Design Mode');
    expect(text).toContain('[C:UPSTREAM]');
  });

  it('injects the exit reminder when design mode turns off after being active', async () => {
    const stub: DesignModeStub = { isActive: true, planFilePath: '/tmp/design.md' };
    const agent = designAgent(stub);
    const injector = new DesignModeInjector(agent);

    await injector.inject();
    stub.isActive = false;
    await injector.inject();

    expect(lastReminder(agent)).toContain('Design mode is no longer active');
  });

  it('does not inject anything when design mode is inactive from the start', async () => {
    const agent = designAgent({ isActive: false });
    const injector = new DesignModeInjector(agent);

    await injector.inject();

    expect(history(agent)).toHaveLength(0);
  });
});

describe('DesignModeInjector cadence', () => {
  it('skips reinjection before the assistant-turn threshold', async () => {
    const agent = designAgent({ isActive: true, planFilePath: '/tmp/design.md' });
    const injector = new DesignModeInjector(agent);

    await injector.inject();
    const messages = history(agent);
    messages.push({ role: 'assistant' });
    await injector.inject();

    expect(messages).toHaveLength(2);
  });

  it('refreshes the full reminder after the long assistant-turn threshold', async () => {
    const agent = designAgent({ isActive: true, planFilePath: '/tmp/design.md' });
    const injector = new DesignModeInjector(agent);

    await injector.inject();
    const messages = history(agent);
    for (let i = 0; i < 5; i += 1) {
      messages.push({ role: 'assistant' });
    }
    await injector.inject();

    const text = lastReminder(agent);
    expect(text).toContain('Design mode is active');
    expect(text).not.toContain('Design mode still active');
  });

  it('refreshes the full reminder if a user message appears after the last injection', async () => {
    const agent = designAgent({ isActive: true, planFilePath: '/tmp/design.md' });
    const injector = new DesignModeInjector(agent);

    await injector.inject();
    history(agent).push({ role: 'user', content: [{ text: 'next task' }] });
    await injector.inject();

    const text = lastReminder(agent);
    expect(text).toContain('Design mode is active');
    expect(text).not.toContain('Design mode still active');
  });
});
