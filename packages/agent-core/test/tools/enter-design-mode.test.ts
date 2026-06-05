import { describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../src/agent';
import type { PermissionMode } from '../../src/agent/permission';
import {
  EnterDesignModeInputSchema,
  EnterDesignModeTool,
} from '../../src/tools/builtin/planning/enter-design-mode';
import { executeTool } from './fixtures/execute-tool';

const signal = new AbortController().signal;

function makeAgent(
  input: {
    readonly active?: boolean;
    readonly kind?: 'plan' | 'design';
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
  const requestApproval = vi.fn(async () => ({ decision: 'approved' }));
  const enterSpy = vi.fn(async () => {
    active = true;
    if (input.enter) await input.enter();
  });
  const agent = {
    sessionMode: {
      get isActive() {
        return active;
      },
      get kind() {
        return input.kind ?? 'design';
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

describe('EnterDesignModeTool', () => {
  it('has name, description, and parameters from the current schema', () => {
    const { agent } = makeAgent();
    const tool = new EnterDesignModeTool(agent);

    expect(tool.name).toBe('EnterDesignMode');
    expect(tool.description.length).toBeGreaterThan(0);
    expect(EnterDesignModeInputSchema.safeParse({}).success).toBe(true);
    expect(EnterDesignModeInputSchema.safeParse({ topic: 'Auth Refactor' }).success).toBe(true);
    expect(tool.parameters).toMatchObject({
      type: 'object',
      properties: {
        topic: { type: 'string' },
      },
    });
  });

  it('returns an error when design mode is already active', async () => {
    const { agent } = makeAgent({ active: true, kind: 'design' });
    const result = await executeTool(new EnterDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'tc_1',
      args: {},
      signal,
    });

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('Design mode is already active');
  });

  it('returns an error when plan mode is already active', async () => {
    const { agent } = makeAgent({ active: true, kind: 'plan' });
    const result = await executeTool(new EnterDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'tc_2',
      args: {},
      signal,
    });

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('Plan mode is already active');
  });

  it.each(['manual', 'auto', 'yolo'] satisfies PermissionMode[])(
    'enters in %s mode without approval and defers filename to planId',
    async (mode) => {
      const { agent, requestApproval, enterSpy } = makeAgent({
        mode,
        history: [
          { role: 'user', content: [{ type: 'text', text: 'Build a user dashboard' }], origin: { kind: 'user' } },
        ],
      });

      const result = await executeTool(new EnterDesignModeTool(agent), {
        turnId: '0',
        toolCallId: `tc_${mode}`,
        args: {},
        signal,
      });

      expect(result.isError).toBeFalsy();
      expect(result.output).toContain('Design mode is now active');
      expect(requestApproval).not.toHaveBeenCalled();
      expect(enterSpy).toHaveBeenCalledWith(
        undefined,
        undefined,
        undefined,
        'design',
        undefined,
      );
    },
  );

  it('uses user-provided topic when given', async () => {
    const { agent, enterSpy } = makeAgent({ mode: 'yolo' });

    const result = await executeTool(new EnterDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'tc_topic',
      args: { topic: 'User Profile' },
      signal,
    });

    expect(result.isError).toBeFalsy();
    expect(enterSpy).toHaveBeenCalledWith(
      undefined,
      undefined,
      undefined,
      'design',
      'user-profile',
    );
  });

  it('returns an error when entering design mode fails', async () => {
    const { agent } = makeAgent({
      mode: 'yolo',
      enter: vi.fn().mockRejectedValue(new Error('state error')),
    });

    const result = await executeTool(new EnterDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'tc_error',
      args: {},
      signal,
    });

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('state error');
  });
});
