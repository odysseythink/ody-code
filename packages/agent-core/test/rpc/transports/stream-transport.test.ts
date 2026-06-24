import { PassThrough } from 'node:stream';
import { describe, expect, it, vi } from 'vitest';

import { ErrorCodes } from '@odysseythink/agent-core-shared';
import { createStreamTransport } from '../../../src/rpc/transports/stream';
import { decodeJson, encodeJson } from '../../../src/rpc/transport';
import type { Transport } from '../../../src/rpc/transport';

interface ReadableLike {
  on(event: 'data', listener: (chunk: Uint8Array) => void): this;
  on(event: 'error', listener: (error: Error) => void): this;
  on(event: 'end', listener: () => void): this;
}

interface WritableLike {
  write(chunk: Uint8Array, cb?: (error?: Error | null) => void): boolean;
  end(cb?: () => void): this;
  on(event: 'error', listener: (error: Error) => void): this;
}

function createStreamTransportPair(
  dispatchLeft: (bytes: Uint8Array) => Promise<Uint8Array>,
  dispatchRight: (bytes: Uint8Array) => Promise<Uint8Array>,
  options?: { framing?: 'length-prefixed' | 'ndjson'; token?: string; requiredToken?: string },
): [Transport, Transport] {
  const leftToRight = new PassThrough();
  const rightToLeft = new PassThrough();

  const left = createStreamTransport(
    rightToLeft as unknown as ReadableLike,
    leftToRight as unknown as WritableLike,
    dispatchLeft,
    { framing: options?.framing, handshakeFraming: options?.framing, token: options?.token },
  );
  const right = createStreamTransport(
    leftToRight as unknown as ReadableLike,
    rightToLeft as unknown as WritableLike,
    dispatchRight,
    { framing: options?.framing, requiredToken: options?.requiredToken },
  );

  return [left, right];
}

describe('stream transport', () => {
  it('round-trips request/response with length-prefixed framing', async () => {
    const [client, server] = createStreamTransportPair(
      async () => encodeJson('unused'),
      async (bytes) => {
        expect(decodeJson(bytes)).toBe('ping');
        return encodeJson('pong');
      },
      { framing: 'length-prefixed' },
    );

    const response = await client.send(encodeJson('ping'));
    expect(decodeJson(response)).toBe('pong');

    client.close?.();
    server.close?.();
  });

  it('negotiates ndjson framing via handshake', async () => {
    const [client, server] = createStreamTransportPair(
      async () => encodeJson('unused'),
      async (bytes) => {
        return encodeJson(`echo:${decodeJson(bytes)}`);
      },
    );

    const response = await client.send(encodeJson('hello'));
    expect(decodeJson(response)).toBe('echo:hello');

    client.close?.();
    server.close?.();
  });

  it('rejects handshake with wrong token', async () => {
    const leftToRight = new PassThrough();
    const rightToLeft = new PassThrough();

    // Client sends with wrong token
    const client = createStreamTransport(
      rightToLeft as unknown as ReadableLike,
      leftToRight as unknown as WritableLike,
      async () => encodeJson('unused'),
      { token: 'ody_wrong', handshakeFraming: 'length-prefixed' },
    );
    // Server requires correct token
    createStreamTransport(
      leftToRight as unknown as ReadableLike,
      rightToLeft as unknown as WritableLike,
      async () => encodeJson('unused'),
      { requiredToken: 'ody_correct' },
    );

    // The first send triggers the handshake; server sees wrong token and closes
    const pending = client.send(encodeJson('x'));
    await expect(pending).rejects.toMatchObject({ code: ErrorCodes.TRANSPORT_CLOSED });
  });

  it('rejects pending sends after close', async () => {
    const passthrough = new PassThrough();
    const transport = createStreamTransport(
      passthrough as unknown as ReadableLike,
      passthrough as unknown as WritableLike,
      async () => new Promise(() => {}),
      { framing: 'length-prefixed' },
    );

    const pending = transport.send(encodeJson('hang'));
    transport.close?.();
    await expect(pending).rejects.toMatchObject({ code: ErrorCodes.TRANSPORT_CLOSED });
  });
});
