# Part 3: `ody serve` CLI Command (apps/ody-code)

**Goal:** 在 `apps/ody-code` 注册并实现 `ody serve` 子命令，支持 headless 运行 Core：stdio、UDS、TCP（带一次性 token）以及 TCP/WebSocket 同端口共享。

**Architecture:** `apps/ody-code/src/cli/sub/serve.ts` 提供 `registerServeCommand` 与 `handleServe`；`handleServe` 根据参数选择 transport，调用 `node-sdk` 的 `createCoreServer`，并通过 stderr 输出 JSON ready 消息。UDS 依赖操作系统文件权限；TCP/WebSocket 使用启动时生成的 `ody_<base64url>` token 鉴权；同一时刻只接受一个客户端，第二个连接立即销毁。WebSocket 通过 `ws` 库实现，与 TCP 共用端口：服务器嗅探首字节，HTTP 请求交给 `http` + `WebSocketServer` 处理，否则按原始 TCP 处理。

**Tech Stack:** TypeScript 6.0 / Node.js ≥24.15 / commander / Vitest，复用 Part 1 的 `createStreamTransport`/`createWebSocketTransport` 与 Part 2 的 `createCoreServer`。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task). Steps use - [ ] checkboxes for tracking.

---

### Task 10: 实现 `ody serve --stdio` 与 `--socket`（UDS）

**Depends on:** `2026-06-25-backend-architecture-evolution-phase2-b/sdk.md` Task 8

**Files:**
- Create: `apps/ody-code/src/cli/sub/serve.ts`
- Modify: `apps/ody-code/src/cli/commands.ts:6-7`（新增 import）
- Modify: `apps/ody-code/src/cli/commands.ts:100-103`（注册子命令）
- Create: `apps/ody-code/test/cli/serve.test.ts`

- [ ] Write the failing test：创建 `apps/ody-code/test/cli/serve.test.ts`，验证子命令注册、UDS 模式 ready 消息、单客户端拒绝，以及 stdio 模式 ready 消息。

```ts
// apps/ody-code/test/cli/serve.test.ts
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createConnection, createServer } from 'node:net';
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
  readonly write: ReturnType<typeof vi.fn>;
}

function makeWritableSpy(): WritableSpy {
  const chunks: string[] = [];
  return {
    chunks,
    write: vi.fn((chunk: string) => {
      chunks.push(chunk);
      return true;
    }),
  };
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

    const client = createConnection(socketPath);
    await once(client, 'connect');
    expect(mocks.createCoreServer).toHaveBeenCalledOnce();

    const second = createConnection(socketPath);
    await once(second, 'close');
    expect(second.destroyed).toBe(true);

    await controller.close();
  });

  it('uses stdio transport and emits ready to stderr', async () => {
    const deps = makeDeps();

    const controller = await handleServe(deps, { stdio: true });

    const ready = lastReady(deps) as { type: string; stdio: boolean };
    expect(ready.type).toBe('ready');
    expect(ready.stdio).toBe(true);
    expect(mocks.createCoreServer).toHaveBeenCalledOnce();

    const [createTransport] = mocks.createCoreServer.mock.calls[0] as [
      (dispatch: unknown) => unknown,
      unknown,
    ];
    expect(typeof createTransport).toBe('function');

    await controller.close();
  });
});
```

- [ ] Run it and verify it FAILS：

```bash
pnpm --filter ody-code test apps/ody-code/test/cli/serve.test.ts
```

Expected failure：`#/cli/sub/serve` module not found；`registerServeCommand` / `handleServe` 未定义。

- [ ] Write the minimal implementation：

```ts
// apps/ody-code/src/cli/sub/serve.ts
import { mkdirSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { createServer, type AddressInfo, type Server, type Socket } from 'node:net';
import { once } from 'node:events';
import type { Command } from 'commander';

import {
  createStreamTransport,
  type Dispatch,
  type Transport,
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
  mkdirSync(dirname(socketPath), { recursive: true });
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
```

```ts
// apps/ody-code/src/cli/commands.ts
// 新增 import（约第 6-7 行）
import { registerServeCommand } from './sub/serve';

// 在 registerExportCommand 之后注册（约第 100-103 行）
registerExportCommand(program);
registerProviderCommand(program);
registerRequestCodeReviewCommand(program);
registerServeCommand(program, version);
```

- [ ] Run it and verify it PASSES：

```bash
pnpm --filter ody-code test apps/ody-code/test/cli/serve.test.ts
```

Expected：3 个测试全绿。

- [ ] Commit：`git add -A && git commit -m "feat(cli): add ody serve command with stdio and UDS modes"`

- [ ] Whole-tree typecheck：

```bash
pnpm -r typecheck
```

Expected：全绿（依赖 Part 2 已导出 `createCoreServer`）。

---

### Task 11: 实现 `ody serve --host/--port`（TCP + 一次性 token）

**Depends on:** Task 10

**Files:**
- Modify: `apps/ody-code/src/cli/sub/serve.ts`
- Modify: `apps/ody-code/test/cli/serve.test.ts`

- [ ] Write the failing test：追加 TCP 模式测试，验证 ready 消息携带 host/port/token，且第二个连接被拒绝。

