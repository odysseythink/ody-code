import { describe, expect, it } from 'vitest';
import type { Agent } from '../../../../src/agent';
import type { ExecutableToolResult, RunnableToolExecution, ToolExecution } from '../../../../src/loop/types';
import { AppendBuilderProfileTool } from '../../../../src/tools/builtin/office-hours/append-profile';
import { AppendLearningTool } from '../../../../src/tools/builtin/office-hours/append-learning';
import { SearchLearningsTool } from '../../../../src/tools/builtin/office-hours/search-learnings';

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

function mockAgent(userLanguage?: string) {
  return {
    sessionMode: { isActive: true, kind: 'office-hours' },
    userLanguage,
    officeHoursStateStore: {
      appendProfile: async () => {},
      appendLearning: async () => {},
      searchLearnings: async (args: any) => [] as any[],
    },
    config: { cwd: '/tmp' },
  } as unknown as Agent;
}

describe('AppendBuilderProfileTool localized', () => {
  it('returns Chinese success message (zh)', async () => {
    const agent = mockAgent('zh');
    const tool = new AppendBuilderProfileTool(agent);
    const result = await runnable(tool.resolveExecution({
      mode: 'startup', projectSlug: 'test',
      signalCount: 5, signals: [], resourcesShown: [], topics: [],
    } as any)).execute();
    expect(result.output).toBe('Builder 档案条目已追加成功。下次层级计算时将更新会话计数。');
  });

  it('returns English success message (en)', async () => {
    const agent = mockAgent('en');
    const tool = new AppendBuilderProfileTool(agent);
    const result = await runnable(tool.resolveExecution({
      mode: 'startup', projectSlug: 'test',
      signalCount: 5, signals: [], resourcesShown: [], topics: [],
    } as any)).execute();
    expect(result.output).toBe('Builder profile entry appended successfully. Session count will be updated for next tier computation.');
  });
});

describe('AppendLearningTool localized', () => {
  it('returns Chinese message with key (zh)', async () => {
    const agent = mockAgent('zh');
    const tool = new AppendLearningTool(agent);
    const result = await runnable(tool.resolveExecution({
      type: 'eureka', key: 'insight-1', insight: 'test',
      confidence: 1.0,
    })).execute();
    expect(result.output).toBe('学习洞察 "insight-1" 已记录成功。');
  });
});

describe('SearchLearningsTool localized', () => {
  it('returns Chinese no learnings message (zh)', async () => {
    const agent = mockAgent('zh');
    const tool = new SearchLearningsTool(agent);
    const result = await runnable(tool.resolveExecution({ limit: 10 })).execute();
    expect(result.output).toBe('未找到过往学习洞察。');
  });

  it('returns Chinese header with count', async () => {
    const agent = {
      sessionMode: { isActive: true, kind: 'office-hours' },
      userLanguage: 'zh',
      officeHoursStateStore: {
        searchLearnings: async () => [{
          ts: '2026-01-01', type: 'eureka', key: 'x', insight: 'y',
          confidence: 0.5, source: 'observed' as const,
        }],
      },
      config: { cwd: '/tmp' },
    } as unknown as Agent;
    const tool = new SearchLearningsTool(agent);
    const result = await runnable(tool.resolveExecution({ limit: 10 })).execute();
    expect(result.output).toContain('找到 1 条学习洞察：');
    expect(result.output).toContain('类型');
    expect(result.output).toContain('洞察');
    expect(result.output).toContain('置信度');
    expect(result.output).toContain('日期');
  });
});
