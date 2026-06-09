import type { ToolCall } from '@odysseythink/kosong';
import { basename, dirname, join } from 'pathe';
import { describe, expect, it, vi } from 'vitest';

import { splitContinuationDirective } from '../../src/agent/injection/plan-mode-contract';
import { formatDatePrefix } from '../../src/agent/session-mode/topic-generator';
import { createFakeKaos } from '../tools/fixtures/fake-kaos';
import { createCommandKaos, testAgent } from './harness/agent';

function createPlanKaos(overrides: Parameters<typeof createFakeKaos>[0] = {}) {
  return createFakeKaos({
    mkdir: vi.fn().mockResolvedValue(undefined),
    ...overrides,
  });
}

async function setPlanPath(
  ctx: ReturnType<typeof testAgent>,
  path = '/workspace/.ody-code/plans/test-plan.md',
): Promise<void> {
  (ctx.agent.sessionMode as unknown as { _sessionModeFilePath: string | null })._sessionModeFilePath = path;
}

describe('manual plan entry', () => {
  it('keeps permission gating out of the SessionMode state object', () => {
    const ctx = testAgent();

    expect('beforeToolCall' in ctx.agent.sessionMode).toBe(false);
  });

  it('enters plan mode without starting a model turn and prepares the plan directory', async () => {
    const mkdir = vi.fn().mockResolvedValue(undefined);
    const writeText = vi.fn().mockResolvedValue(0);
    const ctx = testAgent({
      kaos: createFakeKaos({ mkdir, writeText }),
    });

    await ctx.rpc.enterPlan({});
    await delay(10);

    expect(ctx.agent.sessionMode.isActive).toBe(true);
    // No user prompt yet at entry → naming is DEFERRED (path stays null) so the
    // first prompt/write can name the file, rather than locking in "untitled".
    expect(ctx.agent.sessionMode.sessionModeFilePath).toBeNull();
    expect(mkdir).toHaveBeenCalledWith('/workspace/.ody-code/plans', { parents: true, existOk: true });
    expect(writeText).not.toHaveBeenCalled();
    expect(ctx.allEvents.some((event) => event.event === 'turn.started')).toBe(false);
    expect(ctx.llmCalls).toHaveLength(0);
  });

  it('derives the no-homedir plan path from cwd on enter and restore', async () => {
    const ctx = testAgent({
      kaos: createPlanKaos({
        writeText: vi.fn(async (_path: string, content: string) => content.length),
      }),
    });
    await ctx.agent.sessionMode.enter('stable-plan');

    // No user prompt yet at entry → naming deferred (path null), not "untitled".
    expect(ctx.agent.sessionMode.sessionModeFilePath).toBeNull();

    const enterRecord = ctx.allEvents.find(
      (event) => event.type === '[wire]' && event.event === 'session_mode.enter',
    );
    expect(enterRecord?.args).toEqual({
      id: 'stable-plan',
      kind: 'plan',
      time: expect.any(Number),
    });

    const resumed = testAgent({ kaos: createFakeKaos() });
    resumed.dispatch({
      type: 'session_mode.enter',
      id: 'stable-plan',
      path: '/workspace/.ody-code/plans/stable-plan.md',
    });

    expect(resumed.agent.sessionMode.sessionModeFilePath).toBeNull();
  });

  it('enters plan mode through the EnterPlanMode tool and reminds the next step', async () => {
    const enterPlanModeCall: ToolCall = {
      type: 'function',
      id: 'call_enter_plan',
      name: 'EnterPlanMode',
      arguments: '{}',
    };
    const ctx = testAgent({
      kaos: createPlanKaos({
        writeText: vi.fn(async (_path: string, content: string) => content.length),
      }),
    });
    ctx.configure({ tools: ['EnterPlanMode'] });
    await ctx.rpc.setPermission({ mode: 'yolo' });

    ctx.mockNextResponse({ type: 'text', text: 'I will enter plan mode.' }, enterPlanModeCall);
    ctx.mockNextResponse({ type: 'text', text: 'Plan mode is active now.' });
    await ctx.rpc.prompt({ input: [{ type: 'text', text: 'Plan first' }] });

    await ctx.untilTurnEnd();
    await delay(10);
    expect(ctx.agent.sessionMode.isActive).toBe(true);
    expect(ctx.llmCalls).toHaveLength(2);
    expect(toolResultText(ctx.llmCalls[1]!.history)).toContain('Plan mode is now active');
    await ctx.expectResumeMatches();
  });
});

describe('plan clear', () => {
  it('empties the current plan file without leaving plan mode', async () => {
    const files = new Map<string, string>();
    const mkdir = vi.fn().mockResolvedValue(undefined);
    const readText = vi.fn(async (path: string) => files.get(path) ?? '');
    const writeText = vi.fn(async (path: string, content: string) => {
      files.set(path, content);
      return content.length;
    });

    const stat = vi.fn(async (path: string) => {
      if (files.has(path)) return { stMode: 0o100644 } as never;
      throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
    });
    const ctx = testAgent({
      kaos: createPlanKaos({ mkdir, readText, writeText, stat }),
    });
    await ctx.agent.sessionMode.enter('test-plan', false);
    await setPlanPath(ctx);

    const planPath = ctx.agent.sessionMode.sessionModeFilePath;
    if (planPath === null) throw new Error('expected active plan path');
    files.set(planPath, '# Plan\n\n- Step 1');

    await ctx.rpc.clearPlan({});

    expect(writeText).toHaveBeenCalledWith(planPath, '');
    expect(files.get(planPath)).toBe('');
    expect(ctx.agent.sessionMode.isActive).toBe(true);
    expect(ctx.agent.sessionMode.sessionModeFilePath).toBe(planPath);
    await expect(ctx.rpc.getPlan({})).resolves.toMatchObject({
      id: 'test-plan',
      content: '',
      path: planPath,
    });
    await ctx.expectResumeMatches();
  });
});

