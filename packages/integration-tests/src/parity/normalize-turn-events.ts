import type { AgentEvent } from '@odysseythink/agent-core';

/**
 * Normalize turn/tool events so Rust and TS event streams can be compared.
 *
 * Rust ody-host emits:
 *   "tool.call" (with `toolCallId`, `toolName`, `args`)
 *   "tool.result" (with `toolCallId`, `result`, `isError`)
 *
 * TS agent-core emits:
 *   "tool.call.started" (with `toolCallId`, `name`, `args`)
 *   "tool.result" (with `toolCallId`, `output`, `isError`)
 *
 * This normalizer maps Rust shapes to TS shapes.
 */
export function normalizeTurnEvents(events: AgentEvent[]): AgentEvent[] {
  return events.map((event) => normalizeEvent(event));
}

function normalizeEvent(event: AgentEvent): AgentEvent {
  const e = event as unknown as Record<string, unknown>;
  const type = e['type'];

  // Rust emits "tool.call" → map to TS "tool.call.started"
  if (type === 'tool.call') {
    return {
      type: 'tool.call.started',
      turnId: e['turnId'],
      toolCallId: e['toolCallId'] ?? '<id>',
      name: e['toolName'] ?? e['name'],
      args: e['args'],
    } as unknown as AgentEvent;
  }

  // Normalize tool.result field names: Rust uses "result", TS uses "output"
  if (type === 'tool.result') {
    // Keep only the shared fields; drop Rust-only "tool_name" field
    const result = e['result'] ?? e['output'];
    const isError = e['isError'] ?? false;
    return {
      type: 'tool.result',
      turnId: e['turnId'],
      toolCallId: e['toolCallId'] ?? '<id>',
      output: result,
      isError,
    } as unknown as AgentEvent;
  }

  // turn.started / turn.ended — keep core fields only
  if (type === 'turn.started') {
    return {
      type: 'turn.started',
      turnId: e['turnId'],
    } as unknown as AgentEvent;
  }

  if (type === 'turn.ended') {
    return {
      type: 'turn.ended',
      turnId: e['turnId'],
      reason: e['reason'],
    } as unknown as AgentEvent;
  }

  // assistant.delta — normalize field names
  if (type === 'assistant.delta') {
    return {
      type: 'assistant.delta',
      turnId: e['turnId'],
      delta: e['delta'],
    } as unknown as AgentEvent;
  }

  return event;
}
