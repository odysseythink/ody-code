# Part 4: 集成测试 + Parity + Payload 审计 + 最终验证

**Scope:** 把 Part 1–3 的所有改动串联起来，补充覆盖 Worker 生命周期、LLM 代理端到端、崩溃隔离、传输层等价性、可序列化负载审计，并执行全仓库类型检查与测试，确保 Phase 1-B 完整可用。

**Goal:** 证明同进程 `KimiCore` 与 Worker 模式在 Agent turn 行为上无回归，且 Worker 崩溃后 SDK 能降级继续服务。

**Architecture:**
- 集成测试分为三个层级：单元集成（transport parity）、进程集成（core-worker spawn）、行为集成（Agent turn 同进程 vs Worker 一致性）。
- 通过 `LLMFactoryConfig` 与 `llmFactory` 注入，让 `Agent` 在 Worker 内部使用 `RemoteKosongLLM`，在主线程使用真实 `KosongLLM`。
- 崩溃隔离测试通过 `worker.terminate()` 模拟，断言 `SDKRpcClient` 降级后同一 Agent 实例可继续请求。

**Tech Stack:** TypeScript, Vitest, Node.js `worker_threads`, `pnpm -r typecheck`。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

## File Structure

| Path | Responsibility |
|------|----------------|
| `packages/agent-core/test/rpc/transports/transport-parity.test.ts` | 验证 MessagePortTransport 与内存 Transport 行为等价（Create） |
| `packages/node-sdk/test/core-worker.llm-proxy.test.ts` | Worker 内 Agent turn 经 ClientAPI 代理 LLM（Create） |
| `packages/node-sdk/test/sdk-rpc-client.crash-fallback.test.ts` | Worker 崩溃后 SDK 降级并继续服务（Create） |
| `packages/agent-core/src/agent/__tests__/agent.llm-factory.test.ts` | `llmFactory` 注入路径验证（Create） |
| `packages/agent-core/src/rpc/llm-stream.ts` | 序列化负载类型审计（Modify / 已在 Part 1 创建） |
| `packages/node-sdk/src/core-worker.ts` |  Worker 启动时发送 ready 信号（Modify） |
| `packages/node-sdk/src/rpc.ts` |  等待 Worker ready 信号（Modify） |

## Dependency Overview

```
Part 1 (transport) ──┐
                     ├──► Part 4 (integration)
Part 2 (worker-core)─┤
Part 3 (sdk-client) ─┘
```

本 Part 内部顺序：

- **Phase 4.1**（T12）：Transport parity + payload 类型审计。
- **Phase 4.2**（T13）：Agent `llmFactory` 注入测试。
- **Phase 4.3**（T14）：core-worker LLM 代理端到端测试 + ready 信号。
- **Phase 4.4**（T15）：Worker 崩溃隔离与降级测试。
- **Phase 4.5**（T16）：最终全仓库类型检查、测试、文档与变更集。

---

### Task 12: Transport parity 测试 + 序列化负载审计

**Depends on:** Part 1 `transport.md`: Task 3

**Files:**
- Create: `packages/agent-core/test/rpc/transports/transport-parity.test.ts`
- Modify: `packages/agent-core/src/rpc/llm-stream.ts`

**Background:** `createMessagePortTransport` 需要与现有内存 Transport 在请求/响应/通知/错误/取消行为上保持一致；`ChatStreamRequest` 中所有字段必须能被 `structuredClone` 序列化。

#### Step 1: Write the failing test

在 `packages/agent-core/test/rpc/transports/transport-parity.test.ts` 写入：

