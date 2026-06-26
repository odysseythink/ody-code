# Phase 0: RPC Transport 抽象与 Golden Parity 实施计划

> **Goal:** 在 `packages/agent-core/src/rpc` 中引入 `Transport` 字节通道抽象与默认 `InProcessTransport` 实现，把 `createRPC` 改造为通过 `Transport` 交换 `{ method, args }` 消息，并保证默认路径与显式 transport 路径的线消息语义一致。
>
> **Architecture:** `Transport` 只搬运 `Uint8Array`，不解析 RPC 语义；`createInProcessTransportPair` 提供进程内对端直连并保留 `setTimeout(0)` 异步 tick；`createRPC` 内部用方法名路由调用远端实现。默认无参调用与旧签名 100% 兼容，所有现有调用点无需改动。
>
> **Tech Stack:** TypeScript, Vitest, pnpm workspace, Node.js `TextEncoder`/`TextDecoder`。
>
> > For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## 设计变更说明（相对已批准设计）

已批准设计在 ALG-2 的伪代码里同时出现了“保留当前 `mapRpcFunction` 闭包语义（Option A）”的注释与“发送 `{ method, args }` 信封（Option B）”的算法。二者在通用 `Transport` 抽象下不可兼得：通用 per-side Transport 必须知道要调用远端哪个方法，因此采用 **方法信封（method envelope）** 实现。该决策已由用户确认。

由此带来的 refine：
1. 线消息格式从“原始 payload JSON”变为 `{ method: string, args: [payload] }` 与 `{ ok, value }` / `{ ok, false, error }` 响应。Golden Parity 因此改为比较**解码后的语义消息**（方向、method、args、ok/value/error），而非逐字节。
2. `CreateRPCOptions.transport` 支持工厂函数形式 `(dispatchLeft, dispatchRight) => TransportPair`，以便 `createInProcessTransportPair` 在 `createRPC` 创建出 dispatch 函数后再被绑定；同时保留直接传入 `TransportPair` 的能力供未来 Socket/MessagePort 使用。

## File Structure

| File | Responsibility |
|---|---|
| `packages/agent-core/src/rpc/transport.ts` (Create) | `Transport` / `TransportPair` / `Dispatch` / `CreateRPCOptions` 类型；`encodeJson` / `decodeJson`；`createInProcessTransportPair`。 |
| `packages/agent-core/src/rpc/client.ts` (Modify) | `createRPC` 接受 `CreateRPCOptions`，使用 `Transport` 与方法信封，默认创建 `InProcessTransportPair`，保持无参兼容。 |
| `packages/agent-core/src/rpc/index.ts` (Modify) | 导出 transport 相关公共类型与 `createInProcessTransportPair`。 |
| `packages/agent-core/test/rpc/transport.test.ts` (Create) | `encodeJson`/`decodeJson`、`createInProcessTransportPair`、onWire/close 的单元测试。 |
| `packages/agent-core/test/rpc/create-rpc.test.ts` (Modify) | 现有用例继续覆盖默认路径；新增显式 transport 工厂注入用例。 |
| `packages/agent-core/test/rpc/transport-wire.test.ts` (Create) | `onWire` 记录、transport `send` reject、`onError` 传播。 |
| `packages/agent-core/test/rpc/transport-parity.test.ts` (Create) | 默认路径与显式 InProcessTransport 路径的线消息语义 golden parity。 |

## Dependency Overview

```text
Task 1: transport.ts 基础
  │
  ├──→ Task 2: client.ts 改造（共享签名变更，需 whole-tree typecheck）
  │     │
  │     ├──→ Task 3: transport-wire.test.ts
  │     │
  │     ├──→ Task 4: transport-parity.test.ts
  │     │
  │     └──→ Task 6: 全量验证
  │
  └──→ Task 5: index.ts 导出
        │
        └──→ Task 6: 全量验证
```

Task 1、Task 5 可并行；Task 2 必须在 Task 1 之后；Task 3/4 必须在 Task 2 之后；Task 6 收尾。

## Risks & Open Questions

| # | Risk | Mitigation in Plan |
|---|---|---|
| R1 | 方法信封改变线字节，旧 golden 快照失效 | 用语义消息 parity 替代逐字节 parity；Task 4 同时断言默认与显式路径消息完全一致。 |
| R2 | `createRPC` 新增可选参数破坏现有无参调用 | 签名改为 `createRPC(options?)`，所有调用点 `grep` 已确认均为无参；Task 2 末尾跑 `pnpm -r typecheck`。 |
| R3 | `setTimeout(0)` 顺序变化导致 flaky | `InProcessTransport` 严格复用 `setTimeout(0)`，Task 4 parity 覆盖并发与顺序调用。 |
| R4 | `AbortSignal` 在 transport 路径下行为漂移 | 保留现有 `abortableRpc` 与 `signal.throwIfAborted()` 检查点；Task 2 现有 abort 相关测试需继续通过。 |


