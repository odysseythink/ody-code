import { describe, expect, it, vi } from 'vitest';

import { ErrorCodes, OdyError } from '../../src/errors';
import { createRPC } from '../../src/rpc';
import {
  createInProcessTransportPair,
  decodeJson,
  encodeJson,
  type Dispatch,
  type Transport,
  type TransportPair,
} from '../../src/rpc/transport';

interface CoreSide {
  getConfig(payload: { sessionId: string }): { model: string };
}

interface HostSide {
  requestApproval(request: { requestId: string; toolName: string }): Promise<{ decision: string }>;
}

describe('transport wire behavior', () => {
  it('records send and recv bytes through onWire', async () => {
    const wire: { direction: 'send' | 'recv'; json: unknown }[] = [];

    const [connectCore, connectHost] = createRPC<CoreSide, HostSide>({
      transport: (dispatchLeft, dispatchRight) => {
        const [left, right] = createInProcessTransportPair(dispatchLeft, dispatchRight);
        const wrappedLeft: Transport = {
          send: async (bytes) => {
            wire.push({ direction: 'send', json: decodeJson(bytes) });
            const response = await left.send(bytes);
            wire.push({ direction: 'recv', json: decodeJson(response) });
            return response;
          },
        };
        const wrappedRight: Transport = {
          send: async (bytes) => {
            wire.push({ direction: 'send', json: decodeJson(bytes) });
            const response = await right.send(bytes);
            wire.push({ direction: 'recv', json: decodeJson(response) });
            return response;
          },
        };
        return [wrappedLeft, wrappedRight];
      },
    });
    const hostProxyPromise = connectCore({
      getConfig: ({ sessionId }) => ({ model: `model-for:${sessionId}` }),
    });
    await connectHost({
      requestApproval: async (request) => ({ decision: `approved:${request.toolName}` }),
    });
    const hostProxy = await hostProxyPromise;

    await hostProxy.requestApproval({ requestId: 'wire-1', toolName: 'Bash' });

    expect(wire).toHaveLength(2);
    expect(wire[0]!.direction).toBe('send');
    expect(wire[0]!.json).toEqual({ method: 'requestApproval', args: [{ requestId: 'wire-1', toolName: 'Bash' }] });
    expect(wire[1]!.direction).toBe('recv');
    expect(wire[1]!.json).toEqual({ ok: true, value: { decision: 'approved:Bash' } });
  });

  it('propagates a rejected send to the caller', async () => {
    const buggyTransport: Transport = {
      send: () => Promise.reject(new Error('channel broken')),
    };
    const pair: TransportPair = [buggyTransport, buggyTransport];
    const [connectCore, connectHost] = createRPC<CoreSide, HostSide>({ transport: pair });
    const hostProxyPromise = connectCore({
      getConfig: ({ sessionId }) => ({ model: `model-for:${sessionId}` }),
    });
    await connectHost({
      requestApproval: async () => ({ decision: 'approved' }),
    });
    const hostProxy = await hostProxyPromise;

    await expect(hostProxy.requestApproval({ requestId: 'x', toolName: 'Bash' })).rejects.toThrow('channel broken');
  });

  it('rejects pending calls when onError fires', async () => {
    let resolveSend: (() => void) | undefined;
    const hangingTransport: Transport = {
      send: () =>
        new Promise((_resolve, reject) => {
          resolveSend = () => reject(new Error('should have been rejected by onError'));
        }),
      onError: undefined,
    };
    const pair: TransportPair = [hangingTransport, hangingTransport];
    const [connectCore, connectHost] = createRPC<CoreSide, HostSide>({ transport: pair });
    const hostProxyPromise = connectCore({
      getConfig: ({ sessionId }) => ({ model: `model-for:${sessionId}` }),
    });
    await connectHost({
      requestApproval: async () => ({ decision: 'approved' }),
    });
    const hostProxy = await hostProxyPromise;

    const callPromise = hostProxy.requestApproval({ requestId: 'on-error', toolName: 'Bash' });
    // Give send a tick to register the pending Promise.
    await new Promise((resolve) => setTimeout(resolve, 10));

    hangingTransport.onError?.(new Error('transport fatal'));

    await expect(callPromise).rejects.toMatchObject({
      message: expect.stringContaining('transport fatal'),
      code: ErrorCodes.INTERNAL,
    });
    await expect(callPromise).rejects.toBeInstanceOf(OdyError);

    // Make sure we don't accidentally resolve later.
    resolveSend?.();
  });
});
