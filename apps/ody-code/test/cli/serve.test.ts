import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createConnection, createServer, type AddressInfo } from 'node:net';
import { PassThrough } from 'node:stream';
import { once } from 'node:events';

import { Command } from 'commander';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { handleServe, registerServeCommand, type ServeDeps } from '#/cli/sub/serve';

let tmp: string;

const mocks = vi.hoisted(() => ({
  createCoreServer: vi.fn<(...args: unknown[]) => { close: () => void }>(() => ({
    close: vi.fn(),
  })),
}));

vi.mock('@odysseythink/ody-code-sdk', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@odysseythink/ody-code-sdk')>();
  return { ...actual, createCoreServer: mocks.createCoreServer };
});

beforeEach(() => {
  tmp = mkdtempSync(join(tmpdir(), 'ody-serve-'));
});

afterEach(() => {
  rmSync(tmp, { recursive: true, force: true });
  vi.clearAllMocks();
});

interface WritableSpy {
  readonly chunks: string[];
  readonly write: (chunk: Uint8Array) => boolean;
  end: () => WritableSpy;
  on: () => WritableSpy;
}

function makeWritableSpy(): WritableSpy {
  const chunks: string[] = [];
  const self: WritableSpy = {
    chunks,
    write: (chunk: Uint8Array) => {
      chunks.push(new TextDecoder().decode(chunk));
      return true;
    },
    end: () => self,
    on: () => self,
  };
  return self;
}

function makeDeps(overrides: Partial<ServeDeps> = {}): ServeDeps {
  return {
    version: '1.0.0-test',
    createCoreServer: mocks.createCoreServer,
    createServer,
    stdin: new PassThrough(),
    stdout: new PassThrough(),
    stderr: makeWritableSpy(),
    exit: ((code: number) => {
      throw new Error(`exit(${code})`);
    }) as ServeDeps['exit'],
    ...overrides,
  };
}

function lastReady(deps: ServeDeps): unknown {
  const spy = deps.stderr as unknown as WritableSpy;
  const line = spy.chunks[spy.chunks.length - 1]?.trim() ?? '{}';
  return JSON.parse(line);
}

describe('ody serve', () => {
  it('registers the serve subcommand', () => {
    const program = new Command('ody');
    registerServeCommand(program, '1.0.0-test');

    const command = program.commands.find((item) => item.name() === 'serve');
    expect(command).toBeDefined();
    expect(command?.description()).toContain('headless');
  });

  it('starts a UDS server and emits a ready message', async () => {
    const socketPath = join(tmp, 'test.sock');
    const deps = makeDeps();

    const controller = await handleServe(deps, { socket: socketPath });

    const ready = lastReady(deps) as { type: string; socketPath: string; stdio: boolean };
    expect(ready.type).toBe('ready');
    expect(ready.socketPath).toBe(socketPath);
    expect(ready.stdio).toBe(false);

    // First connection should succeed and trigger createCoreServer
    // Keep it open so connected=true for the second connection test
    const client = createConnection(socketPath);
    await once(client, 'connect');
    expect(mocks.createCoreServer).toHaveBeenCalledOnce();

    // Second connection should be immediately destroyed (single-client policy)
    const second = new Promise<boolean>((resolve) => {
      const sock = createConnection(socketPath);
      sock.on('close', () => resolve(true));
      sock.on('error', () => resolve(true));
      setTimeout(() => resolve(false), 2000);
    });
    await expect(second).resolves.toBe(true);

    client.end();
    await controller.close();
  });

  it('uses stdio transport and emits ready to stderr', async () => {
    const deps = makeDeps();

    const controller = await handleServe(deps, { stdio: true });

    const ready = lastReady(deps) as { type: string; stdio: boolean };
    expect(ready.type).toBe('ready');
    expect(ready.stdio).toBe(true);
    expect(mocks.createCoreServer).toHaveBeenCalledOnce();

    await controller.close();
  });
});

describe('ody serve TCP mode', () => {
  it('starts a TCP server with a bearer token and rejects a second client', async () => {
    const deps = makeDeps();

    const controller = await handleServe(deps, { host: '127.0.0.1', port: 0 });

    const ready = lastReady(deps) as {
      type: string;
      host: string;
      port: number;
      token: string;
      stdio: boolean;
    };
    expect(ready.type).toBe('ready');
    expect(ready.host).toBe('127.0.0.1');
    expect(typeof ready.port).toBe('number');
    expect(ready.token).toMatch(/^ody_[A-Za-z0-9_-]+$/);
    expect(ready.stdio).toBe(false);

    // First connection should succeed and trigger createCoreServer
    // Keep it open so connected=true for the second connection test
    const client = createConnection(ready.port, ready.host);
    await once(client, 'connect');
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(mocks.createCoreServer).toHaveBeenCalledOnce();

    // Second connection should be immediately destroyed (single-client policy)
    const second = new Promise<boolean>((resolve) => {
      const sock = createConnection(ready.port, ready.host);
      sock.on('close', () => resolve(true));
      sock.on('error', () => resolve(true));
      setTimeout(() => resolve(false), 2000);
    });
    await expect(second).resolves.toBe(true);

    client.end();
    await controller.close();
  });
});