### Task 1: 创建 `transport.ts` 与基础单元测试

**Depends on:** none

**Files:**
- Create: `packages/agent-core/src/rpc/transport.ts`
- Create: `packages/agent-core/test/rpc/transport.test.ts`

**Goal:** 定义 `Transport` 抽象、`TransportPair`、`Dispatch`、`CreateRPCOptions`，实现 JSON ↔ `Uint8Array` 编解码与进程内 Transport 对。

- [ ] Write the failing test. 在 `packages/agent-core/test/rpc/transport.test.ts` 写入：

```typescript
import { describe, expect, it, vi } from 'vitest';

import {
  createInProcessTransportPair,
  decodeJson,
  encodeJson,
  type Dispatch,
} from '../../src/rpc/transport';

describe('transport', () => {
  describe('encodeJson / decodeJson', () => {
    it('round-trips undefined as empty bytes', () => {
      const bytes = encodeJson(undefined);
      expect(bytes).toBeInstanceOf(Uint8Array);
      expect(bytes.length).toBe(0);
      expect(decodeJson(bytes)).toBe(undefined);
    });

    it('round-trips null, string, and objects', () => {
      expect(decodeJson(encodeJson(null))).toBe(null);
      expect(decodeJson(encodeJson(''))).toBe('');
      expect(decodeJson(encodeJson({ x: 1 }))).toEqual({ x: 1 });
    });

    it('matches JSON.stringify edge semantics', () => {
      const input = {
        at: new Date('2026-05-18T00:00:00.000Z'),
        notFinite: Number.NaN,
        dropped: undefined,
        nested: { ok: true },
      };
      expect(decodeJson(encodeJson(input))).toEqual({
        at: '2026-05-18T00:00:00.000Z',
        notFinite: null,
        nested: { ok: true },
      });
    });
  });

  describe('createInProcessTransportPair', () => {
    it('delivers bytes to peer dispatch via setTimeout(0)', async () => {
      const leftHandler = vi.fn<Dispatch>(async (bytes) => {
        expect(decodeJson(bytes)).toBe('ping-from-right');
        return encodeJson('pong-from-left');
      });
      const rightHandler = vi.fn<Dispatch>(async (bytes) => {
        expect(decodeJson(bytes)).toBe('ping-from-left');
        return encodeJson('pong-from-right');
      });

      const [left, right] = createInProcessTransportPair(leftHandler, rightHandler);

      const leftPromise = left.send(encodeJson('ping-from-left'));
      const rightPromise = right.send(encodeJson('ping-from-right'));

      await expect(leftPromise).resolves.toEqual(encodeJson('pong-from-right'));
      await expect(rightPromise).resolves.toEqual(encodeJson('pong-from-left'));
      expect(leftHandler).toHaveBeenCalledTimes(1);
      expect(rightHandler).toHaveBeenCalledTimes(1);
    });

    it('calls onWire for each send and recv', async () => {
      const leftHandler: Dispatch = async () => encodeJson('left-response');
      const rightHandler: Dispatch = async () => encodeJson('right-response');
      const leftWire: { direction: 'send' | 'recv'; json: unknown }[] = [];
      const rightWire: { direction: 'send' | 'recv'; json: unknown }[] = [];

      const [left, right] = createInProcessTransportPair(leftHandler, rightHandler);
      left.onWire = (direction, bytes) => leftWire.push({ direction, json: decodeJson(bytes) });
      right.onWire = (direction, bytes) => rightWire.push({ direction, json: decodeJson(bytes) });

      await left.send(encodeJson('hello'));

      expect(leftWire).toEqual([
        { direction: 'send', json: 'hello' },
        { direction: 'recv', json: 'right-response' },
      ]);
      expect(rightWire).toEqual([
        { direction: 'recv', json: 'hello' },
        { direction: 'send', json: 'right-response' },
      ]);
    });

    it('close is a no-op and does not break subsequent sends', async () => {
      const handler: Dispatch = async () => encodeJson('ok');
      const [left, right] = createInProcessTransportPair(handler, handler);
      left.close?.();
      right.close?.();
      await expect(left.send(encodeJson('x'))).resolves.toEqual(encodeJson('ok'));
    });
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
cd /Users/ranwei/workspace/ody-code
pnpm vitest run packages/agent-core/test/rpc/transport.test.ts
```

Expected failure: modules/files not found (`Cannot find module ... transport` / `Cannot find ... transport.test.ts`).

- [ ] Write the minimal implementation. 创建 `packages/agent-core/src/rpc/transport.ts`：

