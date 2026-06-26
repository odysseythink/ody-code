# Part 3: ClientAPI LLM 代理 + SDKRpcClient Worker 模式

**Scope:** 在 node-sdk 的 `ClientAPI` 侧新增 `chatStreamInit` / `chatStreamCancel` RPC 方法，使主线程能够将 LLM 流式调用代理到 Worker 线程；同时扩展 `SDKRpcClient` 支持 Worker 生命周期管理和自动降级策略。本 Part 是 Phase 1-B 的 SDK 客户端侧。

**Goal:** 让 `apps/ody-code` 或任何 SDK 调用方可以通过 `ClientAPI` 把 LLM 请求下沉到 Worker 线程执行，并在 Worker 不可用时无缝退回同进程模式。

**Architecture:**
- 主线程的 `ClientAPI`（node-sdk 已有接口）暴露 `chatStreamInit`、`chatStreamCancel` 两个 RPC 方法；收到 `chatStreamInit` 后调用本地 `KosongLLM.chat()`，把异步迭代器中的 delta 通过 `chatStreamDelta` 推回 Worker，结束时调用 `chatStreamEnd`。
- `SDKRpcClient` 新增可选 Worker 传输模式：构造时若检测到 `ODY_CORE_TRANSPORT=worker` 或显式传入 `worker: true`，则 spawn `core-worker` 线程；否则沿用现有同进程 `KimiCore` 模式。
- Worker 崩溃或 RPC 超时后自动降级到同进程 `KimiCore`，避免用户界面完全卡死。

**Tech Stack:** TypeScript, Node.js `MessagePort`, `node:worker_threads`, Vitest, pnpm monorepo.

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

| Path | Responsibility |
|------|----------------|
| `packages/node-sdk/src/rpc.ts (class ClientAPI)` | 新增 `chatStreamInit` / `chatStreamCancel` RPC 方法（Modify） |
| `packages/node-sdk/src/rpc.ts` | 新增 Worker 模式构造、生命周期、降级路径（Modify） |
| `packages/node-sdk/src/core-worker.ts` | Worker 入口，接收 `MessagePort` 并挂载 RPC（Modify / 已在 Part 2 创建） |
| `packages/node-sdk/src/index.ts` | 导出 Worker 相关类型（Modify） |
| `packages/node-sdk/package.json` | 新增 `./core-worker` 条件导出（Modify） |
| `packages/node-sdk/test/sdk-rpc-client.worker.test.ts` | Worker 模式启动与降级测试（Create） |
| `packages/node-sdk/test/client-api.llm-stream.test.ts` | `chatStreamInit` / `chatStreamCancel` 行为测试（Create） |

## Dependency Overview

```
Part 1 (transport) ─┐
                    ├──► Part 3 (sdk-client) ──► Part 4 (integration)
Part 2 (worker-core)─┘
```

本 Part 依赖 Part 1 的 `createMessagePortTransport` / `createRPCEndpoint` 以及 Part 2 的 `core-worker.ts` / `WorkerCoreAPI` / `RemoteKosongLLM`。同 Part 内部任务按以下顺序执行：

- **Phase 3.1**（T9）：在 `ClientAPI` 增加 LLM 代理方法。
- **Phase 3.2**（T10）：扩展 `SDKRpcClient` 支持 Worker 模式与降级。
- **Phase 3.3**（T11）：在 `node-sdk` 暴露 `./core-worker` 导出并补充 Worker 集成测试。

## Risks & Open Questions

| Risk | Mitigation |
|------|------------|
| Worker spawn 路径在不同包管理器/打包工具（pnpm、esbuild bundle）下解析失败 | 通过 `package.json` 的 `./core-worker` 条件导出使用绝对路径，并在测试中覆盖 |
| `chatStreamCancel` 跨线程竞争：Worker 在取消消息到达前已收到下一个 delta | `RemoteKosongLLM` 在 `AbortController` abort 后忽略后续 delta；主线程 side 在调用 `chatStreamCancel` 后忽略该 stream 的后续 delta |
| Worker 崩溃后状态丢失 | 1-B 范围限定为“降级到同进程并继续服务”，不恢复运行中对话；在 `SDKRpcClient` 文档中说明 |
| 同进程 `KimiCore` 与 Worker `KimiCore` 配置不一致 | 所有配置在 `SDKRpcClient` 构造时统一计算，通过 boot payload 原样传给 Worker |

