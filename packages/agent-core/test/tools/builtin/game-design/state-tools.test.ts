import { describe, it, expect, vi } from 'vitest';
import { AppendGameDesignProfileTool } from '../../../../src/tools/builtin/game-design/append-game-design-profile';
import { AppendGameDesignLearningTool } from '../../../../src/tools/builtin/game-design/append-game-design-learning';
import { SearchGameDesignLearningsTool } from '../../../../src/tools/builtin/game-design/search-game-design-learnings';
import type { RunnableToolExecution } from '../../../../src/loop/types';

function mockAgent() {
  return {
    userLanguage: 'en',
    sessionMode: {
      isActive: true,
      kind: 'game-design' as const,
      sessionModeFilePath: '/fake/.ody-code/game-design/game-design.md',
    },
    gameDesignStateStore: {
      appendProfile: vi.fn().mockResolvedValue(undefined),
      appendLearning: vi.fn().mockResolvedValue(undefined),
      searchLearnings: vi.fn().mockResolvedValue([]),
    },
  } as any;
}

describe('AppendGameDesignProfileTool', () => {
  it('appends profile entry', async () => {
    const agent = mockAgent();
    const tool = new AppendGameDesignProfileTool(agent);
    const execution = tool.resolveExecution({
      mode: 'builder',
      projectSlug: 'test-game',
      pillars: 'Explore, Build',
      audience: 'Casual',
      platform: 'Mobile',
      genre: 'Adventure',
      designDoc: '/fake/game-design.md',
    }) as RunnableToolExecution;
    const result = await execution.execute({} as any);
    expect(agent.gameDesignStateStore.appendProfile).toHaveBeenCalled();
    expect(result.isError).toBeFalsy();
  });
});

describe('AppendGameDesignLearningTool', () => {
  it('appends learning entry', async () => {
    const agent = mockAgent();
    const tool = new AppendGameDesignLearningTool(agent);
    const execution = tool.resolveExecution({
      type: 'eureka',
      key: 'difficulty-spike-level-3',
      insight: 'Players hit a wall at Level 3 boss.',
      confidence: 0.9,
    }) as RunnableToolExecution;
    const result = await execution.execute({} as any);
    expect(agent.gameDesignStateStore.appendLearning).toHaveBeenCalled();
    expect(result.isError).toBeFalsy();
  });
});

describe('SearchGameDesignLearningsTool', () => {
  it('returns no learnings message for empty result', async () => {
    const agent = mockAgent();
    agent.gameDesignStateStore.searchLearnings = vi.fn().mockResolvedValue([]);
    const tool = new SearchGameDesignLearningsTool(agent);
    const execution = tool.resolveExecution({ limit: 5 }) as RunnableToolExecution;
    const result = await execution.execute({} as any);
    expect(result.output).toContain('No past learnings');
  });
});