```typescript
export interface Transport {
  send(bytes: Uint8Array): Promise<Uint8Array>;
  onError?(error: Error): void;
  onWire?(direction: 'send' | 'recv', bytes: Uint8Array): void;
  close?(): void;
}

export type TransportPair = [Transport, Transport];

export type Dispatch = (bytes: Uint8Array) => Promise<Uint8Array>;

export interface CreateRPCOptions {
  transport?: TransportPair | ((dispatchLeft: Dispatch, dispatchRight: Dispatch) => TransportPair);
}

export function encodeJson(value: unknown): Uint8Array {
  const json = JSON.stringify(value);
  if (json === undefined) {
    return new Uint8Array();
  }
  return new TextEncoder().encode(json);
}

export function decodeJson(bytes: Uint8Array): unknown {
  if (bytes.length === 0) {
    return undefined;
  }
  return JSON.parse(new TextDecoder().decode(bytes));
}

export function createInProcessTransportPair(
  dispatchLeft: Dispatch,
  dispatchRight: Dispatch,
): TransportPair {
  const left: Transport = {
    send(bytes: Uint8Array): Promise<Uint8Array> {
      return new Promise((resolve, reject) => {
        setTimeout(() => {
          dispatchRight(bytes).then(resolve, reject);
        }, 0);
      });
    },
    close(): void {
      // no-op for in-process transport
    },
  };

  const right: Transport = {
    send(bytes: Uint8Array): Promise<Uint8Array> {
      return new Promise((resolve, reject) => {
        setTimeout(() => {
          dispatchLeft(bytes).then(resolve, reject);
        }, 0);
      });
    },
    close(): void {
      // no-op for in-process transport
    },
  };

  return [left, right];
}
```

- [ ] Run it and verify it PASSES.

```bash
pnpm vitest run packages/agent-core/test/rpc/transport.test.ts
```

Expected: all tests pass.

- [ ] Commit.

```bash
git add packages/agent-core/src/rpc/transport.ts packages/agent-core/test/rpc/transport.test.ts
git commit -m "feat(agent-core): add Transport abstraction and InProcessTransport pair"
```


### Task 2: 改造 `createRPC` 使用 Transport 与方法信封

**Depends on:** Task 1

**Files:**
- Modify: `packages/agent-core/src/rpc/client.ts:1-108`
- Modify: `packages/agent-core/test/rpc/create-rpc.test.ts:1-203`

**Goal:** 让 `createRPC` 接受 `CreateRPCOptions`，内部通过 `Transport` 发送 `{ method, args }` 消息；未传 options 时使用默认 `InProcessTransportPair`；保持现有无参调用完全兼容。

- [ ] Write the failing test. 在 `packages/agent-core/test/rpc/create-rpc.test.ts` 的 `describe('createRPC', () => { ... })` 末尾追加：

```typescript
  it('works with an explicit InProcessTransport factory', async () => {
    const [connectCore, connectHost] = createRPC<CoreSide, HostSide>({
      transport: createInProcessTransportPair,
    });
    const hostImpl = {
      emitEvent: vi.fn(),
      requestApproval: vi.fn(async (request: { requestId: string; toolName: string }) => ({
        decision: `approved:${request.toolName}`,
      })),
      fail: vi.fn(async () => {}),
    };

    const hostProxyPromise = connectCore({
      getConfig: ({ sessionId }) => ({ model: `model-for:${sessionId}` }),
    });
    const coreProxy = await connectHost(hostImpl);
    const hostProxy = await hostProxyPromise;

    await hostProxy.emitEvent({ type: 'agent.status.updated', payload: { value: 1 } });
    await expect(
      hostProxy.requestApproval({ requestId: 'approval-explicit', toolName: 'Bash' }),
    ).resolves.toEqual({ decision: 'approved:Bash' });
    await expect(coreProxy.getConfig({ sessionId: 'session-explicit' })).resolves.toEqual({
      model: 'model-for:session-explicit',
    });
    expect(hostImpl.emitEvent).toHaveBeenCalledWith({
      type: 'agent.status.updated',
      payload: { value: 1 },
    });
  });

  it('respects AbortSignal on the caller side', async () => {
    const [connectCore, connectHost] = createRPC<CoreSide, HostSide>();
    const hostImpl = {
      emitEvent: vi.fn(),
      requestApproval: vi.fn(async (_request: { requestId: string; toolName: string }) => {
        await new Promise((resolve) => setTimeout(resolve, 50));
        return { decision: 'approved' };
      }),
      fail: vi.fn(async () => {}),
    };

    const hostProxyPromise = connectCore({
      getConfig: ({ sessionId }) => ({ model: `model-for:${sessionId}` }),
    });
    await connectHost(hostImpl);
    const hostProxy = await hostProxyPromise;

    const controller = new AbortController();
    const callPromise = hostProxy.requestApproval({ requestId: 'abort-1', toolName: 'Bash' }, { signal: controller.signal });
    controller.abort();

    await expect(callPromise).rejects.toThrow('Aborted');
  });
```

并添加 import：

```typescript
import { createInProcessTransportPair } from '../../src/rpc/transport';
```

- [ ] Run it and verify it FAILS.

```bash
pnpm vitest run packages/agent-core/test/rpc/create-rpc.test.ts
```