describe('plan exit tool', () => {
  it('reads the current plan file and exits plan mode directly in auto mode', async () => {
    const files = new Map<string, string>();
    const readText = vi.fn(async (path: string) => files.get(path) ?? '');
    const ctx = testAgent({
      kaos: createPlanKaos({ readText }),
    });
    ctx.configure({ tools: ['ExitPlanMode'] });
    await ctx.rpc.setPermission({ mode: 'auto' });
    await ctx.agent.sessionMode.enter('test-plan', false);
    await setPlanPath(ctx);

    const planPath = ctx.agent.sessionMode.sessionModeFilePath;
    if (planPath === null) throw new Error('expected active plan path');
    files.set(planPath, '# Plan\n\n- Inspect\n- Change\n- Verify');

    const exitPlanModeCall: ToolCall = {
      type: 'function',
      id: 'call_exit_plan',
      name: 'ExitPlanMode',
      arguments: '{}',
    };
    ctx.mockNextResponse({ type: 'text', text: 'I will present the plan.' }, exitPlanModeCall);
    ctx.mockNextResponse({ type: 'text', text: 'This response must not be requested.' });
    await ctx.rpc.prompt({ input: [{ type: 'text', text: 'Show the plan' }] });

    await ctx.untilTurnEnd();
    expect(
      ctx.allEvents.some((event) => event.type === '[rpc]' && event.event === 'requestApproval'),
    ).toBe(false);
    expect(readText).toHaveBeenCalledWith(planPath);
    expect(ctx.agent.sessionMode.isActive).toBe(false);
    // ExitPlanMode hard-stops the turn: no second model step runs after approval,
    // so the planning context can be freed before the user starts execution.
    expect(ctx.llmCalls).toHaveLength(1);
    expect(toolResultText(ctx.agent.context.history)).toContain('Plan mode deactivated');
    expect(toolResultText(ctx.agent.context.history)).toContain('# Plan');
    expect(toolResultText(ctx.agent.context.history)).toContain('STOP');
    await ctx.expectResumeMatches();
  });

  it('hard-stops the turn after the user approves the plan in manual mode', async () => {
    const files = new Map<string, string>();
    const readText = vi.fn(async (path: string) => files.get(path) ?? '');
    const ctx = testAgent({
      kaos: createPlanKaos({ readText }),
    });
    ctx.configure({ tools: ['ExitPlanMode'] });
    await ctx.rpc.setPermission({ mode: 'manual' });
    await ctx.agent.sessionMode.enter('approve-plan', false);
    await setPlanPath(ctx);

    const planPath = ctx.agent.sessionMode.sessionModeFilePath;
    if (planPath === null) throw new Error('expected active plan path');
    files.set(planPath, '# Plan\n\n- Inspect\n- Change\n- Verify');

    const exitPlanModeCall: ToolCall = {
      type: 'function',
      id: 'call_exit_approve',
      name: 'ExitPlanMode',
      arguments: '{}',
    };
    ctx.mockNextResponse({ type: 'text', text: 'I will present the plan.' }, exitPlanModeCall);
    ctx.mockNextResponse({ type: 'text', text: 'This response must not be requested.' });
    await ctx.rpc.prompt({ input: [{ type: 'text', text: 'Show the plan' }] });

    const approval = await ctx.takeApprovalRequest();
    approval.respond({ decision: 'approved' });

    await ctx.untilTurnEnd();
    expect(ctx.agent.sessionMode.isActive).toBe(false);
    // Approval ends the turn — the model gets no follow-up step to auto-execute.
    expect(ctx.llmCalls).toHaveLength(1);
    expect(toolResultText(ctx.agent.context.history)).toContain('Plan mode deactivated');
    expect(toolResultText(ctx.agent.context.history)).toContain('# Plan');
    expect(toolResultText(ctx.agent.context.history)).toContain('STOP');
    await ctx.expectResumeMatches();
  });

  it('stops the turn and stays in plan mode when the user rejects the plan', async () => {
    const files = new Map<string, string>();
    const readText = vi.fn(async (path: string) => files.get(path) ?? '');
    const ctx = testAgent({
      kaos: createPlanKaos({ readText }),
    });
    ctx.configure({ tools: ['ExitPlanMode'] });
    await ctx.rpc.setPermission({ mode: 'manual' });
    await ctx.agent.sessionMode.enter('reject-plan', false);
    await setPlanPath(ctx);

    const planPath = ctx.agent.sessionMode.sessionModeFilePath;
    if (planPath === null) throw new Error('expected active plan path');
    files.set(planPath, '# Plan\n\n- Inspect\n- Change\n- Verify');

    const exitPlanModeCall: ToolCall = {
      type: 'function',
      id: 'call_exit_reject',
      name: 'ExitPlanMode',
      arguments: '{}',
    };
    ctx.mockNextResponse({ type: 'text', text: 'I will present the plan.' }, exitPlanModeCall);
    ctx.mockNextResponse({ type: 'text', text: 'This response must not be requested.' });
    await ctx.rpc.prompt({ input: [{ type: 'text', text: 'Show the plan' }] });

    const approval = await ctx.takeApprovalRequest();
    approval.respond({ decision: 'rejected', selectedLabel: 'Reject' });

    await ctx.untilTurnEnd();
    expect(readText).toHaveBeenCalledWith(planPath);
    expect(ctx.agent.sessionMode.isActive).toBe(true);
    expect(ctx.llmCalls).toHaveLength(1);
    expect(toolResultText(ctx.agent.context.history)).toContain('Plan rejected by user');
    await ctx.expectResumeMatches();
  });

  it('does not execute later tool calls in the same batch after plan rejection', async () => {
    const files = new Map<string, string>();
    const readText = vi.fn(async (path: string) => files.get(path) ?? '');
    const execWithEnv = vi.fn(() => {
      throw new Error('Bash should not execute after plan rejection');
    });
    const ctx = testAgent({
      kaos: createPlanKaos({ readText, execWithEnv }),
    });
    ctx.configure({ tools: ['ExitPlanMode', 'Bash'] });
    await ctx.rpc.setPermission({ mode: 'yolo' });
    await ctx.agent.sessionMode.enter('reject-and-exit-plan', false);
    await setPlanPath(ctx);

    const planPath = ctx.agent.sessionMode.sessionModeFilePath;
    if (planPath === null) throw new Error('expected active plan path');
    files.set(planPath, '# Plan\n\n- Inspect\n- Change\n- Verify');

    const exitPlanModeCall: ToolCall = {
      type: 'function',
      id: 'call_exit_reject_and_exit',
      name: 'ExitPlanMode',
      arguments: '{}',
    };
    const bashCall: ToolCall = {
      type: 'function',
      id: 'call_bash_after_reject',
      name: 'Bash',
      arguments: '{"command":"touch should-not-run","timeout":60}',
    };
    ctx.mockNextResponse(
      { type: 'text', text: 'I will present the plan and then run a command.' },
      exitPlanModeCall,
      bashCall,
    );
    await ctx.rpc.prompt({ input: [{ type: 'text', text: 'Show the plan' }] });

    const approval = await ctx.takeApprovalRequest();
    approval.respond({ decision: 'rejected', selectedLabel: 'Reject' });

    await ctx.untilTurnEnd();
    expect(ctx.agent.sessionMode.isActive).toBe(true);
    expect(execWithEnv).not.toHaveBeenCalled();
    expect(ctx.llmCalls).toHaveLength(1);
    expect(toolResultText(ctx.agent.context.history)).toContain('Plan rejected by user');
    expect(toolResultText(ctx.agent.context.history)).toContain(
      'Tool skipped because a previous tool call stopped the turn.',
    );
    await ctx.expectResumeMatches();
  });

  it('refuses to exit when the current plan file is empty', async () => {
    const readText = vi.fn(async () => '');
    const ctx = testAgent({
      kaos: createPlanKaos({ readText }),
    });
    ctx.configure({ tools: ['ExitPlanMode'] });
    await ctx.rpc.setPermission({ mode: 'yolo' });
    await ctx.agent.sessionMode.enter('empty-plan', false);

    const exitPlanModeCall: ToolCall = {
      type: 'function',
      id: 'call_exit_empty_plan',
      name: 'ExitPlanMode',
      arguments: '{}',
    };
    ctx.mockNextResponse(
      { type: 'text', text: 'I will present the empty plan.' },
      exitPlanModeCall,
    );
    ctx.mockNextResponse({ type: 'text', text: 'I need to write the plan first.' });
    await ctx.rpc.prompt({ input: [{ type: 'text', text: 'Show an empty plan' }] });

    await ctx.untilTurnEnd();
    expect(ctx.agent.sessionMode.isActive).toBe(true);
    expect(toolResultText(ctx.llmCalls[1]!.history)).toContain('No plan file found');
    await ctx.expectResumeMatches();
  });
});