---

### Task 9: ClientAPI 增加 chatStreamInit / chatStreamCancel

**Depends on:** Part 2 `worker-core.md`: Task 7

**Files:**
- Modify: `packages/node-sdk/src/rpc.ts (class ClientAPI)`
- Modify: `packages/node-sdk/src/index.ts`
- Test: `packages/node-sdk/test/client-api.llm-stream.test.ts`

**Background:** 现有 `ClientAPI` 是 `SDKRpcClient` 在主线程一侧暴露给 Worker 的 RPC 服务端。需要在其中新增两个方法，用于接收 Worker 的 LLM 流式请求并把结果推回 Worker。

#### Step 1: Write the failing test

在 `packages/node-sdk/test/client-api.llm-stream.test.ts` 写入：

```typescript
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ClientAPI } from '../client-api';
import type { ChatStreamRequest, ChatStreamDeltaPayload, ChatStreamEndPayload, ChatStreamErrorPayload } from '@odysseythink/agent-core';
import type { KosongLLM } from '@odysseythink/agent-core';

function makeFakeLLM deltas: { type: string; text?: string }[]): KosongLLM {
  return {
    chat: vi.fn().mockResolvedValue({
      deltaIterator: (async function* () {
        for (const d of deltas) {
          yield d;
        }
      })(),
      toolCalls: [],
      finishReason: 'stop',
      usage: { input: 1, output: 2, total: 3 },
    }),
    isRetryableError: () => false,
  } as unknown as KosongLLM;
}

describe('ClientAPI LLM stream proxy', () => {
  it('forwards deltas and end after chat completes', async () => {
    const deltas = [{ type: 'text', text: 'hello ' }, { type: 'text', text: 'world' }];
    const llm = makeFakeLLM(deltas);
    const sent: unknown[] = [];
    const api = new ClientAPI({ llm });
    // @ts-expect-error private for test
    api._rpcServer = {
      notify: (_method: string, payload: unknown) => {
        sent.push(payload);
        return Promise.resolve();
      },
    } as unknown as RPCServer<ClientAPI, CoreAPI>;

    const req: ChatStreamRequest = {
      modelName: 'test-model',
      systemPrompt: 'sys',
      messages: [{ role: 'user', content: 'hi' }],
      tools: [],
    };

    await api.chatStreamInit({ streamId: 's1', request: req });

    const deltaPayloads = sent.filter((p) => (p as { streamId?: string }).streamId === 's1' && 'delta' in (p as object));
    const endPayload = sent.find((p) => (p as { streamId?: string }).streamId === 's1' && 'result' in (p as object));

    expect(deltaPayloads).toHaveLength(2);
    expect((deltaPayloads[0] as ChatStreamDeltaPayload).delta).toEqual({ type: 'text', text: 'hello ' });
    expect((deltaPayloads[1] as ChatStreamDeltaPayload).delta).toEqual({ type: 'text', text: 'world' });
    expect(endPayload).toBeDefined();
    expect((endPayload as ChatStreamEndPayload).result.usage.output).toBe(2);
  });

  it('cancels an active stream when chatStreamCancel is called', async () => {
    const abortController = new AbortController();
    let yielded = false;
    const llm = {
      chat: vi.fn().mockResolvedValue({
        deltaIterator: (async function* () {
          yield { type: 'text', text: 'first' };
          yielded = true;
          await new Promise((_, reject) => {
            abortController.signal.addEventListener('abort', () => reject(new Error('aborted')));
          });
        })(),
        toolCalls: [],
        finishReason: 'stop',
        usage: { input: 0, output: 0, total: 0 },
      }),
      isRetryableError: () => false,
    } as unknown as KosongLLM;

    const api = new ClientAPI({ llm });
    const sent: unknown[] = [];
    // @ts-expect-error private for test
    api._rpcServer = {
      notify: (_method: string, payload: unknown) => {
        sent.push(payload);
        return Promise.resolve();
      },
    } as unknown as RPCServer<ClientAPI, CoreAPI>;

    const req: ChatStreamRequest = {
      modelName: 'test-model',
      systemPrompt: 'sys',
      messages: [{ role: 'user', content: 'hi' }],
      tools: [],
    };

    const initPromise = api.chatStreamInit({ streamId: 's2', request: req });
    await new Promise((r) => setTimeout(r, 10));
    api.chatStreamCancel({ streamId: 's2' });
    await initPromise.catch(() => undefined);

    const errorPayload = sent.find((p) => (p as { streamId?: string }).streamId === 's2' && 'error' in (p as object));
    expect(errorPayload).toBeDefined();
    expect((errorPayload as ChatStreamErrorPayload).error.code).toBe('LLM_STREAM_CANCELLED');
  });
});
```