Expected failure: `createRPC` 不接受对象参数 / `createInProcessTransportPair` 不是函数（若尚未从 `transport.ts` 导出）/ TypeScript 类型错误。

- [ ] Write the minimal implementation. 修改 `packages/agent-core/src/rpc/client.ts` 为：

```typescript
import type { PromisableMethods, Promisify } from '#/utils/types';
import { createControlledPromise, objectMap } from '@antfu/utils';

import {
  fromOdyErrorPayload,
  type OdyErrorPayload,
  toOdyErrorPayload,
} from '../errors';
import { abortable } from '../utils/abort';
import type { CoreAPI } from './core-api';
import type { SDKAPI } from './sdk-api';
import {
  createInProcessTransportPair,
  decodeJson,
  encodeJson,
  type CreateRPCOptions,
  type Dispatch,
  type Transport,
  type TransportPair,
} from './transport';

export type { CreateRPCOptions, Transport, TransportPair } from './transport';

export interface RPCCallOptions {
  signal?: AbortSignal;
}

type RpcResponse =
  | { readonly ok: true; readonly value: unknown }
  | { readonly ok: false; readonly error: OdyErrorPayload };

export type RPCMethods<T> = {
  [K in keyof T]: T[K] extends (payload: infer Payload) => infer Return
    ? (payload: Payload, options?: RPCCallOptions) => Promisify<Return>
    : never;
};

export type RPCClient<Self extends Record<string, any>, Other extends Record<string, any>> = (
  self: PromisableMethods<Self>,
) => Promise<RPCMethods<Other>>;

export function createRPC<Left extends Record<string, any>, Right extends Record<string, any>>(
  options?: CreateRPCOptions,
): [RPCClient<Left, Right>, RPCClient<Right, Left>] {
  const leftReady = createControlledPromise<PromisableMethods<Left>>();
  const rightReady = createControlledPromise<PromisableMethods<Right>>();

  async function dispatchLeft(bytes: Uint8Array): Promise<Uint8Array> {
    const payload = decodeJson(bytes) as { method: string; args: unknown[] };
    const boundSelf = await leftReady;
    return handleRpcCall(boundSelf, payload);
  }

  async function dispatchRight(bytes: Uint8Array): Promise<Uint8Array> {
    const payload = decodeJson(bytes) as { method: string; args: unknown[] };
    const boundSelf = await rightReady;
    return handleRpcCall(boundSelf, payload);
  }

  const transportPair: TransportPair =
    typeof options?.transport === 'function'
      ? options.transport(dispatchLeft, dispatchRight)
      : options?.transport ?? createInProcessTransportPair(dispatchLeft, dispatchRight);

  const [leftTransport, rightTransport] = transportPair;

  function abortableRpc<T>(promise: Promise<T>, signal?: AbortSignal): Promise<T> {
    return signal === undefined ? promise : abortable(promise, signal);
  }

  async function handleRpcCall(
    boundSelf: PromisableMethods<Left | Right>,
    payload: { method: string; args: unknown[] },
  ): Promise<Uint8Array> {
    const fn = (boundSelf as Record<string, unknown>)[payload.method] as Function | undefined;
    if (typeof fn !== 'function') {
      return encodeJson({
        ok: false,
        error: toOdyErrorPayload(new Error(`RPC method not found: ${payload.method}`)),
      });
    }
    try {
      const value = await abortableRpc(Promise.resolve(fn(...payload.args)));
      return encodeJson({ ok: true, value });
    } catch (error) {
      return encodeJson({ ok: false, error: toOdyErrorPayload(error) });
    }
  }

  function mapRpcFunction(methodName: string, fn: Function, transport: Transport): Function {
    return async (payload: any, options?: RPCCallOptions) => {
      const signal = options?.signal;
      signal?.throwIfAborted();
      const requestBytes = encodeJson({ method: methodName, args: [payload] });
      transport.onWire?.('send', requestBytes);
      const responseBytes = await abortableRpc(transport.send(requestBytes), signal);
      transport.onWire?.('recv', responseBytes);
      const response = decodeJson(responseBytes) as RpcResponse;
      signal?.throwIfAborted();
      if (response.ok) return response.value;
      throw fromOdyErrorPayload(response.error);
    };
  }

  function bindAllFunctions<T extends Record<string, any>>(obj: T): T {
    const bound: Record<string, unknown> = {};
    let current: object | null = obj;

    while (current !== null && current !== Object.prototype) {
      for (const key of Object.getOwnPropertyNames(current)) {
        if (key === 'constructor' || Object.hasOwn(bound, key)) {
          continue;
        }

        const descriptor = Object.getOwnPropertyDescriptor(current, key);
        if (typeof descriptor?.value === 'function') {
          bound[key] = descriptor.value.bind(obj);
        }
      }

      current = Object.getPrototypeOf(current);
    }

    return bound as T;
  }

  async function leftClient(self: PromisableMethods<Left>): Promise<RPCMethods<Right>> {
    leftReady.resolve(bindAllFunctions(self));
    return objectMap(await rightReady, (key, fn) => [key, mapRpcFunction(key, fn, leftTransport)]) as RPCMethods<Right>;
  }

  async function rightClient(self: PromisableMethods<Right>): Promise<RPCMethods<Left>> {
    rightReady.resolve(bindAllFunctions(self));
    return objectMap(await leftReady, (key, fn) => [key, mapRpcFunction(key, fn, rightTransport)]) as RPCMethods<Left>;
  }

  return [leftClient, rightClient];
}

export type CoreRPCClient = RPCClient<CoreAPI, SDKAPI>;
export type SDKRPCClient = RPCClient<SDKAPI, CoreAPI>;

export type CoreRPC = RPCMethods<CoreAPI>;
```

