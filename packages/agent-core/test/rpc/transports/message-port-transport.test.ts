import { describe, expect, it, vi } from 'vitest';
import { MessageChannel } from 'node:worker_threads';

import { createMessagePortTransport } from '../../../src/rpc/transports/message-port';
import { decodeJson, encodeJson } from '../../../src/rpc/transport';

describe('message-port transport', () => {
  it('round-trips request/response bytes', async () => {
    const channel = new MessageChannel();
    const rightHandler = vi.fn(async (bytes: Uint8Array) => {
      expect(decodeJson(bytes)).toBe('ping');
      return encodeJson('pong');
    });

    const left = createMessagePortTransport(channel.port1, async () => encodeJson('unused'));
    createMessagePortTransport(channel.port2, async (bytes) => rightHandler(bytes));

    const response = await left.send(encodeJson('ping'));
    expect(decodeJson(response)).toBe('pong');
    expect(rightHandler).toHaveBeenCalledTimes(1);
  });

  it('correlates concurrent requests by reqId', async () => {
    const channel = new MessageChannel();
    const left = createMessagePortTransport(channel.port1, async () => encodeJson('unused'));
    createMessagePortTransport(channel.port2, async (bytes) => {
      const delay = decodeJson(bytes) as number;
      await new Promise((resolve) => setTimeout(resolve, delay));
      return encodeJson(`pong:${delay}`);
    });

    const [a, b] = await Promise.all([left.send(encodeJson(30)), left.send(encodeJson(10))]);
    expect(decodeJson(a)).toBe('pong:30');
    expect(decodeJson(b)).toBe('pong:10');
  });

  it('rejects pending requests with TRANSPORT_CLOSED when close() is called', async () => {
    const channel = new MessageChannel();
    const left = createMessagePortTransport(
      channel.port1,
      async () => encodeJson('unused'),
    );
    createMessagePortTransport(
      channel.port2,
      async () => new Promise(() => {}),
    );

    const pending = left.send(encodeJson('hang'));
    left.close();

    await expect(pending).rejects.toMatchObject({ code: 'transport.closed' });
  });

  it('calls onWire for each send and recv', async () => {
    const channel = new MessageChannel();
    const wire: { direction: 'send' | 'recv'; json: unknown }[] = [];
    const left = createMessagePortTransport(
      channel.port1,
      async () => encodeJson('unused'),
      {
        onWire: (direction, bytes) => wire.push({ direction, json: decodeJson(bytes) }),
      },
    );
    createMessagePortTransport(channel.port2, async () => encodeJson('pong'));

    await left.send(encodeJson('ping'));

    expect(wire).toHaveLength(2);
    expect(wire[0]).toEqual({ direction: 'send', json: 'ping' });
    expect(wire[1]).toEqual({ direction: 'recv', json: 'pong' });
  });
});
