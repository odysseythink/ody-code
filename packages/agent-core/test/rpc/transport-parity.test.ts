import { describe, expect, it, vi } from 'vitest';

import { createRPC } from '../../src/rpc';
import {
  createInProcessTransportPair,
  decodeJson,
  type Dispatch,
  type TransportPair,
} from '../../src/rpc/transport';

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

describe('transport parity', () => {
  it('default path and explicit InProcessTransport produce identical wire semantics', async () => {
    const defaultLeftWire: WireEntry[] = [];
    const defaultRightWire: WireEntry[] = [];
    const [connectCoreDefault, connectHostDefault] = createRPC<CoreSide, HostSide>({
      transport: createRecordingFactory(defaultLeftWire, defaultRightWire),
    });

    const explicitLeftWire: WireEntry[] = [];
    const explicitRightWire: WireEntry[] = [];
    const [connectCoreExplicit, connectHostExplicit] = createRPC<CoreSide, HostSide>({
      transport: createRecordingFactory(explicitLeftWire, explicitRightWire),
    });

    await runScenario(connectCoreDefault, connectHostDefault);
    await runScenario(connectCoreExplicit, connectHostExplicit);

    expect(defaultLeftWire).toEqual(explicitLeftWire);
    expect(defaultRightWire).toEqual(explicitRightWire);
    expect(defaultLeftWire.length).toBeGreaterThan(0);
    expect(defaultRightWire.length).toBeGreaterThan(0);
  });
});