- [ ] Run it and verify it PASSES.

```bash
pnpm vitest run packages/agent-core/test/rpc/create-rpc.test.ts
```

Expected: all tests pass.

- [ ] Update every caller / whole-tree typecheck. 由于签名新增的是可选参数，调用点无需改动；但必须确认无调用点需要更新。

```bash
# 搜索所有 createRPC 调用点
grep -rn "createRPC\s*<" packages/ apps/
```

Expected: 所有调用均为无参 `createRPC<...>()` 或新测试中的 `{ transport: createInProcessTransportPair }`；无编译错误。

```bash
# 全 workspace 类型检查
pnpm -r typecheck
```

Expected: 所有包类型检查通过。

- [ ] Commit.

```bash
git add packages/agent-core/src/rpc/client.ts packages/agent-core/test/rpc/create-rpc.test.ts
git commit -m "feat(agent-core): route createRPC through Transport with method envelope"
```


### Task 3: Transport 线消息记录与错误传播测试

**Depends on:** Task 2

**Files:**
- Create: `packages/agent-core/test/rpc/transport-wire.test.ts`

**Goal:** 验证 `Transport.onWire` 能正确记录每个方向的原始字节；`Transport.send` reject 与 `onError` 能正确传播给调用方。

- [ ] Write the failing test. 创建 `packages/agent-core/test/rpc/transport-wire.test.ts`：

```typescript
import { describe, expect, it, vi } from 'vitest';

import { ErrorCodes, OdyError } from '../../src/errors';
import { createRPC } from '../../src/rpc';
import {
  createInProcessTransportPair,
  decodeJson,
  encodeJson,
  type Dispatch,
  type Transport,
  type TransportPair,
} from '../../src/rpc/transport';

interface CoreSide {
  getConfig(payload: { sessionId: string }): { model: string };
}

interface HostSide {
  requestApproval(request: { requestId: string; toolName: string }): Promise<{ decision: string }>;
}

describe('transport wire behavior', () => {
  it('records send and recv bytes through onWire', async () => {
    const wire: { direction: 'send' | 'recv'; json: unknown }[] = [];

    function wrapWithRecorder(transport: Transport): Transport {
      return {
        send: async (bytes) => {
          wire.push({ direction: 'send', json: decodeJson(bytes) });
          const response = await transport.send(bytes);
          wire.push({ direction: 'recv', json: decodeJson(response) });
          return response;
        },
      };
    }

    const [baseLeft, baseRight] = createInProcessTransportPair(
      vi.fn<Dispatch>(async () => encodeJson({ ok: true, value: undefined })),
      vi.fn<Dispatch>(async () => encodeJson({ ok: true, value: undefined })),
    );
    const pair: TransportPair = [wrapWithRecorder(baseLeft), wrapWithRecorder(baseRight)];

    const [connectCore, connectHost] = createRPC<CoreSide, HostSide>({ transport: pair });
    const hostProxyPromise = connectCore({
      getConfig: ({ sessionId }) => ({ model: `model-for:${sessionId}` }),
    });
    await connectHost({
      requestApproval: async (request) => ({ decision: `approved:${request.toolName}` }),
    });
    const hostProxy = await hostProxyPromise;

    await hostProxy.requestApproval({ requestId: 'wire-1', toolName: 'Bash' });

    expect(wire).toHaveLength(2);
    expect(wire[0].direction).toBe('send');
    expect(wire[0].json).toEqual({ method: 'requestApproval', args: [{ requestId: 'wire-1', toolName: 'Bash' }] });
    expect(wire[1].direction).toBe('recv');
    expect(wire[1].json).toEqual({ ok: true, value: { decision: 'approved:Bash' } });
  });

  it('propagates a rejected send to the caller', async () => {
    const buggyTransport: Transport = {
      send: () => Promise.reject(new Error('channel broken')),
    };
    const pair: TransportPair = [buggyTransport, buggyTransport];
    const [connectCore, connectHost] = createRPC<CoreSide, HostSide>({ transport: pair });
    const hostProxyPromise = connectCore({
      getConfig: ({ sessionId }) => ({ model: `model-for:${sessionId}` }),
    });
    await connectHost({
      requestApproval: async () => ({ decision: 'approved' }),
    });
    const hostProxy = await hostProxyPromise;

    await expect(hostProxy.requestApproval({ requestId: 'x', toolName: 'Bash' })).rejects.toThrow('channel broken');
  });

  it('rejects pending calls when onError fires', async () => {
    let resolveSend: (() => void) | undefined;
    const hangingTransport: Transport = {
      send: () =>
        new Promise((_resolve, reject) => {
          resolveSend = () => reject(new Error('should have been rejected by onError'));
        }),
      onError: undefined,
    };
    const pair: TransportPair = [hangingTransport, hangingTransport];
    const [connectCore, connectHost] = createRPC<CoreSide, HostSide>({ transport: pair });
    const hostProxyPromise = connectCore({
      getConfig: ({ sessionId }) => ({ model: `model-for:${sessionId}` }),
    });
    await connectHost({
      requestApproval: async () => ({ decision: 'approved' }),
    });
    const hostProxy = await hostProxyPromise;

    const callPromise = hostProxy.requestApproval({ requestId: 'on-error', toolName: 'Bash' });
    // Give send a tick to register the pending Promise.
    await new Promise((resolve) => setTimeout(resolve, 10));

    hangingTransport.onError?.(new Error('transport fatal'));

    await expect(callPromise).rejects.toMatchObject({
      message: expect.stringContaining('transport fatal'),
      code: ErrorCodes.INTERNAL,
    });
    await expect(callPromise).rejects.toBeInstanceOf(OdyError);

    // Make sure we don't accidentally resolve later.
    resolveSend?.();
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
pnpm vitest run packages/agent-core/test/rpc/transport-wire.test.ts
```