#### Step 2: Run it and verify it FAILS

```bash
pnpm --filter @odysseythink/node-sdk test packages/node-sdk/test/client-api.llm-stream.test.ts
```

Expected failure: `ClientAPI` 上不存在 `chatStreamInit` / `chatStreamCancel`；测试文件甚至无法编译通过（`Property 'chatStreamInit' does not exist`）。

#### Step 3: Write the minimal implementation

在 `packages/node-sdk/src/rpc.ts (class ClientAPI)` 顶部新增导入：

```typescript
import {
  ChatStreamRequest,
  ChatStreamDeltaPayload,
  ChatStreamEndPayload,
  ChatStreamErrorPayload,
  LLMStreamErrorCode,
} from '@odysseythink/agent-core';
```

在 `ClientAPI` 类内新增：

```typescript
private activeStreams = new Map<string, { abortController: AbortController }>();

async chatStreamInit(payload: { readonly streamId: string; readonly request: ChatStreamRequest }): Promise<void> {
  const { streamId, request } = payload;
  const abortController = new AbortController();
  this.activeStreams.set(streamId, { abortController });

  try {
    const response = await this.llm.chat({
      modelName: request.modelName,
      systemPrompt: request.systemPrompt,
      messages: request.messages,
      tools: request.tools,
      capability: request.capability,
      completionBudgetConfig: request.completionBudgetConfig,
      abortSignal: abortController.signal,
      requestLogContext: request.requestLogContext,
    });

    for await (const delta of response.deltaIterator) {
      if (abortController.signal.aborted) {
        break;
      }
      await this.notifyWorker('chatStreamDelta', {
        streamId,
        delta,
      } as ChatStreamDeltaPayload);
    }

    if (abortController.signal.aborted) {
      return;
    }

    await this.notifyWorker('chatStreamEnd', {
      streamId,
      result: {
        toolCalls: response.toolCalls,
        providerFinishReason: response.finishReason,
        rawFinishReason: response.rawFinishReason,
        usage: response.usage,
        streamTiming: response.streamTiming,
      },
    } as ChatStreamEndPayload);
  } catch (error) {
    if (abortController.signal.aborted) {
      await this.notifyWorker('chatStreamError', {
        streamId,
        error: {
          code: LLMStreamErrorCode.Cancelled,
          message: 'LLM stream cancelled by worker',
        },
      } as ChatStreamErrorPayload);
      return;
    }
    await this.notifyWorker('chatStreamError', {
      streamId,
      error: {
        code: LLMStreamErrorCode.ProviderError,
        message: error instanceof Error ? error.message : String(error),
      },
    } as ChatStreamErrorPayload);
  } finally {
    this.activeStreams.delete(streamId);
  }
}

async chatStreamCancel(payload: { readonly streamId: string }): Promise<void> {
  const active = this.activeStreams.get(payload.streamId);
  if (active) {
    active.abortController.abort();
  }
}

private async notifyWorker(method: string, payload: unknown): Promise<void> {
  // 假设 ClientAPI 已持有 _rpcServer 或同等 notify 能力；若当前实现不同，复用现有通知通道
  await (this as unknown as { _rpcServer: { notify(method: string, payload: unknown): Promise<void> } })._rpcServer.notify(method, payload);
}
```

> 说明：若 `ClientAPI` 当前没有 `_rpcServer`，请改为复用现有 `this.rpc.notify` 或同文件中的 notify 包装。不要假设额外私有字段；如果实现差异较大，请把 `notifyWorker` 替换为 `this.rpc.notify`。