```ts
// 追加到 apps/ody-code/test/cli/serve.test.ts
import { createConnection, type AddressInfo } from 'node:net';

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

    const client = createConnection(ready.port, ready.host);
    await once(client, 'connect');
    expect(mocks.createCoreServer).toHaveBeenCalledOnce();

    const second = createConnection(ready.port, ready.host);
    await once(second, 'close');
    expect(second.destroyed).toBe(true);

    await controller.close();
  });
});
```

- [ ] Run it and verify it FAILS：

```bash
pnpm --filter ody-code test apps/ody-code/test/cli/serve.test.ts
```

Expected failure：新增 TCP 测试失败，`handleServe` 不识别 `host`/`port` 选项。

- [ ] Write the minimal implementation：在 `serve.ts` 新增 `generateToken` 与 `startTcpServer`，并在 `handleServe` 中优先处理 TCP 选项。

```ts
// apps/ody-code/src/cli/sub/serve.ts
// 新增 import
import { randomBytes } from 'node:crypto';

function generateToken(): string {
  return `ody_${randomBytes(32).toString('base64url')}`;
}

function startTcpServer(
  deps: ServeDeps,
  host: string,
  port: number,
  coreOptions: CoreServerOptions,
): Promise<{ close(): Promise<void> }> {
  const token = generateToken();
  let connected = false;
  let currentCore: { close(): void } | undefined;

  const server = deps.createServer((socket: Socket) => {
    if (connected) {
      socket.destroy();
      return;
    }
    connected = true;
    currentCore = deps.createCoreServer(
      (dispatch: Dispatch) =>
        createStreamTransport(socket, socket, dispatch, { requiredToken: token }),
      coreOptions,
    );
    socket.once('close', () => {
      currentCore?.close();
      connected = false;
    });
  });

  return new Promise((resolve, reject) => {
    server.listen(port, host, () => {
      const address = server.address() as AddressInfo;
      emitReady(deps.stderr, {
        type: 'ready',
        stdio: false,
        host: address.address,
        port: address.port,
        token,
      });
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

// 修改 handleServe，在 UDS 默认分支之前插入 TCP 分支
export async function handleServe(
  deps: ServeDeps,
  options: ServeCommandOptions,
): Promise<{ close(): Promise<void> }> {
  const coreOptions = createCoreOptions(deps, options);

  if (options.stdio) {
    return startStdioServer(deps, coreOptions);
  }

  if (options.host !== undefined || options.port !== undefined) {
    const host = options.host ?? '127.0.0.1';
    const port = options.port ?? 0;
    return startTcpServer(deps, host, port, coreOptions);
  }

  if (options.socket !== undefined) {
    return startUnixServer(deps, options.socket, coreOptions);
  }

  const homeDir = resolveOdyHome(options.homeDir);
  const socketPath = join(homeDir, 'run', `serve-${process.pid}.sock`);
  mkdirSync(dirname(socketPath), { recursive: true });
  return startUnixServer(deps, socketPath, coreOptions);
}
```

- [ ] Run it and verify it PASSES：

```bash
pnpm --filter ody-code test apps/ody-code/test/cli/serve.test.ts
```

Expected：所有测试全绿，TCP ready 消息包含 token。

- [ ] Commit：`git add -A && git commit -m "feat(cli): add TCP mode with bearer token to ody serve"`

---

### Task 12: TCP/WebSocket 同端口共享

**Depends on:** Task 11

**Files:**
- Modify: `apps/ody-code/package.json:78-89`（新增 `ws` 与 `@types/ws`）
- Modify: `apps/ody-code/src/cli/sub/serve.ts`
- Modify: `apps/ody-code/test/cli/serve.test.ts`

- [ ] Write the failing test：追加 WebSocket 同端口测试，验证 WS 客户端可连接，第二个 WS 客户端被拒绝。

```ts
// 追加到 apps/ody-code/test/cli/serve.test.ts
import WebSocket from 'ws';

describe('ody serve WebSocket sharing', () => {
  it('accepts WebSocket on the same TCP port and rejects a second client', async () => {
    const deps = makeDeps();

    const controller = await handleServe(deps, { host: '127.0.0.1', port: 0 });

    const ready = lastReady(deps) as {
      host: string;
      port: number;
      token: string;
    };
    const url = `ws://${ready.host}:${ready.port}?token=${encodeURIComponent(ready.token)}`;

    const ws = new WebSocket(url);
    await new Promise<void>((resolve, reject) => {
      ws.on('open', resolve);
      ws.on('error', reject);
    });
    expect(mocks.createCoreServer).toHaveBeenCalledOnce();

    const second = new WebSocket(url);
    await new Promise<void>((resolve) => {
      second.on('close', resolve);
    });
    expect(second.readyState).toBe(WebSocket.CLOSED);

    await controller.close();
  });
});
```

- [ ] Run it and verify it FAILS：

```bash
pnpm --filter ody-code test apps/ody-code/test/cli/serve.test.ts
```

Expected failure：`ws` 模块未安装；`WebSocket` 路径未实现。

- [ ] Write the minimal implementation：

```json
// apps/ody-code/package.json
// dependencies 中新增（约第 77 行后）
    "ws": "^8.18.0",