Expected failure: 文件不存在 / `onError` 未在 `createRPC` 中实现导致 pending call 未被 reject。

- [ ] Write the minimal implementation. 修改 `packages/agent-core/src/rpc/client.ts` 以支持 `onError`。

在 `createRPC` 内、拿到 `transportPair` 之后添加 pending Promise 跟踪，并引入 `OdyError` / `ErrorCodes`：

```typescript
import {
  ErrorCodes,
  fromOdyErrorPayload,
  OdyError,
  type OdyErrorPayload,
  toOdyErrorPayload,
} from '../errors';
```

在 `createRPC` 函数体中添加：

```typescript
  interface PendingDeferred<T> {
    promise: Promise<T>;
    resolve(value: T): void;
    reject(reason: unknown): void;
  }

  function createDeferred<T>(): PendingDeferred<T> {
    let resolve!: (value: T) => void;
    let reject!: (reason: unknown) => void;
    const promise = new Promise<T>((res, rej) => {
      resolve = res;
      reject = rej;
    });
    return { promise, resolve, reject };
  }

  const pending = new Set<PendingDeferred<Uint8Array>>();

  function attachTransportErrorHandling(transport: Transport): void {
    const originalOnError = transport.onError;
    transport.onError = (error: Error) => {
      const errorToThrow =
        error instanceof OdyError ? error : new OdyError(ErrorCodes.INTERNAL, error.message);
      for (const deferred of pending) {
        deferred.reject(errorToThrow);
      }
      pending.clear();
      originalOnError?.(error);
    };
  }

  attachTransportErrorHandling(leftTransport);
  attachTransportErrorHandling(rightTransport);
```

修改 `mapRpcFunction` 把每次 `send` 注册到 pending 集合：

```typescript
  function mapRpcFunction(methodName: string, fn: Function, transport: Transport): Function {
    return async (payload: any, options?: RPCCallOptions) => {
      const signal = options?.signal;
      signal?.throwIfAborted();
      const requestBytes = encodeJson({ method: methodName, args: [payload] });
      transport.onWire?.('send', requestBytes);

      const deferred = createDeferred<Uint8Array>();
      pending.add(deferred);
      transport.send(requestBytes).then(deferred.resolve, deferred.reject).finally(() => {
        pending.delete(deferred);
      });

      const responseBytes = await abortableRpc(deferred.promise, signal);
      transport.onWire?.('recv', responseBytes);
      const response = decodeJson(responseBytes) as RpcResponse;
      signal?.throwIfAborted();
      if (response.ok) return response.value;
      throw fromOdyErrorPayload(response.error);
    };
  }
```

- [ ] Run it and verify it PASSES.

```bash
pnpm vitest run packages/agent-core/test/rpc/transport-wire.test.ts
```

Expected: all tests pass。

- [ ] Commit.

```bash
git add packages/agent-core/src/rpc/client.ts packages/agent-core/test/rpc/transport-wire.test.ts
git commit -m "feat(agent-core): wire onWire recording and transport onError propagation"
```


### Task 4: Golden Parity 测试（默认路径 vs 显式 Transport）

**Depends on:** Task 2

**Files:**
- Create: `packages/agent-core/test/rpc/transport-parity.test.ts`

