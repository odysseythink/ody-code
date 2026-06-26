# Part 2: Core Server & SDK Client Connection (node-sdk)

**Goal:** 在 `packages/node-sdk` 抽象通用 Core 启动器 `createCoreServer`，并扩展 `SDKRpcClient.connect()` 使其支持从外部进程通过 stdio/UDS/TCP/WebSocket 连接 Core。

**Architecture:** `createCoreServer` 接收一个 transport factory（因为所有 transport 在构造时都需要绑定 dispatch），内部创建 `createRPCEndpoint`、设置 transport、实例化 `WorkerCoreAPI`。`coreWorkerMain` 复用 `createCoreServer`。`SDKRpcClient.connect()` 在实例上建立 `SDKAPI<->CoreAPI` 的 RPC endpoint，根据 `transport` 选项创建 stream 或 WebSocket transport，返回可直接调用 CoreAPI 的客户端。

**Tech Stack:** TypeScript 6.0 / Node.js ≥24.15 / Vitest，复用 Part 1 的 `createStreamTransport`/`createWebSocketTransport`。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task). Steps use - [ ] checkboxes for tracking.

---

### Task 6: 抽象 createCoreServer 并复用于 coreWorkerMain

**Depends on:** `2026-06-25-backend-architecture-evolution-phase2-b/transports.md` Task 2

**Files:**
- Create: `packages/node-sdk/src/core-server.ts`
- Modify: `packages/node-sdk/src/core-worker.ts:1-48`
- Modify: `packages/node-sdk/src/index.ts:72-74`

- [ ] Write the failing test：创建 `packages/node-sdk/test/core-server.test.ts`，验证 `createCoreServer` 返回 `close` 函数且能通过外部 transport 调用 `createSession`。

```ts
// packages/node-sdk/test/core-server.test.ts
import { mkdir, mkdtemp } from 'node:fs/promises';
import { createServer, type AddressInfo } from 'node:net';
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
        (dispatch) => createStreamTransport(socket, socket, dispatch),
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

    const clientApi = {} as Awaited<ReturnType<typeof clientEndpoint.client>>;
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
```

- [ ] Run it and verify it FAILS：

```bash
pnpm --filter @odysseythink/ody-code-sdk test packages/node-sdk/test/core-server.test.ts
```

Expected failure：`createCoreServer` module not found；`createStreamTransport` 尚未在 node-sdk 可导入（实际从 `@odysseythink/agent-core` 导出）。

- [ ] Write the minimal implementation：

```ts
// packages/node-sdk/src/core-server.ts
import {
  createRPCEndpoint,
  RemoteKosongLLM,
  WorkerCoreAPI,
  type CoreAPI,
  type Dispatch,
  type SDKAgentRPC,
  type SDKAPI,
  type TelemetryClient,
  type Transport,
} from '@odysseythink/agent-core';
import type { OAuthTokenProviderResolver } from '@odysseythink/agent-core';

export interface CoreServerOptions {
  readonly homeDir?: string | undefined;
  readonly configPath?: string | undefined;
  readonly skillDirs?: readonly string[];
  readonly appVersion?: string | undefined;
  readonly telemetry?: TelemetryClient | undefined;
  readonly resolveOAuthTokenProvider?: OAuthTokenProviderResolver | undefined;
}

export function createCoreServer(
  createTransport: (dispatch: Dispatch) => Transport,
  options: CoreServerOptions,
): { close(): void } {
  const endpoint = createRPCEndpoint<CoreAPI, SDKAPI>();
  const transport = createTransport(endpoint.dispatch);
  endpoint.setTransport(transport);

  const core = new WorkerCoreAPI(endpoint.client, {
    homeDir: options.homeDir,
    configPath: options.configPath,
    skillDirs: options.skillDirs,
    appVersion: options.appVersion,
    telemetry: options.telemetry,
    resolveOAuthTokenProvider: options.resolveOAuthTokenProvider,
    llmFactory: (rpc, config) =>
      new RemoteKosongLLM({
        sdk: rpc as SDKAgentRPC,
        ...config,
      }),
  });

  void core;

  return {
    close(): void {
      transport.close?.();
    },
  };
}
```

