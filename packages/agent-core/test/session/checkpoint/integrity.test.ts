import { describe, expect, it } from 'vitest';

import {
  verifyCheckpointIntegrity,
  type CheckpointIntegrityResult,
} from '../../../src/session/checkpoint/integrity';
import type { SessionCheckpointPayload } from '../../../src/session/checkpoint/checkpoint';

function makePayload(
  overrides: Partial<SessionCheckpointPayload> = {},
): SessionCheckpointPayload {
  return {
    sessionID: 's1',
    createdAt: '2026-06-12T10:00:00.000Z',
    lastUpdatedAt: '2026-06-12T10:00:00.000Z',
    currentMode: 'design',
    messages: [{ role: 'user', content: [{ type: 'text', text: 'hi' }] }],
    designModeContext: { sessions: [] },
    toolCallIndex: { callIdToResult: {} },
    ...overrides,
  };
}

describe('verifyCheckpointIntegrity', () => {
  it('passes for a valid payload with no optional expectations', () => {
    const result = verifyCheckpointIntegrity(makePayload());
    assertValid(result);
  });

  it('fails jsonValid for a non-object payload', () => {
    const result = verifyCheckpointIntegrity(null);
    expect(result.valid).toBe(false);
    expect(result.checks.jsonValid).toBe(false);
    expect(result.errors).toContain('Checkpoint payload is not an object');
  });

  it('fails jsonValid when required fields are missing', () => {
    const result = verifyCheckpointIntegrity({});
    expect(result.checks.jsonValid).toBe(false);
    expect(result.errors).toContain('Missing or invalid sessionID');
    expect(result.errors).toContain('Missing or invalid messages array');
  });

  it('fails jsonValid for an invalid currentMode', () => {
    const result = verifyCheckpointIntegrity(makePayload({ currentMode: 'unknown' as never }));
    expect(result.checks.jsonValid).toBe(false);
    expect(result.errors).toContain('Invalid currentMode: unknown');
  });

  it('checks message count when expectedMessageCount is provided', () => {
    const result = verifyCheckpointIntegrity(makePayload(), { expectedMessageCount: 1 });
    expect(result.checks.messageCountMatch).toBe(true);
    assertValid(result);
  });

  it('fails messageCountMatch when counts differ', () => {
    const result = verifyCheckpointIntegrity(makePayload(), { expectedMessageCount: 5 });
    expect(result.checks.messageCountMatch).toBe(false);
    expect(result.errors).toContain('Message count mismatch: expected 5, got 1');
    expect(result.valid).toBe(false);
  });

  it('checks sessionID when expectedSessionID is provided', () => {
    const result = verifyCheckpointIntegrity(makePayload({ sessionID: 's1' }), {
      expectedSessionID: 's2',
    });
    expect(result.errors).toContain('Session ID mismatch: expected s2, got s1');
    expect(result.valid).toBe(false);
  });

  it('passes designMode consistency for empty sessions', () => {
    const result = verifyCheckpointIntegrity(makePayload());
    expect(result.checks.designModeConsistent).toBe(true);
  });

  it('passes designMode consistency for a valid session range', () => {
    const result = verifyCheckpointIntegrity(
      makePayload({
        messages: [
          { role: 'user', content: [{ type: 'text', text: 'a' }] },
          { role: 'assistant', content: [{ type: 'text', text: 'b' }] },
          { role: 'user', content: [{ type: 'text', text: 'c' }] },
        ],
        designModeContext: {
          sessions: [{ designSessionID: 'd1', startedAtMsg: 0, exitedAtMsg: 2 }],
        },
      }),
    );
    expect(result.checks.designModeConsistent).toBe(true);
  });

  it('fails designMode consistency when startedAtMsg is out of range', () => {
    const result = verifyCheckpointIntegrity(
      makePayload({
        messages: [{ role: 'user', content: [{ type: 'text', text: 'a' }] }],
        designModeContext: {
          sessions: [{ designSessionID: 'd1', startedAtMsg: 5 }],
        },
      }),
    );
    expect(result.checks.designModeConsistent).toBe(false);
    expect(result.errors.some((e) => e.includes('startedAtMsg'))).toBe(true);
  });

  it('fails designMode consistency when exitedAtMsg is before startedAtMsg', () => {
    const result = verifyCheckpointIntegrity(
      makePayload({
        messages: [
          { role: 'user', content: [{ type: 'text', text: 'a' }] },
          { role: 'assistant', content: [{ type: 'text', text: 'b' }] },
        ],
        designModeContext: {
          sessions: [{ designSessionID: 'd1', startedAtMsg: 1, exitedAtMsg: 0 }],
        },
      }),
    );
    expect(result.checks.designModeConsistent).toBe(false);
    expect(result.errors.some((e) => e.includes('exitedAtMsg'))).toBe(true);
  });

  it('passes toolCallIndex completeness when no tool exchanges exist', () => {
    const result = verifyCheckpointIntegrity(makePayload());
    expect(result.checks.toolCallIndexComplete).toBe(true);
  });

  it('passes when every tool call has a matching result', () => {
    const result = verifyCheckpointIntegrity(
      makePayload({
        messages: [
          {
            role: 'assistant',
            content: [],
            toolCalls: [{ type: 'function', id: 'call-1', name: 'x', arguments: null }],
          },
          { role: 'tool', content: [{ type: 'text', text: 'ok' }], toolCallId: 'call-1' },
        ],
      }),
    );
    expect(result.checks.toolCallIndexComplete).toBe(true);
  });

  it('fails when a tool result has no matching call', () => {
    const result = verifyCheckpointIntegrity(
      makePayload({
        messages: [
          { role: 'tool', content: [{ type: 'text', text: 'ok' }], toolCallId: 'call-missing' },
        ],
      }),
    );
    expect(result.checks.toolCallIndexComplete).toBe(false);
    expect(result.errors).toContain(
      'Tool result call-missing has no matching assistant tool call',
    );
  });

  it('fails when a tool call has no matching result', () => {
    const result = verifyCheckpointIntegrity(
      makePayload({
        messages: [
          {
            role: 'assistant',
            content: [],
            toolCalls: [{ type: 'function', id: 'call-1', name: 'x', arguments: null }],
          },
        ],
      }),
    );
    expect(result.checks.toolCallIndexComplete).toBe(false);
    expect(result.errors).toContain('Tool call call-1 has no matching tool result message');
  });
});

function assertValid(result: CheckpointIntegrityResult): void {
  expect(result.valid).toBe(true);
  expect(result.errors).toHaveLength(0);
}
