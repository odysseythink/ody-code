# Part 1: MessagePort Transport + RPC Endpoint + ChatStream 契约

本 Part 搭建跨线程 RPC 的字节通道与单端绑定能力，并冻结 Phase 1-B 新增的 `chatStream*` RPC 类型契约。

---

### Task 1: 新增 worker / transport 错误码

**Depends on:** none

**Files:**
- Modify: `packages/agent-core/src/errors/codes.ts:79-81`（`ErrorCodes` 追加）与 `packages/agent-core/src/errors/codes.ts:439-451`（`ODY_ERROR_INFO` 追加）
- Create: `packages/agent-core/test/errors/codes.test.ts`

**Goal:** 为 worker 启动失败、异常退出、transport 关闭定义结构化错误码，并保证错误可在 `OdyErrorPayload` 中跨线 round-trip。

- [ ] Write the failing test. 创建 `packages/agent-core/test/errors/codes.test.ts`：

```typescript
import { describe, expect, it } from 'vitest';

import {
  ErrorCodes,
  fromOdyErrorPayload,
  OdyError,
  ODY_ERROR_INFO,
  toOdyErrorPayload,
} from '../../src/errors';

describe('worker/transport error codes', () => {
  it('exposes worker and transport codes', () => {
    expect(ErrorCodes.WORKER_SPAWN_FAILED).toBe('worker.spawn_failed');
    expect(ErrorCodes.WORKER_EXITED).toBe('worker.exited');
    expect(ErrorCodes.TRANSPORT_CLOSED).toBe('transport.closed');
  });

  it('has metadata for every new code', () => {
    const codes = [ErrorCodes.WORKER_SPAWN_FAILED, ErrorCodes.WORKER_EXITED, ErrorCodes.TRANSPORT_CLOSED];
    for (const code of codes) {
      const info = ODY_ERROR_INFO[code];
      expect(info).toBeDefined();
      expect(info.title).toBeTruthy();
      expect(typeof info.retryable).toBe('boolean');
      expect(typeof info.public).toBe('boolean');
    }
  });

  it('round-trips through OdyError payload', () => {
    const error = new OdyError(ErrorCodes.WORKER_EXITED, 'worker died');
    const payload = toOdyErrorPayload(error);
    expect(payload.code).toBe('worker.exited');
    expect(payload.retryable).toBe(false);

    const restored = fromOdyErrorPayload(payload);
    expect(restored.code).toBe('worker.exited');
    expect(restored.message).toBe('worker died');
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
cd /Users/ranwei/workspace/ody-code
pnpm vitest run packages/agent-core/test/errors/codes.test.ts
```

Expected failure: `Cannot find module ... /codes.test.ts` 或 `ErrorCodes.WORKER_EXITED is undefined`。

- [ ] Write the minimal implementation. 修改 `packages/agent-core/src/errors/codes.ts`：

在 `ErrorCodes` 对象末尾、`INTERNAL` 之前插入：

```typescript
  WORKER_SPAWN_FAILED: 'worker.spawn_failed',
  WORKER_EXITED: 'worker.exited',
  TRANSPORT_CLOSED: 'transport.closed',
```

在 `ODY_ERROR_INFO` 末尾、`internal` 之前插入：

```typescript
  'worker.spawn_failed': {
    title: 'Core worker spawn failed',
    retryable: false,
    public: true,
    action: 'Check the worker entry path or set transport to inproc.',
  },
  'worker.exited': {
    title: 'Core worker exited unexpectedly',
    retryable: false,
    public: true,
    action: 'Create a new session.',
  },
  'transport.closed': {
    title: 'Transport closed',
    retryable: false,
    public: true,
    action: 'The worker connection was closed; create a new session.',
  },
```

- [ ] Run it and verify it PASSES.

```bash
pnpm vitest run packages/agent-core/test/errors/codes.test.ts
```

Expected: 3 tests pass.

- [ ] Commit.

