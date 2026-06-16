import { describe, expect, it, vi } from 'vitest';
import { EnterOfficeHoursModeTool } from '../../../src/tools/builtin/office-hours/enter-office-hours';
import { ExitOfficeHoursModeTool } from '../../../src/tools/builtin/office-hours/exit-office-hours';
import type { Agent } from '../../../src/agent';
import { SessionMode } from '../../../src/agent/session-mode';
import { executeTool } from '../../tools/fixtures/execute-tool';

const signal = new AbortController().signal;

function mockAgent(sessionMode?: SessionMode): Agent {
  const kaos = {
    mkdir: vi.fn().mockResolvedValue(undefined),
    readText: vi.fn().mockRejectedValue({ code: 'ENOENT' }),
    writeText: vi.fn().mockResolvedValue(undefined),
    stat: vi.fn().mockRejectedValue({ code: 'ENOENT' }),
  };
  const config = {
    cwd: '/fake/project',
    modelAlias: 'default',
    update: vi.fn(),
  };
  const records = { logRecord: vi.fn() };
  const mode = sessionMode ?? new SessionMode({
    kaos, config, records, homedir: '/x',
    kimiConfig: undefined, modelProvider: undefined,
    replayBuilder: { push: vi.fn() },
    emitStatusUpdated: vi.fn(),
    setContextMode: vi.fn(),
    log: undefined, rpc: undefined,
    telemetry: { track: vi.fn() },
  } as unknown as Agent);
  return {
    kaos, config, records, sessionMode: mode, homedir: '/x',
    kimiConfig: undefined, modelProvider: undefined,
    replayBuilder: { push: vi.fn() },
    emitStatusUpdated: vi.fn(),
    setContextMode: vi.fn(),
    log: undefined, rpc: undefined,
    telemetry: { track: vi.fn() },
  } as unknown as Agent;
}

describe('EnterOfficeHoursModeTool', () => {
  it('enters office-hours mode when not active', async () => {
    const agent = mockAgent();
    const tool = new EnterOfficeHoursModeTool(agent);
    const result = await executeTool(tool, {
      turnId: '0',
      toolCallId: 'tc_1',
      args: {},
      signal,
    });
    expect(result.isError).toBeFalsy();
    expect(agent.sessionMode.kind).toBe('office-hours');
  });

  it('returns isError when office-hours is already active', async () => {
    const agent = mockAgent();
    await agent.sessionMode.enter('id', false, false, 'office-hours');
    const tool = new EnterOfficeHoursModeTool(agent);
    const result = await executeTool(tool, {
      turnId: '0',
      toolCallId: 'tc_2',
      args: {},
      signal,
    });
    expect(result.isError).toBe(true);
  });
});

describe('ExitOfficeHoursModeTool', () => {
  it('exits office-hours mode and returns completion message', async () => {
    const agent = mockAgent();
    await agent.sessionMode.enter('id', false, false, 'office-hours');
    const tool = new ExitOfficeHoursModeTool(agent);
    const result = await executeTool(tool, {
      turnId: '0',
      toolCallId: 'tc_3',
      args: { approved: true },
      signal,
    });
    expect(result.isError).toBeFalsy();
    expect(agent.sessionMode.isActive).toBe(false);
  });

  it('returns isError when office-hours is not active', async () => {
    const agent = mockAgent();
    const tool = new ExitOfficeHoursModeTool(agent);
    const result = await executeTool(tool, {
      turnId: '0',
      toolCallId: 'tc_4',
      args: { approved: true },
      signal,
    });
    expect(result.isError).toBe(true);
  });
});