describe('plan exit tool options', () => {
  it('keeps options for approval when an option omits the optional description', async () => {
    const files = new Map<string, string>();
    const readText = vi.fn(async (path: string) => files.get(path) ?? '');
    const ctx = testAgent({
      kaos: createPlanKaos({ readText }),
    });
    ctx.configure({ tools: ['ExitPlanMode'] });
    await ctx.rpc.setPermission({ mode: 'manual' });
    await ctx.agent.sessionMode.enter('options-plan', false);
    await setPlanPath(ctx);

    const planPath = ctx.agent.sessionMode.sessionModeFilePath;
    if (planPath === null) throw new Error('expected active plan path');
    files.set(planPath, '# Plan\n\n- Inspect\n- Change\n- Verify');

    const exitPlanModeCall: ToolCall = {
      type: 'function',
      id: 'call_exit_options',
      name: 'ExitPlanMode',
        // The second option omits `description` — valid input after the
        // schema relaxation. The approval policy must still surface both.
        arguments: JSON.stringify({
          options: [
            { label: 'Approach A', description: 'Smaller refactor.' },
            { label: 'Approach B' },
          ],
        }),
    };
    ctx.mockNextResponse({ type: 'text', text: 'I will present the plan.' }, exitPlanModeCall);
    await ctx.rpc.prompt({ input: [{ type: 'text', text: 'Show the plan' }] });

    const approval = await ctx.takeApprovalRequest();
    const rpcArgs = (
      ctx.allEvents.find(
        (event) => event.type === '[rpc]' && event.event === 'requestApproval',
      ) as { args: { action?: string; display?: { options?: readonly unknown[] } } } | undefined
    )?.args;

    expect(rpcArgs?.action).toBe('Presenting plan and exiting plan mode');
    expect(rpcArgs?.display?.options).toHaveLength(2);

    approval.respond({ decision: 'approved', selectedLabel: 'Approach A' });
    await ctx.untilTurnEnd();
    expect(ctx.llmCalls).toHaveLength(1);
    expect(toolResultText(ctx.agent.context.history)).toContain('Selected approach: Approach A');
    expect(toolResultText(ctx.agent.context.history)).toContain('STOP');
  });
});

describe('plan allows safe tool flow', () => {
  it.each(['Write', 'Edit'] as const)(
    'runs %s on the active plan file without approval in manual mode',
    async (toolName) => {
      const files = new Map<string, string>();
      const readText = vi.fn(async (path: string) => files.get(path) ?? '');
      const writeText = vi.fn(async (path: string, content: string) => {
        files.set(path, content);
        return content.length;
      });
      const ctx = testAgent({
        kaos: createPlanKaos({ readText, writeText }),
      });
      ctx.configure({ tools: [toolName] });
      await ctx.agent.sessionMode.enter('test-plan', false);
      await setPlanPath(ctx);

      const planPath = ctx.agent.sessionMode.sessionModeFilePath;
      if (planPath === null) throw new Error('expected active plan path');
      files.set(planPath, '# Plan\n\n- Draft');

      const expectedContent =
        toolName === 'Write' ? '# Plan\n\n- Inspect\n- Verify' : '# Plan\n\n- Draft\n- Verify';
      const args =
        toolName === 'Write'
          ? { path: planPath, content: expectedContent }
          : { path: planPath, old_string: '- Draft', new_string: '- Draft\n- Verify' };
      const writePlanCall: ToolCall = {
        type: 'function',
        id: `call_${toolName.toLowerCase()}_plan`,
        name: toolName,
          arguments: JSON.stringify(args),
      };

      ctx.mockNextResponse({ type: 'text', text: 'I will update the plan file.' }, writePlanCall);
      ctx.mockNextResponse({ type: 'text', text: 'Plan file updated.' });
      await ctx.rpc.prompt({ input: [{ type: 'text', text: 'Update the plan file' }] });

      await ctx.untilTurnEnd();

      expect(files.get(planPath)).toBe(expectedContent);
      expect(writeText).toHaveBeenCalledWith(planPath, expectedContent);
      expect(
        ctx.allEvents.some((event) => event.type === '[rpc]' && event.event === 'requestApproval'),
      ).toBe(false);
      await ctx.expectResumeMatches();
    },
  );

  it('keeps explicit deny rules above active plan file writes', async () => {
    const files = new Map<string, string>();
    const writeText = vi.fn(async (path: string, content: string) => {
      files.set(path, content);
      return content.length;
    });
    const ctx = testAgent({
      kaos: createPlanKaos({ writeText }),
    });
    ctx.configure({ tools: ['Write'] });
    ctx.agent.permission.rules.push({
      decision: 'deny',
      scope: 'user',
      pattern: 'Write',
      reason: 'blocked by test',
    });
    await ctx.agent.sessionMode.enter('test-plan', false);
    await setPlanPath(ctx);

    const planPath = ctx.agent.sessionMode.sessionModeFilePath;
    if (planPath === null) throw new Error('expected active plan path');
    const content = '# Plan\n\n- Inspect\n- Verify';
    const writePlanCall: ToolCall = {
      type: 'function',
      id: 'call_write_plan_with_deny',
      name: 'Write',
      arguments: JSON.stringify({ path: planPath, content }),
    };

    ctx.mockNextResponse({ type: 'text', text: 'I will update the plan file.' }, writePlanCall);
    ctx.mockNextResponse({ type: 'text', text: 'Plan file updated.' });
    await ctx.rpc.prompt({ input: [{ type: 'text', text: 'Update the plan file' }] });

    await ctx.untilTurnEnd();

    expect(files.get(planPath)).toBeUndefined();
    expect(writeText).not.toHaveBeenCalled();
    expect(toolResultText(ctx.agent.context.history)).toContain(
      'Tool "Write" was denied by permission rule. Reason: blocked by test',
    );
    expect(
      ctx.allEvents.some((event) => event.type === '[rpc]' && event.event === 'requestApproval'),
    ).toBe(false);
  });

  it('allows read-only Bash to continue through permission and execution', async () => {
    const bashCall: ToolCall = {
      type: 'function',
      id: 'call_bash',
      name: 'Bash',
      arguments: '{"command":"printf plan-safe","timeout":60}',
    };
    const ctx = testAgent({ kaos: createCommandKaos('plan-safe') });
    ctx.configure({ tools: ['Bash'] });
    await ctx.rpc.setPermission({ mode: 'yolo' });
    await ctx.agent.sessionMode.enter('test-plan', false);

    ctx.mockNextResponse({ type: 'text', text: 'I will inspect safely.' }, bashCall);
    ctx.mockNextResponse({ type: 'text', text: 'The safe command printed plan-safe.' });
    await ctx.rpc.prompt({ input: [{ type: 'text', text: 'Inspect without mutating files' }] });
    await ctx.untilTurnEnd();
    expect(ctx.agent.sessionMode.isActive).toBe(true);
    expect(ctx.agent.sessionMode.kind).toBe('plan');
    expect(
      ctx.allEvents.some((event) => event.type === '[rpc]' && event.event === 'requestApproval'),
    ).toBe(false);
    await ctx.expectResumeMatches();
  });
});

