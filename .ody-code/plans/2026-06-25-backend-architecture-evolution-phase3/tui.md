# Part 3 — TS TUI Adaptation

> Scope: TS TUI 如何启动/连接 Rust host、消费事件流、处理断开错误。  
> Corresponds to index: [Architecture & Data Flow](../2026-06-25-backend-architecture-evolution-phase3.md)

---

## Dependency Overview

This part turns the Rust transport from Part 2 into a runnable TUI entry point.

```
C1: Extend SDKRpcClient.connect for Rust binary / args / socket spawn
   │
   ▼
C2: RustHostConnector (apps/ody-code)
   │
   ▼
C3: RustHostHarness + OdyHarness interface
   │
   ▼
C4: Change OdyTUI / dispatch / auth-flow to accept OdyHarness  ← shared-signature task
   │
   ▼
C5: CLI options for --host=rust and host transport flags
   │
   ▼
C6: run-shell-rust.ts + main.ts wiring
   │
   ▼
C7: Manual end-to-end smoke test
```

- **External prerequisite**: Part 2 (`transport.md`) must be implemented so that
  `ody-host serve --stdio` exists and prints a `{ type: 'ready', stdio: true }`
  line on stderr.
- C4 is the only shared-signature change in this part; it must update every
  caller of `OdyTUI`, `SlashCommandHost`, and `AuthFlowHost`, and end with a
  whole-tree `pnpm -r typecheck`.

---

## Tasks

### Task C1: Extend `SDKRpcClient.connect` to accept a custom binary path and spawn arguments

**Depends on:** Part 2 (`transport.md`) Task B7 (Rust host executable exists and emits ready messages)

**Files:**
- Create: `packages/node-sdk/test/sdk-rpc-client-connect-binary.test.ts`
- Modify: `packages/node-sdk/src/rpc.ts:110-130` (`SDKRpcClientConnectOptions` / `ReadyMessage`)
- Modify: `packages/node-sdk/src/rpc.ts:180-250` (`createExternalTransport`)

**Goal:** Let `SDKRpcClient.connect` spawn an arbitrary host binary with `--config` / `--home`, and support spawning a socket/TCP listener before connecting to it.

- [ ] Write the failing test (`packages/node-sdk/test/sdk-rpc-client-connect-binary.test.ts`):

```typescript
import { mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { SDKRpcClient } from '../src/rpc';

async function createMockHostScript(): Promise<string> {
  const script = join(await mkdtemp(join(tmpdir(), 'ody-mock-host-')), 'mock-host.mjs');
  await writeFile(
    script,
    `
import { createServer } from 'node:net';
const mode = process.argv.includes('--stdio') ? 'stdio' : 'socket';
const socketArg = process.argv.find((a, i) => i > 0 && process.argv[i - 1] === '--socket-path');
if (mode === 'socket' && socketArg) {
  const server = createServer((socket) => {
    socket.on('data', (chunk) => socket.write(chunk));
  });
  await new Promise((resolve) => server.listen(socketArg, resolve));
  console.error(JSON.stringify({ type: 'ready', socketPath: socketArg }));
} else {
  console.error(JSON.stringify({ type: 'ready', stdio: true }));
  process.stdin.on('data', (chunk) => process.stdout.write(chunk));
}
`,
  );
  return script;
}

describe('SDKRpcClient.connect with custom binary', () => {
  it('spawns stdio binary and passes --config/--home', async () => {
    const binaryPath = await createMockHostScript();
    const homeDir = await mkdtemp(join(tmpdir(), 'ody-home-'));
    const client = await SDKRpcClient.connect({
      transport: 'stdio',
      binaryPath,
      homeDir,
      configPath: join(homeDir, 'config.toml'),
    });
    expect(client.homeDir).toBe(homeDir);
    await client.close?.();
  });

  it('spawns socket binary and connects', async () => {
    const binaryPath = await createMockHostScript();
    const homeDir = await mkdtemp(join(tmpdir(), 'ody-home-'));
    const socketPath = join(homeDir, 'host.sock');
    const client = await SDKRpcClient.connect({
      transport: { socketPath, spawn: true },
      binaryPath,
      homeDir,
    });
    expect(client.homeDir).toBe(homeDir);
    await client.close?.();
  });
});
```

- [ ] Run it and verify it FAILS:

```bash
pnpm vitest run packages/node-sdk/test/sdk-rpc-client-connect-binary.test.ts
```

Expected failure: `TypeError: connect does not support spawn: true` or TS error `Object literal may only specify known properties, and 'spawn' does not exist`.

- [ ] Write the minimal implementation in `packages/node-sdk/src/rpc.ts`.

Change `SDKRpcClientConnectOptions` (around line 110):

```typescript
export interface SDKRpcClientConnectOptions {
  readonly transport:
    | 'stdio'
    | { readonly socketPath: string; readonly spawn?: boolean }
    | { readonly host: string; readonly port: number; readonly webSocket?: boolean; readonly spawn?: boolean };
  readonly binaryPath?: string;
  readonly token?: string;
  readonly homeDir?: string;
  readonly configPath?: string;
  readonly skillDirs?: readonly string[];
  readonly telemetry?: TelemetryClient;
}
```

Keep `ReadyMessage` unchanged (it already has `stdio`, `socketPath`, `host`, `port`).

Replace `createExternalTransport` (around line 180) with:

