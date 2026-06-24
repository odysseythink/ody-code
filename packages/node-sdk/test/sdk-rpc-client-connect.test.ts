import { mkdir, mkdtemp } from 'node:fs/promises';
import { createServer, createConnection, type AddressInfo } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { createStreamTransport } from '@odysseythink/agent-core';

import { createCoreServer } from '../src/core-server';
import { SDKRpcClient } from '../src/rpc';

describe('SDKRpcClient.connect', () => {
  it('connects over TCP and creates a session', async () => {
    const tmpDir = await mkdtemp(join(tmpdir(), 'ody-sdk-connect-'));
    await mkdir(join(tmpDir, 'sessions'), { recursive: true });

    const server = createServer((socket) => {
      createCoreServer(
        (dispatch) => createStreamTransport(socket, socket, dispatch, { framing: 'length-prefixed' }),
        { homeDir: tmpDir },
      );
    });

    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
    const port = (server.address() as AddressInfo).port;

    const client = await SDKRpcClient.connect({
      transport: { host: '127.0.0.1', port },
      homeDir: tmpDir,
    });

    const session = await client.createSession({
      workDir: tmpDir,
      id: 'sdk-connect-session',
    });
    expect(session.id).toBe('sdk-connect-session');

    server.close();
  });
});