**Goal:** 验证 `createRPC()` 默认路径与 `createRPC({ transport: factory })` 显式路径对同一组调用产生完全相同的线消息语义。

- [ ] Write the failing test. 创建 `packages/agent-core/test/rpc/transport-parity.test.ts`：

```typescript
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
    fail: vi.fn(async () => {}),
  };

  const hostProxyPromise = connectCore({
    getConfig: ({ sessionId }) => ({ model: `model-for:${sessionId}` }),
  });
  const coreProxy = (await connectHost(hostImpl)) as { getConfig: CoreSide['getConfig'] };
  const hostProxy = (await hostProxyPromise) as HostSide;

  await hostProxy.emitEvent({ type: 'agent.status.updated', payload: { value: 1 } });
  await hostProxy.requestApproval({ requestId: 'approval-1', toolName: 'Bash' });
  await expect(hostProxy.fail({ code: 'boom' })).rejects.toMatchObject({ code: 'INTERNAL' });
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
```

- [ ] Run it and verify it FAILS.

```bash
pnpm vitest run packages/agent-core/test/rpc/transport-parity.test.ts
```

Expected failure: 文件不存在，或 `createRPC` 的 `transport` 选项尚未实现工厂函数形式。

- [ ] Write the minimal implementation. 确保 Task 2 中的 `CreateRPCOptions.transport` 已支持工厂函数形式且 `createRPC` 会调用它。此时无需再修改 `client.ts`；直接运行测试应通过。

- [ ] Run it and verify it PASSES.

```bash
pnpm vitest run packages/agent-core/test/rpc/transport-parity.test.ts
```

Expected: 两个路径的 `leftWire` / `rightWire` 完全相等，测试通过。

- [ ] Commit.