```bash
git add packages/agent-core/src/errors/codes.ts packages/agent-core/test/errors/codes.test.ts
git commit -m "feat(agent-core): add worker and transport error codes"
```

---

### Task 2: 实现 MessagePortTransport

**Depends on:** Task 1

**Files:**
- Create: `packages/agent-core/src/rpc/transports/message-port.ts`
- Create: `packages/agent-core/test/rpc/transports/message-port-transport.test.ts`

**Goal:** 提供基于 Node `MessagePort` 的 `Transport` 实现，支持并发请求-响应关联、`onWire`、`onError` 传播与 `close()` 清理。

- [ ] Write the failing test. 创建 `packages/agent-core/test/rpc/transports/message-port-transport.test.ts`：

```typescript
import { describe, expect, it, vi } from 'vitest';
import { MessageChannel } from 'node:worker_threads';

import { createMessagePortTransport } from '../../../src/rpc/transports/message-port';
import { decodeJson, encodeJson } from '../../../src/rpc/transport';

describe('message-port transport', () => {
  it('round-trips request/response bytes', async () => {
    const channel = new MessageChannel();
    const rightHandler = vi.fn(async (bytes: Uint8Array) => {
      expect(decodeJson(bytes)).toBe('ping');
      return encodeJson('pong');
    });

    const left = createMessagePortTransport(channel.port1, async (bytes) => rightHandler(bytes));
    createMessagePortTransport(channel.port2, async () => encodeJson('unused'));

    const response = await left.send(encodeJson('ping'));
    expect(decodeJson(response)).toBe('pong');
    expect(rightHandler).toHaveBeenCalledTimes(1);
  });

  it('correlates concurrent requests by reqId', async () => {
    const channel = new MessageChannel();
    const left = createMessagePortTransport(channel.port1, async (bytes) => {
      const delay = decodeJson(bytes) as number;
      await new Promise((resolve) => setTimeout(resolve, delay));
      return encodeJson(`pong:${delay}`);
    });
    createMessagePortTransport(channel.port2, async () => encodeJson('unused'));

    const [a, b] = await Promise.all([left.send(encodeJson(30)), left.send(encodeJson(10))]);
    expect(decodeJson(a)).toBe('pong:30');
    expect(decodeJson(b)).toBe('pong:10');
  });

  it('rejects pending requests with TRANSPORT_CLOSED when close() is called', async () => {
    const channel = new MessageChannel();
    const left = createMessagePortTransport(
      channel.port1,
      async () => new Promise(() => {}),
    );
    createMessagePortTransport(channel.port2, async () => encodeJson('x'));

    const pending = left.send(encodeJson('hang'));
    left.close();

    await expect(pending).rejects.toMatchObject({ code: 'transport.closed' });
  });

  it('calls onWire for each send and recv', async () => {
    const channel = new MessageChannel();
    const wire: { direction: 'send' | 'recv'; json: unknown }[] = [];
    const left = createMessagePortTransport(
      channel.port1,
      async () => encodeJson('pong'),
      {
        onWire: (direction, bytes) => wire.push({ direction, json: decodeJson(bytes) }),
      },
    );
    createMessagePortTransport(channel.port2, async () => encodeJson('unused'));

    await left.send(encodeJson('ping'));

    expect(wire).toHaveLength(2);
    expect(wire[0]).toEqual({ direction: 'send', json: 'ping' });
    expect(wire[1]).toEqual({ direction: 'recv', json: 'pong' });
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
pnpm vitest run packages/agent-core/test/rpc/transports/message-port-transport.test.ts
```

Expected failure: 文件不存在 / `createMessagePortTransport` 未导出。

- [ ] Write the minimal implementation. 创建 `packages/agent-core/src/rpc/transports/message-port.ts`：