describe('plan mode Bash ordinary permission behavior', () => {
  it('allows Bash through ordinary yolo permission behavior', async () => {
    const bashCall: ToolCall = {
      type: 'function',
      id: 'call_bash',
      name: 'Bash',
      arguments: '{"command":"rm forbidden.txt","timeout":60}',
    };
    const ctx = testAgent({ kaos: createCommandKaos('removed') });
    ctx.configure({ tools: ['Bash'] });
    await ctx.rpc.setPermission({ mode: 'yolo' });
    await ctx.agent.sessionMode.enter('test-plan', false);

    ctx.mockNextResponse({ type: 'text', text: 'I will mutate a file.' }, bashCall);
    ctx.mockNextResponse({ type: 'text', text: 'The command completed.' });
    await ctx.rpc.prompt({ input: [{ type: 'text', text: 'Remove forbidden.txt' }] });

    await ctx.untilTurnEnd();
    expect(ctx.agent.sessionMode.isActive).toBe(true);
    expect(ctx.agent.sessionMode.kind).toBe('plan');
    expect(toolResultText(ctx.agent.context.history)).toContain('removed');
    await ctx.expectResumeMatches();
  });
});

describe('plan mode injection cadence', () => {
  it('dedupes immediate repeats and emits sparse reminders after assistant turns', async () => {
    const ctx = testAgent();
    ctx.configure();
    await ctx.agent.sessionMode.enter('test-plan', false);

    await ctx.agent.injection.inject();
    const afterFull = ctx.agent.context.history.length;
    expect(lastUserText(ctx.agent.context.history)).toContain('Plan mode is active');

    await ctx.agent.injection.inject();
    expect(ctx.agent.context.history).toHaveLength(afterFull);

    ctx.appendAssistantTurn(1, 'assistant one');
    ctx.appendAssistantTurn(2, 'assistant two');
    await ctx.agent.injection.inject();

    expect(lastUserText(ctx.agent.context.history)).toContain('Plan mode still active');
    expect(lastUserText(ctx.agent.context.history)).toContain('ExitPlanMode');
    await ctx.expectResumeMatches();
  });

  it('emits a reentry reminder when restored plan mode already has plan content', async () => {
    const ctx = testAgent({
      kaos: createFakeKaos({
        readText: vi.fn(async () => '# Existing Plan\n\n- Keep this context'),
      }),
    });
    ctx.configure();
    ctx.dispatch({
      type: 'session_mode.enter',
      id: 'restored-plan',
    });
    (ctx.agent.sessionMode as unknown as { _sessionModeFilePath: string | null })._sessionModeFilePath = '/workspace/.ody-code/plans/restored-plan.md';

    await ctx.agent.injection.inject();

    expect(lastUserText(ctx.agent.context.history)).toContain('Re-entering Plan Mode');
    expect(lastUserText(ctx.agent.context.history)).toContain('Read the existing plan file');
    await ctx.expectResumeMatches();
  });

  it('emits one exit reminder after leaving plan mode', async () => {
    const ctx = testAgent();
    ctx.configure();
    await ctx.agent.sessionMode.enter('test-plan', false);
    await ctx.agent.injection.inject();

    ctx.agent.sessionMode.exit();
    await ctx.agent.injection.inject();
    const afterExit = ctx.agent.context.history.length;
    expect(lastUserText(ctx.agent.context.history)).toContain('Plan mode is no longer active');

    await ctx.agent.injection.inject();
    expect(ctx.agent.context.history).toHaveLength(afterExit);
    await ctx.expectResumeMatches();
  });

  it('keeps the preserved injection index aligned after undo removes earlier messages', async () => {
    const ctx = testAgent();
    ctx.configure();
    await ctx.agent.sessionMode.enter('test-plan', false);

    ctx.agent.context.appendUserMessage([{ type: 'text', text: 'draft the plan' }]);
    await ctx.agent.injection.inject();
    ctx.appendAssistantTurn(1, 'Plan drafted.');

    ctx.agent.context.undo(1);
    ctx.agent.context.appendUserMessage([{ type: 'text', text: 'new plan request' }]);
    await ctx.agent.injection.inject();

    expect(lastUserText(ctx.agent.context.history)).toContain('Plan mode is active');
  });
});