```bash
git add packages/agent-core/test/rpc/transport-parity.test.ts
git commit -m "test(agent-core): add default-vs-explicit transport golden parity test"
```
```


### Task 5: 在 `rpc/index.ts` 导出 Transport 公共类型

**Depends on:** Task 1

**Files:**
- Modify: `packages/agent-core/src/rpc/index.ts`

**Goal:** 把 `Transport`、`TransportPair`、`CreateRPCOptions`、`Dispatch`、`createInProcessTransportPair`、`encodeJson`、`decodeJson` 作为公共 API 从 `rpc` 模块导出。

- [ ] Write the complete code. 修改 `packages/agent-core/src/rpc/index.ts`：

```typescript
export * from './client';
export * from './core-api';
export * from './core-impl';
export * from './resumed';
export * from './sdk-api';
export * from './events';
export * from './types';
export {
  createInProcessTransportPair,
  decodeJson,
  encodeJson,
  type CreateRPCOptions,
  type Dispatch,
  type Transport,
  type TransportPair,
} from './transport';
```

- [ ] Build / manual verification. 运行类型检查确认导出无冲突：

```bash
pnpm -r typecheck
```

Expected: 全 workspace 类型检查通过。

确认公共导出可用（在任一测试文件中临时验证）：

```typescript
import { createInProcessTransportPair, type Transport } from '../../src/rpc';
```

- [ ] Commit.

```bash
git add packages/agent-core/src/rpc/index.ts
git commit -m "feat(agent-core): export Transport types and InProcessTransport from rpc barrel"
```


### Task 6: 全量验证与回归检查

**Depends on:** Task 2, Task 3, Task 4, Task 5

**Files:**
- Test: `packages/agent-core/test/rpc/*.test.ts`
- Test: `packages/agent-core/test/harness/*.test.ts`
- Test: `packages/agent-core/test/mcp/*.test.ts`
- Build: `rust-ody/build.sh`

**Goal:** 确认 Transport 改造未引入回归，所有 RPC 相关测试、agent-core 全量测试、rust-ody 构建均通过。

- [ ] Run RPC  focused tests.

```bash
pnpm vitest run packages/agent-core/test/rpc/create-rpc.test.ts
pnpm vitest run packages/agent-core/test/rpc/plugins-rpc.test.ts
pnpm vitest run packages/agent-core/test/rpc/transport.test.ts
pnpm vitest run packages/agent-core/test/rpc/transport-wire.test.ts
pnpm vitest run packages/agent-core/test/rpc/transport-parity.test.ts
```

Expected: 全部通过。

- [ ] Run full `agent-core` test suite.

```bash
pnpm vitest run packages/agent-core
```

Expected: 全部通过。

- [ ] Run whole-tree typecheck (one more time after all files are in place).

```bash
pnpm -r typecheck
```

Expected: 全部通过。

- [ ] Verify rust-ody build still works.

```bash
cd rust-ody && ./build.sh
```

Expected: 构建成功退出，无错误。

- [ ] Commit.

```bash
# If all checks pass, optionally tag the final state with a no-op commit or skip if nothing changed.
# Since Task 6 is verification-only, no source changes remain; do not create an empty commit.
```


## Self-Review

- [ ] 1. Spec-coverage table: map every spec section/requirement → Task(s), marked covered / GAP / no-op (GAP means add the task).

| 设计需求 / 规格项 | 覆盖任务 | 状态 |
|---|---|---|
| 新增 `Transport` 抽象接口：`send` 返回 `Promise<Uint8Array>`，`onError`/`onWire`/`close` 可选 | Task 1 | covered |
| 新增 `InProcessTransport` 默认实现，两端 `send` 直连对端 dispatch，保留 `setTimeout(0)` | Task 1, Task 2, Task 4 | covered |
| 改造 `createRPC` 通过选项对象接收 transport；未传入时内部创建默认 InProcessTransport | Task 2 | covered |
| 保留旧调用点零改动与旧签名兼容 | Task 2（grep 确认 + whole-tree typecheck） | covered |
| RPC golden parity：记录线消息并断言语义一致 | Task 4 | covered |
| `onWire` 线消息记录测试 | Task 3 | covered |
| Transport 错误传播（`send` reject、`onError`） | Task 3 | covered |
| `rust-ody/` 框架保持现状 | Task 6 | no-op（仅验证） |
| Phase 0 不引入新的错误码，transport 层错误复用 `ErrorCodes.INTERNAL` | Task 3 | covered |
| `packages/agent-core/src/rpc/index.ts` 导出 Transport 公共 API | Task 5 | covered |

- [ ] 2. Placeholder scan: no TODO/TBD, no deferred-by-dependency excuses, no dead-code placeholders.

已扫描：所有任务均给出完整可运行代码、精确命令与预期输出；无 `TODO`/`TBD`/"implement later"；无 phantom 占位任务。

- [ ] 3. No phantom tasks: every task produces a verifiable change; zero `--allow-empty` / "already done in Task N".

Task 1–5 均产生文件变更与可运行测试；Task 6 为验证任务，不产生新文件，因此不提交空 commit。无 `--allow-empty`。

- [ ] 4. Dependency soundness: every `Depends on:` is satisfied by an earlier task; nothing references a symbol only a later task creates.

依赖图：Task 1 → Task 2/5；Task 2 → Task 3/4/6；Task 3/4/5 → Task 6。所有依赖均指向前序任务。Task 3 使用 `OdyError`/`ErrorCodes`，在 Task 3 中补充 import，未依赖后续任务。

- [ ] 5. Caller & build soundness: every shared-signature task updated all callers (incl. test files) and ends with a whole-tree typecheck, not a single-package build; the same signature is not changed across multiple tasks. Beyond the type level — for any identifier, path, or filename a task changes, open the runtime consumer that reads or validates it and trace one concrete value end-to-end.

共享签名变更仅发生在 Task 2（`createRPC` 新增可选 `options` 参数）。已确认：
- 所有调用点 `grep -rn "createRPC\s*<" packages/ apps/` 均为无参调用或新测试中的显式工厂形式；无需修改。
- Task 2 末尾执行 `pnpm -r typecheck`；Task 6 再次执行。
- `createRPC` 返回的 `RPCClient` 类型未变；`packages/node-sdk/src/rpc.ts:137` 和 `packages/agent-core/src/rpc/core-impl.ts:205` 通过 `rpcClient(this)` 传入的 `CoreRPCClient` 类型未变，消费者无需改动。

- [ ] 6. Test-the-risk: every state-mutating task has a behavioral test asserting the mutation, not just a compile check. For each test assertion, trace the expected value through the implementation constants it depends on.

- Task 1：`encodeJson(undefined)` → 空 `Uint8Array`；`decodeJson(empty)` → `undefined`；JSON 边界值（`null`、`""`、NaN、Date、undefined 字段）均有行为断言。
- Task 2：现有 `create-rpc.test.ts` 中 `hostImpl.emitEvent`、`requestApproval`、`fail` 的调用结果均通过代理返回/抛出断言；新增显式 transport 路径复用同一套断言。
- Task 3：`onWire` 记录方向与解码内容；`send` reject 抛出原始错误；`onError` 触发后所有 pending 调用以 `OdyError(INTERNAL)` reject。断言值均来自实现中的常量（`ErrorCodes.INTERNAL`）。
- Task 4：parity 断言两个路径的线消息数组逐条相等，直接验证默认路径与显式路径行为一致。

- [ ] 7. Type consistency: types, signatures and property names used in later tasks match what earlier tasks defined.

- `Transport`、`TransportPair`、`Dispatch`、`CreateRPCOptions` 在 Task 1 定义，Task 2/3/4/5 使用同一命名与形状。
- `CreateRPCOptions.transport` 支持 `TransportPair | factory`，Task 2 实现与 Task 4 测试一致。
- `onWire` 方向类型 `'send' | 'recv'` 在 Task 1/3/4 中一致。
- `encodeJson`/`decodeJson` 签名在 Task 1 定义，Task 2/3/4 使用一致。

