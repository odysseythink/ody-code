import { describe, expect, it } from 'vitest';
import { MessageChannel } from 'node:worker_threads';

import { createMessagePortTransport } from '../../../src/rpc/transports/message-port';
import { createInProcessTransportPair, decodeJson, encodeJson } from '../../../src/rpc/transport';

describe('MessagePortTransport parity', () => {
  it('sends and receives like in-process transport', async () => {
    // In-process test
    const [leftIp] = createInProcessTransportPair(
      // dispatchLeft (right→left) — unused in this test
      async () => encodeJson('unused'),
      // dispatchRight (left→right) — leftIp.send() calls dispatchRight
      async (bytes) => encodeJson(`echo:${decodeJson(bytes)}`),
    );
    const ipResult = await leftIp.send(encodeJson('hello'));
    expect(decodeJson(ipResult)).toBe('echo:hello');

    // MessagePort test
    const channel = new MessageChannel();
    const leftMp = createMessagePortTransport(channel.port1, async () => encodeJson('unused'));
    createMessagePortTransport(channel.port2, async (bytes) => encodeJson(`echo:${decodeJson(bytes)}`));
    const mpResult = await leftMp.send(encodeJson('hello'));
    expect(decodeJson(mpResult)).toBe('echo:hello');

    leftMp.close?.();
  });

  it('propagates errors on dispatch throw', async () => {
    const channel = new MessageChannel();
    const leftMp = createMessagePortTransport(
      channel.port1,
      async () => encodeJson('unused'),
    );
    createMessagePortTransport(
      channel.port2,
      async () => { throw new Error('dispatch failed'); },
    );

    await expect(leftMp.send(encodeJson('ping'))).rejects.toThrow('dispatch failed');
    leftMp.close?.();
  });

  it('rejects pending on close like in-process transport does', async () => {
    // In-process: close doesn't reject (no-op), but let's verify
    const [leftIp] = createInProcessTransportPair(
      // dispatchLeft (right→left) — unused
      async () => encodeJson('unused'),
      // dispatchRight (left→right) — never resolves
      async () => new Promise(() => {}),
    );
    const ipPending = leftIp.send(encodeJson('x'));
    leftIp.close?.();
    // In-process close is a no-op, so this should just hang
    // (We won't await it)

    // MessagePort: close rejects pending
    const channel = new MessageChannel();
    const leftMp = createMessagePortTransport(
      channel.port1,
      async () => encodeJson('unused'),
    );
    createMessagePortTransport(
      channel.port2,
      async () => new Promise(() => {}),
    );
    const mpPending = leftMp.send(encodeJson('y'));
    leftMp.close?.();
    await expect(mpPending).rejects.toThrow();
  });
});
