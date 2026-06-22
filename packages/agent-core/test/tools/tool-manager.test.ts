import { describe, it, expect, beforeEach } from 'vitest';
import { ToolManager } from '../../src/agent/tool';
import { createFakeKaos } from './fixtures/fake-kaos';

function createMockAgent(): any {
  return {
    config: { cwd: '/workspace', hasProvider: true, provider: {}, modelCapabilities: {} },
    kaos: createFakeKaos(),
    skills: { registry: { getSkillRoots: () => [], listInvocableSkills: () => [] } },
    cron: undefined,
    subagentHost: undefined,
    background: {} as any,
    records: { logRecord: () => {} },
    mcp: undefined,
    type: 'main',
    goals: { getGoal: () => ({ goal: null }) },
    telemetry: {} as any,
    log: { debug: () => {} },
    rpc: undefined,
    toolServices: {},
    homedir: '/home/test',
    kimiConfig: { browser: { traceEnabled: false } },
    emitEvent: () => {},
  };
}

describe('ToolManager builtin wiring', () => {
  let manager: ToolManager;

  beforeEach(() => {
    manager = new ToolManager(createMockAgent());
  });

  it('exposes SaveIdeaReportTool when activated', () => {
    manager.setActiveTools(['SaveIdeaReport']);
    const tool = manager.loopTools.find((t) => t.name === 'SaveIdeaReport');
    expect(tool).toBeDefined();
    expect(tool?.name).toBe('SaveIdeaReport');
  });

  it('does not expose SaveIdeaReportTool when not activated', () => {
    const tool = manager.loopTools.find((t) => t.name === 'SaveIdeaReport');
    expect(tool).toBeUndefined();
  });
});
