import { describe, expect, it } from 'vitest';
import type { Agent } from '../../../../src/agent';
import { EnterProductModeTool } from '../../../../src/tools/builtin/product/enter-product';
import { ExitProductModeTool } from '../../../../src/tools/builtin/product/exit-product';

import type { ExecutableToolResult, RunnableToolExecution, ToolExecution } from '../../../../src/loop/types';

function runnable(
  exec: ToolExecution,
): Omit<RunnableToolExecution, 'execute'> & { execute(): Promise<ExecutableToolResult> } {
  if ('execute' in exec) {
    return {
      ...exec,
      execute: () =>
        exec.execute({
          turnId: 'test',
          toolCallId: 'test',
          signal: new AbortController().signal,
        }),
    };
  }
  throw new Error(exec.message ?? String(exec.output));
}

function mockAgent(overrides: Partial<{ isActive: boolean; kind: string; userLanguage: string | undefined; path: string | null }> = {}) {
  return {
    sessionMode: {
      isActive: overrides.isActive ?? false,
      kind: overrides.kind ?? 'normal',
      exit: () => {},
      enter: async () => {},
      sessionModeFilePath: overrides.path ?? null,
    },
    userLanguage: overrides.userLanguage,
    kaos: { stat: async () => {} },
  } as unknown as Agent;
}

describe('EnterProductModeTool localized output', () => {
  it('returns Chinese error when already active in zh', async () => {
    const agent = mockAgent({ isActive: true, kind: 'product', userLanguage: 'zh' });
    const tool = new EnterProductModeTool(agent);
    const result = await runnable(tool.resolveExecution({})).execute();
    expect(result.isError).toBe(true);
    expect(result.output).toBe('Office Hours 模式已经处于激活状态。会话结束后请调用 ExitProductMode。');
  });

  it('returns Chinese error when another mode active in zh', async () => {
    const agent = mockAgent({ isActive: true, kind: 'plan', userLanguage: 'zh' });
    const tool = new EnterProductModeTool(agent);
    const result = await runnable(tool.resolveExecution({})).execute();
    expect(result.isError).toBe(true);
    expect(result.output).toBe('另一个会话模式已经激活。请先退出该模式再进入 Office Hours。');
  });

  it('returns English error when already active in en', async () => {
    const agent = mockAgent({ isActive: true, kind: 'product', userLanguage: 'en' });
    const tool = new EnterProductModeTool(agent);
    const result = await runnable(tool.resolveExecution({})).execute();
    expect(result.isError).toBe(true);
    expect(result.output).toContain('already active');
  });

  it('returns English error when language is undefined (fallback)', async () => {
    const agent = mockAgent({ isActive: true, kind: 'product', userLanguage: undefined });
    const tool = new EnterProductModeTool(agent);
    const result = await runnable(tool.resolveExecution({})).execute();
    expect(result.isError).toBe(true);
    expect(result.output).toContain('already active');
  });
});

describe('ExitProductModeTool localized output', () => {
  it('returns Chinese error when not in product (zh)', async () => {
    const agent = mockAgent({ isActive: false, userLanguage: 'zh' });
    const tool = new ExitProductModeTool(agent);
    const result = await runnable(tool.resolveExecution({})).execute();
    expect(result.isError).toBe(true);
    expect(result.output).toBe('Office Hours 模式未激活。');
  });

  it('returns Chinese success with path (zh)', async () => {
    const agent = mockAgent({ isActive: true, kind: 'product', userLanguage: 'zh', path: '/tmp/design.md' });
    const tool = new ExitProductModeTool(agent);
    const result = await runnable(tool.resolveExecution({})).execute();
    expect(result.output).toContain('Office Hours 会话已结束。');
    expect(result.output).toContain('设计文档已保存至：/tmp/design.md');
    expect(result.output).toContain('应用即将退出。');
  });
});