```typescript
import { randomUUID } from 'node:crypto';
import type { MessagePort } from 'node:worker_threads';

import { ErrorCodes, OdyError } from '../../errors';
import type { Dispatch, Transport } from '../transport';

interface WireRequest {
  readonly kind: 'request';
  readonly reqId: string;
  readonly bytes: Uint8Array;
}

interface WireResponse {
  readonly kind: 'response';
  readonly reqId: string;
  readonly bytes: Uint8Array;
}

type WireMessage = WireRequest | WireResponse;

export interface MessagePortTransportOptions {
  onError?: (error: Error) => void;
  onWire?: (direction: 'send' | 'recv', bytes: Uint8Array) => void;
}

interface PendingDeferred<T> {
  readonly promise: Promise<T>;
  readonly resolve: (value: T) => void;
  readonly reject: (reason: unknown) => void;
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

function generateRequestId(): string {
  return randomUUID();
}

export function createMessagePortTransport(
  port: MessagePort,
  dispatch: Dispatch,
  options?: MessagePortTransportOptions,
): Transport {
  const pending = new Map<string, PendingDeferred<Uint8Array>>();
  let closed = false;

  function onError(error: Error): void {
    if (closed) return;
    const odyError =
      error instanceof OdyError ? error : new OdyError(ErrorCodes.INTERNAL, error.message);
    for (const deferred of pending.values()) {
      deferred.reject(odyError);
    }
    pending.clear();
    options?.onError?.(odyError);
  }

  async function handleMessage(msg: WireMessage): Promise<void> {
    if (closed) return;
    if (msg.kind === 'request') {
      try {
        const responseBytes = await dispatch(msg.bytes);
        port.postMessage({ kind: 'response', reqId: msg.reqId, bytes: responseBytes });
      } catch (error) {
        onError(error instanceof Error ? error : new Error(String(error)));
      }
    } else if (msg.kind === 'response') {
      const deferred = pending.get(msg.reqId);
      if (deferred === undefined) return;
      pending.delete(msg.reqId);
      deferred.resolve(msg.bytes);
    }
  }

  port.on('message', (msg: WireMessage) => {
    void handleMessage(msg);
  });
  port.on('messageerror', (error: Error) => {
    onError(error);
  });

  return {
    send(bytes: Uint8Array): Promise<Uint8Array> {
      if (closed) {
        return Promise.reject(
          new OdyError(ErrorCodes.TRANSPORT_CLOSED, 'MessagePort closed'),
        );
      }
      const reqId = generateRequestId();
      const deferred = createDeferred<Uint8Array>();
      pending.set(reqId, deferred);
      const msg: WireRequest = { kind: 'request', reqId, bytes };
      options?.onWire?.('send', bytes);
      port.postMessage(msg);
      return deferred.promise.then((responseBytes) => {
        options?.onWire?.('recv', responseBytes);
        return responseBytes;
      });
    },
    onError(error) {
      onError(error);
    },
    close() {
      if (closed) return;
      closed = true;
      port.close();
      const error = new OdyError(ErrorCodes.TRANSPORT_CLOSED, 'MessagePort closed');
      for (const deferred of pending.values()) {
        deferred.reject(error);
      }
      pending.clear();
      options?.onError?.(error);
    },
  };
}
```

- [ ] Run it and verify it PASSES.

```bash
pnpm vitest run packages/agent-core/test/rpc/transports/message-port-transport.test.ts
```

Expected: 4 tests pass。

- [ ] Commit.

```bash
git add packages/agent-core/src/rpc/transports/message-port.ts packages/agent-core/test/rpc/transports/message-port-transport.test.ts
git commit -m "feat(agent-core): add MessagePortTransport"
```

---

### Task 3: 添加 createRPCEndpoint 单端 RPC 绑定

**Depends on:** Task 2

**Files:**
- Modify: `packages/agent-core/src/rpc/client.ts:1-186`
- Create: `packages/agent-core/test/rpc/create-rpc-endpoint.test.ts`

