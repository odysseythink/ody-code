import type { ContentPart, Message, TextPart } from '@odysseythink/kosong';

import type { ContextMessage } from './types';

export function project(history: readonly ContextMessage[]): Message[] {
  // Keep partial or empty assistant placeholders away from providers.
  // They can appear when a turn is aborted or errors before any content
  // or tool call is appended.
  const usable = history.filter((message) => {
    return (
      message.partial !== true &&
      !(message.role === 'assistant' && message.content.length === 0 && message.toolCalls.length === 0)
    );
  });
  return mergeAdjacentUserMessages(usable);
}

/**
 * Drop tool-result messages whose matching tool call is absent from the
 * preceding history. The provider rejects any such orphan with
 * "400 tool_call_id is not found", which would otherwise wedge the whole
 * session on every request.
 *
 * In a correctly-partitioned conversation every tool result is preceded by its
 * assistant tool call in the same partition, so this is a no-op. It only fires
 * for histories corrupted by a partition-routing bug (e.g. a pre-fix design→plan
 * handoff that persisted the ExitDesignMode result into the plan partition while
 * its call stayed in design). Healing happens at projection of the FULL history
 * (the send boundary) so legacy sessions resume cleanly and any future routing
 * regression is contained.
 *
 * IMPORTANT: only call this on a COMPLETE history, never on a sub-slice — a
 * windowed view (e.g. full-compaction's slice or token accounting of pending
 * messages) can legitimately hold a result whose call sits outside the window,
 * and dropping it there would corrupt valid exchanges. `project()` has already
 * run by the time this is called, so partial/placeholder assistant messages are
 * never seen here.
 *
 * A tool-shaped message with NO toolCallId is left untouched: it is not a
 * provider tool result keyed by id, and existing flows rely on it surviving.
 *
 * SCOPE: this heals the orphaned-RESULT side of the corruption. The symmetric
 * case — a dangling assistant tool CALL with no following result (which the same
 * pre-fix handoff bug leaves stranded in the design partition) — is NOT handled
 * here: dropping a call is more invasive (it can strip an assistant message's
 * only tool content) and only manifests if a legacy session is resumed back into
 * that dormant partition. New sessions are immune (the deferred partition switch
 * keeps call+result together), so that side is left as a known, lower-severity
 * gap rather than risk a broad rewrite.
 */
export function dropOrphanToolResults(messages: Message[]): Message[] {
  const seenCallIds = new Set<string>();
  const out: Message[] = [];
  for (const message of messages) {
    if (message.role === 'assistant') {
      for (const call of message.toolCalls) seenCallIds.add(call.id);
      out.push(message);
      continue;
    }
    // Only a result that carries a toolCallId with no matching earlier call is an
    // orphan. A result is only valid after its call, so "seen so far" is correct.
    if (
      message.role === 'tool' &&
      message.toolCallId !== undefined &&
      !seenCallIds.has(message.toolCallId)
    ) {
      continue;
    }
    out.push(message);
  }
  return out;
}

function mergeAdjacentUserMessages(history: readonly ContextMessage[]): Message[] {
  const out: ContextMessage[] = [];
  for (const message of history) {
    const previous = out.at(-1);
    if (
      canMergeUserMessage(message) &&
      previous !== undefined &&
      canMergeUserMessage(previous)
    ) {
      out[out.length - 1] = mergeTwoUserMessages(previous, message);
      continue;
    }
    out.push(message);
  }
  return out.map(stripContextMetadata);
}

function canMergeUserMessage(message: ContextMessage): boolean {
  return message.role === 'user' && message.origin?.kind === 'user';
}

function mergeTwoUserMessages(a: ContextMessage, b: ContextMessage): ContextMessage {
  const aText = extractTextOnly(a);
  const bText = extractTextOnly(b);
  const nonTextParts = [
    ...a.content.filter((p) => p.type !== 'text'),
    ...b.content.filter((p) => p.type !== 'text'),
  ];
  const mergedText: TextPart = { type: 'text', text: `${aText}\n\n${bText}` };
  const content: ContentPart[] = [mergedText, ...nonTextParts];
  return {
    role: 'user',
    content,
    toolCalls: [],
    origin: a.origin,
  };
}

function extractTextOnly(message: Message): string {
  return message.content
    .filter((p): p is TextPart => p.type === 'text')
    .map((p) => p.text)
    .join('');
}

function stripContextMetadata(message: ContextMessage): Message {
  return {
    role: message.role,
    name: message.name,
    content: message.content.map((p) => ({ ...p })) as ContentPart[],
    toolCalls: message.toolCalls.map((tc) => ({ ...tc })),
    toolCallId: message.toolCallId,
    partial: message.partial,
  };
}