```typescript
import { describe, it, expect } from 'vitest';
import { MessageChannel } from 'node:worker_threads';
import { createMessagePortTransport } from '../message-port';
import { createMemoryTransport } from '../memory'; // 若不存在，使用任意现有内存 transport

describe('MessagePortTransport parity', () => {
  it('echoes a request/response through MessagePort', async () => {
    const { port1, port2 } = new MessageChannel();

    const serverDispatch = {
      onRequest: async (req: unknown) => ({ result: `echo:${(req as { body: string }).body}` }),
      onNotify: () => undefined,
    };
    const clientDispatch = { onRequest: async () => undefined, onNotify: () => undefined };

    const serverTransport = createMessagePortTransport(port1, serverDispatch as any);
    const clientTransport = createMessagePortTransport(port2, clientDispatch as any);

    const response = await clientTransport.request({ body: 'hi' });
    expect(response).toEqual({ result: 'echo:hi' });

    serverTransport.close();
    clientTransport.close();
  });

  it('propagates errors across MessagePort', async () => {
    const { port1, port2 } = new MessageChannel();

    const serverDispatch = {
      onRequest: async () => {
        throw new Error('server boom');
      },
      onNotify: () => undefined,
    };
    const clientTransport = createMessagePortTransport(port2, { onRequest: async () => undefined, onNotify: () => undefined } as any);
    createMessagePortTransport(port1, serverDispatch as any);

    await expect(clientTransport.request({})).rejects.toThrow('server boom');

    clientTransport.close();
  });

  it('rejects pending requests when transport closes', async () => {
    const { port1, port2 } = new MessageChannel();
    const clientTransport = createMessagePortTransport(port2, { onRequest: async () => undefined, onNotify: () => undefined } as any);
    createMessagePortTransport(port1, { onRequest: async () => new Promise(() => {}), onNotify: () => undefined } as any);

    const pending = clientTransport.request({});
    clientTransport.close();
    await expect(pending).rejects.toThrow();
  });
});
```

#### Step 2: Run it and verify it FAILS

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/rpc/transports/transport-parity.test.ts
```

Expected failure: `createMessagePortTransport` 未实现或 `request` 方法不存在；错误信息包含 `createMessagePortTransport is not a function` 或 `clientTransport.request is not a function`。

#### Step 3: Write the minimal implementation

确保 `packages/agent-core/src/rpc/message-port.ts` 已经实现（Part 1），并包含：

```typescript
export function createMessagePortTransport(
  port: MessagePort,
  dispatch: Dispatch,
  options?: MessagePortTransportOptions,
): Transport {
  // ... existing implementation ...
  return {
    request: async (body: unknown) => { /* send reqId, wait for matching response, timeout/abort */ },
    notify: (body: unknown) => { /* send without waiting */ },
    close: () => { /* remove listener, reject pending, close port if owned */ },
  };
}
```

对 `packages/agent-core/src/rpc/llm-stream.ts` 做序列化审计：所有字段类型只能是 `string`、`number`、`boolean`、`null`、 plain object、或这些类型的只读数组。禁止使用 `Date`、`Map`、`Set`、`bigint`、`Function`、类实例。

```typescript
// 在 ChatStreamRequest 注释中显式声明：
/** All fields must be structuredClone-serializable. */
export interface ChatStreamRequest { /* ... */ }

// 新增一个编译时辅助类型（类型测试用）
export type StructuredCloneable =
  | string | number | boolean | null
  | StructuredCloneable[]
  | { readonly [key: string]: StructuredCloneable };
```

#### Step 4: Run it and verify it PASSES

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/test/rpc/transports/transport-parity.test.ts
```

期望：三个用例全部通过。

#### Step 5: Commit

```bash
git add packages/agent-core/test/rpc/transports/transport-parity.test.ts packages/agent-core/src/rpc/llm-stream.ts
git commit -m "test(agent-core): MessagePort transport parity and structuredClone audit"
```

---

### Task 13: Agent llmFactory 注入路径测试

**Depends on:** Part 2 `worker-core.md`: Task 6

**Files:**
- Create: `packages/agent-core/src/agent/__tests__/agent.llm-factory.test.ts`
- Modify: `packages/agent-core/src/agent/index.ts`（已在 Part 2 修改）

**Background：** 当 `AgentOptions.llmFactory` 存在时，`Agent` 应使用该工厂创建 `LLM` 而不是直接实例化 `KosongLLM`。

#### Step 1: Write the failing test

```typescript
import { describe, it, expect, vi } from 'vitest';
import { Agent } from '../index';
import type { LLM, LLMChatResponse, LLMChatParams } from '@odysseythink/agent-core/loop';

describe('Agent llmFactory injection', () => {
  it('uses llmFactory when provided', async () => {
    const fakeLLM: LLM = {
      chat: vi.fn().mockResolvedValue({
        deltaIterator: (async function* () {
          yield { type: 'text', text: 'ok' };
        })(),
        toolCalls: [],
        finishReason: 'stop',
        usage: { input: 1, output: 1, total: 2 },
      } as LLMChatResponse),
      isRetryableError: () => false,
    };
    const factory = vi.fn().mockReturnValue(fakeLLM);

    const agent = new Agent({
      name: 'test',
      instructions: 'do nothing',
      llmFactory: factory,
    });

    // @ts-expect-error private
    const llm = agent.llm;
    expect(llm).toBe(fakeLLM);
    expect(factory).toHaveBeenCalledWith({
      modelName: expect.any(String),
      systemPrompt: expect.any(String),
    });
  });
});
```

