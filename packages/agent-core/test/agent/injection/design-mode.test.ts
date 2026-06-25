import { describe, expect, it } from 'vitest';

import type { Agent } from '../../../src/agent';
import { DesignModeInjector } from '../../../src/agent/injection/design-mode';
import { designModeEntryMessage } from '../../../src/agent/injection/design-mode-contract';

interface DesignModeStub {
  isActive: boolean;
  sessionModeFilePath?: string | null;
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
    sessionMode: {
      get isActive() {
        return stub.isActive;
      },
      get kind() {
        return 'design';
      },
      get sessionModeFilePath() {
        return stub.sessionModeFilePath ?? null;
      },
      data: async () =>
        stub.content === undefined
          ? null
          : { id: 'd1', content: stub.content, path: stub.sessionModeFilePath ?? '', kind: 'design' },
      consumePendingHandoffForPlan: () => null,
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
    const agent = designAgent({ isActive: true, sessionModeFilePath: '/tmp/design.md' });
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

  it('tells the model to invent its own filename when path is null', () => {
    const entry = designModeEntryMessage(null, false);
    expect(entry).toContain('Invent your own filename');
    expect(entry).not.toContain('wait for one before calling ExitDesignMode');
  });

  it('shows assigned path and "do not invent" when path is non-null', () => {
    const entry = designModeEntryMessage('/workspace/.ody-code/designs/2026-06-10-my-topic.md', false);
    expect(entry).toContain('Design file:');
    expect(entry).toContain('Do NOT invent your own path');
  });

  it('keeps the entry message and the full reminder in sync (shared contract)', async () => {
    const agent = designAgent({ isActive: true, sessionModeFilePath: '/tmp/design.md' });
    const injector = new DesignModeInjector(agent);
    await injector.inject();
    const full = lastReminder(agent);
    const entry = designModeEntryMessage('/tmp/design.md', false);

    // Both compose from the same contract fragments, so the fidelity rubric,
    // the upstream branch, and the audit gate must appear in both.
    for (const marker of [
      'Step 0 — Audit strategy gate',
      'Step 0.5 — Upstream inventory',
      'Step 0.6 — Internal reuse scan',
      'Call-site integration',
      'Step 4.5',
      '[C:UPSTREAM]',
    ]) {
      expect(full).toContain(marker);
      expect(entry).toContain(marker);
    }
    expect(entry).toContain('Design mode is now active');
  });

  it('carries the C1-C8 exit checklist in the entry message, full reminder, and sparse reminder', async () => {
    const agent = designAgent({ isActive: true, sessionModeFilePath: '/tmp/design.md' });
    const injector = new DesignModeInjector(agent);
    await injector.inject();
    const full = lastReminder(agent);
    const entry = designModeEntryMessage('/tmp/design.md', false);

    // Push enough assistant turns to trigger the sparse reminder.
    const messages = history(agent);
    messages.push({ role: 'assistant' }, { role: 'assistant' });
    await injector.inject();
    const sparse = lastReminder(agent);

    for (const text of [full, entry, sparse]) {
      for (const marker of [
        'C1. Scope In/Out',
        'C2. Architecture',
        'C3. Data Models',
        'C4. Algorithms',
        'C5. Error Handling',
        'C6. Self-Review',
        'C7. User Final Approval',
        'C8. Reuse Analysis',
      ]) {
        expect(text).toContain(marker);
      }
    }
  });

  // Regression guard for the anti-self-deception "blades" (see design-mode-contract.ts).
  // These are the safeguards that catch the class of bug where a design bakes in a
  // too-broad filter / a test that contradicts its own constants. This test does NOT
  // prove the model behaves better — it only fails loudly if a future edit silently
  // strips a blade. Behavioural confidence requires the eval harness (see repo notes).
  it('carries the adversarial-review blades in both the entry message and the full reminder', async () => {
    const agent = designAgent({ isActive: true, sessionModeFilePath: '/tmp/design.md' });
    const injector = new DesignModeInjector(agent);
    await injector.inject();
    const full = lastReminder(agent);
    const entry = designModeEntryMessage('/tmp/design.md', false);

    for (const marker of [
      // Blade A — ephemeral verification is explicitly allowed (not "implementation").
      'verification is not implementation',
      'node -e',
      // Blade B — Step 4.5 is adversarial: triage + concrete-input trace + test-vs-constant check.
      'Adversarial self-review',
      'most expensive',
      '3 concrete inputs',
      'HARD failure',
      // Blade C — fixed multi-lens sweep so coverage does not depend on the model picking the right focus.
      'four fixed lenses',
      'false positives',
    ]) {
      expect(full).toContain(marker);
      expect(entry).toContain(marker);
    }
  });

  it('tells the model ShowDesignMockup IS available when the host advertises openExternal', async () => {
    const agent = designAgent({
      isActive: true,
      sessionModeFilePath: '/tmp/design.md',
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
    const agent = designAgent({ isActive: true, sessionModeFilePath: '/tmp/design.md' });
    const injector = new DesignModeInjector(agent);

    await injector.inject();
    const text = lastReminder(agent);

    expect(text).toContain('ShowDesignMockup is NOT available');
    expect(text).not.toContain('ShowDesignMockup IS available');
  });

  it('resolves the turn-discipline / visual-companion contradiction in the full reminder', async () => {
    const agent = designAgent({
      isActive: true,
      sessionModeFilePath: '/tmp/design.md',
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
      sessionModeFilePath: '/tmp/design.md',
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
    const agent = designAgent({ isActive: true, sessionModeFilePath: '/tmp/design.md' });
    const injector = new DesignModeInjector(agent);

    await injector.inject();
    const messages = history(agent);
    messages.push({ role: 'assistant' }, { role: 'assistant' });
    await injector.inject();

    const text = lastReminder(agent);
    expect(text).toContain('Design mode still active');
    expect(text).toContain('fidelity rubric');
    expect(text).toContain('post-write audit gate');
    expect(text).toContain('Design file: /tmp/design.md');
  });

  it('injects the reentry reminder when prior design content exists', async () => {
    const agent = designAgent({
      isActive: true,
      sessionModeFilePath: '/tmp/design.md',
      content: '# Previous design',
    });
    const injector = new DesignModeInjector(agent);

    await injector.inject();

    const text = lastReminder(agent);
    expect(text).toContain('Re-entering Design Mode');
    expect(text).toContain('[C:UPSTREAM]');
  });

  it('injects the exit reminder when design mode turns off after being active', async () => {
    const stub: DesignModeStub = { isActive: true, sessionModeFilePath: '/tmp/design.md' };
    const agent = designAgent(stub);
    const injector = new DesignModeInjector(agent);

    await injector.inject();
    stub.isActive = false;
    await injector.inject();

    expect(lastReminder(agent)).toContain('Design mode was cancelled');
  });

  it('injects the handoff reminder (with design artifact) when a pending handoff for plan is set', async () => {
    const stub: DesignModeStub = { isActive: true, sessionModeFilePath: '/tmp/design.md' };
    let pendingHandoff: { path: string; filename: string; selectedLabel?: string } | null = {
      path: '/tmp/design.md',
      filename: 'design.md',
    };
    const agent = {
      ...designAgent(stub),
      sessionMode: {
        ...designAgent(stub).sessionMode,
        get isActive() { return stub.isActive; },
        get kind() { return 'design'; },
        get sessionModeFilePath() { return stub.sessionModeFilePath ?? null; },
        data: async () => stub.content === undefined ? null : { id: 'd1', content: stub.content, path: stub.sessionModeFilePath ?? '', kind: 'design' },
        consumePendingHandoffForPlan: () => {
          const p = pendingHandoff;
          pendingHandoff = null;
          return p;
        },
      },
    } as unknown as import('../../../src/agent').Agent;
    const injector = new DesignModeInjector(agent);

    await injector.inject();
    stub.isActive = false;
    await injector.inject();

    const text = lastReminder(agent);
    expect(text).toContain('Design mode completed');
    expect(text).toContain('plan mode');
    expect(text).toContain('Design saved to: /tmp/design.md');
    expect(text).toContain("approved design in `design.md`");
    expect(text).not.toContain('# My Design');
  });

  it('does not inject anything when design mode is inactive from the start', async () => {
    const agent = designAgent({ isActive: false });
    const injector = new DesignModeInjector(agent);

    await injector.inject();

    expect(history(agent)).toHaveLength(0);
  });
});

describe('DesignModeInjector contract guards', () => {
  it('carries the no-production-code guard in both the entry message and the full reminder', async () => {
    const agent = designAgent({ isActive: true, sessionModeFilePath: '/tmp/design.md' });
    const injector = new DesignModeInjector(agent);
    await injector.inject();
    const full = lastReminder(agent);
    const entry = designModeEntryMessage('/tmp/design.md', false);

    for (const marker of [
      'language-agnostic',
      "implementer's job",
    ]) {
      expect(full).toContain(marker);
      expect(entry).toContain(marker);
    }
  });

  it('carries the prior-art search guidance in both the entry message and the full reminder', async () => {
    const agent = designAgent({ isActive: true, sessionModeFilePath: '/tmp/design.md' });
    const injector = new DesignModeInjector(agent);
    await injector.inject();
    const full = lastReminder(agent);
    const entry = designModeEntryMessage('/tmp/design.md', false);

    for (const marker of [
      'Prior art search',
      '## Prior Art',
    ]) {
      expect(full).toContain(marker);
      expect(entry).toContain(marker);
    }
  });

  // Scope gate: a goal that is really several independent sub-projects should be
  // decomposed (each its own design→plan cycle), not refined as one oversized spec.
  // This mirrors gpowers brainstorming's "assess scope / decompose" step.
  it('carries the scope-decomposition gate in both the entry message and the full reminder', async () => {
    const agent = designAgent({ isActive: true, sessionModeFilePath: '/tmp/design.md' });
    const injector = new DesignModeInjector(agent);
    await injector.inject();
    const full = lastReminder(agent);
    const entry = designModeEntryMessage('/tmp/design.md', false);

    for (const marker of [
      'decompose into sub-projects',
      'multiple independent subsystems',
    ]) {
      expect(full).toContain(marker);
      expect(entry).toContain(marker);
    }
  });

  // Placement fidelity: when the request names a concrete target (e.g.
  // `backend/cmd/server`), the design must land THERE — retargeting to a
  // different location requires an explicit user decision, never a silent swap.
  it('carries the placement-fidelity rule in both the entry message and the full reminder', async () => {
    const agent = designAgent({ isActive: true, sessionModeFilePath: '/tmp/design.md' });
    const injector = new DesignModeInjector(agent);
    await injector.inject();
    const full = lastReminder(agent);
    const entry = designModeEntryMessage('/tmp/design.md', false);

    for (const marker of [
      'Placement fidelity',
      'names a concrete target',
      'may NOT silently retarget',
    ]) {
      expect(full).toContain(marker);
      expect(entry).toContain(marker);
    }
  });

  // Post-write audit gate: every [C:INFERRED] assumption must be enumerated
  // verbatim for per-item sign-off, and ExitDesignMode is blocked until the
  // level-appropriate items are signed off (mirrors gpowers' User Audit Gate).
  it('carries the per-assumption sign-off hard gate in both the entry message and the full reminder', async () => {
    const agent = designAgent({ isActive: true, sessionModeFilePath: '/tmp/design.md' });
    const injector = new DesignModeInjector(agent);
    await injector.inject();
    const full = lastReminder(agent);
    const entry = designModeEntryMessage('/tmp/design.md', false);

    for (const marker of [
      'list each [C:INFERRED] assumption verbatim',
      'MUST NOT call ExitDesignMode until',
    ]) {
      expect(full).toContain(marker);
      expect(entry).toContain(marker);
    }
  });
});

describe('DesignModeInjector split-design steering', () => {
  it('steers to the next pending part when the index has an unfinished manifest', async () => {
    const stub: DesignModeStub = { isActive: true, sessionModeFilePath: '/tmp/design.md' };
    const agent = designAgent(stub);
    const injector = new DesignModeInjector(agent);

    // First injection on an empty design: plain full reminder, no split steering.
    await injector.inject();

    // The model has now written an index with a Parts manifest; a new user
    // message forces a full refresh, which should carry the split directive.
    stub.content = [
      '## Parts',
      '| # | File | Scope | Status |',
      '|---|---|---|---|',
      '| 1 | design/core.md | data types | done |',
      '| 2 | design/api.md | endpoints | pending |',
    ].join('\n');
    history(agent).push({ role: 'user', content: [{ text: 'continue' }] });
    await injector.inject();

    const text = lastReminder(agent);
    expect(text).toContain('Split design in progress');
    // Target reconstructed as `<index-stem>/<part-basename>` = `design/api.md`.
    expect(text).toContain('design/api.md');
    expect(text).not.toContain('Split design — all parts written');
  });

  it('steers to the cross-file final review once every manifest row is done', async () => {
    const stub: DesignModeStub = { isActive: true, sessionModeFilePath: '/tmp/design.md' };
    const agent = designAgent(stub);
    const injector = new DesignModeInjector(agent);

    await injector.inject();
    stub.content = [
      '| # | File | Scope | Status |',
      '|---|---|---|---|',
      '| 1 | design/core.md | data types | done |',
      '| 2 | design/api.md | endpoints | done |',
    ].join('\n');
    history(agent).push({ role: 'user', content: [{ text: 'continue' }] });
    await injector.inject();

    expect(lastReminder(agent)).toContain('Split design — all parts written');
  });

  it('includes split directive in reentry reminder when manifest has pending parts', async () => {
    const agent = designAgent({
      isActive: true,
      sessionModeFilePath: '/tmp/design.md',
      content: [
        '## Parts',
        '| # | File | Scope | Status |',
        '|---|---|---|---|',
        '| 1 | design/core.md | core types | done |',
        '| 2 | design/api.md | endpoints | pending |',
      ].join('\n'),
    });
    const injector = new DesignModeInjector(agent);

    await injector.inject();

    const text = lastReminder(agent);
    expect(text).toContain('Re-entering Design Mode');
    expect(text).toContain('Split design in progress');
    expect(text).toContain('design/api.md');
  });

  it('injects no split directive for a single-file design (no manifest)', async () => {
    const stub: DesignModeStub = { isActive: true, sessionModeFilePath: '/tmp/design.md' };
    const agent = designAgent(stub);
    const injector = new DesignModeInjector(agent);

    await injector.inject();
    // A single-file design with prose but no Parts manifest table.
    stub.content = '# My design\n\nJust one coherent component, written inline. No manifest.';
    history(agent).push({ role: 'user', content: [{ text: 'continue' }] });
    await injector.inject();

    const text = lastReminder(agent);
    expect(text).not.toContain('Split design in progress');
    expect(text).not.toContain('Split design — all parts written');
    // The full reminder still renders normally.
    expect(text).toContain('Design mode is active');
  });
});

describe('DesignModeInjector cadence', () => {
  it('skips reinjection before the assistant-turn threshold', async () => {
    const agent = designAgent({ isActive: true, sessionModeFilePath: '/tmp/design.md' });
    const injector = new DesignModeInjector(agent);

    await injector.inject();
    const messages = history(agent);
    messages.push({ role: 'assistant' });
    await injector.inject();

    expect(messages).toHaveLength(2);
  });

  it('refreshes the full reminder after the long assistant-turn threshold', async () => {
    const agent = designAgent({ isActive: true, sessionModeFilePath: '/tmp/design.md' });
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
    const agent = designAgent({ isActive: true, sessionModeFilePath: '/tmp/design.md' });
    const injector = new DesignModeInjector(agent);

    await injector.inject();
    history(agent).push({ role: 'user', content: [{ text: 'next task' }] });
    await injector.inject();

    const text = lastReminder(agent);
    expect(text).toContain('Design mode is active');
    expect(text).not.toContain('Design mode still active');
  });

  it('includes selected approach in handoff reminder when selectedLabel is present', async () => {
    const stub: DesignModeStub = { isActive: true, sessionModeFilePath: '/tmp/design.md' };
    let pendingHandoff: { path: string; filename: string; selectedLabel?: string } | null = {
      path: '/tmp/design.md',
      filename: 'design.md',
      selectedLabel: 'Approach A',
    };
    const baseAgent = designAgent(stub);
    const agent = {
      ...baseAgent,
      sessionMode: {
        ...baseAgent.sessionMode,
        get isActive() { return stub.isActive; },
        get kind() { return 'design'; },
        get sessionModeFilePath() { return stub.sessionModeFilePath ?? null; },
        data: async () => stub.content === undefined ? null : { id: 'd1', content: stub.content, path: stub.sessionModeFilePath ?? '', kind: 'design' },
        consumePendingHandoffForPlan: () => {
          const p = pendingHandoff;
          pendingHandoff = null;
          return p;
        },
      },
    } as unknown as import('../../../src/agent').Agent;
    const injector = new DesignModeInjector(agent);

    await injector.inject();
    stub.isActive = false;
    await injector.inject();

    const text = lastReminder(agent);
    expect(text).toContain('Selected approach: Approach A');
    expect(text).toContain('Execute ONLY the selected approach');
    expect(text).not.toContain('# My Design');
  });
});
