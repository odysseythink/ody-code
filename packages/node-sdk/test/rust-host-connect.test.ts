import type { ChildProcess } from 'node:child_process';
import { existsSync } from 'node:fs';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import { Session } from '../src/session';
import { SDKRpcClient, type SDKRpcClientConnectOptions } from '../src/rpc';

interface WithRustHostOptions {
  readonly transport:
    | SDKRpcClientConnectOptions['transport']
    | ((homeDir: string) => SDKRpcClientConnectOptions['transport']);
  readonly binaryPath?: string | undefined;
  readonly extraArgs?: readonly string[];
}

interface HostFixture {
  readonly client: SDKRpcClient;
  readonly homeDir: string;
  readonly proc?: ChildProcess | undefined;
}

function resolveBinaryPath(env: NodeJS.ProcessEnv = process.env): string {
  if (env['ODY_HOST_BINARY_PATH']) {
    return env['ODY_HOST_BINARY_PATH'];
  }
  const candidate = fileURLToPath(
    new URL('../../../rust-ody/target/release/ody-host', import.meta.url),
  );
  if (existsSync(candidate)) {
    return candidate;
  }
  throw new Error(
    `ody-host binary not found at ${candidate}. ` +
      `Build with "pnpm run build:host" or set ODY_HOST_BINARY_PATH.`,
  );
}

async function withRustHost<T>(
  options: WithRustHostOptions,
  testFn: (fixture: HostFixture) => Promise<T>,
): Promise<T> {
  const homeDir = await mkdtemp(join(tmpdir(), 'ody-rust-host-'));
  const transport =
    typeof options.transport === 'function'
      ? options.transport(homeDir)
      : options.transport;
  const client = await SDKRpcClient.connect({
    transport,
    binaryPath: options.binaryPath ?? resolveBinaryPath(process.env),
    homeDir,
    extraArgs: options.extraArgs,
  });
  const proc = (client as unknown as { _hostProc?: ChildProcess })._hostProc;
  try {
    return await testFn({ client, homeDir, proc });
  } finally {
    await client.close?.();
    await rm(homeDir, { recursive: true, force: true });
  }
}

function makeUdsTransport(homeDir: string): SDKRpcClientConnectOptions['transport'] {
  return { socketPath: join(homeDir, 'host.sock'), spawn: true };
}

function isAddrInUse(err: Error): boolean {
  const message = err.message.toLowerCase();
  return (
    message.includes('eaddrinuse') ||
    message.includes('address already in use') ||
    message.includes('os error 48') ||
    message.includes('os error 98')
  );
}

async function bindTcpHost(
  testFn: (fixture: HostFixture) => Promise<void>,
): Promise<void> {
  const basePort = 19090;
  const maxAttempts = 10;
  let lastErr: unknown;
  for (let offset = 0; offset < maxAttempts; offset += 1) {
    const port = basePort + offset;
    try {
      await withRustHost(
        { transport: { host: '127.0.0.1', port, spawn: true } },
        testFn,
      );
      return;
    } catch (err) {
      lastErr = err;
      if (err instanceof Error && isAddrInUse(err) && offset < maxAttempts - 1) {
        continue;
      }
      throw err;
    }
  }
  throw lastErr instanceof Error
    ? lastErr
    : new Error('bindTcpHost exhausted all ports');
}

async function assertSessionLifecycle(
  client: SDKRpcClient,
  homeDir: string,
  fixedId?: string,
): Promise<void> {
  const input =
    fixedId !== undefined
      ? { workDir: homeDir, id: fixedId }
      : { workDir: homeDir };
  const session = await client.createSession(input);
  expect(typeof session.id).toBe('string');
  if (fixedId !== undefined) {
    expect(session.id).toBe(fixedId);
  }
  const sessions = await client.listSessions({ workDir: homeDir });
  expect(sessions.some((s) => s.id === session.id)).toBe(true);
  await client.closeSession({ sessionId: session.id });
}

function wrapSession(client: SDKRpcClient, summary: { readonly id: string; readonly workDir: string }): Session {
  return new Session({ id: summary.id, workDir: summary.workDir, rpc: client });
}