#### Step 2: Run it and verify it FAILS

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/src/agent/__tests__/agent.llm-factory.test.ts
```

Expected failure: `AgentOptions` 没有 `llmFactory` 字段；`agent.llm` getter 未使用工厂。

#### Step 3: Write the minimal implementation

在 `packages/agent-core/src/agent/index.ts` 中（已在 Part 2 提供），确保：

```typescript
export interface AgentOptions {
  // ... existing fields ...
  llmFactory?: (config: LLMFactoryConfig) => LLM;
}

export class Agent {
  private _llm: LLM | undefined;

  get llm(): LLM {
    if (!this._llm) {
      const config: LLMFactoryConfig = {
        modelName: this.options.modelName ?? defaultModelName,
        systemPrompt: this.options.systemPrompt ?? defaultSystemPrompt,
        capability: this.options.capability,
        completionBudgetConfig: this.options.completionBudgetConfig,
      };
      this._llm = this.options.llmFactory?.(config) ?? new KosongLLM(config);
    }
    return this._llm;
  }
}
```

#### Step 4: Run it and verify it PASSES

```bash
pnpm --filter @odysseythink/agent-core test packages/agent-core/src/agent/__tests__/agent.llm-factory.test.ts
```

期望：工厂被调用，返回的 fake LLM 被使用。

#### Step 5: Commit

```bash
git add packages/agent-core/src/agent/__tests__/agent.llm-factory.test.ts packages/agent-core/src/agent/index.ts
git commit -m "feat(agent-core): Agent llmFactory injection path"
```

---

### Task 14: core-worker LLM 代理端到端测试 + ready 信号

**Depends on:** Task 13, Part 3 `sdk-client.md`: Task 9

**Files:**
- Create: `packages/node-sdk/test/core-worker.llm-proxy.test.ts`
- Modify: `packages/node-sdk/src/core-worker.ts`
- Modify: `packages/node-sdk/src/rpc.ts`

**Background：** 需要一个完整的端到端测试，验证 Worker 内部的 `RemoteKosongLLM` 能够通过 `ClientAPI.chatStreamInit` 调用主线程的 fake LLM。

#### Step 1: Write the failing test

```typescript
import { describe, it, expect, vi } from 'vitest';
import { Worker, MessageChannel } from 'node:worker_threads';
import { createMessagePortTransport, createRPCEndpoint } from '@odysseythink/agent-core';
import { ClientAPI } from '../client-api';
import type { CoreAPI } from '@odysseythink/agent-core';
import type { KosongLLM } from '@odysseythink/agent-core';