**Goal:** 提供单端 `RPCEndpoint`，worker / SDK 各取一端即可；避免 `createRPC` 双端必须同时被调用的限制。

- [ ] Write the failing test. 创建 `packages/agent-core/test/rpc/create-rpc-endpoint.test.ts`：

```typescript
import { describe, expect, it, vi } from 'vitest';

import { createRPCEndpoint } from '../../src/rpc/client';
import {
  createInProcessTransportPair,
  decodeJson,
  encodeJson,
} from '../../src/rpc/transport';

interface CoreSide {
  getConfig(payload: { sessionId: string }): { model: string };
}

interface HostSide {
  requestApproval(request: {
    requestId: string;
    toolName: string;
  }): Promise<{ decision: string }>;
  fail(request: { code: string }): Promise<void>;
}

describe('createRPCEndpoint', () => {
  it('round-trips calls over InProcessTransport', async () => {
    const left = createRPCEndpoint<CoreSide, HostSide>();
    const right = createRPCEndpoint<HostSide, CoreSide>();
    const [leftTransport, rightTransport] = createInProcessTransportPair(
      left.dispatch,
      right.dispatch,
    );
    left.setTransport(leftTransport);
    right.setTransport(rightTransport);

    const hostProxy = await left.client({
      getConfig: ({ sessionId }) => ({ model: `model:${sessionId}` }),
    });
    const coreProxy = await right.client({
      requestApproval: async (request) => ({ decision: `approved:${request.toolName}` }),
      fail: async () => {
        throw new Error('boom');
      },
    });

    await expect(
      hostProxy.requestApproval({ requestId: 'a', toolName: 'Bash' }),
    ).resolves.toEqual({ decision: 'approved:Bash' });
    await expect(coreProxy.getConfig({ sessionId: 's1' })).resolves.toEqual({
      model: 'model:s1',
    });
    await expect(hostProxy.fail({ code: 'x' })).rejects.toThrow('boom');
  });

  it('propagates transport onError to pending calls', async () => {
    const left = createRPCEndpoint<CoreSide, HostSide>();
    const right = createRPCEndpoint<HostSide, CoreSide>();
    const [leftTransport, rightTransport] = createInProcessTransportPair(
      left.dispatch,
      right.dispatch,
    );
    left.setTransport(leftTransport);
    right.setTransport(rightTransport);

    await right.client({
      requestApproval: async () => ({ decision: 'ok' }),
      fail: async () => {},
    });
    const hostProxy = await left.client({
      getConfig: () => ({ model: 'x' }),
    });

    const pending = hostProxy.requestApproval({ requestId: 'x', toolName: 'Bash' });
    await new Promise((resolve) => setTimeout(resolve, 10));
    leftTransport.onError?.(new Error('transport fatal'));

    await expect(pending).rejects.toThrow('transport fatal');
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
pnpm vitest run packages/agent-core/test/rpc/create-rpc-endpoint.test.ts
```

Expected failure: `createRPCEndpoint` 未导出 / 类型错误。

- [ ] Write the minimal implementation. 修改 `packages/agent-core/src/rpc/client.ts`：

在文件顶部 `RPCCallOptions` 之后、其他类型之前添加：

```typescript
export interface RPCEndpoint<Self extends Record<string, any>, Other extends Record<string, any>> {
  readonly dispatch: Dispatch;
  setTransport(transport: Transport): void;
  readonly client: RPCClient<Self, Other>;
}
```

在 `createRPC` 之后、`export type CoreRPCClient` 之前添加 `createRPCEndpoint`：

