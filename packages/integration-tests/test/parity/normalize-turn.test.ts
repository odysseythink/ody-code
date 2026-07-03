import { describe, expect, it } from 'vitest';

import { normalizeTurnSnapshot } from '../../src/parity/normalize-turn';

describe('normalizeTurnSnapshot', () => {
  it('replaces uuids, step ids, and timestamps', () => {
    const input = {
      name: 'end-turn',
      turns: [{ turnId: 0, reason: 'completed' }],
      events: [
        {
          type: 'turn.step.started',
          turnId: 0,
          step: 1,
          stepId: '123e4567-e89b-12d3-a456-426614174000',
        },
        {
          type: 'turn.step.completed',
          turnId: 0,
          step: 1,
          stepId: '123e4567-e89b-12d3-a456-426614174001',
          usage: { inputOther: 5, output: 3, inputCacheRead: 0, inputCacheCreation: 0 },
          llmFirstTokenLatencyMs: 120,
          llmStreamDurationMs: 340,
        },
      ],
      records: [
        {
          type: 'turn.prompt',
          time: 1700000000000,
          input: [{ type: 'text', text: 'Hello' }],
          origin: { kind: 'user' },
        },
      ],
      contextInputs: [{ text: 'Hello', originKind: 'user' }],
      telemetry: [{ event: 'turn_started', properties: { mode: 'agent' } }],
    };

    const normalized = normalizeTurnSnapshot(input as never) as Record<string, unknown>;
    const events = normalized['events'] as Array<Record<string, unknown>>;
    const records = normalized['records'] as Array<Record<string, unknown>>;

    expect(events[0]!['stepId']).toBe('<id>');
    expect(events[1]!['stepId']).toBe('<id>');
    expect(events[1]!['llmFirstTokenLatencyMs']).toBe(0);
    expect(events[1]!['llmStreamDurationMs']).toBe(0);
    expect(records[0]!['time']).toBe('<time>');
  });

  it('does not replace deterministic turn ids or step numbers', () => {
    const input = {
      name: 'x',
      turns: [{ turnId: 2, reason: 'completed' }],
      events: [{ type: 'turn.started', turnId: 2 }],
      records: [],
      contextInputs: [],
      telemetry: [],
    };
    const normalized = normalizeTurnSnapshot(input as never) as Record<string, unknown>;
    const turns = normalized['turns'] as Array<Record<string, unknown>>;
    const events = normalized['events'] as Array<Record<string, unknown>>;
    expect(turns[0]!['turnId']).toBe(2);
    expect(events[0]!['turnId']).toBe(2);
  });
});
