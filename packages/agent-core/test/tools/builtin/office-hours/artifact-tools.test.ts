import { describe, expect, it, vi } from 'vitest';
import type { Agent } from '../../../../src/agent';
import { EnsureClaudeMdRoutingTool } from '../../../../src/tools/builtin/office-hours/ensure-routing';
import { SyncOfficeHoursArtifactTool } from '../../../../src/tools/builtin/office-hours/sync-artifact';

function mockAgent(userLanguage?: string, mcp?: { name: string; status: string }[]) {
  const agent = {
    sessionMode: { isActive: true, kind: 'office-hours' },
    userLanguage,
    config: { cwd: '/tmp' },
    kaos: {
      readText: vi.fn(async () => { throw new Error('not found'); }),
      writeText: vi.fn(async () => {}),
      stat: vi.fn(async () => {}),
    },
    mcp: mcp ? {
      list: () => mcp,
    } : undefined,
  } as unknown as Agent;
  return agent;
}

describe('EnsureClaudeMdRoutingTool localized', () => {
  it('returns Chinese created message (zh)', async () => {
    const agent = mockAgent('zh');
    const tool = new EnsureClaudeMdRoutingTool(agent);
    const result = await tool.resolveExecution({}).execute();
    expect(result.output).toContain('创建');
  });

  it('returns Chinese already-has message (zh)', async () => {
    const agent = {
      ...mockAgent('zh'),
      kaos: {
        readText: vi.fn(async () => '## Skill routing\nexisting'),
        writeText: vi.fn(async () => {}),
        stat: vi.fn(async () => {}),
      },
    } as unknown as Agent;
    const tool = new EnsureClaudeMdRoutingTool(agent);
    const result = await tool.resolveExecution({}).execute();
    expect(result.output).toContain('包含');
  });
});

describe('SyncOfficeHoursArtifactTool localized', () => {
  it('returns Chinese design-file-not-found (zh)', async () => {
    const agent = {
      ...mockAgent('zh'),
      kaos: {
        readText: vi.fn(async () => { throw new Error('no file'); }),
        stat: vi.fn(async () => { throw new Error('no file'); }),
      },
    } as unknown as Agent;
    const tool = new SyncOfficeHoursArtifactTool(agent);
    const result = await tool.resolveExecution({ designFilePath: '/tmp/missing.md' }).execute();
    expect(result.isError).toBe(true);
    expect(result.output).toContain('在');
    expect(result.output).toContain('未找到设计文件');
  });

  it('returns Chinese MCP connected message (zh)', async () => {
    const agent = mockAgent('zh', [{ name: 'gbrain-server', status: 'connected', transport: 'stdio', toolCount: 1 }]);
    const tool = new SyncOfficeHoursArtifactTool(agent);
    const result = await tool.resolveExecution({ designFilePath: '/tmp/test.md' }).execute();
    expect(result.output).toContain('gbrain MCP');
    expect(result.output).toContain('连接');
  });
});