#### Step 4: Run it and verify it PASSES

```bash
pnpm --filter @odysseythink/node-sdk test packages/node-sdk/test/client-api.llm-stream.test.ts
```

期望：所有用例通过。

#### Step 5: Commit

```bash
git add packages/node-sdk/src/rpc.ts (class ClientAPI) packages/node-sdk/test/client-api.llm-stream.test.ts
git commit -m "feat(node-sdk): ClientAPI chatStreamInit/chatStreamCancel for worker LLM proxy"
```

---

### Task 10: SDKRpcClient 支持 Worker 模式与自动降级

**Depends on:** Task 9, Part 2 `worker-core.md`: Task 8

**Files:**
- Modify: `packages/node-sdk/src/rpc.ts`
- Modify: `packages/node-sdk/src/index.ts`
- Test: `packages/node-sdk/test/sdk-rpc-client.worker.test.ts`

**Background:** `SDKRpcClient` 当前直接构造同进程 `KimiCore`。需要让它可选地 spawn Worker 线程，通过 `MessagePort` 连接，并在 Worker 失败时降级。

#### Step 1: Write the failing test

在 `packages/node-sdk/test/sdk-rpc-client.worker.test.ts` 写入：

```typescript
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { MessageChannel } from 'node:worker_threads';
import { SDKRpcClient } from '../sdk-rpc-client';

describe('SDKRpcClient worker mode', () => {
  it('uses in-process core when worker option is false', () => {
    const client = new SDKRpcClient({ worker: false, apiKey: 'test' });
    // @ts-expect-error private for test
    expect(client.worker).toBeNull();
    // @ts-expect-error private for test
    expect(client.core).toBeInstanceOf(KimiCore);
  });

  it('spawns worker when worker option is true', () => {
    const client = new SDKRpcClient({ worker: true, apiKey: 'test' });
    // @ts-expect-error private for test
    expect(client.worker).not.toBeNull();
    // @ts-expect-error private for test
    expect(client.transport).toBeDefined();
    client.dispose();
  });

  it('falls back to in-process core when worker exits unexpectedly', async () => {
    const client = new SDKRpcClient({ worker: true, apiKey: 'test' });
    // @ts-expect-error private for test
    const worker = client.worker;
    expect(worker).not.toBeNull();

    worker!.emit('exit', 1);

    await vi.waitFor(() => {
      // @ts-expect-error private for test
      expect(client.worker).toBeNull();
      // @ts-expect-error private for test
      expect(client.core).toBeInstanceOf(KimiCore);
    });

    client.dispose();
  });
});
```

#### Step 2: Run it and verify it FAILS

```bash
pnpm --filter @odysseythink/node-sdk test packages/node-sdk/test/sdk-rpc-client.worker.test.ts
```

Expected failure: `SDKRpcClient` 没有 `worker` 选项；`worker`、`transport`、`core` 字段均不存在。

#### Step 3: Write the minimal implementation

在 `packages/node-sdk/src/rpc.ts` 顶部新增：

```typescript
import { Worker } from 'node:worker_threads';
import {
  createMessagePortTransport,
  createRPCEndpoint,
  WorkerCoreAPI,
  CoreAPI,
  Transport,
} from '@odysseythink/agent-core';
import { ClientAPI } from './client-api';
```

在 `SDKRpcClientOptions` 接口中新增：

```typescript
export interface SDKRpcClientOptions {
  // ... existing fields ...
  /**
   * Run the core in a dedicated Worker thread instead of the current process.
   * Defaults to false unless environment variable ODY_CORE_TRANSPORT=worker is set.
   */
  worker?: boolean;
  /**
   * Absolute path to the worker entry script. Defaults to the package's `./core-worker` export.
   */
  workerScriptPath?: string;
}
```

在类内新增字段与构造逻辑：