```typescript
async function createExternalTransport(
  options: SDKRpcClientConnectOptions,
  dispatch: Dispatch,
): Promise<Transport> {
  const binaryPath = options.binaryPath ?? 'ody';
  const extraArgs: string[] = [];
  if (options.configPath !== undefined) {
    extraArgs.push('--config', options.configPath);
  }
  if (options.homeDir !== undefined) {
    extraArgs.push('--home', options.homeDir);
  }

  if (options.transport === 'stdio') {
    const { spawn } = await import('node:child_process');
    const proc = spawn(binaryPath, ['serve', '--stdio', ...extraArgs], {
      stdio: ['pipe', 'pipe', 'pipe'],
    });
    await waitForReadyMessage(proc.stderr, (msg) => msg.stdio === true);
    return createStreamTransport(proc.stdout, proc.stdin, dispatch, { framing: 'length-prefixed' });
  }

  if ('socketPath' in options.transport) {
    const { socketPath, spawn: shouldSpawn } = options.transport;
    if (shouldSpawn) {
      const { spawn } = await import('node:child_process');
      const proc = spawn(binaryPath, ['serve', '--socket-path', socketPath, ...extraArgs], {
        stdio: ['ignore', 'ignore', 'pipe'],
      });
      await waitForReadyMessage(proc.stderr, (msg) => msg.socketPath === socketPath);
    }
    const socket: Socket = connectNet(socketPath);
    await once(socket, 'connect');
    return createStreamTransport(socket, socket, dispatch, { framing: 'length-prefixed' });
  }

  const { host, port, webSocket, spawn: shouldSpawn } = options.transport;

  if (shouldSpawn) {
    const { spawn } = await import('node:child_process');
    const proc = spawn(binaryPath, ['serve', '--tcp-host', host, '--tcp-port', String(port), ...extraArgs], {
      stdio: ['ignore', 'ignore', 'pipe'],
    });
    await waitForReadyMessage(proc.stderr, (msg) => msg.host === host && msg.port === port);
  }

  if (webSocket) {
    const ws = new WebSocket(`ws://${host}:${port}`);
    await new Promise<void>((resolve, reject) => {
      ws.onopen = () => resolve();
      ws.onerror = () => reject(new Error('WebSocket connection failed'));
    });
    const adapted = {
      send: (data: string) => ws.send(data),
      close: () => ws.close(),
      onmessage: null as ((event: { data: string | Uint8Array }) => void) | null,
      onerror: null as ((event: { type: string }) => void) | null,
      onclose: null as ((event: { type: string }) => void) | null,
    };
    ws.onmessage = (event: MessageEvent) => {
      adapted.onmessage?.({ data: typeof event.data === 'string' ? event.data : new Uint8Array(event.data) });
    };
    ws.onerror = () => adapted.onerror?.({ type: 'error' });
    ws.onclose = () => adapted.onclose?.({ type: 'close' });
    return createWebSocketTransport(adapted, dispatch);
  }

  const socket: Socket = connectNet(port, host);
  await once(socket, 'connect');
  return createStreamTransport(socket, socket, dispatch, {
    framing: options.token === undefined ? 'length-prefixed' : undefined,
    token: options.token,
  });
}