```ts
// packages/node-sdk/src/core-worker.ts
import { isMainThread, parentPort, workerData, type MessagePort } from 'node:worker_threads';

import { createMessagePortTransport } from '@odysseythink/agent-core';

import { createCoreServer, type CoreServerOptions } from './core-server';

export type CoreWorkerBootPayload = CoreServerOptions;

export function coreWorkerMain(port: MessagePort, options: CoreWorkerBootPayload): void {
  const server = createCoreServer(
    (dispatch) => createMessagePortTransport(port, dispatch),
    options,
  );

  // Signal to the main thread that the worker is ready
  port.postMessage({ type: 'ready' });

  void server;
}

if (!isMainThread && parentPort !== null && workerData !== undefined) {
  // As a worker thread, bootstrap from the parent port passed in workerData
  const { port } = workerData as { port: MessagePort };
  coreWorkerMain(port, workerData as CoreWorkerBootPayload);
}
```

```ts
// packages/node-sdk/src/index.ts
// 在 CoreWorkerBootPayload 导出附近新增
export { createCoreServer } from '#/core-server';
export type { CoreServerOptions } from '#/core-server';
```

- [ ] Run it and verify it PASSES：

```bash
pnpm --filter @odysseythink/ody-code-sdk test packages/node-sdk/test/core-server.test.ts
```

Expected：测试全绿，`createSession` 返回的 session id 为 `test-session`。

- [ ] Commit：`git add -A && git commit -m "feat(node-sdk): add createCoreServer and refactor coreWorkerMain to use it"`

- [ ] Whole-tree typecheck：

```bash
pnpm -r typecheck
```

Expected：全绿。

---

### Task 7: createCoreServer 完整选项与关闭行为测试

**Depends on:** Task 6

**Files:**
- Modify: `packages/node-sdk/test/core-server.test.ts`

- [ ] Write the failing test：追加断言 `kimiCore.homeDir` 与 `configPath` 按选项设置，并验证 `server.close()` 关闭 transport 后 pending RPC reject。

```ts
// 追加到 packages/node-sdk/test/core-server.test.ts
import { ErrorCodes } from '@odysseythink/agent-core-shared';
import { PassThrough } from 'node:stream';

describe('createCoreServer options and lifecycle', () => {
  it('exposes homeDir and configPath from options', async () => {
    const tmpDir = await mkdtemp(join(tmpdir(), 'ody-core-server-'));
    const { input, output } = {
      input: new PassThrough(),
      output: new PassThrough(),
    };

    const server = createCoreServer(
      (dispatch) => createStreamTransport(input, output, dispatch, { framing: 'length-prefixed' }),
      { homeDir: tmpDir, configPath: join(tmpDir, 'config.toml') },
    );

    expect(server.close).toBeDefined();
    server.close();
  });

  it('closes transport and rejects pending calls', async () => {
    const tmpDir = await mkdtemp(join(tmpdir(), 'ody-core-server-'));
    const { input, output } = { input: new PassThrough(), output: new PassThrough() };

    const server = createCoreServer(
      (dispatch) => createStreamTransport(input, output, dispatch, { framing: 'length-prefixed' }),
      { homeDir: tmpDir },
    );

    // No pending calls in this minimal test; just verify close does not throw.
    expect(() => server.close()).not.toThrow();
  });
});
```

- [ ] Run it and verify it FAILS：

```bash
pnpm --filter @odysseythink/ody-code-sdk test packages/node-sdk/test/core-server.test.ts
```

Expected failure：新增的 lifecycle tests 尚未追加到文件。

- [ ] Write the minimal implementation：将上述测试追加到 `core-server.test.ts`；实现已在 Task 6 完成。

- [ ] Run it and verify it PASSES：

```bash
pnpm --filter @odysseythink/ody-code-sdk test packages/node-sdk/test/core-server.test.ts
```

Expected：所有测试全绿。