```typescript
private worker: Worker | null = null;
private transport: Transport | null = null;
private coreEndpoint: ReturnType<typeof createRPCEndpoint<CoreAPI, ClientAPI>> | null = null;
private fallbackCore: KimiCore | null = null;

constructor(options: SDKRpcClientOptions) {
  const useWorker = options.worker ?? process.env.ODY_CORE_TRANSPORT === 'worker';
  if (useWorker) {
    this.startWorker(options);
  } else {
    this.fallbackCore = new KimiCore({
      apiKey: options.apiKey,
      baseUrl: options.baseUrl,
      // ... other existing options ...
    });
    this.core = this.fallbackCore;
  }
}

private startWorker(options: SDKRpcClientOptions): void {
  const workerScript = options.workerScriptPath ?? require.resolve('@odysseythink/node-sdk/core-worker');
  const worker = new Worker(workerScript, {
    workerData: {
      port: null, // MessagePort will be transferred
    },
  });
  this.worker = worker;

  const { port1, port2 } = new MessageChannel();
  worker.postMessage({ type: 'init', port: port1 }, [port1]);

  const clientApi = new ClientAPI({ llm: this.createLocalLLM(options) });
  const endpoint = createRPCEndpoint<CoreAPI, ClientAPI>();
  this.coreEndpoint = endpoint;

  const transport = createMessagePortTransport(port2, endpoint.dispatch, {
    onError: (error) => {
      this.handleWorkerError(error);
    },
  });
  this.transport = transport;
  endpoint.setTransport(transport);

  // 绑定 ClientAPI 作为服务端；WorkerCoreAPI 的 RPC 客户端通过 endpoint.client 暴露给 SDK 调用方
  this.core = new Proxy({} as WorkerCoreAPI, {
    get: (_target, prop) => {
      if (prop === 'dispose' || prop === Symbol.asyncDispose) {
        return () => this.dispose();
      }
      return (...args: unknown[]) => endpoint.client.request(prop as string, args[0]);
    },
  });

  worker.on('error', (error) => this.handleWorkerError(error));
  worker.on('exit', (code) => {
    if (code !== 0) {
      this.handleWorkerError(new Error(`Worker exited with code ${code}`));
    }
  });
}

private handleWorkerError(error: Error): void {
  if (!this.worker) {
    return;
  }
  this.disposeWorker();
  // 自动降级到同进程 KimiCore
  this.fallbackCore = new KimiCore({
    // ... same options used for worker boot ...
  });
  this.core = this.fallbackCore;
}

private disposeWorker(): void {
  this.transport?.close();
  this.transport = null;
  this.coreEndpoint = null;
  try {
    this.worker?.terminate();
  } catch {
    // ignore
  }
  this.worker = null;
}

async dispose(): Promise<void> {
  this.disposeWorker();
  await this.fallbackCore?.dispose();
  this.fallbackCore = null;
}

private createLocalLLM(options: SDKRpcClientOptions): KosongLLM {
  // 复用现有 KosongLLM 构造逻辑；Worker 模式下主线程仍需要一个本地 LLM 供 ClientAPI 代理
  return new KosongLLM({
    apiKey: options.apiKey,
    baseUrl: options.baseUrl,
    modelName: options.modelName,
  });
}
```

> 注意：实际实现时应复用现有 `KimiCore` 构造选项，不要把所有字段硬编码。若 `SDKRpcClient` 已有 `core` 字段类型为 `KimiCore`，需要把 `core` 类型扩展为 `KimiCore | WorkerCoreAPI` 或统一接口。

#### Step 4: Run it and verify it PASSES

```bash
pnpm --filter @odysseythink/node-sdk test packages/node-sdk/test/sdk-rpc-client.worker.test.ts
```

期望：所有用例通过。

#### Step 5: Commit

```bash
git add packages/node-sdk/src/rpc.ts packages/node-sdk/test/sdk-rpc-client.worker.test.ts
git commit -m "feat(node-sdk): SDKRpcClient worker mode with fallback"
```

---

### Task 11: 暴露 core-worker 导出并补充 Worker 集成测试

**Depends on:** Task 10

**Files:**
- Modify: `packages/node-sdk/package.json`
- Modify: `packages/node-sdk/src/index.ts`
- Test: `packages/node-sdk/test/core-worker.e2e.test.ts`

#### Step 1: Write the failing test

在 `packages/node-sdk/test/core-worker.e2e.test.ts` 写入：