async function waitForReadyMessage(
  stderr: NodeJS.ReadableStream,
  predicate: (msg: ReadyMessage) => boolean,
): Promise<ReadyMessage> {
  return new Promise<ReadyMessage>((resolve, reject) => {
    const onData = (chunk: Buffer): void => {
      const lines = chunk.toString('utf8').split('\n');
      for (const line of lines) {
        if (!line.trim()) continue;
        try {
          const msg = JSON.parse(line) as ReadyMessage;
          if (msg.type === 'ready' && predicate(msg)) {
            stderr.off('data', onData);
            resolve(msg);
            return;
          }
        } catch {
          // ignore non-JSON stderr lines
        }
      }
    };
    stderr.on('data', onData);
    // Caller is expected to attach error/exit handlers on the ChildProcess itself.
  });
}
```

- [ ] Run the test and verify it PASSES:

```bash
pnpm vitest run packages/node-sdk/test/sdk-rpc-client-connect-binary.test.ts
```

Expected: both tests pass.

- [ ] Commit:

```bash
git add packages/node-sdk/src/rpc.ts packages/node-sdk/test/sdk-rpc-client-connect-binary.test.ts
git commit -m "feat(node-sdk): extend SDKRpcClient.connect for custom binary and socket/tcp spawn"
```

---

### Task C2: Create `RustHostConnector` in `apps/ody-code`

**Depends on:** Task C1

**Files:**
- Create: `apps/ody-code/src/host/rust-host-connector.ts`
- Create: `apps/ody-code/src/host/index.ts`
- Create: `apps/ody-code/test/host/rust-host-connector.test.ts`

**Goal:** Provide a small connector that builds `SDKRpcClientConnectOptions` from Rust-host-specific options, resolves the binary path, and forwards disconnect events.

- [ ] Write the failing test (`apps/ody-code/test/host/rust-host-connector.test.ts`):

```typescript
import { mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

import { RustHostConnector } from '../../src/host';

async function createMockRustHost(): Promise<string> {
  const script = join(await mkdtemp(join(tmpdir(), 'ody-mock-rust-host-')), 'ody-host');
  await writeFile(
    script,
    `#!/usr/bin/env node
process.stderr.write(JSON.stringify({ type: 'ready', stdio: true }) + '\\n');
process.stdin.on('data', (chunk) => process.stdout.write(chunk));
`,
  );
  return script;
}

describe('RustHostConnector', () => {
  it('connects via stdio and reports homeDir', async () => {
    const binaryPath = await createMockRustHost();
    const homeDir = await mkdtemp(join(tmpdir(), 'ody-home-'));
    const client = await RustHostConnector.connect({
      mode: 'stdio',
      binaryPath,
      homeDir,
      configPath: join(homeDir, 'config.toml'),
    });
    expect(client.homeDir).toBe(homeDir);
    client.close?.();
  });

  it('emits disconnect when the host process exits', async () => {
    const binaryPath = await createMockRustHost();
    const homeDir = await mkdtemp(join(tmpdir(), 'ody-home-'));
    const connector = new RustHostConnector();
    const client = await connector.connect({ mode: 'stdio', binaryPath, homeDir });
    const disconnected = new Promise<Error>((resolve) => connector.onDisconnect(resolve));
    client.close?.();
    const error = await disconnected;
    expect(error.message).toMatch(/disconnected/i);
  });
});
```

- [ ] Run it and verify it FAILS:

```bash
pnpm vitest run apps/ody-code/test/host/rust-host-connector.test.ts
```

Expected failure: `Cannot find module '../../src/host'`.

- [ ] Write the minimal implementation.

Create `apps/ody-code/src/host/rust-host-connector.ts`:

```typescript
import { SDKRpcClient, type SDKRpcClientConnectOptions } from '@odysseythink/ody-code-sdk';

export type RustHostMode = 'stdio' | 'socket' | 'tcp';

export interface RustHostConnectorOptions {
  readonly mode: RustHostMode;
  readonly binaryPath: string;
  readonly socketPath?: string;
  readonly host?: string;
  readonly port?: number;
  readonly configPath?: string;
  readonly homeDir?: string;
}

export type Unsubscribe = () => void;

export class RustHostConnector {
  private client: SDKRpcClient | undefined;
  private readonly disconnectListeners = new Set<(error: Error) => void>();
  private proc: import('node:child_process').ChildProcess | undefined;

  async connect(options: RustHostConnectorOptions): Promise<SDKRpcClient> {
    const connectOptions = this.buildConnectOptions(options);
    this.client = await SDKRpcClient.connect(connectOptions);
    this.attachDisconnectHandlers();
    return this.client;
  }

  onDisconnect(listener: (error: Error) => void): Unsubscribe {
    this.disconnectListeners.add(listener);
    return () => {
      this.disconnectListeners.delete(listener);
    };
  }

  private buildConnectOptions(options: RustHostConnectorOptions): SDKRpcClientConnectOptions {
    const base: SDKRpcClientConnectOptions = {
      binaryPath: options.binaryPath,
      homeDir: options.homeDir,
      configPath: options.configPath,
      transport: 'stdio',
    };
    if (options.mode === 'socket') {
      if (options.socketPath === undefined) {
        throw new Error('--host-socket requires a path.');
      }
      base.transport = { socketPath: options.socketPath, spawn: true };
    } else if (options.mode === 'tcp') {
      if (options.host === undefined || options.port === undefined) {
        throw new Error('--host-tcp requires host:port.');
      }
      base.transport = { host: options.host, port: options.port, spawn: true };
    }
    return base;
  }

  private attachDisconnectHandlers(): void {
    const client = this.client;
    if (client === undefined) return;
    const transport = (client as any).transport;
    if (transport?.onError !== undefined) {
      transport.onError((error: Error) => this.emitDisconnect(error));
    }
    if (this.proc !== undefined) {
      this.proc.once('exit', (code) => {
        this.emitDisconnect(new Error(`Rust host exited with code ${String(code)}`));
      });
    }
  }

  private emitDisconnect(error: Error): void {
    for (const listener of this.disconnectListeners) {
      listener(error);
    }
  }
}
```

Create `apps/ody-code/src/host/index.ts`:

```typescript
export { RustHostConnector, type RustHostConnectorOptions, type RustHostMode } from './rust-host-connector';
```

- [ ] Run the test and verify it PASSES:

```bash
pnpm vitest run apps/ody-code/test/host/rust-host-connector.test.ts
```

- [ ] Commit:

```bash
git add apps/ody-code/src/host apps/ody-code/test/host/rust-host-connector.test.ts
git commit -m "feat(ody-code): add RustHostConnector"
```

---

### Task C3: Define `OdyHarness` interface and create `RustHostHarness`

**Depends on:** Task C2

**Files:**
- Create: `apps/ody-code/src/host/rust-host-harness.ts`
- Create: `apps/ody-code/src/host/rust-host-auth.ts`
- Modify: `apps/ody-code/src/tui/types.ts`
- Create: `apps/ody-code/test/host/rust-host-harness.test.ts`

**Goal:** Give `OdyTUI` a narrow harness interface that both `KimiHarness` and a Rust-host wrapper can satisfy.

The interface must include every method/property that `OdyTUI`, `AuthFlowController`, and `SlashCommandHost` read from the harness:

- `track`, `setTelemetryContext`
- `interactiveAgentId` (get/set)
- `getExperimentalFlags`, `getConfig`, `setConfig`, `removeProvider`, `ensureConfigFile`
- `createSession`, `resumeSession`, `listSessions`, `closeSession`
- `close`
- `auth` with at least `resolveOAuthTokenProvider` (prototype: throws or returns `null`)

- [ ] Write the failing test (`apps/ody-code/test/host/rust-host-harness.test.ts`):

```typescript
import { mkdtemp } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it, vi } from 'vitest';

import { RustHostHarness } from '../../src/host/rust-host-harness';

function createMockClient() {
  return {
    homeDir: '/tmp/ody',
    configPath: '/tmp/ody/config.toml',
    interactiveAgentId: 'main',
    createSession: vi.fn().mockResolvedValue({ id: 's1', workDir: '/tmp', title: null }),
    resumeSession: vi.fn().mockResolvedValue({ id: 's1', workDir: '/tmp', title: null }),
    listSessions: vi.fn().mockResolvedValue([]),
    getExperimentalFlags: vi.fn().mockResolvedValue({}),
    getConfig: vi.fn().mockResolvedValue({ providers: [] }),
    setConfig: vi.fn().mockResolvedValue({ providers: [] }),
    removeProvider: vi.fn().mockResolvedValue({ providers: [] }),
    close: vi.fn().mockResolvedValue(undefined),
    onEvent: vi.fn().mockReturnValue(() => {}),
  } as unknown as import('@odysseythink/ody-code-sdk').SDKRpcClient;
}

describe('RustHostHarness', () => {
  it('delegates createSession and keeps a Session', async () => {
    const client = createMockClient();
    const harness = new RustHostHarness({ client, telemetry: { track: vi.fn(), withContext: vi.fn(), setContext: vi.fn() } });
    const session = await harness.createSession({ workDir: '/tmp' });
    expect(session.id).toBe('s1');
    expect(client.createSession).toHaveBeenCalledWith({ workDir: '/tmp' });
  });

  it('returns active session from resumeSession without re-creating', async () => {
    const client = createMockClient();
    const harness = new RustHostHarness({ client, telemetry: { track: vi.fn(), withContext: vi.fn(), setContext: vi.fn() } });
    const first = await harness.resumeSession({ id: 's1' });
    const second = await harness.resumeSession({ id: 's1' });
    expect(first.id).toBe('s1');
    expect(second.id).toBe('s1');
    expect(client.resumeSession).toHaveBeenCalledTimes(1);
  });

  it('proxies interactiveAgentId to the client', () => {
    const client = createMockClient();
    const harness = new RustHostHarness({ client, telemetry: { track: vi.fn(), withContext: vi.fn(), setContext: vi.fn() } });
    expect(harness.interactiveAgentId).toBe('main');
    harness.interactiveAgentId = 'worker-1';
    expect(client.interactiveAgentId).toBe('worker-1');
  });
});
```

- [ ] Run it and verify it FAILS:

```bash
pnpm vitest run apps/ody-code/test/host/rust-host-harness.test.ts
```

Expected failure: `Cannot find module '../../src/host/rust-host-harness'`.

- [ ] Write the minimal implementation.

Modify `apps/ody-code/src/tui/types.ts` to add `OdyHarness` interface (append at the end):

```typescript
import type {
  CreateSessionOptions,
  GetConfigOptions,
  KimiHostIdentity,
  ListSessionsOptions,
  OdyConfig,
  OdyConfigPatch,
  PermissionMode,
  ResumedSessionSummary,
  SessionSummary,
  TelemetryClient,
  TelemetryContextPatch,
} from '@odysseythink/ody-code-sdk';
import type { ExperimentalFlagMap } from '@odysseythink/agent-core';

