import { describe, expect, it, vi } from 'vitest';

import type { Agent } from '../../src/agent';
import { AgentRecords } from '../../src/agent/records';
import { InMemoryAgentRecordPersistence } from '../../src/agent/records/persistence';
import type { AgentRecord } from '../../src/agent/records/types';

function makeAgent(): Agent {
  return {
    appVersion: 'test',
    log: {
      error: vi.fn(),
      info: vi.fn(),
      warn: vi.fn(),
      debug: vi.fn(),
    },
  } as unknown as Agent;
}

describe('AgentRecords.subscribe', () => {
  it('notifies subscribers for every logged record', () => {
    const agent = makeAgent();
    const persistence = new InMemoryAgentRecordPersistence();
    const records = new AgentRecords(agent, persistence);
    const received: AgentRecord[] = [];

    const unsubscribe = records.subscribe((record) => {
      received.push(record);
    });

    records.logRecord({ type: 'turn.prompt', input: [], origin: { kind: 'user' } });
    records.logRecord({ type: 'context.append_message', message: { role: 'user', content: [], toolCalls: [] } });

    expect(received).toHaveLength(2);
    expect(received[0]!.type).toBe('turn.prompt');
    expect(received[1]!.type).toBe('context.append_message');

    unsubscribe();
  });

  it('does not notify subscribers while restoring', () => {
    const agent = makeAgent();
    const persistence = new InMemoryAgentRecordPersistence();
    const records = new AgentRecords(agent, persistence);
    const received: AgentRecord[] = [];

    records.subscribe((record) => received.push(record));
    // Goal records are no-ops during restore, so the mock agent needs no turn/context stubs.
    records.restore({
      type: 'goal.create',
      goalId: 'g1',
      objective: 'test',
      status: 'active',
      actor: 'user',
      budgetLimits: {},
    });

    expect(received).toHaveLength(0);
  });

  it('unsubscribe stops further notifications', () => {
    const agent = makeAgent();
    const persistence = new InMemoryAgentRecordPersistence();
    const records = new AgentRecords(agent, persistence);
    const received: AgentRecord[] = [];

    const unsubscribe = records.subscribe((record) => received.push(record));
    records.logRecord({ type: 'turn.prompt', input: [], origin: { kind: 'user' } });
    unsubscribe();
    records.logRecord({ type: 'turn.prompt', input: [], origin: { kind: 'user' } });

    expect(received).toHaveLength(1);
  });

  it('isolates subscriber errors and keeps notifying other subscribers', () => {
    const agent = makeAgent();
    const persistence = new InMemoryAgentRecordPersistence();
    const records = new AgentRecords(agent, persistence);
    const received: AgentRecord[] = [];

    records.subscribe(() => {
      throw new Error('subscriber boom');
    });
    records.subscribe((record) => received.push(record));

    records.logRecord({ type: 'turn.prompt', input: [], origin: { kind: 'user' } });

    expect(received).toHaveLength(1);
    expect(agent.log.error).toHaveBeenCalledWith(
      'AgentRecords subscriber threw',
      expect.any(Error),
    );
  });

  it('passes the stamped record (with time) to subscribers', () => {
    const agent = makeAgent();
    const persistence = new InMemoryAgentRecordPersistence();
    const records = new AgentRecords(agent, persistence);
    let stamped: AgentRecord | undefined;

    records.subscribe((record) => {
      stamped = record;
    });

    const before = Date.now();
    records.logRecord({ type: 'turn.prompt', input: [], origin: { kind: 'user' } });

    expect(stamped).toBeDefined();
    expect(stamped!.type).toBe('turn.prompt');
    expect(stamped!.time).toBeGreaterThanOrEqual(before);
  });
});