describe('core-worker LLM proxy end-to-end', () => {
  it('Worker Agent turn streams deltas through ClientAPI', async () => {
    const workerScript = require.resolve('@odysseythink/node-sdk/core-worker');
    const worker = new Worker(workerScript);

    const fakeLLM = {
      chat: vi.fn().mockResolvedValue({
        deltaIterator: (async function* () {
          yield { type: 'text', text: 'hello' };
          yield { type: 'text', text: ' worker' };
        })(),
        toolCalls: [],
        finishReason: 'stop',
        usage: { input: 1, output: 2, total: 3 },
        streamTiming: { firstTokenMs: 10, totalMs: 20 },
      }),
      isRetryableError: () => false,
    } as unknown as KosongLLM;

    const { port1, port2 } = new MessageChannel();
    worker.postMessage({ type: 'init', port: port1 }, [port1]);

    const clientApi = new ClientAPI({ llm: fakeLLM });
    const endpoint = createRPCEndpoint<CoreAPI, ClientAPI>();
    const transport = createMessagePortTransport(port2, endpoint.dispatch);
    endpoint.setTransport(transport);

    await new Promise<void>((resolve, reject) => {
      worker.once('message', (msg) => {
        if (msg.type === 'ready') resolve();
        else reject(new Error(`unexpected worker message ${JSON.stringify(msg)}`));
      });
    });

    // 调用 WorkerCoreAPI 的某个测试方法触发 Agent turn；方法名与签名按 Part 2 定义
    const result = await endpoint.client.request('health', {});
    expect(result).toBeDefined();

    // 更严格的断言：触发一个需要 LLM 的请求并收集 delta
    const deltas: string[] = [];
    await endpoint.client.request('runTestTurn', {
      instructions: 'say hello',
      onDelta: (delta: { text?: string }) => deltas.push(delta.text ?? ''),
    });
    expect(deltas).toEqual(['hello', ' worker']);

    await worker.terminate();
  });
});
```

#### Step 2: Run it and verify it FAILS

```bash
pnpm --filter @odysseythink/node-sdk test packages/node-sdk/test/core-worker.llm-proxy.test.ts
```

Expected failure: Worker 没有发送 `ready` 消息；`runTestTurn` 方法不存在。

#### Step 3: Write the minimal实现

在 `packages/node-sdk/src/core-worker.ts` 的 `coreWorkerMain` 中，初始化完成后向主线程发送 `ready`：

```typescript
port.postMessage({ type: 'ready' });
```

在 `packages/node-sdk/src/rpc.ts` 的 `startWorker` 中等待 ready：

```typescript
await new Promise<void>((resolve, reject) => {
  const timeout = setTimeout(() => reject(new Error('Worker did not become ready')), 10_000);
  worker.once('message', (msg) => {
    if (msg?.type === 'ready') {
      clearTimeout(timeout);
      resolve();
    }
  });
});
```

> 说明：`runTestTurn` 仅用于测试；若不想在 CoreAPI 中暴露测试方法，可改为直接构造 `RemoteKosongLLM` 并调用 `chatStreamInit`，验证 delta 被正确转发。

#### Step 4: Run it and verify it PASSES

```bash
pnpm --filter @odysseythink/node-sdk test packages/node-sdk/test/core-worker.llm-proxy.test.ts
```

期望：ready 消息到达，测试 turn 的 delta 序列正确。

#### Step 5: Commit

```bash
git add packages/node-sdk/test/core-worker.llm-proxy.test.ts packages/node-sdk/src/core-worker.ts packages/node-sdk/src/rpc.ts
git commit -m "feat(node-sdk): core-worker ready signal and end-to-end LLM proxy test"
```

---

### Task 15: Worker 崩溃隔离与自动降级测试

**Depends on:** Task 14, Part 3 `sdk-client.md`: Task 10

**Files：**
- Create: `packages/node-sdk/test/sdk-rpc-client.crash-fallback.test.ts`
- Modify: `packages/node-sdk/src/rpc.ts`

**Background：** 验证 Worker 异常退出后，`SDKRpcClient` 能够自动创建同进程 `KimiCore` 并继续响应请求。

#### Step 1: Write the failing test

```typescript
import { describe, it, expect } from 'vitest';
import { SDKRpcClient } from '../sdk-rpc-client';

describe('SDKRpcClient crash fallback', () => {
  it('falls back and serves a request after worker exits', async () => {
    const client = new SDKRpcClient({ worker: true, apiKey: 'test' });

    // @ts-expect-error private
    const worker = client.worker;
    expect(worker).not.toBeNull();

    // 模拟崩溃
    worker!.process.exit(1);

    await new Promise((r) => setTimeout(r, 100));

    // @ts-expect-error private
    expect(client.worker).toBeNull();
    // @ts-expect-error private
    expect(client.fallbackCore).not.toBeNull();

    // 降级后应仍能响应 health / 简单请求
    // @ts-expect-error private
    const health = await client.core.health();
    expect(health).toBeDefined();

    await client.dispose();
  });
});
```

> 注意：Worker 对象没有 `process.exit` 方法，正确写法是 `worker!.terminate()` 或直接 emit `'exit'` 事件。根据 Node.js Worker API，测试应使用 `worker!.terminate()` 或 `worker!.postMessage({ type: 'crash' })` 让 Worker 自杀。这里改为：

```typescript
import { Worker } from 'node:worker_threads';
// ...
await worker!.terminate();
```

#### Step 2: Run it and verify it FAILS

```bash
pnpm --filter @odysseythink/node-sdk test packages/node-sdk/test/sdk-rpc-client.crash-fallback.test.ts
```

Expected failure: `SDKRpcClient` 的 `handleWorkerError` 未正确降级，或 `core` 的 Proxy 在降级后仍指向已销毁的 endpoint。

#### Step 3: Write the minimal实现

确保 `packages/node-sdk/src/rpc.ts` 中的 `handleWorkerError` 满足：

```typescript
private handleWorkerError(error: Error): void {
  if (this.fallbackCore) {
    // already falling back or disposed
    return;
  }
  this.disposeWorker();
  this.fallbackCore = this.createInProcessCore();
  this.core = this.fallbackCore;
}