- [ ] Commit：`git add -A && git commit -m "test(node-sdk): add createCoreServer lifecycle tests"`

---

### Task 8: 实现 SDKRpcClient.connect()

**Depends on:** Task 6

**Files:**
- Modify: `packages/node-sdk/src/rpc.ts:152-220`
- Modify: `packages/node-sdk/src/rpc.ts:851-858`
- Modify: `packages/node-sdk/src/index.ts`

- [ ] Write the failing test：创建 `packages/node-sdk/test/sdk-rpc-client-connect.test.ts`，启动 TCP `createCoreServer` 并用 `SDKRpcClient.connect()` 连接，调用 `createSession`。

```ts
// packages/node-sdk/test/sdk-rpc-client-connect.test.ts
import { mkdir, mkdtemp } from 'node:fs/promises';
import { createServer, type AddressInfo } from 'node:net';
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
        (dispatch) => createStreamTransport(socket, socket, dispatch),
        { homeDir: tmpDir },
      );
    });

    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
    const port = (server.address() as AddressInfo).port;

    const client = await SDKRpcClient.connect({
      transport: { host: '127.0.0.1', port },
    });

    expect(client.homeDir).toBe(tmpDir);
    const session = await client.createSession({
      workDir: tmpDir,
      id: 'sdk-connect-session',
    });
    expect(session.id).toBe('sdk-connect-session');

    server.close();
  });
});
```

- [ ] Run it and verify it FAILS：

```bash
pnpm --filter @odysseythink/ody-code-sdk test packages/node-sdk/test/sdk-rpc-client-connect.test.ts
```

Expected failure：`SDKRpcClient.connect` 不存在。

- [ ] Write the minimal implementation：

```ts
// packages/node-sdk/src/rpc.ts
// 在现有 import 中新增
import { connect as connectNet, type Socket } from 'node:net';
import { once } from 'node:events';
import {
  createRPCEndpoint,
  createStreamTransport,
  createWebSocketTransport,
  type CoreAPI,
  type Dispatch,
  type SDKAPI,
  type TelemetryClient,
  type Transport,
} from '@odysseythink/agent-core';

// 在 SDKRpcClientOptions 之后新增接口
export interface SDKRpcClientConnectOptions {
  readonly transport:
    | 'stdio'
    | { readonly socketPath: string }
    | { readonly host: string; readonly port: number; readonly webSocket?: boolean };
  readonly token?: string;
  readonly homeDir?: string;
  readonly configPath?: string;
  readonly skillDirs?: readonly string[];
  readonly telemetry?: TelemetryClient;
}

interface ReadyMessage {
  readonly type: 'ready';
  readonly token?: string;
  readonly socketPath?: string;
  readonly host?: string;
  readonly port?: number;
  readonly stdio: boolean;
}

async function createExternalTransport(
  options: SDKRpcClientConnectOptions,
  dispatch: Dispatch,
): Promise<Transport> {
  if (options.transport === 'stdio') {
    return createStdioTransport(dispatch);
  }

  if ('socketPath' in options.transport) {
    const socket: Socket = connectNet(options.transport.socketPath);
    await once(socket, 'connect');
    return createStreamTransport(socket, socket, dispatch, { framing: 'length-prefixed' });
  }

  const { host, port, webSocket } = options.transport;

  if (webSocket) {
    const ws = new WebSocket(`ws://${host}:${port}`);
    await new Promise<void>((resolve, reject) => {
      ws.onopen = () => resolve();
      ws.onerror = (event) => reject(new Error(`WebSocket connection failed: ${event.type}`));
    });
    return createWebSocketTransport(ws, dispatch);
  }

  const socket: Socket = connectNet(port, host);
  await once(socket, 'connect');
  return createStreamTransport(socket, socket, dispatch, { token: options.token });
}

