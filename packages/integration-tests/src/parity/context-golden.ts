import {
  ContextMemory,
  dropOrphanToolResults,
  estimateTokensForMessages,
  project,
} from '@odysseythink/agent-core/agent/context';
import { renderNotificationXml } from '@odysseythink/agent-core/agent/context/notification-xml';
import type {
  Agent,
  AgentRecord,
  ContextMessage,
} from '@odysseythink/agent-core';
import type { Message } from '@odysseythink/kosong';

const FIXED_TIME = 12345;

export type Fixture =
  | { kind: 'project'; history: ContextMessage[] }
  | { kind: 'tokens'; messages: Message[] }
  | { kind: 'notification'; data: Record<string, unknown> }
  | { kind: 'memory'; operations: AgentRecord[] };

export function runTsContextGolden(fixture: Fixture): unknown {
  switch (fixture.kind) {
    case 'project':
      return { messages: dropOrphanToolResults(project(fixture.history)) };
    case 'tokens':
      return { tokens: estimateTokensForMessages(fixture.messages) };
    case 'notification':
      return { xml: renderNotificationXml(fixture.data) };
    case 'memory':
      return runMemory(fixture.operations);
    default:
      throw new Error(`unknown fixture kind: ${(fixture as { kind: string }).kind}`);
  }
}

function runMemory(operations: AgentRecord[]): unknown {
  const { agent, records } = makeStubAgent();
  const context = new ContextMemory(agent);
  const originalNow = Date.now;
  globalThis.Date.now = () => FIXED_TIME;
  try {
    for (const op of operations) {
      replayTs(context, op);
    }
  } finally {
    globalThis.Date.now = originalNow;
  }
  return {
    history: context.data().history,
    messages: context.messages,
    token_count: context.tokenCount,
    token_count_with_pending: context.tokenCountWithPending,
    records,
  };
}

function replayTs(context: ContextMemory, op: AgentRecord): void {
  switch (op.type) {
    case 'context.append_message':
      context.appendMessage(op.message);
      return;
    case 'context.append_loop_event':
      context.appendLoopEvent(op.event);
      return;
    case 'context.clear':
      context.clear();
      return;
    case 'context.apply_compaction':
      context.applyCompaction(op);
      return;
    case 'context.undo':
      context.undo(op.count);
      return;
    default:
      return;
  }
}

function makeStubAgent(): { agent: Agent; records: AgentRecord[] } {
  const records: AgentRecord[] = [];
  const agent = {
    records: {
      logRecord: (r: AgentRecord) => {
        records.push(r);
      },
      get restoring() {
        return null;
      },
    },
    microCompaction: {
      compact: (messages: ContextMessage[]) => messages,
      reset: () => {},
    },
    injection: {
      onContextClear: () => {},
      onContextCompacted: () => {},
      onContextMessageRemoved: () => {},
    },
    background: {
      markDeliveredNotification: () => {},
    },
    replayBuilder: {
      push: () => {},
      removeLastMessages: () => {},
    },
    emitStatusUpdated: () => {},
    flushDeferredContextSwitch: () => {},
  } as unknown as Agent;
  return { agent, records };
}