```typescript
export function createRPCEndpoint<
  Self extends Record<string, any>,
  Other extends Record<string, any>,
>(): RPCEndpoint<Self, Other> {
  const selfReady = createControlledPromise<PromisableMethods<Self>>();
  let transport: Transport | undefined;
  const pending = new Set<PendingDeferred<Uint8Array>>();

  function attachTransportErrorHandling(t: Transport): void {
    const originalOnError = t.onError;
    t.onError = (error: Error) => {
      const errorToThrow =
        error instanceof OdyError ? error : new OdyError(ErrorCodes.INTERNAL, error.message);
      for (const deferred of pending) {
        deferred.reject(errorToThrow);
      }
      pending.clear();
      originalOnError?.(error);
    };
  }

  async function dispatch(bytes: Uint8Array): Promise<Uint8Array> {
    const payload = decodeJson(bytes) as { method: string; args: unknown[] };
    const boundSelf = await selfReady;
    const fn = (boundSelf as Record<string, unknown>)[payload.method] as Function | undefined;
    if (typeof fn !== 'function') {
      return encodeJson({
        ok: false,
        error: toOdyErrorPayload(new Error(`RPC method not found: ${payload.method}`)),
      });
    }
    try {
      const value = await abortable(Promise.resolve(fn(...payload.args)));
      return encodeJson({ ok: true, value });
    } catch (error) {
      return encodeJson({ ok: false, error: toOdyErrorPayload(error) });
    }
  }

  function mapMethod(methodName: string): Function {
    return async (payload: any, options?: RPCCallOptions) => {
      if (transport === undefined) {
        throw new OdyError(ErrorCodes.INTERNAL, 'RPC endpoint transport not set');
      }
      const signal = options?.signal;
      signal?.throwIfAborted();
      const requestBytes = encodeJson({ method: methodName, args: [payload] });
      transport.onWire?.('send', requestBytes);

      const deferred = createDeferred<Uint8Array>();
      pending.add(deferred);
      transport.send(requestBytes).then(deferred.resolve, deferred.reject).finally(() => {
        pending.delete(deferred);
      });

      const responseBytes = await abortable(deferred.promise, signal);
      transport.onWire?.('recv', responseBytes);
      const response = decodeJson(responseBytes) as RpcResponse;
      signal?.throwIfAborted();
      if (response.ok) return response.value;
      throw fromOdyErrorPayload(response.error);
    };
  }

  async function client(self: PromisableMethods<Self>): Promise<RPCMethods<Other>> {
    selfReady.resolve(bindAllFunctions(self));
    return new Proxy({} as RPCMethods<Other>, {
      get(_target, prop) {
        if (typeof prop !== 'string') return undefined;
        return mapMethod(prop);
      },
    });
  }

  function setTransport(t: Transport): void {
    transport = t;
    attachTransportErrorHandling(t);
  }

  return { dispatch, setTransport, client };
}
```

注意：`bindAllFunctions` 与 `abortable` 已在同一文件内定义，可直接复用。`PendingDeferred` 类型当前定义在 `createRPC` 内部，无法被外部使用；将 `interface PendingDeferred<T>` 上提到 `createRPC` 外部（保持 private），或在本函数内重新内联定义。为减少改动，直接把 `interface PendingDeferred<T>` 移到 `RPCCallOptions` 之后作为模块级私有类型即可。

- [ ] Run it and verify it PASSES.

```bash
pnpm vitest run packages/agent-core/test/rpc/create-rpc-endpoint.test.ts
```

Expected: 2 tests pass。

- [ ] Run whole-tree typecheck to make sure the refactor did not break existing callers.

```bash
pnpm -r typecheck
```

Expected: 全 workspace 类型检查通过。

- [ ] Commit.

```bash
git add packages/agent-core/src/rpc/client.ts packages/agent-core/test/rpc/create-rpc-endpoint.test.ts
git commit -m "feat(agent-core): add single-sided createRPCEndpoint"
```

---

### Task 4: 新增 chatStream* RPC 契约

**Depends on:** none（逻辑上可在 Task 3 之后；不依赖具体 transport 实现，仅新增类型）