describe('modeModels: per-mode model routing', () => {
  const planKaos = createFakeKaos({ mkdir: vi.fn().mockResolvedValue(undefined) });

  // baseConfig provides both the default model and an extra 'design-model'.
  // configure() will merge in the mock provider via configWithProvider, so
  // 'design-model' must reference the same 'test-provider' name that harness uses.
  const baseConfig = {
    providers: { 'test-provider': { type: 'kimi' as const, apiKey: 'test-key' } },
    models: {
      'mock-model': { provider: 'test-provider', model: 'mock-model', maxContextSize: 1_000_000 },
      'plan-model': { provider: 'test-provider', model: 'plan-model', maxContextSize: 1_000_000 },
      'design-model': { provider: 'test-provider', model: 'design-model', maxContextSize: 1_000_000 },
      'switched-model': { provider: 'test-provider', model: 'switched-model', maxContextSize: 1_000_000 },
    },
  };

  it('switches to the configured design-mode model on enter', async () => {
    const ctx = testAgent({
      kaos: planKaos,
      initialConfig: { ...baseConfig, modeModels: { design: 'design-model' } },
    });
    ctx.configure(); // sets modelAlias = 'mock-model'

    expect(ctx.agent.config.modelAlias).toBe('mock-model');
    await ctx.agent.sessionMode.enter('plan-id', false, false, 'design');

    expect(ctx.agent.config.modelAlias).toBe('design-model');
  });

  it('restores the original model after exiting design mode', async () => {
    const ctx = testAgent({
      kaos: planKaos,
      initialConfig: { ...baseConfig, modeModels: { design: 'design-model' } },
    });
    ctx.configure(); // sets modelAlias = 'mock-model'

    await ctx.agent.sessionMode.enter('plan-id', false, false, 'design');
    expect(ctx.agent.config.modelAlias).toBe('design-model');

    ctx.agent.sessionMode.exit();
    expect(ctx.agent.config.modelAlias).toBe('mock-model');
  });

  it('gracefully skips switching when the mode model alias does not resolve', async () => {
    const ctx = testAgent({
      kaos: planKaos,
      initialConfig: { ...baseConfig, modeModels: { design: 'nonexistent-alias' } },
    });
    ctx.configure(); // sets modelAlias = 'mock-model'

    await ctx.agent.sessionMode.enter('plan-id', false, false, 'design');

    expect(ctx.agent.config.modelAlias).toBe('mock-model');
    expect(ctx.agent.sessionMode.isActive).toBe(true);
  });

  it('allows switching directly from plan to design without throwing', async () => {
    const ctx = testAgent({
      kaos: planKaos,
      initialConfig: {
        ...baseConfig,
        modeModels: { plan: 'plan-model', design: 'design-model' },
      },
    });
    ctx.configure();

    await ctx.agent.sessionMode.enter('plan-id', false, false, 'plan');
    expect(ctx.agent.config.modelAlias).toBe('plan-model');
    expect(ctx.agent.sessionMode.kind).toBe('plan');

    // This used to throw "Already in plan mode"
    await ctx.agent.sessionMode.enter('design-id', false, false, 'design');
    expect(ctx.agent.config.modelAlias).toBe('design-model');
    expect(ctx.agent.sessionMode.kind).toBe('design');
    expect(ctx.agent.sessionMode.isActive).toBe(true);
  });

  it('exits plan mode and restores the pre-plan default model', async () => {
    const ctx = testAgent({
      kaos: planKaos,
      initialConfig: {
        ...baseConfig,
        modeModels: { plan: 'plan-model' },
      },
    });
    ctx.configure();

    await ctx.agent.sessionMode.enter('plan-id', false, false, 'plan');
    expect(ctx.agent.config.modelAlias).toBe('plan-model');

    // User manually switches model while inside plan mode
    ctx.agent.rpcMethods.setModel({ model: 'switched-model' });
    expect(ctx.agent.config.modelAlias).toBe('switched-model');

    // Exit should restore the pre-plan (normal mode) model, not the model
    // that was temporarily switched to inside plan mode.
    ctx.agent.sessionMode.exit();
    expect(ctx.agent.config.modelAlias).toBe('mock-model');
  });
});

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function lastUserText(history: readonly { role: string; content: readonly unknown[] }[]): string {
  const message = history.findLast((item) => item.role === 'user');
  if (message === undefined) return '';
  return message.content
    .map((part) => {
      if (
        part !== null &&
        typeof part === 'object' &&
        (part as { type?: unknown }).type === 'text'
      ) {
        const text = (part as { text?: unknown }).text;
        return typeof text === 'string' ? text : '';
      }
      return '';
    })
    .join('');
}

function toolResultText(history: readonly { role: string; content: readonly unknown[] }[]): string {
  return history
    .filter((message) => message.role === 'tool')
    .flatMap((message) => message.content)
    .map((part) => {
      if (
        part !== null &&
        typeof part === 'object' &&
        (part as { type?: unknown }).type === 'text'
      ) {
        const text = (part as { text?: unknown }).text;
        return typeof text === 'string' ? text : '';
      }
      return '';
    })
    .join('\n');
}

/** Push a real user prompt into history so eager naming has a topic source. */
function seedPrompt(ctx: { agent: { context: { history: unknown } } }, text: string): void {
  (ctx.agent.context.history as unknown as unknown[]).push({
    role: 'user',
    origin: { kind: 'user' },
    content: [{ type: 'text', text }],
  });
}