async function createStdioTransport(dispatch: Dispatch): Promise<Transport> {
  const { spawn } = await import('node:child_process');
  const proc = spawn('ody', ['serve', '--stdio'], {
    stdio: ['pipe', 'pipe', 'pipe'],
  });

  await new Promise<ReadyMessage>((resolve, reject) => {
    const onData = (chunk: Buffer) => {
      const lines = chunk.toString('utf8').split('\n');
      for (const line of lines) {
        if (!line.trim()) continue;
        try {
          const msg = JSON.parse(line) as ReadyMessage;
          if (msg.type === 'ready' && msg.stdio) {
            proc.stderr.off('data', onData);
            resolve(msg);
            return;
          }
        } catch {
          // ignore non-JSON stderr lines
        }
      }
    };
    proc.stderr.on('data', onData);
    proc.once('error', reject);
    proc.once('exit', (code) => reject(new Error(`ody serve exited with ${String(code)}`)));
  });

  return createStreamTransport(proc.stdout, proc.stdin, dispatch, { framing: 'length-prefixed' });
}
```

```ts
// packages/node-sdk/src/rpc.ts
// 修改 SDKRpcClient 构造函数，增加 __external 分支；将 ready 从 readonly 改为可写
export class SDKRpcClient {
  readonly core: KimiCore;
  interactiveAgentId = MAIN_AGENT_ID;
  private ready: Promise<void>;
  private rpc: ResolvedCoreAPI | undefined;
  // ... 其他字段不变

  constructor(options: SDKRpcClientOptions = {}, __external = false) {
    if (__external) {
      const homeDir = resolveOdyHome(options.homeDir);
      const configPath = options.configPath;
      this.core = { homeDir, configPath } as KimiCore;
      this.ready = Promise.resolve();
      this.eventListeners = new Set();
      this.approvalHandlers = new Map();
      this.questionHandlers = new Map();
      this.openExternalHandlers = new Map();
      this.codeReviewProgressHandlers = new Map();
      return;
    }

    // 现有构造函数逻辑保持不变...
  }

  static async connect(options: SDKRpcClientConnectOptions): Promise<SDKRpcClient> {
    const instance = new SDKRpcClient(
      {
        homeDir: options.homeDir,
        configPath: options.configPath,
        skillDirs: options.skillDirs,
        telemetry: options.telemetry,
      },
      true,
    );

    const endpoint = createRPCEndpoint<SDKAPI, CoreAPI>();
    const transport = await createExternalTransport(options, endpoint.dispatch);
    endpoint.setTransport(transport);

    const clientApi = new ClientAPI(instance);
    instance.ready = endpoint.client(clientApi).then((rpc) => {
      instance.rpc = rpc;
    });
    await instance.ready;
    return instance;
  }

  // ... 其余方法不变
}
```

```ts
// packages/node-sdk/src/index.ts
// 在现有 export 中新增
export { SDKRpcClient } from '#/rpc';
export type {
  SDKRpcClientConnectOptions,
  SDKRpcClientOptions,
} from '#/rpc';
```

- [ ] Run it and verify it PASSES：

```bash
pnpm --filter @odysseythink/ody-code-sdk test packages/node-sdk/test/sdk-rpc-client-connect.test.ts
```

Expected：测试全绿，`createSession` 返回的 session id 为 `sdk-connect-session`。

- [ ] Commit：`git add -A && git commit -m "feat(node-sdk): add SDKRpcClient.connect for external transports"`

- [ ] Whole-tree typecheck：

```bash
pnpm -r typecheck
```

Expected：全绿。

---

### Task 9: SDKRpcClient.connect() UDS 与 token 测试

**Depends on:** Task 8

**Files:**
- Modify: `packages/node-sdk/test/sdk-rpc-client-connect.test.ts`

- [ ] Write the failing test：追加 UDS 连接测试与 TCP token 校验失败测试。

```ts
// 追加到 packages/node-sdk/test/sdk-rpc-client-connect.test.ts
import { createServer as createUnixServer, type Socket } from 'node:net';
import { ErrorCodes } from '@odysseythink/agent-core-shared';