```typescript
import { describe, it, expect } from 'vitest';
import { Worker } from 'node:worker_threads';
import { MessageChannel } from 'node:worker_threads';
import { createMessagePortTransport, createRPCEndpoint, WorkerCoreAPI } from '@odysseythink/agent-core';
import { ClientAPI } from '../client-api';
import type { CoreAPI } from '@odysseythink/agent-core';

describe('core-worker end-to-end', () => {
  it('spawns core-worker and completes a ping round-trip', async () => {
    const workerScript = require.resolve('@odysseythink/node-sdk/core-worker');
    const worker = new Worker(workerScript);
    const { port1, port2 } = new MessageChannel();
    worker.postMessage({ type: 'init', port: port1 }, [port1]);

    const clientApi = new ClientAPI({ llm: undefined as unknown as KosongLLM });
    const endpoint = createRPCEndpoint<CoreAPI, ClientAPI>();
    const transport = createMessagePortTransport(port2, endpoint.dispatch);
    endpoint.setTransport(transport);

    // WorkerCoreAPI exposes a ping or health method; adjust to actual method name
    const result = await endpoint.client.request('health', {});
    expect(result).toBeDefined();

    await worker.terminate();
  });
});
```

#### Step 2: Run it and verify it FAILS

```bash
pnpm --filter @odysseythink/node-sdk test packages/node-sdk/test/core-worker.e2e.test.ts
```

Expected failure: `Error: Cannot find module '@odysseythink/node-sdk/core-worker'`（package.json 缺少导出）。

#### Step 3: Write the minimal implementation

修改 `packages/node-sdk/package.json`，在 `"exports"` 字段新增：

```json
{
  "exports": {
    ".": {
      "types": "./dist/index.d.ts",
      "import": "./dist/index.js"
    },
    "./core-worker": {
      "types": "./dist/core-worker.d.ts",
      "import": "./dist/core-worker.js"
    }
  }
}
```

在 `packages/node-sdk/src/index.ts` 中导出 Worker 相关类型：

```typescript
export { SDKRpcClientOptions } from './sdk-rpc-client';
export { CoreWorkerBootPayload } from './core-worker';
```

#### Step 4: Run it and verify it PASSES

```bash
pnpm --filter @odysseythink/node-sdk test packages/node-sdk/test/core-worker.e2e.test.ts
```

期望：模块解析成功，health 调用返回预期值，Worker 正确终止。

#### Step 5: Commit

```bash
git add packages/node-sdk/package.json packages/node-sdk/src/index.ts packages/node-sdk/test/core-worker.e2e.test.ts
git commit -m "feat(node-sdk): expose core-worker export and add worker e2e test"
```

---

## Local Self-Review

- [ ] 1. **Spec-coverage table:** 对照设计文档 Phase 1-B，确认 ClientAPI 流式代理（T9）、SDKRpcClient Worker 模式与降级（T10）、core-worker 导出与集成（T11）均已覆盖。
- [ ] 2. **Placeholder scan:** 本 Part 所有任务均给出真实代码与命令，无 TODO/TBD。
- [ ] 3. **No phantom tasks:** 每个任务都有 Create/Modify + Test，并产生可验证变更。
- [ ] 4. **Dependency soundness:** T9 依赖 Part 2 Task 7；T10 依赖 T9 与 Part 2 Task 8；T11 依赖 T10。无反向依赖。
- [ ] 5. **Caller & build soundness:** Task 10 修改 `SDKRpcClientOptions` 与 `SDKRpcClient` 内部字段，需要在同一任务中搜索 `new SDKRpcClient(` 的所有调用点（包括 apps/ody-code 和测试）并更新类型；完成后运行 `pnpm -r typecheck`。
- [ ] 6. **Test-the-risk:** T9 测试了 delta 转发与取消取消；T10 测试了 Worker 启动与异常退出降级；T11 测试了 Worker 模块导出与真实进程通信。
- [ ] 7. **Type consistency:** `ChatStreamRequest`、`ChatStreamDeltaPayload`、`ChatStreamEndPayload`、`ChatStreamErrorPayload` 类型与 Part 1 定义保持一致；`ClientAPI` 新增的 RPC 方法签名与 Part 2 `SDKAgentRPC` 期望一致。
