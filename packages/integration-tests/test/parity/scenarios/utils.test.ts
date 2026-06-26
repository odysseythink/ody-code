import { describe, expect, it } from 'vitest';
import type { AgentEvent } from '@odysseythink/agent-core';
import type { StreamedMessage } from '@odysseythink/kosong';
import { MockChatProvider } from '../../../src/parity/fixtures/mock-provider';
import { waitForEvent, waitForTurnEnded } from '../../../src/parity/scenarios/utils';

function fakeClient(eventsToEmit: AgentEvent[] = []) {
  const listeners = new Set<(event: AgentEvent) => void>();
  return {
    onEvent(listener: (event: AgentEvent) => void) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    emit(event: AgentEvent) {
      listeners.forEach((l) => l(event));
    },
  } as any;
}

describe('MockChatProvider multi-turn', () => {
  it('cycles through multiple responses', async () => {
    const provider = new MockChatProvider([
      [{ type: 'text', text: 'first' }],
      [{ type: 'text', text: 'second' }],
    ]);
    const msg1 = await provider.generate('', [], []);
    const chunks1 = await collectChunks(msg1);
    expect(chunks1).toEqual([{ type: 'text', text: 'first' }]);

    const msg2 = await provider.generate('', [], []);
    const chunks2 = await collectChunks(msg2);
    expect(chunks2).toEqual([{ type: 'text', text: 'second' }]);
  });

  it('still supports single-response constructor', async () => {
    const provider = new MockChatProvider([{ type: 'text', text: 'hello' }]);
    const msg = await provider.generate('', [], []);
    const chunks = await collectChunks(msg);
    expect(chunks).toEqual([{ type: 'text', text: 'hello' }]);
  });

  async function collectChunks(msg: StreamedMessage) {
    const out: unknown[] = [];
    for await (const chunk of msg) out.push(chunk);
    return out;
  }
});

describe('waitForEvent', () => {
  it('resolves when predicate matches', async () => {
    const client = fakeClient();
    const promise = waitForEvent(client, (e) => e.type === 'turn.ended');
    client.emit({ type: 'turn.ended', turnId: 1, reason: 'completed' } as any);
    const event = await promise;
    expect(event.type).toBe('turn.ended');
  });

  it('rejects on timeout', async () => {
    const client = fakeClient();
    await expect(waitForEvent(client, () => false, { timeoutMs: 10 })).rejects.toThrow('Timeout');
  });
});

describe('waitForTurnEnded', () => {
  it('resolves on turn.ended', async () => {
    const client = fakeClient();
    const promise = waitForTurnEnded(client);
    client.emit({ type: 'turn.ended', turnId: 1, reason: 'completed' } as any);
    await expect(promise).resolves.toBeDefined();
  });
});