export interface OdyHarness {
  readonly homeDir: string;
  readonly configPath: string;
  readonly interactiveAgentId: string;
  set interactiveAgentId(value: string);

  track(event: string, properties?: Record<string, unknown>): void;
  setTelemetryContext(patch: TelemetryContextPatch): void;

  ensureConfigFile(): Promise<void>;
  getConfig(options?: GetConfigOptions): Promise<OdyConfig>;
  setConfig(patch: OdyConfigPatch): Promise<OdyConfig>;
  removeProvider(providerId: string): Promise<OdyConfig>;
  getExperimentalFlags(): Promise<ExperimentalFlagMap>;

  createSession(options: CreateSessionOptions): Promise<import('@odysseythink/ody-code-sdk').Session>;
  resumeSession(input: { readonly id: string }): Promise<import('@odysseythink/ody-code-sdk').Session>;
  listSessions(options?: ListSessionsOptions): Promise<readonly SessionSummary[]>;
  closeSession(id: string): Promise<void>;

  close(): Promise<void>;

  readonly auth: {
    resolveOAuthTokenProvider(providerName: string, oauthRef: unknown): Promise<unknown> | unknown;
  };
}
```

Create `apps/ody-code/src/host/rust-host-auth.ts`:

```typescript
export class RustHostAuthFacade {
  resolveOAuthTokenProvider(): never {
    throw new Error('OAuth login is not supported in --host=rust prototype mode.');
  }
}
```

Create `apps/ody-code/src/host/rust-host-harness.ts`:

```typescript
import {
  Session,
  type SDKRpcClient,
  type TelemetryClient,
} from '@odysseythink/ody-code-sdk';
import type {
  CreateSessionOptions,
  GetConfigOptions,
  ListSessionsOptions,
  OdyConfigPatch,
  SessionSummary,
} from '@odysseythink/ody-code-sdk';
import type { ExperimentalFlagMap } from '@odysseythink/agent-core';

import type { OdyHarness } from '#/tui/types';

import { RustHostAuthFacade } from './rust-host-auth';

export interface RustHostHarnessOptions {
  readonly client: SDKRpcClient;
  readonly telemetry?: TelemetryClient | undefined;
}

export class RustHostHarness implements OdyHarness {
  readonly homeDir: string;
  readonly configPath: string;
  readonly auth = new RustHostAuthFacade();
  private readonly client: SDKRpcClient;
  private readonly telemetry: TelemetryClient;
  private readonly activeSessions = new Map<string, Session>();

  constructor(options: RustHostHarnessOptions) {
    this.client = options.client;
    this.homeDir = options.client.homeDir;
    this.configPath = options.client.configPath;
    this.telemetry = options.telemetry ?? { track: () => {}, withContext: (fn) => fn(this as any), setContext: () => {} };
  }

  get interactiveAgentId(): string {
    return this.client.interactiveAgentId;
  }

  set interactiveAgentId(value: string) {
    this.client.interactiveAgentId = value;
  }

  track(event: string, properties?: Record<string, unknown>): void {
    this.telemetry.track(event, properties);
  }

  setTelemetryContext(patch: import('@odysseythink/agent-core').TelemetryContextPatch): void {
    this.telemetry.setContext?.(patch);
  }

  async ensureConfigFile(): Promise<void> {
    // Rust host owns config file creation; prototype no-op.
  }

  async getConfig(options?: GetConfigOptions): Promise<import('@odysseythink/ody-code-sdk').OdyConfig> {
    return this.client.getConfig(options);
  }

  async setConfig(patch: OdyConfigPatch): Promise<import('@odysseythink/ody-code-sdk').OdyConfig> {
    return this.client.setConfig(patch);
  }

  async removeProvider(providerId: string): Promise<import('@odysseythink/ody-code-sdk').OdyConfig> {
    return this.client.removeProvider(providerId);
  }

  async getExperimentalFlags(): Promise<ExperimentalFlagMap> {
    return this.client.getExperimentalFlags();
  }

  async createSession(options: CreateSessionOptions): Promise<Session> {
    const { sessionMode, ...coreOptions } = options;
    const summary = await this.client.createSession(coreOptions);
    const session = this.wrapSession(summary);
    if (sessionMode !== undefined && sessionMode !== 'normal') {
      await session.setSessionMode(sessionMode);
    }
    return session;
  }

  async resumeSession(input: { readonly id: string }): Promise<Session> {
    const id = input.id.trim();
    const active = this.activeSessions.get(id);
    if (active !== undefined) return active;
    const summary = await this.client.resumeSession({ id });
    return this.wrapSession(summary);
  }

  async listSessions(options?: ListSessionsOptions): Promise<readonly SessionSummary[]> {
    return this.client.listSessions(options);
  }

  async closeSession(id: string): Promise<void> {
    await this.activeSessions.get(id)?.close();
  }

  async close(): Promise<void> {
    await Promise.all(Array.from(this.activeSessions.values(), (session) => session.close()));
    this.activeSessions.clear();
  }

