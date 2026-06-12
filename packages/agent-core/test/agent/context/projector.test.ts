import { describe, expect, it } from 'vitest';

import { dropOrphanToolResults, project } from '../../../src/agent/context/projector';
import type { ContextMessage } from '../../../src/agent/context/types';

/** Mirror how `context.messages` applies the guard: drop orphans on the full
 *  projected history. */
function projectAndHeal(history: ContextMessage[]) {
  return dropOrphanToolResults(project(history));
}

function assistantWithCall(callId: string, text = ''): ContextMessage {
  return {
    role: 'assistant',
    content: text.length > 0 ? [{ type: 'text', text }] : [],
    toolCalls: [{ type: 'function', id: callId, name: 'ExitDesignMode', arguments: '{}' }],
  };
}

function toolResult(callId: string, output: string): ContextMessage {
  return {
    role: 'tool',
    content: [{ type: 'text', text: output }],
    toolCalls: [],
    toolCallId: callId,
  };
}

function userMessage(text: string): ContextMessage {
  return { role: 'user', content: [{ type: 'text', text }], toolCalls: [] };
}

describe('project — orphaned tool-result guard', () => {
  // Regression: a session corrupted by the pre-fix design→plan partition bug
  // persists a tool RESULT whose matching tool CALL lives in a different
  // partition. When that partition's history is replayed and sent, the result
  // is an orphan: its tool_call_id appears in no prior assistant message, and
  // the provider rejects the request with "400 tool_call_id is not found".
  // project() — the last boundary before the provider — must drop such orphans
  // so the (otherwise valid) session can still be used / resumed.
  it('drops a tool result whose call is absent from the history', () => {
    const history: ContextMessage[] = [
      toolResult('call_orphan', 'Exited design mode.'),
      userMessage('continue'),
    ];
    const projected = projectAndHeal(history);
    expect(projected.some((m) => m.role === 'tool')).toBe(false);
    expect(projected.map((m) => m.role)).toEqual(['user']);
  });

  it('keeps a tool result that has a matching preceding call', () => {
    const history: ContextMessage[] = [
      assistantWithCall('call_ok'),
      toolResult('call_ok', 'ok'),
      userMessage('next'),
    ];
    const projected = projectAndHeal(history);
    expect(projected.map((m) => m.role)).toEqual(['assistant', 'tool', 'user']);
    expect(projected[1]?.toolCallId).toBe('call_ok');
  });

  it('drops only the orphan, keeping a valid exchange in the same history', () => {
    const history: ContextMessage[] = [
      toolResult('call_orphan', 'orphaned result at head'),
      assistantWithCall('call_ok', 'calling tool'),
      toolResult('call_ok', 'ok'),
    ];
    const projected = projectAndHeal(history);
    expect(projected.map((m) => m.role)).toEqual(['assistant', 'tool']);
    expect(projected[1]?.toolCallId).toBe('call_ok');
  });

  it('drops a tool result that appears BEFORE its (later) call (out of order)', () => {
    // A result can only legitimately follow its call. One appearing before any
    // matching call is malformed and must be dropped.
    const history: ContextMessage[] = [
      toolResult('call_late', 'too early'),
      assistantWithCall('call_late'),
    ];
    const projected = projectAndHeal(history);
    expect(projected.map((m) => m.role)).toEqual(['assistant']);
  });

  it('preserves a tool-shaped message that carries NO toolCallId', () => {
    // Not a provider tool result keyed by id; existing flows rely on it surviving.
    const history: ContextMessage[] = [
      { role: 'tool', content: [{ type: 'text', text: 'tool-like output' }], toolCalls: [] },
    ];
    const projected = projectAndHeal(history);
    expect(projected.map((m) => m.role)).toEqual(['tool']);
  });

  it('does not heal a sub-slice via project() alone (guard is messages-only)', () => {
    // project() on a windowed slice must NOT drop a result whose call is outside
    // the window — that is full-compaction / token-accounting behavior.
    const slice: ContextMessage[] = [toolResult('call_outside', 'result only')];
    expect(project(slice).map((m) => m.role)).toEqual(['tool']);
  });

  it('is a strict no-op for a healthy multi-turn, multi-call history', () => {
    // The load-bearing invariant: parallel tool calls in one assistant message
    // and exchanges spread across turns must all survive untouched.
    const asstTwoCalls: ContextMessage = {
      role: 'assistant',
      content: [{ type: 'text', text: 'doing two things' }],
      toolCalls: [
        { type: 'function', id: 'call_a', name: 'A', arguments: '{}' },
        { type: 'function', id: 'call_b', name: 'B', arguments: '{}' },
      ],
    };
    const history: ContextMessage[] = [
      userMessage('start'),
      asstTwoCalls,
      toolResult('call_a', 'a done'),
      toolResult('call_b', 'b done'),
      userMessage('again'),
      assistantWithCall('call_c', 'one more'),
      toolResult('call_c', 'c done'),
    ];
    const projected = projectAndHeal(history);
    expect(projected.map((m) => m.role)).toEqual([
      'user',
      'assistant',
      'tool',
      'tool',
      'user',
      'assistant',
      'tool',
    ]);
    expect(projected.filter((m) => m.role === 'tool').map((m) => m.toolCallId)).toEqual([
      'call_a',
      'call_b',
      'call_c',
    ]);
  });

  it('keeps a result whose call is several turns back', () => {
    // seenCallIds persists across the whole scan, so a call recorded early stays
    // valid for a result that arrives after unrelated intervening messages.
    const history: ContextMessage[] = [
      assistantWithCall('call_early', 'kick off'),
      userMessage('interjection 1'),
      userMessage('interjection 2'),
      toolResult('call_early', 'late result'),
    ];
    const projected = projectAndHeal(history);
    expect(projected.some((m) => m.role === 'tool' && m.toolCallId === 'call_early')).toBe(true);
  });
});