**Files:**
- Create: `packages/agent-core/src/rpc/llm-stream.ts`
- Modify: `packages/agent-core/src/rpc/core-api.ts:432-452`
- Modify: `packages/agent-core/src/rpc/sdk-api.ts:81-93`
- Modify: `packages/agent-core/src/rpc/index.ts`（新增 `llm-stream` 与 `transports/message-port` 导出）
- Create: `packages/agent-core/test/rpc/llm-stream-contract.test.ts`

**Goal:** 在主线程与 worker 之间冻结 LLM 流式代理的 RPC 契约，所有 payload/return 均为 JSON-safe 对象。

- [ ] Write the failing test. 创建 `packages/agent-core/test/rpc/llm-stream-contract.test.ts`：

```typescript
import { describe, expect, it } from 'vitest';

import type {
  ChatStreamRequest,
  ChatStreamResult,
  StreamDelta,
} from '../../src/rpc/llm-stream';

describe('llm stream contract', () => {
  it('ChatStreamRequest round-trips through JSON', () => {
    const request: ChatStreamRequest = {
      modelName: 'kimi-k2',
      systemPrompt: 'You are a helpful assistant.',
      messages: [{ role: 'user', content: 'hello' }] as any,
      tools: [],
      capability: 'reasoning',
      requestLogContext: { turnId: 't1', step: 1 },
    };
    const json = JSON.stringify(request);
    expect(JSON.parse(json)).toEqual(request);
  });

  it('ChatStreamResult round-trips through JSON', () => {
    const delta: StreamDelta = { type: 'text', text: 'hi' };
    const result: ChatStreamResult = {
      toolCalls: [],
      providerFinishReason: 'stop',
      rawFinishReason: 'stop',
      usage: { promptTokens: 1, completionTokens: 1, totalTokens: 2 } as any,
      streamTiming: { firstTokenLatencyMs: 10, streamDurationMs: 20 },
    };
    expect(JSON.parse(JSON.stringify(delta))).toEqual(delta);
    expect(JSON.parse(JSON.stringify(result))).toEqual(result);
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
pnpm vitest run packages/agent-core/test/rpc/llm-stream-contract.test.ts
```

Expected failure: 模块/文件不存在。

- [ ] Write the minimal implementation.

创建 `packages/agent-core/src/rpc/llm-stream.ts`：

```typescript
import type {
  FinishReason,
  Message,
  ModelCapability,
  TokenUsage,
  Tool,
  ToolCall,
} from '@odysseythink/kosong';

import type { LLMRequestLogContext, LLMStreamTiming } from '#/loop/llm';
import type { CompletionBudgetConfig } from '#/utils/completion-budget';
import type { OdyErrorPayload } from '../errors';

export interface ChatStreamRequest {
  readonly modelName: string;
  readonly systemPrompt: string;
  readonly messages: Message[];
  readonly tools: Tool[];
  readonly capability?: ModelCapability | undefined;
  readonly completionBudgetConfig?: CompletionBudgetConfig | undefined;
  readonly requestLogContext?: LLMRequestLogContext;
}

export type StreamDelta =
  | { readonly type: 'text'; readonly text: string }
  | { readonly type: 'think'; readonly think: string }
  | {
      readonly type: 'tool_call_part';
      readonly toolCallId: string;
      readonly name?: string | undefined;
      readonly argumentsPart?: string | undefined;
    };

export interface ChatStreamResult {
  readonly toolCalls: ToolCall[];
  readonly providerFinishReason?: FinishReason;
  readonly rawFinishReason?: string;
  readonly usage: TokenUsage;
  readonly streamTiming?: LLMStreamTiming;
}

export interface ChatStreamInitPayload {
  readonly request: ChatStreamRequest;
}

export interface ChatStreamCancelPayload {
  readonly streamId: string;
}

export interface ChatStreamInitResponse {
  readonly streamId: string;
}

export interface ChatStreamDeltaPayload {
  readonly streamId: string;
  readonly delta: StreamDelta;
}

export interface ChatStreamEndPayload {
  readonly streamId: string;
  readonly result: ChatStreamResult;
}

export interface ChatStreamErrorPayload {
  readonly streamId: string;
  readonly error: OdyErrorPayload;
}
```

