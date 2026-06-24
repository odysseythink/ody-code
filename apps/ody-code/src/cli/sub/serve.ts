import { mkdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { createServer, type Server, type Socket } from 'node:net';
import { once } from 'node:events';
import type { Command } from 'commander';

import {
  createStreamTransport,
  type Dispatch,
} from '@odysseythink/agent-core';
import {
  createCoreServer,
  resolveOdyHome,
  type CoreServerOptions,
} from '@odysseythink/ody-code-sdk';

export interface ServeCommandOptions {
  readonly socket?: string | undefined;
  readonly host?: string | undefined;
  readonly port?: number | undefined;
  readonly stdio?: boolean | undefined;
  readonly homeDir?: string | undefined;
  readonly configPath?: string | undefined;
  readonly skillDirs?: readonly string[];
}

interface ReadyMessage {
  readonly type: 'ready';
  readonly stdio: boolean;
  readonly socketPath?: string | undefined;
  readonly host?: string | undefined;
  readonly port?: number | undefined;
  readonly token?: string | undefined;
}

interface ReadableLike {
  on(event: 'data', listener: (chunk: Uint8Array) => void): this;
  on(event: 'error', listener: (error: Error) => void): this;
  on(event: 'end', listener: () => void): this;
}

interface WritableLike {
  write(chunk: Uint8Array): boolean;
  end(cb?: () => void): this;
  on(event: 'error', listener: (error: Error) => void): this;
}

export interface ServeDeps {
  readonly version: string;
  readonly createCoreServer: typeof import('@odysseythink/ody-code-sdk').createCoreServer;
  readonly createServer: typeof import('node:net').createServer;
  readonly stdin: ReadableLike;
  readonly stdout: WritableLike;
  readonly stderr: WritableLike;
  readonly exit: (code: number) => never;
}

function emitReady(stderr: WritableLike, message: ReadyMessage): void {
  stderr.write(new TextEncoder().encode(JSON.stringify(message) + '\n'));
}

function createCoreOptions(
  deps: ServeDeps,
  options: ServeCommandOptions,
): CoreServerOptions {
  return {
    homeDir: options.homeDir,
    configPath: options.configPath,
    skillDirs: options.skillDirs,
    appVersion: deps.version,
  };
}

function startStdioServer(
  deps: ServeDeps,
  coreOptions: CoreServerOptions,
): { close(): Promise<void> } {
  const server = deps.createCoreServer(
    (dispatch: Dispatch) =>
      createStreamTransport(deps.stdin, deps.stdout, dispatch, {
        framing: 'length-prefixed',
      }),
    coreOptions,
  );
  emitReady(deps.stderr, { type: 'ready', stdio: true });
  return {
    close: async () => {
      server.close();
    },
  };
}

function startUnixServer(
  deps: ServeDeps,
  socketPath: string,
  coreOptions: CoreServerOptions,
): Promise<{ close(): Promise<void> }> {
  let connected = false;
  let currentCore: { close(): void } | undefined;

  const server = deps.createServer((socket: Socket) => {
    if (connected) {
      socket.destroy();
      return;
    }
    connected = true;
    currentCore = deps.createCoreServer(
      (dispatch: Dispatch) => createStreamTransport(socket, socket, dispatch),
      coreOptions,
    );
    socket.once('close', () => {
      currentCore?.close();
      connected = false;
    });
  });

  return new Promise((resolve, reject) => {
    rmSync(socketPath, { force: true });
    server.listen(socketPath, () => {
      emitReady(deps.stderr, { type: 'ready', stdio: false, socketPath });
      resolve({
        close: async () => {
          server.close();
          currentCore?.close();
          await once(server, 'close');
        },
      });
    });
    server.once('error', reject);
  });
}

export async function handleServe(
  deps: ServeDeps,
  options: ServeCommandOptions,
): Promise<{ close(): Promise<void> }> {
  const coreOptions = createCoreOptions(deps, options);

  if (options.stdio) {
    return startStdioServer(deps, coreOptions);
  }

  if (options.socket !== undefined) {
    return startUnixServer(deps, options.socket, coreOptions);
  }

  const homeDir = resolveOdyHome(options.homeDir);
  const socketPath = join(homeDir, 'run', `serve-${process.pid}.sock`);
  mkdirSync(join(homeDir, 'run'), { recursive: true });
  return startUnixServer(deps, socketPath, coreOptions);
}

function parseIntOption(value: string): number {
  const parsed = parseInt(value, 10);
  if (Number.isNaN(parsed)) {
    throw new Error(`Invalid integer: ${value}`);
  }
  return parsed;
}

function collectSkillDir(value: string, previous: string[]): string[] {
  return [...previous, value];
}

function createDefaultServeDeps(version: string): ServeDeps {
  return {
    version,
    createCoreServer,
    createServer,
    stdin: process.stdin,
    stdout: process.stdout,
    stderr: process.stderr,
    exit: (code: number) => process.exit(code),
  };
}

export function registerServeCommand(parent: Command, version: string): void {
  parent
    .command('serve')
    .description('Run Ody Code core as a headless server.')
    .option('--socket <path>', 'Unix domain socket path.')
    .option('--host <ip>', 'TCP host.')
    .option('--port <n>', 'TCP port.', parseIntOption)
    .option('--stdio', 'Use stdio transport.', false)
    .option('--home-dir <path>', 'Ody home directory.')
    .option('--config-path <path>', 'Path to config.toml.')
    .option(
      '--skill-dir <dir>',
      'Skill directory. Can be repeated.',
      collectSkillDir,
      [] as string[],
    )
    .action(async (raw: Record<string, unknown>) => {
      const options: ServeCommandOptions = {
        socket: raw['socket'] as string | undefined,
        host: raw['host'] as string | undefined,
        port: raw['port'] as number | undefined,
        stdio: raw['stdio'] === true,
        homeDir: raw['homeDir'] as string | undefined,
        configPath: raw['configPath'] as string | undefined,
        skillDirs: raw['skillDir'] as string[],
      };
      await handleServe(createDefaultServeDeps(version), options);
    });
}
