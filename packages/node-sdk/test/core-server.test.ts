import { mkdir, mkdtemp } from 'node:fs/promises';
import { createServer, createConnection, type AddressInfo } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import {
  createRPCEndpoint,
  createStreamTransport,
  type CoreAPI,
  type SDKAPI,
} from '@odysseythink/agent-core';

import { createCoreServer } from '../src/core-server';

describe('createCoreServer', () => {
  it('boots KimiCore and serves createSession over TCP stream transport', async () => {
    const tmpDir = await mkdtemp(join(tmpdir(), 'ody-core-server-'));
    await mkdir(join(tmpDir, 'sessions'), { recursive: true });

    const server = createServer((socket) => {
      createCoreServer(
        (dispatch) => createStreamTransport(socket, socket, dispatch, { framing: 'length-prefixed' }),
        { homeDir: tmpDir },
      );
    });

    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
    const port = (server.address() as AddressInfo).port;

    const clientSocket = await new Promise<import('node:net').Socket>((resolve) => {
      const socket = createConnection(port, '127.0.0.1', () => resolve(socket));
    });

    const clientEndpoint = createRPCEndpoint<SDKAPI, CoreAPI>();
    const clientTransport = createStreamTransport(clientSocket, clientSocket, clientEndpoint.dispatch, {
      framing: 'length-prefixed',
    });
    clientEndpoint.setTransport(clientTransport);

    const clientApi: SDKAPI = {
      emitEvent: async () => {},
      requestApproval: async () => ({ decision: 'approved' } as never),
      requestQuestion: async () => null,
      openExternal: async () => ({ opened: false }),
      toolCall: async () => ({ output: '', isError: false }),
      chatStreamInit: async () => { throw new Error('not implemented'); },
      chatStreamCancel: async () => {},
    } as unknown as SDKAPI;
    const rpc = await clientEndpoint.client(clientApi);
    const session = await rpc.createSession({
      workDir: tmpDir,
      id: 'test-session',
    });

    expect(session.id).toBe('test-session');
    expect(session.workDir).toBe(tmpDir);

    clientTransport.close?.();
    clientSocket.end();
    server.close();
  });
});