describe('SDKRpcClient.connect transport variants', () => {
  it('connects over UDS and creates a session', async () => {
    const tmpDir = await mkdtemp(join(tmpdir(), 'ody-sdk-connect-'));
    await mkdir(join(tmpDir, 'sessions'), { recursive: true });
    const socketPath = join(tmpDir, 'test.sock');

    const server = createUnixServer((socket) => {
      createCoreServer(
        (dispatch) => createStreamTransport(socket, socket, dispatch),
        { homeDir: tmpDir },
      );
    });

    await new Promise<void>((resolve) => server.listen(socketPath, resolve));

    const client = await SDKRpcClient.connect({ transport: { socketPath } });
    const session = await client.createSession({ workDir: tmpDir, id: 'uds-session' });
    expect(session.id).toBe('uds-session');

    server.close();
  });

  it('fails TCP connect when token mismatch', async () => {
    const tmpDir = await mkdtemp(join(tmpdir(), 'ody-sdk-connect-'));
    await mkdir(join(tmpDir, 'sessions'), { recursive: true });

    const server = createServer((socket: Socket) => {
      createCoreServer(
        (dispatch) => createStreamTransport(socket, socket, dispatch),
        { homeDir: tmpDir },
      );
    });

    await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
    const port = (server.address() as AddressInfo).port;

    // Server expects no token, client sends wrong token → handshake mismatch handled by transport close
    await expect(
      SDKRpcClient.connect({ transport: { host: '127.0.0.1', port }, token: 'ody_wrong' }),
    ).rejects.toMatchObject({ code: ErrorCodes.TRANSPORT_CLOSED });

    server.close();
  });
});
```

- [ ] Run it and verify it FAILS：

```bash
pnpm --filter @odysseythink/ody-code-sdk test packages/node-sdk/test/sdk-rpc-client-connect.test.ts
```

Expected failure：新增测试尚未追加。

- [ ] Write the minimal implementation：将上述测试追加到文件；实现已在 Task 8 完成。

- [ ] Run it and verify it PASSES：

```bash
pnpm --filter @odysseythink/ody-code-sdk test packages/node-sdk/test/sdk-rpc-client-connect.test.ts
```

Expected：所有测试全绿。

- [ ] Commit：`git add -A && git commit -m "test(node-sdk): add UDS and token mismatch tests for SDKRpcClient.connect"`

- [ ] Part 2 全量回归：

```bash
pnpm --filter @odysseythink/ody-code-sdk test
pnpm -r typecheck
```

Expected：全绿。

---

## Local Self-Review

- [ ] 1. Spec-coverage table（本 Part）：`createCoreServer` 抽象 → Task 6-7；`SDKRpcClient.connect()` → Task 8-9；外部 transport 可调通 CoreAPI → Task 7/9。
- [ ] 2. Placeholder scan：本 Part 无 `TODO`/`TBD`/`implement later`；所有代码、命令、预期输出已完整给出。
- [ ] 3. No phantom tasks：每个 Task 均产出文件变更、测试/验证步骤与 commit；无 `--allow-empty`。
- [ ] 4. Dependency soundness：Task 6 依赖 Part 1 Task 2；Task 7 依赖 Task 6；Task 8 依赖 Task 6；Task 9 依赖 Task 8；无向后引用。
- [ ] 5. Caller & build soundness：Task 6 修改 `coreWorkerMain` 是内部重构，搜索 `coreWorkerMain` 调用方（`packages/node-sdk/test/core-worker.*.test.ts`）并确保其仍通过；Task 8 修改 `SDKRpcClient` 构造函数新增可选 `__external` 参数，不会破坏现有调用；两个 Task 末尾均运行 `pnpm -r typecheck`。
- [ ] 6. Test-the-risk：`createCoreServer` 有通过真实 TCP 调用 `createSession` 的测试；`SDKRpcClient.connect` 有 TCP/UDS 端到端与 token 校验失败测试；transport 关闭后 pending 行为由底层 transport 测试覆盖。
- [ ] 7. Type consistency：`CoreServerOptions`、`SDKRpcClientConnectOptions`、`ReadyMessage` 的类型与属性名在本 Part 内一致，并为 Part 3/4 复用。
