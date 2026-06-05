import { describe, expect, it } from 'vitest';

import type { Agent } from '../../../src/agent';
import { PlanModeInjector } from '../../../src/agent/injection/plan-mode';
import {
  parseManifestFiles,
  parsePartsManifest,
  planModeEntryMessage,
} from '../../../src/agent/injection/plan-mode-contract';

interface PlanModeStub {
  isActive: boolean;
  planFilePath?: string | null;
  /** Content returned by planMode.data(); when set non-empty on first inject, triggers reentry. */
  content?: string;
}

function planAgent(stub: PlanModeStub): Agent {
  const history: unknown[] = [];
  return {
    type: 'main',
    planMode: {
      get isActive() {
        return stub.isActive;
      },
      get kind() {
        return 'plan';
      },
      get planFilePath() {
        return stub.planFilePath ?? null;
      },
      data: async () =>
        stub.content === undefined
          ? null
          : { id: 'p1', content: stub.content, path: stub.planFilePath ?? '', kind: 'plan' },
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

describe('PlanModeInjector content', () => {
  it('injects the full reminder with the writing-plans rubric and plan file footer', async () => {
    const agent = planAgent({ isActive: true, planFilePath: '/tmp/plan.md' });
    const injector = new PlanModeInjector(agent);

    await injector.inject();
    const text = lastReminder(agent);

    expect(text).toContain('Plan mode is active');
    // TDD bite-sized task skeleton.
    expect(text).toContain('Depends on:');
    expect(text).toContain('Write the failing test');
    // Shared-signature caller + whole-tree build-green invariant.
    expect(text).toContain('grep -rn');
    expect(text).toContain('whole-tree typecheck');
    // No-placeholders + seven-item self-review + spec-coverage table.
    expect(text).toContain('No placeholders');
    expect(text).toContain('Spec-coverage table');
    // Split decision + durable manifest.
    expect(text).toContain('Parts manifest');
    expect(text).toContain('Plan file: /tmp/plan.md');
  });

  it('keeps the entry message and the full reminder in sync (shared contract)', async () => {
    const agent = planAgent({ isActive: true, planFilePath: '/tmp/plan.md' });
    const injector = new PlanModeInjector(agent);
    await injector.inject();
    const full = lastReminder(agent);
    const entry = planModeEntryMessage('/tmp/plan.md');

    for (const marker of [
      'Depends on:',
      'Write the failing test',
      'No placeholders',
      'Spec-coverage table',
      'Parts manifest',
    ]) {
      expect(full).toContain(marker);
      expect(entry).toContain(marker);
    }
    expect(entry).toContain('Plan mode is now active');
  });

  // Regression guard for the adversarial-review blades in plan-mode.
  // These catch the class of bug where a plan bakes in a filter/word-list that
  // contradicts the test assertion that must pass through it (e.g. 'auth' in the
  // sensitive-word list while a test expects 'auth-refactor' to survive).
  // This test fails loudly if a future edit strips a blade; it does NOT prove
  // the model behaves better — that requires a behavioral eval harness.
  it('carries the filter-verification blades in both the entry message and the full reminder', async () => {
    const agent = planAgent({ isActive: true, planFilePath: '/tmp/plan.md' });
    const injector = new PlanModeInjector(agent);
    await injector.inject();
    const full = lastReminder(agent);
    const entry = planModeEntryMessage('/tmp/plan.md');

    for (const marker of [
      // Blade A — task-skeleton: enumerate must-survive inputs for every filter/regex.
      'inputs that MUST survive',
      'constant is wrong and must be fixed',
      // Blade B — self-review item 6: trace each test assertion against its constants.
      'HARD failure',
      'fix the constant or the assertion',
    ]) {
      expect(full).toContain(marker);
      expect(entry).toContain(marker);
    }
  });

  // Regression guard for the Integration lens ported into self-review item 5.
  // Catches the class of bug where a task changes an identifier/path/filename but
  // its runtime consumer (a permission guard, a path matcher, a field a lookup
  // keys on) still validates a different value — compile-clean, type-consistent,
  // and therefore invisible to the shared-signature/type checks (e.g. a plan file
  // written under `fileStem` but authorized by `isWritablePlanPath` matching
  // `planId`). Fails loudly if a future edit strips the blade; does NOT prove the
  // model behaves better — that requires a behavioral eval harness.
  it('carries the consumer-reconciliation blade in both the entry message and the full reminder', async () => {
    const agent = planAgent({ isActive: true, planFilePath: '/tmp/plan.md' });
    const injector = new PlanModeInjector(agent);
    await injector.inject();
    const full = lastReminder(agent);
    const entry = planModeEntryMessage('/tmp/plan.md');

    for (const marker of [
      'open the runtime consumer',
      'keys off a different value',
      'Verify the consumer with Read/Grep',
      'never assume it',
    ]) {
      expect(full).toContain(marker);
      expect(entry).toContain(marker);
    }
  });

  it('uses the inline reminder when no plan file path is available', async () => {
    const agent = planAgent({ isActive: true, planFilePath: null });
    const injector = new PlanModeInjector(agent);

    await injector.inject();

    const text = lastReminder(agent);
    expect(text).toContain('Plan mode is active');
    expect(text).toContain('Wait for the host to provide a plan file path');
    expect(text).not.toContain('Plan file:');
  });

  it('injects the exit reminder when plan mode turns off after being active', async () => {
    const stub: PlanModeStub = { isActive: true, planFilePath: '/tmp/plan.md' };
    const agent = planAgent(stub);
    const injector = new PlanModeInjector(agent);

    await injector.inject();
    stub.isActive = false;
    await injector.inject();

    expect(lastReminder(agent)).toContain('Plan mode is no longer active');
  });

  it('does not inject anything when plan mode is inactive from the start', async () => {
    const agent = planAgent({ isActive: false });
    const injector = new PlanModeInjector(agent);

    await injector.inject();

    expect(history(agent)).toHaveLength(0);
  });

  it('injects the reentry reminder when prior plan content exists', async () => {
    const agent = planAgent({
      isActive: true,
      planFilePath: '/tmp/plan.md',
      content: '# Previous plan',
    });
    const injector = new PlanModeInjector(agent);

    await injector.inject();

    expect(lastReminder(agent)).toContain('Re-entering Plan Mode');
  });
});

describe('PlanModeInjector split-plan steering', () => {
  it('steers to the next pending part when the index has an unfinished manifest', async () => {
    const stub: PlanModeStub = { isActive: true, planFilePath: '/tmp/plan.md' };
    const agent = planAgent(stub);
    const injector = new PlanModeInjector(agent);

    // First injection on an empty plan: plain full reminder, no split steering.
    await injector.inject();

    // The model has now written an index with a Parts manifest; a new user
    // message forces a full refresh, which should carry the split directive.
    stub.content = [
      '## Parts',
      '| # | File | Scope | Status |',
      '|---|---|---|---|',
      '| 1 | plan-core.md | models | done |',
      '| 2 | plan-api.md | endpoints | pending |',
    ].join('\n');
    history(agent).push({ role: 'user', content: [{ text: 'continue' }] });
    await injector.inject();

    const text = lastReminder(agent);
    expect(text).toContain('Split plan in progress');
    expect(text).toContain('plan-api.md');
    expect(text).not.toContain('Split plan — all parts written');
  });

  it('steers to the cross-file final review once every manifest row is done', async () => {
    const stub: PlanModeStub = { isActive: true, planFilePath: '/tmp/plan.md' };
    const agent = planAgent(stub);
    const injector = new PlanModeInjector(agent);

    await injector.inject();
    stub.content = [
      '| # | File | Scope | Status |',
      '|---|---|---|---|',
      '| 1 | plan-core.md | models | done |',
      '| 2 | plan-api.md | endpoints | done |',
    ].join('\n');
    history(agent).push({ role: 'user', content: [{ text: 'continue' }] });
    await injector.inject();

    expect(lastReminder(agent)).toContain('Split plan — all parts written');
  });
});

describe('parsePartsManifest', () => {
  it('returns null when there is no manifest table', () => {
    expect(parsePartsManifest('# A plan\n\njust prose, no table')).toBeNull();
  });

  it('finds the first pending part and ignores header/separator rows', () => {
    const manifest = parsePartsManifest(
      [
        '| # | File | Scope | Status |',
        '|---|---|---|---|',
        '| 1 | dir/plan-core.md | models | done |',
        '| 2 | dir/plan-api.md | endpoints | pending |',
      ].join('\n'),
    );
    expect(manifest).not.toBeNull();
    expect(manifest?.allDone).toBe(false);
    // The file is reduced to its basename for the writable-sibling directive.
    expect(manifest?.next).toEqual({ file: 'plan-api.md', scope: 'endpoints' });
  });

  it('reports allDone when every row is done', () => {
    const manifest = parsePartsManifest(
      ['| 1 | plan-core.md | x | done |', '| 2 | plan-api.md | y | done |'].join('\n'),
    );
    expect(manifest?.allDone).toBe(true);
    expect(manifest?.next).toBeNull();
  });
});

describe('parseManifestFiles', () => {
  it('lists the basename of every manifest row, regardless of status', () => {
    const files = parseManifestFiles(
      [
        '| # | File | Scope | Status |',
        '|---|---|---|---|',
        '| 1 | dir/plan-core.md | models | done |',
        '| 2 | dir/plan-api.md | endpoints | pending |',
      ].join('\n'),
    );
    expect(files).toEqual(['plan-core.md', 'plan-api.md']);
  });

  it('returns an empty array for a single-file plan with no manifest', () => {
    expect(parseManifestFiles('# A plan\n\njust prose, no table')).toEqual([]);
  });
});

describe('PlanModeInjector cadence', () => {
  it('skips reinjection before the assistant-turn threshold', async () => {
    const agent = planAgent({ isActive: true, planFilePath: '/tmp/plan.md' });
    const injector = new PlanModeInjector(agent);

    await injector.inject();
    const messages = history(agent);
    messages.push({ role: 'assistant' });
    await injector.inject();

    expect(messages).toHaveLength(2);
  });

  it('injects the sparse reminder after the short assistant-turn threshold', async () => {
    const agent = planAgent({ isActive: true, planFilePath: '/tmp/plan.md' });
    const injector = new PlanModeInjector(agent);

    await injector.inject();
    const messages = history(agent);
    messages.push({ role: 'assistant' }, { role: 'assistant' });
    await injector.inject();

    const text = lastReminder(agent);
    expect(text).toContain('Plan mode still active');
    expect(text).toContain('see full instructions earlier');
    expect(text).toContain('Plan file: /tmp/plan.md');
  });

  it('refreshes the full reminder after the long assistant-turn threshold', async () => {
    const agent = planAgent({ isActive: true, planFilePath: '/tmp/plan.md' });
    const injector = new PlanModeInjector(agent);

    await injector.inject();
    const messages = history(agent);
    for (let i = 0; i < 5; i += 1) {
      messages.push({ role: 'assistant' });
    }
    await injector.inject();

    const text = lastReminder(agent);
    expect(text).toContain('Plan mode is active');
    expect(text).not.toContain('Plan mode still active');
  });

  it('refreshes the full reminder if a user message appears after the last injection', async () => {
    const agent = planAgent({ isActive: true, planFilePath: '/tmp/plan.md' });
    const injector = new PlanModeInjector(agent);

    await injector.inject();
    history(agent).push({ role: 'user', content: [{ text: 'next task' }] });
    await injector.inject();

    const text = lastReminder(agent);
    expect(text).toContain('Plan mode is active');
    expect(text).not.toContain('Plan mode still active');
  });
});
