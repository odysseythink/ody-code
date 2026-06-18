import { describe, it, expect, vi } from 'vitest';
import { EnterGameDesignModeTool } from '../../../../src/tools/builtin/game-design/enter-game-design';
import { ExitGameDesignModeTool } from '../../../../src/tools/builtin/game-design/exit-game-design';
import { SetGameDesignLanguageTool } from '../../../../src/tools/builtin/game-design/set-game-design-language';
import type { RunnableToolExecution } from '../../../../src/loop/types';

function mockAgent() {
  return {
    userLanguage: 'en',
    setUserLanguage: vi.fn(),
    sessionMode: {
      isActive: false,
      kind: 'game-design' as const,
      sessionModeFilePath: '/fake/.ody-code/game-design/game-design.md',
      enter: vi.fn().mockResolvedValue(undefined),
      exit: vi.fn(),
    },
  } as any;
}

describe('EnterGameDesignModeTool', () => {
  it('enters game-design mode when not already active', async () => {
    const agent = mockAgent();
    const tool = new EnterGameDesignModeTool(agent);
    const execution = tool.resolveExecution({}) as RunnableToolExecution;
    const result = await execution.execute({} as any);
    expect(agent.sessionMode.enter).toHaveBeenCalledWith(
      undefined, undefined, undefined, 'game-design',
    );
    expect(result.output).toContain('game-design mode is now active');
  });

  it('returns error when game-design already active', async () => {
    const agent = mockAgent();
    agent.sessionMode.isActive = true;
    agent.sessionMode.kind = 'game-design';
    const tool = new EnterGameDesignModeTool(agent);
    const execution = tool.resolveExecution({}) as RunnableToolExecution;
    const result = await execution.execute({} as any);
    expect(result.isError).toBe(true);
  });

  it('returns error when another mode is active', async () => {
    const agent = mockAgent();
    agent.sessionMode.isActive = true;
    agent.sessionMode.kind = 'plan';
    const tool = new EnterGameDesignModeTool(agent);
    const execution = tool.resolveExecution({}) as RunnableToolExecution;
    const result = await execution.execute({} as any);
    expect(result.isError).toBe(true);
  });
});

describe('ExitGameDesignModeTool', () => {
  it('exits game-design mode', async () => {
    const agent = mockAgent();
    agent.sessionMode.isActive = true;
    agent.sessionMode.kind = 'game-design';
    const tool = new ExitGameDesignModeTool(agent);
    const execution = tool.resolveExecution({}) as RunnableToolExecution;
    const result = await execution.execute({} as any);
    expect(agent.sessionMode.exit).toHaveBeenCalled();
    expect(result.output).toContain('Design document saved');
  });

  it('returns error when game-design not active', async () => {
    const agent = mockAgent();
    const tool = new ExitGameDesignModeTool(agent);
    const execution = tool.resolveExecution({}) as RunnableToolExecution;
    const result = await execution.execute({} as any);
    expect(result.isError).toBe(true);
  });
});

describe('SetGameDesignLanguageTool', () => {
  it('sets user language', async () => {
    const agent = mockAgent();
    agent.sessionMode.isActive = true;
    agent.sessionMode.kind = 'game-design';
    const tool = new SetGameDesignLanguageTool(agent);
    const execution = tool.resolveExecution({ language: 'en' }) as RunnableToolExecution;
    const result = await execution.execute({} as any);
    expect(agent.setUserLanguage).toHaveBeenCalledWith('en');
    expect(result.output).toContain('en');
  });
});