describe('lazy file path resolution', () => {
  it('defers the path when entered before any user prompt and creates no file', async () => {
    const mkdir = vi.fn().mockResolvedValue(undefined);
    const writeText = vi.fn().mockResolvedValue(0);
    const ctx = testAgent({
      kaos: createFakeKaos({ mkdir, writeText }),
    });
    await ctx.agent.sessionMode.enter('test-id', false, false, 'plan');

    expect(ctx.agent.sessionMode.isActive).toBe(true);
    // No prompt to name from yet → naming deferred (null), not locked to "untitled".
    expect(ctx.agent.sessionMode.sessionModeFilePath).toBeNull();
    expect(writeText).not.toHaveBeenCalled();
  });

  it('defers naming when the mode is entered BEFORE the first prompt (regression)', async () => {
    // Reproduces the ecopower bug: session launches already in design mode, so
    // enter() runs with an empty history; the prompt arrives ~seconds later.
    const ctx = testAgent({ kaos: createPlanKaos() });

    await ctx.agent.sessionMode.enter('pre-prompt', false, false, 'design');
    // Entered with no user prompt → path must be deferred, NOT "…-untitled.md".
    expect(ctx.agent.sessionMode.sessionModeFilePath).toBeNull();

    // The prompt arrives after entry (with pasted file paths, like the real case).
    (ctx.agent.context.history as unknown as unknown[]).push({
      role: 'user',
      origin: { kind: 'user' },
      content: [
        {
          type: 'text',
          text: '合并两份设计：/Users/a/.ody-code/designs/jay-garrick-flash-monet.md\n/Users/b/2026-06-06-frontend-backend-integration-design.md',
        },
      ],
    });

    // First write resolves the path from the prompt topic (path noise stripped).
    const path = await ctx.agent.sessionMode.resolveFilePathFromContent('# Some Design Title\n\nbody');
    const today = formatDatePrefix(new Date());
    expect(path).toBe(`/workspace/.ody-code/designs/${today}-合并两份设计.md`);
  });

  it('derives the eagerly-reserved path from the latest user message topic', async () => {
    const ctx = testAgent({ kaos: createPlanKaos() });
    // Seed a real user message; the eager resolver extracts a kebab topic from it.
    (ctx.agent.context.history as unknown as unknown[]).push({
      role: 'user',
      origin: { kind: 'user' },
      content: [{ type: 'text', text: 'add a billing module' }],
    });

    await ctx.agent.sessionMode.enter('topic-plan', false, false, 'plan');

    const today = formatDatePrefix(new Date());
    expect(ctx.agent.sessionMode.sessionModeFilePath).toBe(
      `/workspace/.ody-code/plans/${today}-add-a-billing-module.md`,
    );
    // Synchronous extraction — no LLM round-trip on entry.
    expect(ctx.llmCalls).toHaveLength(0);
  });

  it('does not leak a sensitive user message into the reserved filename', async () => {
    const ctx = testAgent({ kaos: createPlanKaos() });
    // Message trips the sensitive-word filter ("key") → topic is rejected.
    (ctx.agent.context.history as unknown as unknown[]).push({
      role: 'user',
      origin: { kind: 'user' },
      content: [{ type: 'text', text: 'store the api key abc123 in redis' }],
    });

    await ctx.agent.sessionMode.enter('secret-plan', false, false, 'plan');

    // No usable (non-sensitive) topic → naming deferred (null); the sensitive text
    // never becomes a filename and nothing is locked in.
    expect(ctx.agent.sessionMode.sessionModeFilePath).toBeNull();
  });

  it('clear() is a no-op when the eagerly-reserved file has not been written yet', async () => {
    const writeText = vi.fn(async (_p: string, c: string) => c.length);
    const stat = vi.fn(async () => {
      throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
    });
    const ctx = testAgent({ kaos: createPlanKaos({ writeText, stat }) });
    // Seed a prompt so entry reserves a concrete path eagerly.
    (ctx.agent.context.history as unknown as unknown[]).push({
      role: 'user',
      origin: { kind: 'user' },
      content: [{ type: 'text', text: 'add a billing module' }],
    });
    await ctx.agent.sessionMode.enter('unwritten-plan', false, false, 'plan');

    // Path is reserved, but nothing has been written to disk.
    expect(ctx.agent.sessionMode.sessionModeFilePath).not.toBeNull();
    await ctx.agent.sessionMode.clear();

    // No empty file is materialised for a plan the model never started.
    expect(writeText).not.toHaveBeenCalled();
  });

  it('enter uses a UUID as the internal session mode id', async () => {
    const ctx = testAgent({
      kaos: createPlanKaos(),
    });
    await ctx.agent.sessionMode.enter();

    const id = (ctx.agent.sessionMode as unknown as { _sessionModeId: string })._sessionModeId;
    expect(id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i);
  });

  it('restoreEnter does not restore file path', async () => {
    const ctx = testAgent({ kaos: createFakeKaos() });
    ctx.dispatch({
      type: 'session_mode.enter',
      id: 'restored-id',
      kind: 'plan',
    });

    expect(ctx.agent.sessionMode.sessionModeFilePath).toBeNull();
    expect(ctx.agent.sessionMode.isActive).toBe(true);
  });

  it('finalizeFileName method is removed', () => {
    const ctx = testAgent({ kaos: createFakeKaos() });
    expect('finalizeFileName' in ctx.agent.sessionMode).toBe(false);
  });

  it('fileStem getter is removed', () => {
    const ctx = testAgent({ kaos: createFakeKaos() });
    expect('fileStem' in ctx.agent.sessionMode).toBe(false);
  });

  it('resolveFilePathFromContent extracts heading and formats dated slug', async () => {
    const files = new Map<string, string>();
    const mkdir = vi.fn().mockResolvedValue(undefined);
    const writeText = vi.fn().mockResolvedValue(0);
    const stat = vi.fn(async (path: string) => {
      if (files.has(path)) return { stMode: 0o100644, stIno: 1, stDev: 1, stNlink: 1, stUid: 1000, stGid: 1000, stSize: files.get(path)!.length, stAtime: Date.now(), stMtime: Date.now(), stCtime: Date.now() };
      throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
    });
    const ctx = testAgent({
      kaos: createFakeKaos({ mkdir, writeText, stat }),
    });
    await ctx.agent.sessionMode.enter('test-id', false, false, 'plan');
    // Clear the eagerly-reserved path so the lazy content-based resolver runs.
    (ctx.agent.sessionMode as unknown as { _sessionModeFilePath: string | null })._sessionModeFilePath = null;

    const path = await ctx.agent.sessionMode.resolveFilePathFromContent('# My Design\n\ncontent');

    const today = formatDatePrefix(new Date());
    expect(path).toBe(`/workspace/.ody-code/plans/${today}-my-design.md`);
    expect(ctx.agent.sessionMode.sessionModeFilePath).toBe(path);
  });

  it('resolveFilePathFromContent calls LLM when no heading exists', async () => {
    const files = new Map<string, string>();
    const mkdir = vi.fn().mockResolvedValue(undefined);
    const writeText = vi.fn().mockResolvedValue(0);
    const stat = vi.fn(async (path: string) => {
      if (files.has(path)) return { stMode: 0o100644, stIno: 1, stDev: 1, stNlink: 1, stUid: 1000, stGid: 1000, stSize: files.get(path)!.length, stAtime: Date.now(), stMtime: Date.now(), stCtime: Date.now() };
      throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
    });
    const ctx = testAgent({
      kaos: createFakeKaos({ mkdir, writeText, stat }),
    });
    ctx.configure();
    ctx.mockNextResponse({ type: 'text', text: 'API Design' });
    await ctx.agent.sessionMode.enter('test-id', false, false, 'plan');
    // Clear the eagerly-reserved path so the lazy content-based resolver (LLM) runs.
    (ctx.agent.sessionMode as unknown as { _sessionModeFilePath: string | null })._sessionModeFilePath = null;

    const path = await ctx.agent.sessionMode.resolveFilePathFromContent('Some prose without a heading.');

    const today = formatDatePrefix(new Date());
    expect(path).toBe(`${process.cwd()}/.ody-code/plans/${today}-api-design.md`);
    expect(ctx.llmCalls).toHaveLength(1);
  });

  it('resolveFilePathFromContent falls back to untitled on LLM failure', async () => {
    const files = new Map<string, string>();
    const mkdir = vi.fn().mockResolvedValue(undefined);
    const writeText = vi.fn().mockResolvedValue(0);
    const stat = vi.fn(async (path: string) => {
      if (files.has(path)) return { stMode: 0o100644, stIno: 1, stDev: 1, stNlink: 1, stUid: 1000, stGid: 1000, stSize: files.get(path)!.length, stAtime: Date.now(), stMtime: Date.now(), stCtime: Date.now() };
      throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
    });
    const generate = vi.fn(async () => { throw new Error('LLM timeout'); });
    const ctx = testAgent({
      kaos: createFakeKaos({ mkdir, writeText, stat }),
      generate,
    });
    await ctx.agent.sessionMode.enter('test-id', false, false, 'plan');
    // Clear the eagerly-reserved path so the lazy resolver runs and hits the LLM fallback.
    (ctx.agent.sessionMode as unknown as { _sessionModeFilePath: string | null })._sessionModeFilePath = null;

    const path = await ctx.agent.sessionMode.resolveFilePathFromContent('No heading.');

    const today = formatDatePrefix(new Date());
    expect(path).toBe(`/workspace/.ody-code/plans/${today}-untitled.md`);
  });

  it('resolveFilePathFromContent resolves collision with suffix', async () => {
    const files = new Map<string, string>();
    const today = formatDatePrefix(new Date());
    files.set(`/workspace/.ody-code/plans/${today}-foo.md`, 'existing');
    const mkdir = vi.fn().mockResolvedValue(undefined);
    const writeText = vi.fn().mockResolvedValue(0);
    const stat = vi.fn(async (path: string) => {
      if (files.has(path)) return { stMode: 0o100644, stIno: 1, stDev: 1, stNlink: 1, stUid: 1000, stGid: 1000, stSize: files.get(path)!.length, stAtime: Date.now(), stMtime: Date.now(), stCtime: Date.now() };
      throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
    });
    const ctx = testAgent({
      kaos: createFakeKaos({ mkdir, writeText, stat }),
    });
    await ctx.agent.sessionMode.enter('test-id', false, false, 'plan');
    // Clear the eagerly-reserved path so the lazy content-based resolver runs.
    (ctx.agent.sessionMode as unknown as { _sessionModeFilePath: string | null })._sessionModeFilePath = null;

    const path = await ctx.agent.sessionMode.resolveFilePathFromContent('# Foo\n\ncontent');

    expect(path).toBe(`/workspace/.ody-code/plans/${today}-foo-1.md`);
  });

  it('resolveFilePathFromContent returns existing path if already resolved', async () => {
    const ctx = testAgent({ kaos: createPlanKaos() });
    await ctx.agent.sessionMode.enter('test-id', false, false, 'plan');
    (ctx.agent.sessionMode as unknown as { _sessionModeFilePath: string | null })._sessionModeFilePath = '/workspace/.ody-code/plans/existing.md';

    const path = await ctx.agent.sessionMode.resolveFilePathFromContent('# New Heading\n\ncontent');

    expect(path).toBe('/workspace/.ody-code/plans/existing.md');
  });
});

