import { describe, expect, it, vi } from 'vitest';
import type { Agent } from '../../../../src/agent';
import type { ExecutableToolResult, RunnableToolExecution, ToolExecution } from '../../../../src/loop/types';
import { SetProductLanguageTool } from '../../../../src/tools/builtin/product/set-language';

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

function mockAgent(overrides: Partial<{ isActive: boolean; kind: string; userLanguage: string | undefined }> = {}) {
  return {
    sessionMode: {
      isActive: overrides.isActive ?? false,
      kind: overrides.kind ?? 'normal',
    },
    userLanguage: overrides.userLanguage,
    setUserLanguage: vi.fn(),
  } as unknown as Agent;
}

describe('SetProductLanguageTool', () => {
  it('sets userLanguage when product is active and code is valid', async () => {
    const agent = mockAgent({ isActive: true, kind: 'product' });
    const tool = new SetProductLanguageTool(agent);
    const exec = runnable(tool.resolveExecution({ language: 'zh' }));
    expect(exec.description).toBe('Setting office hours user language');
    expect(exec.approvalRule).toBe('SetProductLanguage');
    const result = await exec.execute();
    expect(agent.setUserLanguage).toHaveBeenCalledWith('zh');
    expect(result.output).toBe('用户语言已设置为 zh。');
  });

  it('rejects with modeNotActive when not in product', async () => {
    const agent = mockAgent({ isActive: false });
    const tool = new SetProductLanguageTool(agent);
    const result = await runnable(tool.resolveExecution({ language: 'en' })).execute();
    expect(result.isError).toBe(true);
    expect(result.output).toBe('Office hours mode is not active.');
  });

  it('rejects unsupported language code', async () => {
    const agent = mockAgent({ isActive: true, kind: 'product' });
    const tool = new SetProductLanguageTool(agent);
    const result = await runnable(tool.resolveExecution({ language: 'fr' as any })).execute();
    expect(result.isError).toBe(true);
    expect(result.output).toBe('Unsupported language: fr');
  });
});