private createInProcessCore(): KimiCore {
  return new KimiCore({
    apiKey: this.options.apiKey,
    baseUrl: this.options.baseUrl,
    modelName: this.options.modelName,
    // ... all other options that were passed to worker
  });
}
```

同时修改 `core` 字段的类型，使其兼容 `KimiCore | WorkerCoreAPI`：

```typescript
public core!: KimiCore | WorkerCoreAPI;
```

#### Step 4: Run it and verify it PASSES

```bash
pnpm --filter @odysseythink/node-sdk test packages/node-sdk/test/sdk-rpc-client.crash-fallback.test.ts
```

期望：Worker 终止后降级成功，health 调用返回。

#### Step 5: Commit

```bash
git add packages/node-sdk/test/sdk-rpc-client.crash-fallback.test.ts packages/node-sdk/src/rpc.ts
git commit -m "feat(node-sdk): SDKRpcClient crash fallback and integration test"
```

---

### Task 16: 最终全仓库类型检查、测试、文档与变更集

**Depends on:** Task 15

**Files：**
- Modify: `.changeset/feat-backend-worker-boundary-phase1-b.md`
- Modify: `docs/zh/...` 或 `docs/en/...`（如 Part 2/3 引入了新的环境变量或用户可见行为）

#### Step 1: 运行全仓库类型检查

```bash
pnpm -r typecheck
```

预期输出：无类型错误。

#### Step 2: 运行全仓库测试

```bash
pnpm -r test
```

预期输出：所有相关测试通过；允许已有 flaky 测试失败，但本计划新增的测试必须稳定通过。

#### Step 3: 生成 changeset

```bash
pnpm changeset
```

选择：
- `@odysseythink/agent-core` → `minor`（新增 WorkerCoreAPI、RemoteKosongLLM、llmFactory）
- `@odysseythink/node-sdk` → `minor`（新增 core-worker、SDKRpcClient worker 模式）

changeset 内容示例：

```markdown
---
'@odysseythink/agent-core': minor
'@odysseythink/node-sdk': minor
---

Phase 1-B: MessagePort Worker boundary + LLM proxy.

- Adds `MessagePortTransport`, `createRPCEndpoint`, and cross-thread `chatStream*` RPC types.
- Adds `RemoteKosongLLM`, `WorkerCoreAPI`, and `core-worker` entry point.
- Extends `Agent` with optional `llmFactory` injection.
- Extends `SDKRpcClient` with Worker mode and automatic fallback to in-process `KimiCore`.
```

#### Step 4: 更新相关文档

如果 Part 3 引入了 `ODY_CORE_TRANSPORT=worker` 环境变量，在文档中新增：

```markdown
### ODY_CORE_TRANSPORT
- `in-process` (default): run the agent core in the main thread.
- `worker`: run the agent core in a dedicated Worker thread. Falls back to in-process if the Worker crashes.
```

#### Step 5: Commit

```bash
git add .changeset/feat-backend-worker-boundary-phase1-b.md
git commit -m "chore: changeset for Phase 1-B worker boundary"
```

---

## Local Self-Review

- [ ] 1. **Spec-coverage table:** 对照设计文档 Phase 1-B，确认 transport parity（T12）、llmFactory 注入（T13）、core-worker 端到端（T14）、崩溃降级（T15）、最终验证（T16）均已覆盖。
- [ ] 2. **Placeholder scan:** 本 Part 无 TODO/TBD；所有代码与命令具体给出。
- [ ] 3. **No phantom tasks:** 每个任务都有 Create/Modify + Test/Verification，产生可验证变更。
- [ ] 4. **Dependency soundness:** T12 依赖 Part 1 Task 3；T13 依赖 Part 2 Task 6；T14 依赖 T13 + Part 3 Task 9；T15 依赖 T14 + Part 3 Task 10；T16 依赖 T15。
- [ ] 5. **Caller & build soundness：** T16 执行 `pnpm -r typecheck` 与 `pnpm -r test` 覆盖全仓库。Task 10/15 修改的共享类型（`SDKRpcClientOptions`、`core` 字段类型）在同一任务中更新所有调用点与测试。
- [ ] 6. **Test-the-risk：** T12 测试了传输层错误传播与关闭行为；T13 测试了 LLM 工厂注入；T14 测试了跨进程 LLM 流式代理；T15 测试了 Worker 崩溃降级；T16 通过全仓库测试验证无回归。
- [ ] 7. **Type consistency：** `ChatStream*` 类型、LLM 接口、`RemoteKosongLLM` 与 `ClientAPI` 之间的调用链类型一致；`SDKRpcClient.core` 类型覆盖 Worker 与同进程两种形态。