修改 `packages/agent-core/src/rpc/core-api.ts`：

在 imports 中追加：

```typescript
import type {
  ChatStreamDeltaPayload,
  ChatStreamEndPayload,
  ChatStreamErrorPayload,
} from './llm-stream';
```

在 `CoreAPI` 接口末尾、`requestCodeReview` 之后追加：

```typescript
  chatStreamDelta: (payload: ChatStreamDeltaPayload) => void;
  chatStreamEnd: (payload: ChatStreamEndPayload) => void;
  chatStreamError: (payload: ChatStreamErrorPayload) => void;
```

修改 `packages/agent-core/src/rpc/sdk-api.ts`：

在 imports 中追加：

```typescript
import type {
  ChatStreamCancelPayload,
  ChatStreamInitPayload,
  ChatStreamInitResponse,
} from './llm-stream';
```

在 `SDKAgentAPI` 接口 `openExternal` 之后追加：

```typescript
  chatStreamInit: (payload: ChatStreamInitPayload) => Promise<ChatStreamInitResponse>;
  chatStreamCancel: (payload: ChatStreamCancelPayload) => void;
```

修改 `packages/agent-core/src/rpc/index.ts`：

在 `export * from './events';` 之后追加：

```typescript
export * from './llm-stream';
export {
  createMessagePortTransport,
  type MessagePortTransportOptions,
} from './transports/message-port';
```

- [ ] Run it and verify it PASSES.

```bash
pnpm vitest run packages/agent-core/test/rpc/llm-stream-contract.test.ts
```

Expected: 2 tests pass。

- [ ] Run whole-tree typecheck.

```bash
pnpm -r typecheck
```

Expected: 全 workspace 类型检查通过（包括新增的 ChatStream 类型被 CoreAPI/SDKAPI 使用）。

- [ ] Commit.

```bash
git add packages/agent-core/src/rpc/llm-stream.ts packages/agent-core/src/rpc/core-api.ts packages/agent-core/src/rpc/sdk-api.ts packages/agent-core/src/rpc/index.ts packages/agent-core/test/rpc/llm-stream-contract.test.ts
git commit -m "feat(agent-core): add chatStream RPC contract for worker LLM proxy"
```

---

## Local Self-Review

- [ ] 1. Spec-coverage table：本 Part 覆盖 MessagePort Transport、单端 RPC endpoint、ChatStream 契约。

| 设计需求 | 覆盖 Task | 状态 |
|---|---|---|
| `MessagePortTransport` 实现 `Transport` 接口 | T2 | covered |
| 单端 RPC 绑定（worker/SDK 各取一端） | T3 | covered |
| worker/transport 结构化错误码 | T1 | covered |
| 新增 `chatStream*` RPC 契约 | T4 | covered |

- [ ] 2. Placeholder scan：T1-T4 均给出完整代码、命令与预期输出，无 `TODO`/`TBD`。
- [ ] 3. No phantom tasks：每个 Task 都有 Create/Modify/Test 文件与 commit 动作。
- [ ] 4. Dependency soundness：T2 依赖 T1（使用 `TRANSPORT_CLOSED`）；T3 依赖 T2；T4 独立。
- [ ] 5. Caller & build soundness：T3 改动 `client.ts` 后跑 `pnpm -r typecheck`；T4 新增公共类型并跑 `pnpm -r typecheck`。
- [ ] 6. Test-the-risk：T2 测试并发 reqId 关联、close 后 pending reject；T3 测试跨 endpoint 调用与 onError 传播；T1/T4 测试错误码与 JSON 序列化状态。
- [ ] 7. Type consistency：`MessagePortTransportOptions`、`RPCEndpoint`、`ChatStream*` 类型命名与后续 Part 保持一致。
