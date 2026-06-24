import { describe, expect, it } from 'vitest';

import { ErrorCodes } from '@odysseythink/agent-core-shared';
import { createWebSocketTransport } from '../../../src/rpc/transports/websocket';
import { decodeJson, encodeJson } from '../../../src/rpc/transport';

interface FakeWebSocket {
  send(data: string): void;
  close(): void;
  onmessage: ((event: { data: string | Uint8Array }) => void) | null;
  onerror: ((event: { type: string }) => void) | null;
  onclose: ((event: { type: string }) => void) | null;
}

function createFakeSocketPair(): [FakeWebSocket, FakeWebSocket] {
  const a: FakeWebSocket = { send() {}, close() {}, onmessage: null, onerror: null, onclose: null };
  const b: FakeWebSocket = { send() {}, close() {}, onmessage: null, onerror: null, onclose: null };
  a.send = (data: string) => {
    const parsed = JSON.parse(data);
    queueMicrotask(() => b.onmessage?.({ data: JSON.stringify(parsed) }));
  };
  b.send = (data: string) => {
    const parsed = JSON.parse(data);
    queueMicrotask(() => a.onmessage?.({ data: JSON.stringify(parsed) }));
  };
  a.close = () => {
    queueMicrotask(() => b.onclose?.({ type: 'close' }));
  };
  b.close = () => {
    queueMicrotask(() => a.onclose?.({ type: 'close' }));
  };
  return [a, b];
}

describe('websocket transport', () => {
  it('round-trips request/response over text frames', async () => {
    const [sockA, sockB] = createFakeSocketPair();

    const left = createWebSocketTransport(sockA, async () => encodeJson('unused'));
    createWebSocketTransport(sockB, async (bytes) => {
      expect(decodeJson(bytes)).toBe('ping');
      return encodeJson('pong');
    });

    const response = await left.send(encodeJson('ping'));
    expect(decodeJson(response)).toBe('pong');
  });

  it('rejects pending requests when socket closes', async () => {
    const [sockA, sockB] = createFakeSocketPair();

    const left = createWebSocketTransport(sockA, async () => new Promise(() => {}));
    const right = createWebSocketTransport(sockB, async () => new Promise(() => {}));

    const pending = left.send(encodeJson('hang'));
    right.close!();

    await expect(pending).rejects.toMatchObject({ code: ErrorCodes.TRANSPORT_CLOSED });
  });
});
