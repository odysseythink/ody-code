import { describe, expect, it } from 'vitest';
import { ParityDriver } from '../../src/parity/driver';
import type { ParityBackend, Scenario, ScenarioSnapshot } from '../../src/parity/types';

function fakeBackend(eventsToEmit: any[] = []): ParityBackend {
  const listeners = new Set<(event: unknown) => void>();
  return {
    kind: 'ts',
    homeDir: '/tmp/fake',
    client: {
      onEvent(listener: (event: unknown) => void) {
        listeners.add(listener);
        return () => listeners.delete(listener);
      },
      async createSession() {
        listeners.forEach((l) => l({ type: 'session.meta.updated', title: 't' }));
        return { id: 'session-123' };
      },
    } as any,
    close: async () => {},
  };
}

describe('ParityDriver', () => {
  it('collects events emitted during scenario run', async () => {
    const driver = new ParityDriver({ timeoutMs: 1000 });
    const backend = fakeBackend();
    const scenario: Scenario = {
      name: 'emit-and-respond',
      async run(b): Promise<ScenarioSnapshot> {
        const summary = await b.client.createSession({ workDir: b.homeDir });
        return { responses: [summary], events: [] };
      },
    };
    const snapshot = await driver.runScenario(backend, scenario);
    expect(snapshot.responses).toHaveLength(1);
    expect(snapshot.events).toHaveLength(1);
    expect((snapshot.events[0] as any).type).toBe('session.meta.updated');
  });

  it('unsubscribes after scenario run', async () => {
    const driver = new ParityDriver({ timeoutMs: 1000 });
    const backend = fakeBackend();
    const scenario: Scenario = {
      name: 'noop',
      async run(): Promise<ScenarioSnapshot> {
        return { responses: [], events: [] };
      },
    };
    await driver.runScenario(backend, scenario);
    // After run, no listener should receive the event.
    const client = backend.client as any;
    client.onEvent((l: any) => l({ type: 'should.not.happen' }));
  });

  it('times out a hanging scenario', async () => {
    const driver = new ParityDriver({ timeoutMs: 10 });
    const backend = fakeBackend();
    const scenario: Scenario = {
      name: 'hang',
      async run(): Promise<ScenarioSnapshot> {
        await new Promise(() => {});
        return { responses: [], events: [] };
      },
    };
    await expect(driver.runScenario(backend, scenario)).rejects.toThrow('timed out');
  });
});