// devDependencies 中新增（约第 88 行后）
    "@types/ws": "^8.5.13",
```

```bash
pnpm install
```

```ts
// apps/ody-code/src/cli/sub/serve.ts
// 新增 import
import http from 'node:http';
import { WebSocketServer, type WebSocket } from 'ws';
import { createWebSocketTransport } from '@odysseythink/agent-core';

function looksLikeHttp(byte: number): boolean {
  // HTTP method names start with an uppercase letter (G, E, P, D, H, O, T, C...)
  return byte >= 0x41 && byte <= 0x5a;
}

function startTcpServer(
  deps: ServeDeps,
  host: string,
  port: number,
  coreOptions: CoreServerOptions,
): Promise<{ close(): Promise<void> }> {
  const token = generateToken();
  let connected = false;
  let currentCore: { close(): void } | undefined;

  const httpServer = http.createServer((_req, res) => {
    res.writeHead(426, { 'Content-Type': 'text/plain' });
    res.end('Upgrade required');
  });
  const wss = new WebSocketServer({ server: httpServer });

  wss.on('connection', (ws: WebSocket, req: http.IncomingMessage) => {
    if (connected) {
      ws.close();
      return;
    }

    const url = new URL(req.url ?? '/', `http://${host}:${port}`);
    if (url.searchParams.get('token') !== token) {
      ws.close();
      return;
    }

    connected = true;
    currentCore = deps.createCoreServer(
      (dispatch: Dispatch) => createWebSocketTransport(ws, dispatch),
      coreOptions,
    );
    ws.once('close', () => {
      currentCore?.close();
      connected = false;
    });
  });

  const server = deps.createServer((socket: Socket) => {
    socket.once('data', (chunk: Uint8Array) => {
      socket.pause();
      if (looksLikeHttp(chunk[0] ?? 0)) {
        socket.unshift(chunk);
        httpServer.emit('connection', socket);
      } else {
        socket.unshift(chunk);
        if (connected) {
          socket.destroy();
          socket.resume();
          return;
        }
        connected = true;
        currentCore = deps.createCoreServer(
          (dispatch: Dispatch) =>
            createStreamTransport(socket, socket, dispatch, { requiredToken: token }),
          coreOptions,
        );
        socket.once('close', () => {
          currentCore?.close();
          connected = false;
        });
      }
      socket.resume();
    });
  });

  return new Promise((resolve, reject) => {
    server.listen(port, host, () => {
      const address = server.address() as AddressInfo;
      emitReady(deps.stderr, {
        type: 'ready',
        stdio: false,
        host: address.address,
        port: address.port,
        token,
      });
      resolve({
        close: async () => {
          server.close();
          httpServer.close();
          wss.close();
          currentCore?.close();
          await once(server, 'close');
        },
      });
    });
    server.once('error', reject);
  });
}
```

- [ ] Run it and verify it PASSES：

```bash
pnpm --filter ody-code test apps/ody-code/test/cli/serve.test.ts
```

Expected：所有测试全绿，WebSocket 测试通过。

- [ ] Commit：`git add -A && git commit -m "feat(cli): share TCP port with WebSocket in ody serve"`

- [ ] Whole-tree typecheck + install verification：

```bash
pnpm install
pnpm -r typecheck
```

Expected：全绿，`ws` 类型可用。

---

## Local Self-Review

- [ ] 1. Spec-coverage table（本 Part）：`ody serve` 子命令注册 → Task 10；stdio 模式 → Task 10；UDS 默认/显式 socket → Task 10；TCP + 一次性 token → Task 11；TCP/WebSocket 同端口共享 → Task 12；单客户端拒绝 → Task 10-12。
- [ ] 2. Placeholder scan：本 Part 无 `TODO`/`TBD`/`implement later`；所有实现、测试、依赖安装命令已完整给出。
- [ ] 3. No phantom tasks：每个 Task 均产出文件变更、测试/验证步骤与 commit；无 `--allow-empty`。
- [ ] 4. Dependency soundness：Task 10 依赖 Part 2 Task 8（`createCoreServer` 导出）；Task 11 依赖 Task 10；Task 12 依赖 Task 11；无向后引用。
- [ ] 5. Caller & build soundness：Task 10 修改 `apps/ody-code/src/cli/commands.ts` 的 `createProgram` 以注册子命令，需确认 `apps/ody-code/test/cli/main.test.ts` 等调用 `createProgram` 的测试仍通过；Task 10/11/12 末尾均运行 `pnpm -r typecheck`。
- [ ] 6. Test-the-risk：UDS/TCP/WebSocket 均有端到端连接测试与单客户端拒绝断言；ready 消息字段（token/host/port/socketPath/stdio）均通过实际 stderr 输出断言；token 格式通过正则断言。
- [ ] 7. Type consistency：`ServeDeps`、`ServeCommandOptions`、`ReadyMessage` 接口与 `handleServe`、`registerServeCommand` 签名在本 Part 内一致，且与 Part 2 的 `createCoreServer` 签名一致。