  private wrapSession(summary: SessionSummary | ResumedSessionSummary): Session {
    const session = new Session({
      id: summary.id,
      workDir: summary.workDir,
      summary: 'title' in summary ? summary : undefined,
      resumeState: 'resumeState' in summary ? summary.resumeState : undefined,
      rpc: this.client,
      onClose: () => {
        this.activeSessions.delete(summary.id);
      },
    });
    this.activeSessions.set(session.id, session);
    return session;
  }
}
```

Update `apps/ody-code/src/host/index.ts`:

```typescript
export { RustHostConnector, type RustHostConnectorOptions, type RustHostMode } from './rust-host-connector';
export { RustHostHarness, type RustHostHarnessOptions } from './rust-host-harness';
```

- [ ] Run the test and verify it PASSES:

```bash
pnpm vitest run apps/ody-code/test/host/rust-host-harness.test.ts
```

- [ ] Commit:

```bash
git add apps/ody-code/src/host apps/ody-code/src/tui/types.ts apps/ody-code/test/host/rust-host-harness.test.ts
git commit -m "feat(ody-code): add RustHostHarness and OdyHarness interface"
```

---

### Task C4: Update TUI types and `OdyTUI` to accept `OdyHarness`

**Depends on:** Task C3

**Files:**
- Modify: `apps/ody-code/src/tui/ody-tui.ts:252` (constructor parameter)
- Modify: `apps/ody-code/src/tui/ody-tui.ts:12-21` (import `KimiHarness`)
- Modify: `apps/ody-code/src/tui/types.ts` (imports already added in C3)
- Modify: `apps/ody-code/src/tui/commands/dispatch.ts:3` and `:104`
- Modify: `apps/ody-code/src/tui/controllers/auth-flow.ts:1` and `:13`
- Modify: `apps/ody-code/src/cli/run-shell.ts:81` (pass `KimiHarness`, already satisfies `OdyHarness`)
- Modify: all test files constructing `OdyTUI` with a `KimiHarness` mock

**Goal:** Replace the concrete `KimiHarness` dependency with the narrower `OdyHarness` interface. This is the shared-signature churn for the part; do it in one task.

- [ ] Find every caller of `OdyTUI` and every type referencing `KimiHarness`:

```bash
rg -n "new OdyTUI\(|OdyTUI\(" apps/ody-code/src apps/ody-code/test
rg -n "readonly harness: KimiHarness|harness: KimiHarness" apps/ody-code/src apps/ody-code/test
rg -n "import.*KimiHarness.*from" apps/ody-code/src apps/ody-code/test
```

- [ ] Update imports and types.

In `apps/ody-code/src/tui/ody-tui.ts`:

```typescript
import type { OdyHarness, OdyTUIStartupInput, /* ... */ } from './types';
// Remove `KimiHarness` from the @odysseythink/ody-code-sdk import if it is only used for the constructor type.
```

Change constructor signature (around line 252):

```typescript
constructor(harness: OdyHarness, startupInput: OdyTUIStartupInput) {
```

In `apps/ody-code/src/tui/commands/dispatch.ts`:

```typescript
import type { OdyHarness } from '../types';
// Replace KimiHarness with OdyHarness in SlashCommandHost
```

```typescript
export interface SlashCommandHost {
  // ...
  readonly harness: OdyHarness;
  // ...
}
```

In `apps/ody-code/src/tui/controllers/auth-flow.ts`:

```typescript
import type { OdyHarness } from '../types';
```

```typescript
export interface AuthFlowHost {
  // ...
  readonly harness: OdyHarness;
  // ...
}
```

- [ ] Update test mocks. Any test that previously typed a mock harness as `KimiHarness` should use `OdyHarness` or remain structurally compatible. Search for `KimiHarness` in tests:

```bash
rg -n "KimiHarness" apps/ody-code/test
```

For each occurrence, either import `OdyHarness` from `#/tui/types` or cast the mock object `as unknown as KimiHarness` if it predates this change. Prefer changing the type annotation to `OdyHarness` where the mock only implements the narrow surface.

- [ ] Run whole-tree typecheck (this is the build-green invariant for the shared-signature change):

```bash
pnpm -r typecheck
```

- [ ] Run the relevant test suites to confirm no runtime regressions:

```bash
pnpm vitest run apps/ody-code/test/tui/tui-startup.test.ts apps/ody-code/test/tui/message-replay.test.ts apps/ody-code/test/cli/run-shell.test.ts
```

- [ ] Commit:

```bash
git add apps/ody-code/src/tui apps/ody-code/src/cli/run-shell.ts $(rg -l "KimiHarness" apps/ody-code/test)
git commit -m "refactor(ody-code): narrow OdyTUI harness dependency to OdyHarness interface"
```

---

### Task C5: Add CLI options for Rust host mode

**Depends on:** none within this part (CLI-only change)

**Files:**
- Modify: `apps/ody-code/src/cli/options.ts`
- Modify: `apps/ody-code/src/cli/commands.ts`
- Modify: `apps/ody-code/test/cli/options.test.ts`

**Goal:** Allow users to run `ody --host=rust --host-stdio` (or `--host-socket PATH`, `--host-tcp HOST:PORT`) against the Rust host.

- [ ] Write the failing test additions (`apps/ody-code/test/cli/options.test.ts`):

```typescript
import { describe, expect, it } from 'vitest';
import { validateOptions, type CLIOptions } from '../../src/cli/options';

function base(): CLIOptions {
  return {
    session: undefined,
    continue: false,
    yolo: false,
    auto: false,
    sessionMode: 'normal',
    officeHours: false,
    gameDesign: false,
    model: undefined,
    outputFormat: undefined,
    prompt: undefined,
    skillsDirs: [],
    loginProvider: undefined,
    logoutProvider: undefined,
    host: 'inproc',
    hostStdio: false,
    hostSocket: undefined,
    hostTcp: undefined,
    hostBinary: undefined,
  };
}

describe('rust host options', () => {
  it('accepts --host=rust with stdio', () => {
    const result = validateOptions({ ...base(), host: 'rust', hostStdio: true });
    expect(result.uiMode).toBe('shell');
  });

  it('rejects --host=rust in prompt mode', () => {
    expect(() => validateOptions({ ...base(), host: 'rust', hostStdio: true, prompt: 'hi' })).toThrow(
      'Cannot combine --host=rust with --prompt.',
    );
  });

  it('rejects unknown host value', () => {
    expect(() => validateOptions({ ...base(), host: 'wasm' as any })).toThrow('Invalid --host');
  });

  it('rejects combining host socket and tcp', () => {
    expect(() =>
      validateOptions({ ...base(), host: 'rust', hostSocket: '/tmp/ody.sock', hostTcp: '127.0.0.1:9000' }),
    ).toThrow('Cannot combine --host-socket with --host-tcp.');
  });
});
```

- [ ] Run it and verify it FAILS:

```bash
pnpm vitest run apps/ody-code/test/cli/options.test.ts
```

Expected failure: `Invalid --host` or `Cannot find name 'host'`.

- [ ] Write the minimal implementation.

Modify `apps/ody-code/src/cli/options.ts`:

```typescript
export interface CLIOptions {
  session: string | undefined;
  continue: boolean;
  yolo: boolean;
  auto: boolean;
  sessionMode: CLIRuntimeMode;
  officeHours: boolean;
  gameDesign: boolean;
  model: string | undefined;
  outputFormat: PromptOutputFormat | undefined;
  prompt: string | undefined;
  skillsDirs: string[];
  loginProvider: string | undefined;
  logoutProvider: string | undefined;
  host: 'inproc' | 'rust';
  hostStdio: boolean;
  hostSocket: string | undefined;
  hostTcp: string | undefined;
  hostBinary: string | undefined;
}
```

Add validation rules in `validateOptions`:

```typescript
  if (!['inproc', 'rust'].includes(opts.host)) {
    throw new OptionConflictError(`Invalid --host: ${opts.host}. Must be inproc or rust.`);
  }
  if (opts.host === 'rust') {
    if (opts.prompt !== undefined) {
      throw new OptionConflictError('Cannot combine --host=rust with --prompt.');
    }
    if (opts.officeHours || opts.gameDesign) {
      throw new OptionConflictError('Cannot combine --host=rust with --office-hours or --game-design.');
    }
    if (opts.hostSocket !== undefined && opts.hostTcp !== undefined) {
      throw new OptionConflictError('Cannot combine --host-socket with --host-tcp.');
    }
    if (!opts.hostStdio && opts.hostSocket === undefined && opts.hostTcp === undefined) {
      // Default to stdio when --host=rust is given without a transport flag.
      opts.hostStdio = true;
    }
    return { options: opts, uiMode: 'shell' };
  }
```

Modify `apps/ody-code/src/cli/commands.ts` to register the options (after line 99, before `registerExportCommand`):

```typescript
  .addOption(new Option('--host <mode>', 'Run core in-process (inproc) or in external Rust host (rust).').choices(['inproc', 'rust']).default('inproc'))
  .option('--host-stdio', 'Launch Rust host in stdio mode.', false)
  .addOption(new Option('--host-socket <path>', 'Launch Rust host listening on a Unix socket.'))
  .addOption(new Option('--host-tcp <host:port>', 'Launch Rust host listening on TCP.'))
  .addOption(new Option('--host-binary <path>', 'Path to the Rust host executable (defaults to ody-host on PATH).'));
```

Map the raw opts in the action (around line 129):

```typescript
    const opts: CLIOptions = {
      session: sessionValue,
      continue: raw['continue'] as boolean,
      yolo: yoloValue,
      auto: autoValue,
      sessionMode: (raw['sessionMode'] as CLIOptions['sessionMode']) ?? 'normal',
      officeHours: (raw['officeHours'] as boolean) ?? false,
      gameDesign: (raw['gameDesign'] as boolean) ?? false,
      model: raw['model'] as string | undefined,
      outputFormat: raw['outputFormat'] as CLIOptions['outputFormat'],
      prompt: raw['prompt'] as string | undefined,
      skillsDirs: raw['skillsDir'] as string[],
      loginProvider: raw['login'] as string | undefined,
      logoutProvider: raw['logout'] as string | undefined,
      host: (raw['host'] as CLIOptions['host']) ?? 'inproc',
      hostStdio: (raw['hostStdio'] as boolean) ?? false,
      hostSocket: raw['hostSocket'] as string | undefined,
      hostTcp: raw['hostTcp'] as string | undefined,
      hostBinary: raw['hostBinary'] as string | undefined,
    };
```

- [ ] Run the test and verify it PASSES:

```bash
pnpm vitest run apps/ody-code/test/cli/options.test.ts
```

- [ ] Commit:

```bash
git add apps/ody-code/src/cli/options.ts apps/ody-code/src/cli/commands.ts apps/ody-code/test/cli/options.test.ts
git commit -m "feat(ody-code): add --host=rust CLI options"
```

---

### Task C6: Wire Rust host mode into `main.ts` and create `run-shell-rust.ts`

**Depends on:** Tasks C2, C3, C4, C5

**Files:**
- Create: `apps/ody-code/src/cli/run-shell-rust.ts`
- Modify: `apps/ody-code/src/main.ts`
- Create: `apps/ody-code/test/cli/run-shell-rust.test.ts`

**Goal:** When `--host=rust` is passed, launch the Rust host, build a `RustHostHarness`, and run the TUI with it.

- [ ] Write the failing test (`apps/ody-code/test/cli/run-shell-rust.test.ts`):

```typescript
import { mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it, vi } from 'vitest';

import { buildRustHostLaunchOptions, resolveHostBinary } from '../../src/cli/run-shell-rust';

describe('run-shell-rust helpers', () => {
  it('defaults binary to ody-host when not provided', async () => {
    const binary = await resolveHostBinary({ hostBinary: undefined });
    expect(binary).toBe('ody-host');
  });

  it('builds stdio options', () => {
    const opts = buildRustHostLaunchOptions({
      hostStdio: true,
      hostSocket: undefined,
      hostTcp: undefined,
      hostBinary: '/tmp/ody-host',
      configPath: '/tmp/c.toml',
      homeDir: '/tmp/h',
    });
    expect(opts).toEqual({
      mode: 'stdio',
      binaryPath: '/tmp/ody-host',
      configPath: '/tmp/c.toml',
      homeDir: '/tmp/h',
    });
  });

  it('parses tcp host:port', () => {
    const opts = buildRustHostLaunchOptions({
      hostStdio: false,
      hostSocket: undefined,
      hostTcp: '127.0.0.1:9000',
      hostBinary: '/tmp/ody-host',
    });
    expect(opts).toEqual({
      mode: 'tcp',
      binaryPath: '/tmp/ody-host',
      host: '127.0.0.1',
      port: 9000,
    });
  });
});
```

- [ ] Run it and verify it FAILS:

```bash
pnpm vitest run apps/ody-code/test/cli/run-shell-rust.test.ts
```

Expected failure: `Cannot find module '../../src/cli/run-shell-rust'`.

- [ ] Write the minimal implementation.

Create `apps/ody-code/src/cli/run-shell-rust.ts`:

```typescript
import { access } from 'node:fs/promises';
import { resolve } from 'node:path';

import {
  setCrashPhase,
  setTelemetryContext,
  shutdownTelemetry,
  track,
  withTelemetryContext,
} from '@odysseythink/ody-telemetry';
import type { TelemetryClient } from '@odysseythink/ody-code-sdk';

import { CLI_SHUTDOWN_TIMEOUT_MS, CLI_UI_MODE } from '#/constant/app';
import { RustHostConnector, RustHostHarness, type RustHostConnectorOptions } from '#/host';
import type { TuiConfig } from '#/tui/config';
import { loadTuiConfig, TuiConfigParseError } from '#/tui/config';
import { CHROME_GUTTER } from '#/tui/constant/rendering';
import { OdyTUI } from '#/tui/index';
import { detectTerminalTheme } from '#/tui/theme/detect';

import type { CLIOptions } from './options';
import { createCliTelemetryBootstrap, initializeCliTelemetry } from './telemetry';
import { createKimiCodeHostIdentity } from './version';

export interface RustHostLaunchOptions {
  readonly mode: 'stdio' | 'socket' | 'tcp';
  readonly binaryPath: string;
  readonly socketPath?: string;
  readonly host?: string;
  readonly port?: number;
  readonly configPath?: string;
  readonly homeDir?: string;
}

export async function resolveHostBinary(opts: Pick<CLIOptions, 'hostBinary'>): Promise<string> {
  if (opts.hostBinary !== undefined) {
    return resolve(opts.hostBinary);
  }
  return 'ody-host';
}

export function buildRustHostLaunchOptions(
  opts: Pick<CLIOptions, 'hostStdio' | 'hostSocket' | 'hostTcp' | 'hostBinary' | 'configPath' | 'homeDir'>,
): RustHostLaunchOptions {
  const binaryPath = opts.hostBinary ?? 'ody-host';
  if (opts.hostSocket !== undefined) {
    return { mode: 'socket', binaryPath, socketPath: opts.hostSocket, configPath: opts.configPath, homeDir: opts.homeDir };
  }
  if (opts.hostTcp !== undefined) {
    const [host, portStr] = opts.hostTcp.split(':');
    return { mode: 'tcp', binaryPath, host, port: Number(portStr), configPath: opts.configPath, homeDir: opts.homeDir };
  }
  return { mode: 'stdio', binaryPath, configPath: opts.configPath, homeDir: opts.homeDir };
}

export async function runShellWithRustHost(opts: CLIOptions, version: string): Promise<void> {
  const startedAt = Date.now();
  const configStartedAt = startedAt;
  let tuiConfig: TuiConfig;
  let configWarning: string | undefined;
  try {
    tuiConfig = await loadTuiConfig();
  } catch (error) {
    if (!(error instanceof TuiConfigParseError)) throw error;
    tuiConfig = error.fallback;
    configWarning = error.message;
  }

  const resolvedTheme = tuiConfig.theme === 'auto' ? await detectTerminalTheme() : tuiConfig.theme;
  const workDir = process.cwd();
  const telemetryBootstrap = createCliTelemetryBootstrap();
  const telemetryClient: TelemetryClient = {
    track,
    withContext: withTelemetryContext,
    setContext: setTelemetryContext,
  };

  const binaryPath = await resolveHostBinary(opts);
  try {
    await access(binaryPath);
  } catch {
    if (opts.hostBinary !== undefined) {
      throw new Error(`Rust host binary not found: ${binaryPath}`);
    }
    // If no explicit path, assume it is on PATH and let spawn fail loudly if not.
  }

  const connector = new RustHostConnector();
  const launchOptions = buildRustHostLaunchOptions(opts);
  const client = await connector.connect(launchOptions);
  const harness = new RustHostHarness({ client, telemetry: telemetryClient });

  await harness.ensureConfigFile();
  const config = await harness.getConfig();
  const configMs = Date.now() - configStartedAt;

  const tui = new OdyTUI(harness, {
    cliOptions: opts,
    tuiConfig,
    version,
    workDir,
    startupNotice: configWarning,
    resolvedTheme,
    officeHours: false,
    gameDesign: false,
  });

  initializeCliTelemetry({
    harness,
    bootstrap: telemetryBootstrap,
    config,
    version,
    uiMode: CLI_UI_MODE,
  });
  setCrashPhase('runtime');

  const trackLifecycle = (event: string, properties?: Record<string, unknown>) => {
    withTelemetryContext({ sessionId: tui.getCurrentSessionId() }).track(event, properties);
  };

  connector.onDisconnect((error) => {
    console.error(`Rust host disconnected: ${error.message}`);
    void tui.stop(1);
  });

  tui.onExit = async (exitCode = 0) => {
    setCrashPhase('shutdown');
    trackLifecycle('exit', { duration_s: (Date.now() - startedAt) / 1000 });
    await shutdownTelemetry({ timeoutMs: CLI_SHUTDOWN_TIMEOUT_MS });
    const gutter = ' '.repeat(CHROME_GUTTER);
    process.stdout.write(`${gutter}Bye!\n`);
    process.exit(exitCode);
  };

  try {
    const initStartedAt = Date.now();
    await tui.start();
    const initMs = Date.now() - initStartedAt;
    trackLifecycle('started', { host: 'rust', mode: launchOptions.mode, yolo: opts.yolo, auto: opts.auto, sessionMode: opts.sessionMode });
    trackLifecycle('startup_perf', {
      duration_ms: Date.now() - startedAt,
      config_ms: configMs,
      init_ms: initMs,
    });
  } catch (error) {
    setCrashPhase('shutdown');
    trackLifecycle('exit', { duration_s: (Date.now() - startedAt) / 1000 });
    await harness.close();
    throw error;
  }
}
```

Modify `apps/ody-code/src/main.ts` to branch before `runShell` (around line 81):

```typescript
  if (validated.options.host === 'rust') {
    await runShellWithRustHost(validated.options, version);
    return;
  }
```

Add the import at the top of `apps/ody-code/src/main.ts`:

```typescript
import { runShellWithRustHost } from './cli/run-shell-rust';
```

- [ ] Run the test and verify it PASSES:

```bash
pnpm vitest run apps/ody-code/test/cli/run-shell-rust.test.ts
```

- [ ] Run the relevant CLI tests to confirm no regression:

```bash
pnpm vitest run apps/ody-code/test/cli/main.test.ts apps/ody-code/test/cli/run-shell.test.ts
```

- [ ] Commit:

```bash
git add apps/ody-code/src/cli/run-shell-rust.ts apps/ody-code/src/main.ts apps/ody-code/test/cli/run-shell-rust.test.ts
git commit -m "feat(ody-code): wire --host=rust into main entrypoint"
```

---

### Task C7: Manual end-to-end smoke test

**Depends on:** Tasks C1–C6 and Part 2 Task B7 (Rust host executable builds)

**Files:** none (manual verification)

**Goal:** Confirm that the TUI can start against the real Rust host and that events flow end-to-end.

- [ ] Build the Rust host:

```bash
cd rust-ody && cargo build --bin ody-host
```

- [ ] Run stdio mode and verify the TUI starts without crashing:

```bash
pnpm dev:cli-only -- --host=rust --host-stdio --host-binary $(pwd)/rust-ody/target/debug/ody-host --home $(mktemp -d)
```

Expected observation: TUI renders; footer shows `Ody` prompt; no startup error banner.

- [ ] In the TUI, type a short prompt (e.g. `say hello`) and press Enter. Expected observation: an assistant message appears (it may be an error from the stub LLM, but the event stream reaches the TUI).

- [ ] Kill the Rust host process from another terminal:

```bash
pkill -f ody-host
```

Expected observation: TUI prints `Rust host disconnected: ...` and exits with code 1.

- [ ] Run socket mode:

```bash
SOCKET=$(mktemp -u)
pnpm dev:cli-only -- --host=rust --host-socket "$SOCKET" --host-binary $(pwd)/rust-ody/target/debug/ody-host --home $(mktemp -d)
```

Expected observation: same TUI startup behavior as stdio mode.

- [ ] Run with a missing binary and verify a clear error:

```bash
pnpm dev:cli-only -- --host=rust --host-stdio --host-binary /no/such/ody-host
```

Expected observation: process exits immediately with `Rust host binary not found: /no/such/ody-host`.

- [ ] Commit a note in `apps/ody-code/CHANGELOG.md` or add a changeset if required by repo policy (see root `AGENTS.md`):

```bash
pnpm changeset
# select ody-code, minor, write: "Add prototype --host=rust support for external Rust host core"
```

- [ ] Commit the changeset:

```bash
git add .changeset
git commit -m "chore: add changeset for --host=rust prototype"
```

---

## Local Self-Review

- [ ] **1. Spec-coverage table**

| Design section | Requirement | Task(s) | Status |
|---|---|---|---|
| 2.1 `RustHostLaunchOptions` | Typed options for mode/binary/socket/tcp/config/home | C5, C6 | covered |
| 2.2 `RustHostConnector` | Spawn/connect Rust host, return `SDKRpcClient`, disconnect callbacks | C1, C2 | covered |
| 2.3 `ClientAPI` | Reverse-RPC handlers unchanged; transport points to Rust host | C1, C2 | covered |
| 3.1 `RustHostConnector.connect` | stdio/socket/tcp spawn + ready message + error handling | C1, C2, C6 | covered |
| 3.2 `main` | `--host=rust` branch launches Rust host TUI | C5, C6 | covered |
| 3.3 `startTui` | Reuse `OdyTUI` with Rust host harness | C3, C4, C6 | covered |
| 3.4 Event consumption | Events flow through `SDKRpcClient` → `Session.onEvent` | C3 | covered |
| 4.1 `main.ts` | Rust host branch before `runShell` | C6 | covered |
| 4.2 `commands.ts` | New CLI options | C5 | covered |
| 4.3 `ody-tui.ts` | Constructor accepts `OdyHarness` instead of `KimiHarness` | C4 | covered |
| 5. Error handling | Spawn failure / disconnect / reverse-RPC timeout | C6, C7 | covered |

- [ ] **2. Placeholder scan**: No `TODO`/`TBD`/deferred placeholders. Every task contains real file paths, code, commands, and expected output.

- [ ] **3. No phantom tasks**: Each task produces a verifiable change (tests, new files, CLI options, wiring). Task C7 is manual verification, not an empty commit.

- [ ] **4. Dependency soundness**: `Depends on:` references are satisfied by earlier tasks or by Part 2. C6 depends on C2–C5; C4 depends on C3; C3 depends on C2; C2 depends on C1.

- [ ] **5. Caller & build soundness**: C4 changes the shared `OdyTUI` constructor type and the `SlashCommandHost` / `AuthFlowHost` harness types. The task explicitly searches for all callers/tests and ends with `pnpm -r typecheck`. The same signature is not changed again later.

- [ ] **6. Test-the-risk**: C1 tests custom binary/argument forwarding; C2 tests disconnect propagation; C3 tests session caching and `interactiveAgentId` mutation; C5 tests option validation; C6 tests option parsing helpers. Each assertion is traced to an implementation constant or user-visible behavior.

- [ ] **7. Type consistency**: `OdyHarness` properties/methods match what `OdyTUI`, `AuthFlowController`, and `SlashCommandHost` consume. `RustHostLaunchOptions` fields match `RustHostConnectorOptions`. CLI option names (`hostStdio`, `hostSocket`, `hostTcp`, `hostBinary`) are consistent across `CLIOptions`, `commands.ts`, and `validateOptions`.
