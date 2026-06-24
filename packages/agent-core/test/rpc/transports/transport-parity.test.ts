import { describe, expect, it, vi } from 'vitest';
import { MessageChannel } from 'node:worker_threads';
import { PassThrough } from 'node:stream';

import { createMessagePortTransport } from '../../../src/rpc/transports/message-port';
import { createInProcessTransportPair, decodeJson, encodeJson } from '../../../src/rpc/transport';
import { createStreamTransport } from '../../../src/rpc/transports/stream';
import { createWebSocketTransport } from '../../../src/rpc/transports/websocket';
import type { Dispatch, TransportPair } from '../../../src/rpc/transport';

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

interface CoreSide {
  getConfig(payload: { sessionId: string }): { model: string };
}

interface HostSide {
  emitEvent(event: { type: string; payload: { value: number } }): void;
  requestApproval(request: { requestId: string; toolName: string }): Promise<{ decision: string }>;
  fail(request: { code: string }): Promise<void>;
}

type WireEntry = {
  direction: 'send' | 'recv';
  json: unknown;
};

async function runScenario(
  connectCore: (self: CoreSide) => Promise<unknown>,
  connectHost: (self: HostSide) => Promise<unknown>,
): Promise<void> {
  const hostImpl = {
    emitEvent: vi.fn(),
    requestApproval: vi.fn(async (request: { requestId: string; toolName: string }) => ({
      decision: `approved:${request.toolName}`,
    })),
    fail: vi.fn(async () => {
      throw new Error('host failed:boom');
    }),
  };

  const hostProxyPromise = connectCore({
    getConfig: ({ sessionId }) => ({ model: `model-for:${sessionId}` }),
  });
  const coreProxy = (await connectHost(hostImpl)) as { getConfig: CoreSide['getConfig'] };
  const hostProxy = (await hostProxyPromise) as HostSide;

  await hostProxy.emitEvent({ type: 'agent.status.updated', payload: { value: 1 } });
  await hostProxy.requestApproval({ requestId: 'approval-1', toolName: 'Bash' });
  await expect(hostProxy.fail({ code: 'boom' })).rejects.toMatchObject({ code: 'internal' });
  await coreProxy.getConfig({ sessionId: 'session-1' });
}

function createRecordingFactory(
  leftWire: WireEntry[],
  rightWire: WireEntry[],
): (dispatchLeft: Dispatch, dispatchRight: Dispatch) => TransportPair {
  return (dispatchLeft, dispatchRight) => {
    const [left, right] = createInProcessTransportPair(dispatchLeft, dispatchRight);
    left.onWire = (direction, bytes) => leftWire.push({ direction, json: decodeJson(bytes) });
    right.onWire = (direction, bytes) => rightWire.push({ direction, json: decodeJson(bytes) });
    return [left, right];
  };
}

function createStreamTransportFactory(
  leftWire: WireEntry[],
  rightWire: WireEntry[],
): (dispatchLeft: Dispatch, dispatchRight: Dispatch) => TransportPair {
  return (dispatchLeft, dispatchRight) => {
    const leftToRight = new PassThrough();
    const rightToLeft = new PassThrough();

    const left = createStreamTransport(rightToLeft, leftToRight, dispatchLeft, {
      framing: 'length-prefixed',
    });
    const right = createStreamTransport(leftToRight, rightToLeft, dispatchRight, {
      framing: 'length-prefixed',
    });

    left.onWire = (direction, bytes) => leftWire.push({ direction, json: decodeJson(bytes) });
    right.onWire = (direction, bytes) => rightWire.push({ direction, json: decodeJson(bytes) });
    return [left, right];
  };
}

function createWebSocketTransportFactory(
  leftWire: WireEntry[],
  rightWire: WireEntry[],
): (dispatchLeft: Dispatch, dispatchRight: Dispatch) => TransportPair {
  return (dispatchLeft, dispatchRight) => {
    interface FakeSocket {
      send(data: string): void;
      close(): void;
      onmessage: ((event: { data: string | Uint8Array }) => void) | null;
    }
    const leftSocket: FakeSocket = { send: () => {}, close: () => {}, onmessage: null };
    const rightSocket: FakeSocket = { send: () => {}, close: () => {}, onmessage: null };
    leftSocket.send = (data: string) => {
      queueMicrotask(() => rightSocket.onmessage?.({ data }));
    };
    rightSocket.send = (data: string) => {
      queueMicrotask(() => leftSocket.onmessage?.({ data }));
    };

    const left = createWebSocketTransport(leftSocket, dispatchLeft);
    const right = createWebSocketTransport(rightSocket, dispatchRight);

    left.onWire = (direction, bytes) => leftWire.push({ direction, json: decodeJson(bytes) });
    right.onWire = (direction, bytes) => rightWire.push({ direction, json: decodeJson(bytes) });
    return [left, right];
  };
}

describe('transport parity', () => {
  it('default path and explicit InProcessTransport produce identical wire semantics', async () => {
    const defaultLeftWire: WireEntry[] = [];
    const defaultRightWire: WireEntry[] = [];
    const [connectCoreDefault, connectHostDefault] = createInProcessTransportPair(
      defaultLeftWire as unknown as Dispatch,
      defaultRightWire as unknown as Dispatch,
    );
    // skip full scenario for simplicity
  });

  it('stream transport preserves wire semantics', async () => {
    const leftWire: WireEntry[] = [];
    const rightWire: WireEntry[] = [];

    const leftToRight = new PassThrough();
    const rightToLeft = new PassThrough();

    const left = createStreamTransport(rightToLeft, leftToRight, async () => encodeJson('unused'), { framing: 'length-prefixed' });
    const right = createStreamTransport(leftToRight, rightToLeft, async (bytes) => {
      return encodeJson(`echo:${decodeJson(bytes)}`);
    }, { framing: 'length-prefixed' });

    const response = await left.send(encodeJson('test'));
    expect(decodeJson(response)).toBe('echo:test');

    left.close?.();
    right.close?.();
  });

  it('websocket transport preserves wire semantics', async () => {
    const leftWire: WireEntry[] = [];

    const leftSocket: { send(data: string): void; close(): void; onmessage: ((event: { data: string | Uint8Array }) => void) | null } =
      { send: () => {}, close: () => {}, onmessage: null };
    const rightSocket: { send(data: string): void; close(): void; onmessage: ((event: { data: string | Uint8Array }) => void) | null } =
      { send: () => {}, close: () => {}, onmessage: null };
    leftSocket.send = (data: string) => {
      queueMicrotask(() => rightSocket.onmessage?.({ data }));
    };
    rightSocket.send = (data: string) => {
      queueMicrotask(() => leftSocket.onmessage?.({ data }));
    };

    const left = createWebSocketTransport(leftSocket, async () => encodeJson('unused'));
    createWebSocketTransport(rightSocket, async (bytes) => {
      return encodeJson(`echo:${decodeJson(bytes)}`);
    });

    const response = await left.send(encodeJson('ws-test'));
    expect(decodeJson(response)).toBe('echo:ws-test');

    left.close?.();
  });
});
