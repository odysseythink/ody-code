/**
 * EnterPlanModeTool tests against the current Agent-backed tool surface.
 */

import { describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../src/agent';
import type { PermissionMode } from '../../src/agent/permission';
import {
  EnterPlanModeInputSchema,
  EnterPlanModeTool,
} from '../../src/tools/builtin/planning/enter-plan-mode';
import { executeTool } from './fixtures/execute-tool';

const signal = new AbortController().signal;

function makeAgent(
  input: {
    readonly active?: boolean;
    readonly mode?: PermissionMode;
    readonly sessionModeFilePath?: string | null;
    readonly enter?: () => Promise<void>;
    readonly generate?: () => Promise<{
      message: { content: Array<{ type: string; text: string }> };
    }>;
    readonly history?: Array<{
      role: string;
      content: Array<{ type: string; text: string }>;
      origin?: { kind: string };
    }>;
  } = {},
): { agent: Agent; requestApproval: ReturnType<typeof vi.fn>; enterSpy: ReturnType<typeof vi.fn> } {
  let active = input.active ?? false;
  const requestApproval = vi.fn(async () => {
    return { decision: 'approved' };
  });
  const enterSpy = vi.fn(async () => {
    active = true;
    if (input.enter) await input.enter();
  });
  const agent = {
    sessionMode: {
      get isActive() {
        return active;
      },
      get sessionModeFilePath() {
        return input.sessionModeFilePath ?? null;
      },
      enter: enterSpy,
    },
    permission: { mode: input.mode ?? 'manual' },
    rpc: { requestApproval },
    telemetry: { track: vi.fn() },
    context: {
      history: input.history ?? [],
    },
    config: {
      get provider() {
        return { name: 'mock', modelName: 'mock-model' };
      },
    },
    generate:
      input.generate ??
      vi.fn().mockResolvedValue({
        message: { content: [{ type: 'text', text: 'user-dashboard' }] },
      }),
  } as unknown as Agent;
  return { agent, requestApproval, enterSpy };
}

describe('EnterPlanModeTool', () => {
  it('has name, description, and parameters from the current schema', () => {
    const { agent } = makeAgent();
    const tool = new EnterPlanModeTool(agent);

    expect(tool.name).toBe('EnterPlanMode');
    expect(tool.description.length).toBeGreaterThan(0);
    expect(tool.description).toContain('Use it when ANY of these conditions apply');
    expect(tool.description).toContain('New Feature Implementation');
    expect(tool.description).toContain('When NOT to use');
    expect(tool.description).toContain('subagent_type="explore"');
    expect(EnterPlanModeInputSchema.safeParse({}).success).toBe(true);
    expect(EnterPlanModeInputSchema.safeParse({ topic: 'Auth Refactor' }).success).toBe(false);
    expect(tool.parameters).toMatchObject({
      type: 'object',
      properties: {},
    });
    expect((tool.parameters['properties'] as Record<string, unknown>)['reason']).toBeUndefined();
  });

  it('returns an error when plan mode is already active', async () => {
    const { agent } = makeAgent({ active: true });
    const result = await executeTool(new EnterPlanModeTool(agent), {
      turnId: '0',
      toolCallId: 'tc_1',
      args: {},
      signal,
    });

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('already active');
  });

  it.each(['manual', 'auto', 'yolo'] satisfies PermissionMode[])(
    'enters in %s mode without an approval request and defers filename to planId',
    async (mode) => {
      const { agent, requestApproval, enterSpy } = makeAgent({
        mode,
        history: [
          { role: 'user', content: [{ type: 'text', text: 'Build a user dashboard' }], origin: { kind: 'user' } },
        ],
      });

      const result = await executeTool(new EnterPlanModeTool(agent), {
        turnId: '0',
        toolCallId: `tc_${mode}`,
        args: {},
        signal,
      });

      expect(result.isError).toBeFalsy();
      expect(result.output).toContain('Plan mode is now active');
      expect(requestApproval).not.toHaveBeenCalled();
      expect(enterSpy).toHaveBeenCalledWith(
        undefined,
        undefined,
        undefined,
        'plan',
      );
    },
  );

  it('uses inline guidance when no plan file path is available', async () => {
    const { agent } = makeAgent({ mode: 'yolo', sessionModeFilePath: null });

    const result = await executeTool(new EnterPlanModeTool(agent), {
      turnId: '0',
      toolCallId: 'tc_inline',
      args: {},
      signal,
    });

    expect(result.output).toContain('No plan file path is available in this host yet');
    expect(result.output).not.toContain('`plan` parameter');
    expect(result.output).not.toContain('Plan file:');
  });

  it('uses plan-file guidance when the host provides a plan file path', async () => {
    const { agent } = makeAgent({ mode: 'yolo', sessionModeFilePath: '/tmp/kimi/plans/example.md' });

    const result = await executeTool(new EnterPlanModeTool(agent), {
      turnId: '0',
      toolCallId: 'tc_file',
      args: {},
      signal,
    });

    expect(result.output).toContain('Plan file: /tmp/kimi/plans/example.md');
    expect(result.output).toContain('Write the plan — incrementally');
    expect(result.output).toContain('Depends on:');
  });

  it('returns an error when entering plan mode fails', async () => {
    const { agent } = makeAgent({
      mode: 'yolo',
      enter: vi.fn().mockRejectedValue(new Error('state error')),
    });

    const result = await executeTool(new EnterPlanModeTool(agent), {
      turnId: '0',
      toolCallId: 'tc_error',
      args: {},
      signal,
    });

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('state error');
  });

  it('resolveExecution description returns a stable phrase', () => {
    const { agent } = makeAgent();
    const execution = new EnterPlanModeTool(agent).resolveExecution({});
    expect(execution.isError).toBeFalsy();
    if (execution.isError === true) throw new Error('expected runnable execution');
    expect(execution.description).toContain('plan mode');
  });
});
