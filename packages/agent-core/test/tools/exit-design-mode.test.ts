/**
 * ExitDesignModeTool tests against the current Agent-backed tool surface.
 */

import { describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../src/agent';
import {
  ExitDesignModeInputSchema,
  ExitDesignModeTool,
} from '../../src/tools/builtin/planning/exit-design-mode';
import { executeTool } from './fixtures/execute-tool';

const signal = new AbortController().signal;

function makeAgent(
  input: {
    readonly active?: boolean | undefined;
    readonly design?: string | null | undefined;
    readonly path?: string | undefined;
    readonly sessionModeFilePath?: string | null | undefined;
    readonly emit?: ((event: unknown) => void) | undefined;
  } = {},
): { agent: Agent; requestApproval: ReturnType<typeof vi.fn>; emit: ReturnType<typeof vi.fn>; handoffTo: ReturnType<typeof vi.fn> } {
  let active = input.active ?? true;
  const requestApproval = vi.fn(async () => ({ decision: 'approved' }));
  const emit = vi.fn((event: unknown) => {
    input.emit?.(event);
    if ((event as { type?: string }).type === 'session_mode.exit') active = false;
  });
  const handoffTo = vi.fn(async () => {
    emit({ type: 'session_mode.exit' });
  });
  const agent = {
    sessionMode: {
      get isActive() {
        return active;
      },
      get sessionModeFilePath() {
        return input.sessionModeFilePath ?? null;
      },
      data: vi.fn(async () => {
        if (input.design === null) return null;
        return {
          content: input.design ?? 'Step 1: brainstorm\nStep 2: evaluate',
          path: input.path ?? '/tmp/kimi-design.md',
        };
      }),
      finalizeFileName: vi.fn().mockResolvedValue(null),
      handoffTo,
      exit: () => {
        emit({ type: 'session_mode.exit' });
      },
    },
    rpc: { requestApproval },
    telemetry: { track: vi.fn() },
    emit,
  } as unknown as Agent;
  return { agent, requestApproval, emit, handoffTo };
}

describe('ExitDesignModeTool', () => {
  it('has name, description, and parameters from the current schema', () => {
    const { agent } = makeAgent();
    const tool = new ExitDesignModeTool(agent);

    expect(tool.name).toBe('ExitDesignMode');
    expect(tool.description.length).toBeGreaterThan(0);
    expect(ExitDesignModeInputSchema.safeParse({}).success).toBe(true);
    expect(ExitDesignModeInputSchema.safeParse({ plan: '' }).success).toBe(false);
    expect(tool.parameters).toMatchObject({
      type: 'object',
      properties: {
        options: { type: 'array' },
      },
    });
  });

  it('refuses to exit when design mode is inactive', async () => {
    const { agent, emit } = makeAgent({ active: false });

    const result = await executeTool(new ExitDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_1',
      args: {},
      signal,
    });

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('design mode');
    expect(emit).not.toHaveBeenCalled();
  });

  it('exits with the current design without consulting permission approval', async () => {
    const { agent, requestApproval, emit, handoffTo } = makeAgent({
      design: '# File Design',
      path: '/tmp/kimi-design.md',
    });

    const result = await executeTool(new ExitDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_1',
      args: {},
      signal,
    });

    expect(result.isError).toBe(false);
    expect(requestApproval).not.toHaveBeenCalled();
    expect(emit).toHaveBeenCalledWith({ type: 'session_mode.exit' });
    expect(result.output).toContain('Design saved to: /tmp/kimi-design.md');
    expect(result.output).toContain('Design mode deactivated');
    expect(result.output).not.toContain('# File Design');
    expect(handoffTo).toHaveBeenCalledWith('plan', { selectedLabel: undefined });
  });

  it('passes the declared selected label to handoffTo', async () => {
    const { agent, handoffTo } = makeAgent({
      design: '# File Design',
      path: '/tmp/kimi-design.md',
    });

    const result = await executeTool(new ExitDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_label',
      args: { options: [{ label: 'Approach A', description: 'Do A' }] },
      signal,
      metadata: { selectedLabel: 'Approach A' },
    });

    expect(result.isError).toBe(false);
    expect(result.output).toContain('Selected approach: Approach A');
    expect(handoffTo).toHaveBeenCalledWith('plan', { selectedLabel: 'Approach A' });
  });

  it('does not use inline design fallback when no design file exists', async () => {
    const { agent, emit } = makeAgent({ design: null });

    const result = await executeTool(new ExitDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_inline',
      args: {},
      signal,
    });

    expect(result.isError).toBe(true);
    expect(emit).not.toHaveBeenCalled();
    expect(result.output).toContain('No design file found');
  });

  it('allows empty design content when a valid path exists', async () => {
    const { agent, emit, handoffTo } = makeAgent({
      design: '',
      path: '/tmp/kimi-design.md',
    });

    const result = await executeTool(new ExitDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_empty',
      args: {},
      signal,
    });

    expect(result.isError).toBe(false);
    expect(result.output).toContain('Design saved to: /tmp/kimi-design.md');
    expect(handoffTo).toHaveBeenCalledWith('plan', { selectedLabel: undefined });
  });

  it('surfaces errors from design exit as a tool error', async () => {
    const { agent, handoffTo } = makeAgent();
    handoffTo.mockRejectedValue(new Error('journal write failed'));

    const result = await executeTool(new ExitDesignModeTool(agent), {
      turnId: '0',
      toolCallId: 'call_fail',
      args: {},
      signal,
    });

    expect(result).toMatchObject({ isError: true });
    expect(result.output).toContain('journal write failed');
  });
});

describe('ExitDesignMode option description optionality', () => {
  it('exposes options[].description as optional with a default of empty string', () => {
    const { agent } = makeAgent();
    const tool = new ExitDesignModeTool(agent);

    const optionItems = (
      (tool.parameters['properties'] as Record<string, unknown>)['options'] as {
        items?: {
          required?: readonly string[];
          properties?: Record<string, { default?: unknown }>;
        };
      }
    ).items;

    expect(optionItems?.required).toEqual(['label']);
    expect(optionItems?.required).not.toContain('description');
    expect(optionItems?.properties?.['description']?.default).toBe('');
  });

  it('accepts an option that omits description', () => {
    const result = ExitDesignModeInputSchema.safeParse({
      options: [{ label: 'Approach A' }],
    });

    expect(result.success).toBe(true);
  });
});
