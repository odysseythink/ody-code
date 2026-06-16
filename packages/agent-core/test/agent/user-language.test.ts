import { describe, expect, it, vi } from 'vitest';
import { Agent } from '../../src/agent';
import { testKaos } from '../fixtures/test-kaos';

describe('Agent userLanguage', () => {
  it('restores userLanguage from AgentOptions', () => {
    const agent = new Agent({ kaos: testKaos, userLanguage: 'zh' });
    expect(agent.userLanguage).toBe('zh');
  });

  it('defaults userLanguage to undefined', () => {
    const agent = new Agent({ kaos: testKaos });
    expect(agent.userLanguage).toBeUndefined();
  });

  it('setUserLanguage updates runtime and calls callback', () => {
    const spy = vi.fn();
    const agent = new Agent({ kaos: testKaos, setUserLanguage: spy });
    agent.setUserLanguage('zh');
    expect(agent.userLanguage).toBe('zh');
    expect(spy).toHaveBeenCalledWith('zh');
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it('setUserLanguage does not throw when callback is undefined', () => {
    const agent = new Agent({ kaos: testKaos });
    expect(() => agent.setUserLanguage('en')).not.toThrow();
    expect(agent.userLanguage).toBe('en');
  });

  it('setUserLanguage emits status updated event', () => {
    const events: Array<{ type: string; userLanguage?: unknown }> = [];
    const agent = new Agent({
      kaos: testKaos,
      rpc: {
        emitEvent: (event: any) => { events.push(event); },
      } as any,
    });
    // emitStatusUpdated guards on hasModel — seed config
    agent.config.update({
      cwd: '/tmp',
      modelAlias: 'test-model',
      systemPrompt: 'test',
      thinkingLevel: 'off',
    });
    agent.setUserLanguage('zh');
    // Take the LAST status event (the one emitted by setUserLanguage)
    const statusEvents = events.filter(e => e.type === 'agent.status.updated');
    const statusEvent = statusEvents[statusEvents.length - 1];
    expect(statusEvent).toBeDefined();
    expect(statusEvent!.userLanguage).toBe('zh');
  });

  it('getUserLanguage returns undefined when not set', () => {
    const agent = new Agent({ kaos: testKaos });
    expect(agent.rpcMethods.getUserLanguage({})).toBeUndefined();
  });

  it('getUserLanguage returns language after set', () => {
    const agent = new Agent({ kaos: testKaos });
    agent.setUserLanguage('en');
    expect(agent.rpcMethods.getUserLanguage({})).toBe('en');
  });
});