describe('SDKRpcClient.connect with real ody-host', () => {
  it('stdio transport creates and lists a session', async () => {
    await withRustHost({ transport: 'stdio' }, async ({ client, homeDir }) => {
      await assertSessionLifecycle(client, homeDir, 'stdio-session-id');
    });
  });

  it('stdio host process exits after close', async () => {
    await withRustHost({ transport: 'stdio' }, async ({ client, proc }) => {
      expect(proc).toBeDefined();
      const exitPromise = new Promise<number | null>((resolve) =>
        proc!.once('exit', (code) => resolve(code)),
      );
      await client.close?.();
      const exitCode = await exitPromise;
      // kill() sends SIGTERM; null means signal, 0 means clean exit — both are acceptable.
      expect([0, null]).toContain(exitCode);
    });
  });

  it('uds transport creates and lists a session', async () => {
    await withRustHost(
      { transport: (homeDir) => makeUdsTransport(homeDir) },
      async ({ client, homeDir }) => {
        await assertSessionLifecycle(client, homeDir, 'uds-session-id');
      },
    );
  });

  it('tcp transport creates and lists a session', async () => {
    await bindTcpHost(async ({ client, homeDir }) => {
      await assertSessionLifecycle(client, homeDir, 'tcp-session-id');
    });
  });

  it('createSession without id generates a uuid', async () => {
    await withRustHost({ transport: 'stdio' }, async ({ client, homeDir }) => {
      const session = await client.createSession({ workDir: homeDir });
      expect(session.id).toMatch(
        /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i,
      );
      const sessions = await client.listSessions({ workDir: homeDir });
      expect(sessions.some((s) => s.id === session.id)).toBe(true);
      await client.closeSession({ sessionId: session.id });
    });
  });

  it('rejects when the host binary is missing', async () => {
    await expect(
      withRustHost(
        { transport: 'stdio', binaryPath: '/nonexistent/ody-host-binary' },
        async () => {},
      ),
    ).rejects.toThrow(/Failed to spawn host/);
  });

  it('rejects when a UDS socket path is unreachable', async () => {
    await expect(
      withRustHost(
        {
          transport: { socketPath: join(tmpdir(), 'nonexistent-dir', 'host.sock'), spawn: false },
        },
        async () => {},
      ),
    ).rejects.toThrow();
  });

  describe('expanded CoreAPI (mock provider)', () => {
    it('agent-scoped getConfig is exercised by getStatus', async () => {
      await withRustHost(
        { transport: 'stdio', extraArgs: ['--mock-provider'] },
        async ({ client, homeDir }) => {
          const summary = await client.createSession({ workDir: homeDir });
          const session = wrapSession(client, summary);
          const status = await session.getStatus();
          expect(status.model).toBeDefined();
          expect(typeof status.model).toBe('string');
          expect(status.model!.length).toBeGreaterThan(0);
          expect(status.thinkingLevel).toBe('off');
          await client.closeSession({ sessionId: session.id });
        },
      );
    });

    it('setModel updates the session model', async () => {
      await withRustHost(
        { transport: 'stdio', extraArgs: ['--mock-provider'] },
        async ({ client, homeDir }) => {
          const summary = await client.createSession({ workDir: homeDir });
          const session = wrapSession(client, summary);
          await session.setModel('gpt-4o');
          const status = await session.getStatus();
          expect(status.model).toBe('gpt-4o');
          await client.closeSession({ sessionId: session.id });
        },
      );
    });

    it('setThinking updates the session thinking level', async () => {
      await withRustHost(
        { transport: 'stdio', extraArgs: ['--mock-provider'] },
        async ({ client, homeDir }) => {
          const summary = await client.createSession({ workDir: homeDir });
          const session = wrapSession(client, summary);
          await session.setThinking('on');
          const status = await session.getStatus();
          expect(status.thinkingLevel).toBe('on');
          await client.closeSession({ sessionId: session.id });
        },
      );
    });

    it('setPermission updates the session permission mode', async () => {
      await withRustHost(
        { transport: 'stdio', extraArgs: ['--mock-provider'] },
        async ({ client, homeDir }) => {
          const summary = await client.createSession({ workDir: homeDir });
          const session = wrapSession(client, summary);
          await session.setPermission('yolo');
          const status = await session.getStatus();
          expect(status.permission).toBe('yolo');
          await client.closeSession({ sessionId: session.id });
        },
      );
    });

    it('listSkills returns an empty array', async () => {
      await withRustHost(
        { transport: 'stdio', extraArgs: ['--mock-provider'] },
        async ({ client, homeDir }) => {
          const summary = await client.createSession({ workDir: homeDir });
          const session = wrapSession(client, summary);
          expect(await session.listSkills()).toEqual([]);
          await client.closeSession({ sessionId: session.id });
        },
      );
    });

    it('prompt emits turn events and returns ok', async () => {
      await withRustHost(
        { transport: 'stdio', extraArgs: ['--mock-provider'] },
        async ({ client, homeDir }) => {
          const summary = await client.createSession({ workDir: homeDir });
          const session = wrapSession(client, summary);
          const events: Array<{ readonly type: string }> = [];
          const unsubscribe = session.onEvent((event) => {
            events.push(event);
          });
          try {
            await session.prompt('hello');
          } finally {
            unsubscribe();
          }
          expect(events.some((e) => e.type === 'turn.started')).toBe(true);
          expect(events.some((e) => e.type === 'assistant.delta')).toBe(true);
          expect(events.some((e) => e.type === 'turn.ended')).toBe(true);
          await client.closeSession({ sessionId: session.id });
        },
      );
    });

    it('steer returns ok', async () => {
      await withRustHost(
        { transport: 'stdio', extraArgs: ['--mock-provider'] },
        async ({ client, homeDir }) => {
          const summary = await client.createSession({ workDir: homeDir });
          const session = wrapSession(client, summary);
          await expect(session.steer('focus on tests')).resolves.toBeUndefined();
          await client.closeSession({ sessionId: session.id });
        },
      );
    });

    it('getStatus composes config, context, permission, plan and usage', async () => {
      await withRustHost(
        { transport: 'stdio', extraArgs: ['--mock-provider'] },
        async ({ client, homeDir }) => {
          const summary = await client.createSession({ workDir: homeDir });
          const session = wrapSession(client, summary);
          const status = await session.getStatus();
          expect(status.permission).toBe('manual');
          expect(status.contextTokens).toBe(0);
          expect(status.maxContextTokens).toBe(0);
          expect(status.sessionMode).toBe('normal');
          await client.closeSession({ sessionId: session.id });
        },
      );
    });
  });
});