describe('isWritableSessionModePath subdirectory model', () => {
  it('allows the main session mode file', async () => {
    const ctx = testAgent({ kaos: createPlanKaos() });
    await ctx.agent.sessionMode.enter('test-id', false, false, 'plan');
    (ctx.agent.sessionMode as unknown as { _sessionModeFilePath: string | null })._sessionModeFilePath = '/workspace/.ody-code/plans/2026-06-06-foo.md';

    expect(ctx.agent.sessionMode.isWritableSessionModePath('/workspace/.ody-code/plans/2026-06-06-foo.md')).toBe(true);
  });

  it('allows .md files inside a subdirectory named after the main stem', async () => {
    const ctx = testAgent({ kaos: createPlanKaos() });
    await ctx.agent.sessionMode.enter('test-id', false, false, 'plan');
    (ctx.agent.sessionMode as unknown as { _sessionModeFilePath: string | null })._sessionModeFilePath = '/workspace/.ody-code/plans/2026-06-06-foo.md';

    expect(ctx.agent.sessionMode.isWritableSessionModePath('/workspace/.ody-code/plans/2026-06-06-foo/subsystem-a.md')).toBe(true);
  });

  it('denies non-.md files inside the subdirectory', async () => {
    const ctx = testAgent({ kaos: createPlanKaos() });
    await ctx.agent.sessionMode.enter('test-id', false, false, 'plan');
    (ctx.agent.sessionMode as unknown as { _sessionModeFilePath: string | null })._sessionModeFilePath = '/workspace/.ody-code/plans/2026-06-06-foo.md';

    expect(ctx.agent.sessionMode.isWritableSessionModePath('/workspace/.ody-code/plans/2026-06-06-foo/data.json')).toBe(false);
  });

  it('denies .md files in a sibling subdirectory', async () => {
    const ctx = testAgent({ kaos: createPlanKaos() });
    await ctx.agent.sessionMode.enter('test-id', false, false, 'plan');
    (ctx.agent.sessionMode as unknown as { _sessionModeFilePath: string | null })._sessionModeFilePath = '/workspace/.ody-code/plans/2026-06-06-foo.md';

    expect(ctx.agent.sessionMode.isWritableSessionModePath('/workspace/.ody-code/plans/2026-06-06-bar/anything.md')).toBe(false);
  });

  it('denies path traversal escaping the subdirectory', async () => {
    const ctx = testAgent({ kaos: createPlanKaos() });
    await ctx.agent.sessionMode.enter('test-id', false, false, 'plan');
    (ctx.agent.sessionMode as unknown as { _sessionModeFilePath: string | null })._sessionModeFilePath = '/workspace/.ody-code/plans/2026-06-06-foo.md';

    expect(ctx.agent.sessionMode.isWritableSessionModePath('/workspace/.ody-code/plans/2026-06-06-foo/../2026-06-06-bar.md')).toBe(false);
  });

  // Integration lock: the split-continuation directive's emitted write target
  // MUST be a path the guard actually allows. This is the exact contradiction
  // that caused the production bug (directive said same-dir `<id>-<sub>.md`, the
  // guard only allowed the `<id>/` subdirectory). Extract the target from the
  // directive TEXT (not by recomputing it) so re-introducing the same-dir form
  // would resolve to a guard-denied path and fail here.
  it('emits a continuation-directive target the write guard allows', async () => {
    const index = '/workspace/.ody-code/plans/2026-06-06-foo.md';
    const ctx = testAgent({ kaos: createPlanKaos() });
    await ctx.agent.sessionMode.enter('test-id', false, false, 'plan');
    (ctx.agent.sessionMode as unknown as { _sessionModeFilePath: string | null })._sessionModeFilePath = index;

    const indexStem = basename(index).replace(/\.md$/, '');
    const directive = splitContinuationDirective({ file: 'api.md', scope: 'endpoints' }, indexStem);

    // The directive states `write ONLY \`<target>\`` — pull that first quoted token.
    const target = directive.match(/write ONLY `([^`]+)`/)?.[1];
    expect(target).toBe('2026-06-06-foo/api.md');

    // Resolve the (index-relative) target against the index's directory, then
    // assert the guard permits exactly that absolute path.
    const absolute = join(dirname(index), target ?? '');
    expect(ctx.agent.sessionMode.isWritableSessionModePath(absolute)).toBe(true);
  });
});

describe('code review fixes', () => {
  it('cancelPlan RPC handler exits plan mode without finalizing', async () => {
    const ctx = testAgent({
      kaos: createPlanKaos(),
    });
    await ctx.agent.sessionMode.enter('brave-fox-1234', false, false, 'plan');

    await ctx.rpc.cancelPlan({ id: 'brave-fox-1234' });

    expect(ctx.agent.sessionMode.isActive).toBe(false);
  });

  it('enterPlan RPC handler switches kind without finalizing', async () => {
    const ctx = testAgent({
      kaos: createPlanKaos(),
    });
    await ctx.agent.sessionMode.enter('brave-fox-1234', false, false, 'plan');

    await ctx.rpc.enterPlan({ kind: 'design' });

    expect(ctx.agent.sessionMode.kind).toBe('design');
    expect(ctx.agent.sessionMode.isActive).toBe(true);
  });
});

describe('project-scoped directory resolution', () => {
  it('resolves project-scoped plan directory when mkdir succeeds', async () => {
    const mkdir = vi.fn().mockResolvedValue(undefined);
    const ctx = testAgent({ kaos: createFakeKaos({ mkdir }) });
    seedPrompt(ctx, 'add a billing module');
    await ctx.agent.sessionMode.enter('test-plan', false, false, 'plan');
    expect(ctx.agent.sessionMode.sessionModeFilePath).toBe(
      `/workspace/.ody-code/plans/${formatDatePrefix(new Date())}-add-a-billing-module.md`,
    );
    expect(mkdir).toHaveBeenCalledWith('/workspace/.ody-code/plans', { parents: true, existOk: true });
  });

  it('falls back to session-scoped path on permission error', async () => {
    const mkdir = vi.fn(async (path: string) => {
      if (path === '/workspace/.ody-code/plans') {
        throw Object.assign(new Error('EACCES'), { code: 'EACCES' });
      }
    });
    const ctx = testAgent({ kaos: createFakeKaos({ mkdir }), homedir: '/home/session' });
    seedPrompt(ctx, 'add a billing module');
    await ctx.agent.sessionMode.enter('test-plan', false, false, 'plan');
    expect(ctx.agent.sessionMode.sessionModeFilePath).toBe(
      `/home/session/plans/${formatDatePrefix(new Date())}-add-a-billing-module.md`,
    );
    expect(mkdir).toHaveBeenCalledWith('/workspace/.ody-code/plans', { parents: true, existOk: true });
    expect(mkdir).toHaveBeenCalledWith('/home/session/plans', { parents: true, existOk: true });
  });

  it('creates .gitignore entry when project-scoped path is used', async () => {
    const files = new Map<string, string>();
    const mkdir = vi.fn().mockResolvedValue(undefined);
    const readText = vi.fn(async (path: string) => files.get(path) ?? '');
    const writeText = vi.fn(async (path: string, content: string) => {
      files.set(path, content);
      return content.length;
    });
    const ctx = testAgent({ kaos: createFakeKaos({ mkdir, readText, writeText }) });
    await ctx.agent.sessionMode.enter('test-plan', false, false, 'plan');
    expect(files.get('/workspace/.gitignore')).toBe('.ody-code/\n');
  });

  it('does not duplicate .ody-code/ in existing .gitignore', async () => {
    const files = new Map<string, string>();
    files.set('/workspace/.gitignore', '.ody-code/\n');
    const mkdir = vi.fn().mockResolvedValue(undefined);
    const readText = vi.fn(async (path: string) => files.get(path) ?? '');
    const writeText = vi.fn(async (path: string, content: string) => {
      files.set(path, content);
      return content.length;
    });
    const ctx = testAgent({ kaos: createFakeKaos({ mkdir, readText, writeText }) });
    await ctx.agent.sessionMode.enter('test-plan', false, false, 'plan');
    expect(files.get('/workspace/.gitignore')).toBe('.ody-code/\n');
    // Ensure writeText was NOT called for .gitignore
    expect(writeText).not.toHaveBeenCalledWith('/workspace/.gitignore', expect.anything());
  });

  it('keeps findUniqueStem behavior via wrapper', async () => {
    const files = new Map<string, string>();
    const mkdir = vi.fn().mockResolvedValue(undefined);
    const stat = vi.fn(async (path: string) => {
      if (files.has(path)) {
        return { stMode: 0o100644, stIno: 1, stDev: 1, stNlink: 1, stUid: 1000, stGid: 1000, stSize: files.get(path)!.length, stAtime: Date.now(), stMtime: Date.now(), stCtime: Date.now() };
      }
      throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
    });
    const ctx = testAgent({ kaos: createFakeKaos({ mkdir, stat }) });
    await ctx.agent.sessionMode.enter('test-plan', false, false, 'plan');
    const baseStem = '2024-06-05-my-plan';
    // Must survive: base stem itself when file does not exist
    const result = await (ctx.agent.sessionMode as unknown as { findUniqueStem(s: string): Promise<string> }).findUniqueStem(baseStem);
    expect(result).toBe(baseStem);
  });
});

describe('enter with project-scoped paths', () => {
  it('does not log persisted path in session_mode.enter record', async () => {
    const mkdir = vi.fn().mockResolvedValue(undefined);
    const ctx = testAgent({ kaos: createFakeKaos({ mkdir }) });
    await ctx.agent.sessionMode.enter('test-plan', false, false, 'plan');

    const enterRecord = ctx.allEvents.find(
      (event) => event.type === '[wire]' && event.event === 'session_mode.enter',
    );
    expect(enterRecord?.args).toEqual({
      id: 'test-plan',
      kind: 'plan',
      time: expect.any(Number),
    });
  });

  it('enters design mode in project-scoped designs directory', async () => {
    const mkdir = vi.fn().mockResolvedValue(undefined);
    const ctx = testAgent({ kaos: createFakeKaos({ mkdir }) });
    seedPrompt(ctx, 'add a billing module');
    await ctx.agent.sessionMode.enter('test-design', false, false, 'design');
    expect(ctx.agent.sessionMode.sessionModeFilePath).toBe(
      `/workspace/.ody-code/designs/${formatDatePrefix(new Date())}-add-a-billing-module.md`,
    );
    expect(mkdir).toHaveBeenCalledWith('/workspace/.ody-code/designs', { parents: true, existOk: true });
  });
});

describe('resume and finalize with project-scoped paths', () => {
  it('does not restore exact persisted path on resume', async () => {
    const ctx = testAgent({ kaos: createFakeKaos() });
    ctx.dispatch({
      type: 'session_mode.enter',
      id: 'restored-plan',
      path: '/workspace/.ody-code/plans/restored-plan.md',
    });
    expect(ctx.agent.sessionMode.sessionModeFilePath).toBeNull();
  });

  it('does not fall back to legacy path when no persisted path on resume', async () => {
    const ctx = testAgent({ kaos: createFakeKaos() });
    ctx.dispatch({
      type: 'session_mode.enter',
      id: 'legacy-plan',
    });
    expect(ctx.agent.sessionMode.sessionModeFilePath).toBeNull();
  });
});
