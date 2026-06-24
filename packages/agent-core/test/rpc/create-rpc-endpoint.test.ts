import { describe, expect, it, vi } from 'vitest';

import { createRPCEndpoint } from '../../src/rpc/client';
import {
  createInProcessTransportPair,
  decodeJson,
  encodeJson,
} from '../../src/rpc/transport';

interface CoreSide {
  getConfig(payload: { sessionId: string }): { model: string };
}

interface HostSide {
  requestApproval(request: {
    requestId: string;
    toolName: string;
  }): Promise<{ decision: string }>;
  fail(request: { code: string }): Promise<void>;
}

describe('createRPCEndpoint', () => {
  it('round-trips calls over InProcessTransport', async () => {
    const left = createRPCEndpoint<CoreSide, HostSide>();
    const right = createRPCEndpoint<HostSide, CoreSide>();
    const [leftTransport, rightTransport] = createInProcessTransportPair(
      left.dispatch,
      right.dispatch,
    );
    left.setTransport(leftTransport);
    right.setTransport(rightTransport);

    const hostProxy = await left.client({
      getConfig: ({ sessionId }) => ({ model: `model:${sessionId}` }),
    });
    const coreProxy = await right.client({
      requestApproval: async (request) => ({ decision: `approved:${request.toolName}` }),
      fail: async () => {
        throw new Error('boom');
      },
    });

    await expect(
      hostProxy.requestApproval({ requestId: 'a', toolName: 'Bash' }),
    ).resolves.toEqual({ decision: 'approved:Bash' });
    await expect(coreProxy.getConfig({ sessionId: 's1' })).resolves.toEqual({
      model: 'model:s1',
    });
    await expect(hostProxy.fail({ code: 'x' })).rejects.toThrow('boom');
  });

  it('propagates transport onError to pending calls', async () => {
    const left = createRPCEndpoint<CoreSide, HostSide>();
    const right = createRPCEndpoint<HostSide, CoreSide>();
    const [leftTransport, rightTransport] = createInProcessTransportPair(
      left.dispatch,
      right.dispatch,
    );
    left.setTransport(leftTransport);
    right.setTransport(rightTransport);

    await right.client({
      requestApproval: async () => ({ decision: 'ok' }),
      fail: async () => {},
    });
    const hostProxy = await left.client({
      getConfig: () => ({ model: 'x' }),
    });

    const pending = hostProxy.requestApproval({ requestId: 'x', toolName: 'Bash' });
    // Wait for the async mapMethod to suspend on its deferred.promise, but NOT
    // for the InProcessTransport's setTimeout(0) to fire, so that the pending
    // call is still in-flight when onError fires.
    await new Promise<void>((resolve) => queueMicrotask(() => resolve()));
    leftTransport.onError?.(new Error('transport fatal'));

    await expect(pending).rejects.toThrow('transport fatal');
  });
});
