# Part 2: Worker 侧 Remote LLM + WorkerCoreAPI + core-worker 入口

本 Part 实现 worker 线程内的 LLM 代理：worker 内 `Agent` 通过 `RemoteKosongLLM` 把流式请求转发到主线程，`WorkerCoreAPI` 负责把主线程推回的 delta/end/error 路由到对应的 `RemoteKosongLLM` 实例。

---

### Task 5: 添加 `llmFactory` 注入钩子

**Depends on:** none（仅扩展已有选项签名，不依赖 transport）

**Files:**
- Modify: `packages/agent-core/src/loop/llm.ts:63-69`
- Modify: `packages/agent-core/src/rpc/core-impl.ts:123-132` 与 `packages/agent-core/src/rpc/core-impl.ts:236-253`、`packages/agent-core/src/rpc/core-impl.ts:324-342`
- Modify: `packages/agent-core/src/session/index.ts:54-72` 与 `packages/agent-core/src/session/index.ts:502-531`
- Modify: `packages/agent-core/src/agent/index.ts:83-111`、`packages/agent-core/src/agent/index.ts:171-172`、`packages/agent-core/src/agent/index.ts:348-373`
- Create: `packages/agent-core/test/agent/llm-factory.test.ts`

**Goal:** 让 `KimiCore → Session → Agent` 可以注入一个 LLM 工厂，worker 模式下用它替换默认的进程内 `KosongLLM`。

- [ ] Write the failing test. 创建 `packages/agent-core/test/agent/llm-factory.test.ts`：

```typescript
import { describe, expect, it, vi } from 'vitest';

import { Agent, type LLM, type LLMFactoryConfig } from '../../../src/agent';
import { getDefaultConfig } from '../../../src/config';
import { createFakeKaos } from '../../tools/fixtures/fake-kaos';

describe('llmFactory injection', () => {
  it('uses the injected factory and re-creates after refresh', () => {
    const mockLlm: LLM = {
      systemPrompt: 'factory-sp',
      modelName: 'factory-model',
      chat: vi.fn(async () => ({ toolCalls: [], usage: { totalTokens: 0 } as any })),
    };
    const factory = vi.fn((_rpc, config: LLMFactoryConfig) => {
      expect(config.modelName).toBe('mock-model');
      expect(config.systemPrompt).toBe('<system-prompt>');
      return mockLlm;
    });

    const agent = new Agent({
      kaos: createFakeKaos(),
      config: getDefaultConfig(),
      llmFactory: factory,
    });

    expect(agent.llm).toBe(mockLlm);
    expect(factory).toHaveBeenCalledTimes(1);

    agent.refreshLlm();
    expect(agent.llm).toBe(mockLlm);
    expect(factory).toHaveBeenCalledTimes(2);
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
cd /Users/ranwei/workspace/ody-code
pnpm vitest run packages/agent-core/test/agent/llm-factory.test.ts
```

Expected failure: `LLMFactoryConfig` / `llmFactory` 未定义。

- [ ] Write the minimal implementation.

修改 `packages/agent-core/src/loop/llm.ts`，在 `LLMChatResponse` 之后追加：

```typescript
import type { CompletionBudgetConfig } from '../utils/completion-budget';

export interface LLMFactoryConfig {
  readonly modelName: string;
  readonly systemPrompt: string;
  readonly capability?: ModelCapability | undefined;
  readonly completionBudgetConfig?: CompletionBudgetConfig | undefined;
}
```

修改 `packages/agent-core/src/rpc/core-impl.ts`：

导入 `LLMFactoryConfig`：

```typescript
import type { LLM, LLMFactoryConfig } from '../loop/llm';
import type { SDKAgentRPC } from './sdk-api';
```

在 `KimiCoreOptions` 中追加：

```typescript
  readonly llmFactory?: ((rpc: Partial<SDKAgentRPC>, config: LLMFactoryConfig) => LLM) | undefined;
```

在 `createSession` 的 `new Session({ ... })` 参数中追加：

```typescript
      llmFactory: options.llmFactory,
```

在 `resumeSession` 的 `new Session({ ... })` 参数中追加：

```typescript
      llmFactory: options.llmFactory,
```

修改 `packages/agent-core/src/session/index.ts`：

导入 `LLM`、`LLMFactoryConfig`、`SDKAgentRPC`：

```typescript
import type { LLM, LLMFactoryConfig } from '../loop/llm';
import type { SDKAgentRPC } from '../rpc';
```

在 `SessionOptions` 中追加：

```typescript
  readonly llmFactory?: ((rpc: Partial<SDKAgentRPC>, config: LLMFactoryConfig) => LLM) | undefined;
```

在 `instantiateAgent` 返回的 `new Agent({ ... })` 参数中追加：

```typescript
      llmFactory: this.options.llmFactory,
```

修改 `packages/agent-core/src/agent/index.ts`：

导入 `LLMFactoryConfig`：

```typescript
import type { LLM, LLMChatParams, LLMChatResponse, LLMFactoryConfig } from '#/loop/llm';
```

在 `AgentOptions` 中追加：

```typescript
  readonly llmFactory?: ((rpc: Partial<SDKAgentRPC>, config: LLMFactoryConfig) => LLM) | undefined;
```

在 `Agent` 类中添加字段：

```typescript
  private readonly llmFactory?: ((rpc: Partial<SDKAgentRPC>, config: LLMFactoryConfig) => LLM) | undefined;
```

并把 `private _llm: KosongLLM | undefined;` 改为：

```typescript
  private _llm: LLM | undefined;
```

在构造函数中赋值：

```typescript
    this.llmFactory = options.llmFactory;
```

把 `get llm(): KosongLLM {` 改为 `get llm(): LLM {`，并在返回默认 `KosongLLM` 之前优先使用工厂：

```typescript
  get llm(): LLM {
    if (this._llm !== undefined) {
      return this._llm;
    }
    const model = this.config.model;
    const systemPrompt = this.config.systemPrompt;
    const loopControl = this.kimiConfig?.loopControl;
    const completionBudgetConfig = resolveCompletionBudget({
      reservedContextSize: loopControl?.reservedContextSize,
    });

    if (this.llmFactory !== undefined) {
      this._llm = this.llmFactory(this.rpc, {
        modelName: model,
        systemPrompt,
        capability: this.config.modelCapabilities,
        completionBudgetConfig,
      });
      return this._llm;
    }

    const provider = this.config.provider.withThinking(this.config.thinkingLevel);
    this._llm = new KosongLLM({
      provider,
      modelName: model,
      systemPrompt,
      capability: this.config.modelCapabilities,
      generate: this.generate,
      completionBudgetConfig,
    });
    return this._llm;
  }
```

- [ ] Run it and verify it PASSES.

```bash
pnpm vitest run packages/agent-core/test/agent/llm-factory.test.ts
```

Expected: 1 test passes。

- [ ] Update every caller / whole-tree typecheck. 由于新增的是可选字段，调用点无需改动；运行以下命令确认无编译错误：

```bash
# 搜索所有 new KimiCore / new Session / new Agent 调用点，确认无类型错误
grep -rn "new KimiCore(" packages/ apps/
grep -rn "new Session(" packages/ apps/
grep -rn "new Agent(" packages/ apps/
pnpm -r typecheck
```

Expected: `pnpm -r typecheck` 全绿。

- [ ] Commit.

```bash
git add packages/agent-core/src/loop/llm.ts packages/agent-core/src/rpc/core-impl.ts packages/agent-core/src/session/index.ts packages/agent-core/src/agent/index.ts packages/agent-core/test/agent/llm-factory.test.ts
git commit -m "feat(agent-core): add llmFactory hook for worker LLM injection"
```

---

### Task 6: 实现 RemoteKosongLLM 与流注册表

**Depends on:** Task 5

**Files:**
- Create: `packages/agent-core/src/agent/turn/remote-kosong-llm.ts`
- Modify: `packages/agent-core/src/agent/index.ts`（导出 `RemoteKosongLLM` / `remoteLLMStreamRegistry`）
- Create: `packages/agent-core/test/agent/turn/remote-kosong-llm.test.ts`

**Goal:** worker 侧 `LLM` 实现，通过 `chatStreamInit`/`chatStreamCancel` 与主线程交互，并用模块级注册表接收 `chatStreamDelta/End/Error`。

- [ ] Write the failing test. 创建 `packages/agent-core/test/agent/turn/remote-kosong-llm.test.ts`：

```typescript
import { describe, expect, it, vi } from 'vitest';

import {
  RemoteKosongLLM,
  remoteLLMStreamRegistry,
} from '../../../src/agent/turn/remote-kosong-llm';
import { ErrorCodes, OdyError } from '../../../src/errors';
import { toOdyErrorPayload } from '../../../src/errors/serialize';
import type { LLMChatParams } from '../../../src/loop/llm';
import type { SDKAgentRPC } from '../../../src/rpc';

describe('RemoteKosongLLM', () => {
  it('forwards deltas and resolves with the streamed result', async () => {
    const sdk = {
      chatStreamInit: vi.fn(async () => ({ streamId: 's1' })),
      chatStreamCancel: vi.fn(),
    } as unknown as SDKAgentRPC;
    const llm = new RemoteKosongLLM({ sdk, modelName: 'm', systemPrompt: 's' });
    const onTextDelta = vi.fn();
    const onThinkDelta = vi.fn();
    const params: LLMChatParams = {
      messages: [],
      tools: [],
      signal: new AbortController().signal,
      onTextDelta,
      onThinkDelta,
    };

    const chatPromise = llm.chat(params);
    remoteLLMStreamRegistry.dispatchDelta({ streamId: 's1', delta: { type: 'text', text: 'hello' } });
    remoteLLMStreamRegistry.dispatchDelta({ streamId: 's1', delta: { type: 'think', think: '<think>' } });
    remoteLLMStreamRegistry.dispatchEnd({
      streamId: 's1',
      result: { toolCalls: [], usage: { totalTokens: 2 } as any },
    });

    const response = await chatPromise;
    expect(response.usage.totalTokens).toBe(2);
    expect(onTextDelta).toHaveBeenCalledWith('hello');
    expect(onThinkDelta).toHaveBeenCalledWith('<think>');
    expect(sdk.chatStreamCancel).not.toHaveBeenCalled();
  });

  it('cancels the stream when the signal aborts', async () => {
    const sdk = {
      chatStreamInit: vi.fn(async () => ({ streamId: 's2' })),
      chatStreamCancel: vi.fn(),
    } as unknown as SDKAgentRPC;
    const llm = new RemoteKosongLLM({ sdk, modelName: 'm', systemPrompt: 's' });
    const controller = new AbortController();
    const chatPromise = llm.chat({ messages: [], tools: [], signal: controller.signal });
    controller.abort();

    await expect(chatPromise).rejects.toThrow();
    expect(sdk.chatStreamCancel).toHaveBeenCalledWith({ streamId: 's2' });
  });

  it('rejects when the registry reports an error', async () => {
    const sdk = {
      chatStreamInit: vi.fn(async () => ({ streamId: 's3' })),
      chatStreamCancel: vi.fn(),
    } as unknown as SDKAgentRPC;
    const llm = new RemoteKosongLLM({ sdk, modelName: 'm', systemPrompt: 's' });
    const chatPromise = llm.chat({ messages: [], tools: [], signal: new AbortController().signal });

    remoteLLMStreamRegistry.dispatchError({
      streamId: 's3',
      error: toOdyErrorPayload(new OdyError(ErrorCodes.PROVIDER_API_ERROR, 'boom')),
    });

    await expect(chatPromise).rejects.toMatchObject({ code: 'provider.api_error' });
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
pnpm vitest run packages/agent-core/test/agent/turn/remote-kosong-llm.test.ts
```

Expected failure: 模块不存在。

- [ ] Write the minimal implementation. 创建 `packages/agent-core/src/agent/turn/remote-kosong-llm.ts`：

```typescript
import { isRetryableGenerateError } from '@odysseythink/kosong';

import { fromOdyErrorPayload, OdyError } from '#/errors';
import type {
  LLM,
  LLMChatParams,
  LLMChatResponse,
  ToolCallDelta,
} from '#/loop/llm';
import type { SDKAgentRPC } from '#/rpc';
import type {
  ChatStreamDeltaPayload,
  ChatStreamEndPayload,
  ChatStreamErrorPayload,
  ChatStreamRequest,
  ChatStreamResult,
  StreamDelta,
} from '#/rpc/llm-stream';
import type { CompletionBudgetConfig } from '#/utils/completion-budget';

export interface RemoteKosongLLMConfig {
  readonly sdk: SDKAgentRPC;
  readonly modelName: string;
  readonly systemPrompt: string;
  readonly capability?: string | undefined;
  readonly completionBudgetConfig?: CompletionBudgetConfig | undefined;
}

interface StreamHandlers {
  onDelta(delta: StreamDelta): void;
  onEnd(result: ChatStreamResult): void;
  onError(error: Error): void;
}

class RemoteLLMStreamRegistry {
  private readonly streams = new Map<string, StreamHandlers>();

  register(streamId: string, handlers: StreamHandlers): void {
    this.streams.set(streamId, handlers);
  }

  unregister(streamId: string): void {
    this.streams.delete(streamId);
  }

  dispatchDelta({ streamId, delta }: ChatStreamDeltaPayload): void {
    this.streams.get(streamId)?.onDelta(delta);
  }

  dispatchEnd({ streamId, result }: ChatStreamEndPayload): void {
    const handlers = this.streams.get(streamId);
    if (handlers === undefined) return;
    handlers.onEnd(result);
    this.unregister(streamId);
  }

  dispatchError({ streamId, error }: ChatStreamErrorPayload): void {
    const handlers = this.streams.get(streamId);
    if (handlers === undefined) return;
    handlers.onError(fromOdyErrorPayload(error));
    this.unregister(streamId);
  }
}

export const remoteLLMStreamRegistry = new RemoteLLMStreamRegistry();

export class RemoteKosongLLM implements LLM {
  readonly systemPrompt: string;
  readonly modelName: string;
  readonly capability?: string | undefined;

  private readonly sdk: SDKAgentRPC;
  private readonly completionBudgetConfig: CompletionBudgetConfig | undefined;

  constructor(config: RemoteKosongLLMConfig) {
    this.sdk = config.sdk;
    this.modelName = config.modelName;
    this.systemPrompt = config.systemPrompt;
    this.capability = config.capability;
    this.completionBudgetConfig = config.completionBudgetConfig;
  }

  async chat(params: LLMChatParams): Promise<LLMChatResponse> {
    const request = this.buildRequest(params);
    const { streamId } = await this.sdk.chatStreamInit({ request });

    const signal = params.signal;
    const onAbort = (): void => {
      this.sdk.chatStreamCancel({ streamId });
    };
    signal?.throwIfAborted();
    signal?.addEventListener('abort', onAbort, { once: true });

    try {
      return await new Promise<LLMChatResponse>((resolve, reject) => {
        remoteLLMStreamRegistry.register(streamId, {
          onDelta: (delta) => {
            if (signal?.aborted) return;
            this.forwardDelta(delta, params);
          },
          onEnd: (result) => {
            resolve(this.toLLMChatResponse(result));
          },
          onError: (error) => {
            reject(error);
          },
        });
      });
    } finally {
      signal?.removeEventListener('abort', onAbort);
      remoteLLMStreamRegistry.unregister(streamId);
    }
  }

  isRetryableError(error: unknown): boolean {
    return isRetryableGenerateError(error);
  }

  private buildRequest(params: LLMChatParams): ChatStreamRequest {
    return {
      modelName: this.modelName,
      systemPrompt: this.systemPrompt,
      messages: params.messages,
      tools: params.tools,
      capability: this.capability,
      completionBudgetConfig: this.completionBudgetConfig,
      requestLogContext: params.requestLogContext,
    };
  }

  private toLLMChatResponse(result: ChatStreamResult): LLMChatResponse {
    return {
      toolCalls: result.toolCalls,
      providerFinishReason: result.providerFinishReason,
      rawFinishReason: result.rawFinishReason,
      usage: result.usage,
      streamTiming: result.streamTiming,
    };
  }

  private forwardDelta(delta: StreamDelta, params: LLMChatParams): void {
    switch (delta.type) {
      case 'text':
        params.onTextDelta?.(delta.text);
        break;
      case 'think':
        params.onThinkDelta?.(delta.think);
        break;
      case 'tool_call_part': {
        const toolDelta: ToolCallDelta = {
          toolCallId: delta.toolCallId,
          name: delta.name,
          argumentsPart: delta.argumentsPart,
        };
        params.onToolCallDelta?.(toolDelta);
        break;
      }
    }
  }
}
```

注意：`capability` 在 kosong 类型中是 `ModelCapability`（字符串联合）。为简化导入，这里用 `string` 并依赖运行时透传；类型实际约束由 `LLMFactoryConfig` 保证，工厂传入的值必然是 `ModelCapability`。

修改 `packages/agent-core/src/agent/index.ts`，在文件末尾的 `export { buildGoalCompletionMessage }` 附近追加：

```typescript
export { RemoteKosongLLM, remoteLLMStreamRegistry } from './turn/remote-kosong-llm';
```

- [ ] Run it and verify it PASSES.

```bash
pnpm vitest run packages/agent-core/test/agent/turn/remote-kosong-llm.test.ts
```

Expected: 3 tests pass。

- [ ] Commit.

```bash
git add packages/agent-core/src/agent/turn/remote-kosong-llm.ts packages/agent-core/src/agent/index.ts packages/agent-core/test/agent/turn/remote-kosong-llm.test.ts
git commit -m "feat(agent-core): add RemoteKosongLLM for worker-side LLM proxy"
```

---

### Task 7: 实现 WorkerCoreAPI

**Depends on:** Task 5, Task 6

**Files:**
- Create: `packages/agent-core/src/rpc/worker-core.ts`
- Modify: `packages/agent-core/src/rpc/index.ts`（导出 `WorkerCoreAPI`）
- Create: `packages/agent-core/test/rpc/worker-core.test.ts`

**Goal:** 让 worker 内的 `KimiCore` 实现 `CoreAPI.chatStream*` 方法，把主线程推回的 delta/end/error 路由到 `RemoteKosongLLM` 流注册表。

- [ ] Write the failing test. 创建 `packages/agent-core/test/rpc/worker-core.test.ts`：

```typescript
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'pathe';
import { describe, expect, it, vi } from 'vitest';

import { remoteLLMStreamRegistry } from '../../../src/agent/turn/remote-kosong-llm';
import { WorkerCoreAPI } from '../../../src/rpc/worker-core';
import { createRPCEndpoint } from '../../../src/rpc/client';
import { createInProcessTransportPair, encodeJson } from '../../../src/rpc/transport';
import type { CoreAPI, SDKAPI } from '../../../src/rpc';

describe('WorkerCoreAPI stream routing', () => {
  it('routes chatStream* to the remote LLM registry', async () => {
    const tmpDir = mkdtempSync(join(tmpdir(), 'ody-worker-core-'));
    const endpoint = createRPCEndpoint<CoreAPI, SDKAPI>();
    const [transport] = createInProcessTransportPair(
      endpoint.dispatch,
      async () => encodeJson({ ok: true, value: undefined }),
    );
    endpoint.setTransport(transport);

    const core = new WorkerCoreAPI(endpoint.client, { homeDir: tmpDir });
    try {
      const onDelta = vi.fn();
      const onEnd = vi.fn();
      const onError = vi.fn();
      remoteLLMStreamRegistry.register('stream-1', { onDelta, onEnd, onError });

      core.chatStreamDelta({ streamId: 'stream-1', delta: { type: 'text', text: 'hi' } });
      core.chatStreamEnd({
        streamId: 'stream-1',
        result: { toolCalls: [], usage: { totalTokens: 1 } as any },
      });

      expect(onDelta).toHaveBeenCalledWith({ type: 'text', text: 'hi' });
      expect(onEnd).toHaveBeenCalled();
      expect(onError).not.toHaveBeenCalled();
    } finally {
      rmSync(tmpDir, { recursive: true, force: true });
    }
  });
});
```

- [ ] Run it and verify it FAILS.

```bash
pnpm vitest run packages/agent-core/test/rpc/worker-core.test.ts
```

Expected failure: `WorkerCoreAPI` 未定义。

- [ ] Write the minimal implementation. 创建 `packages/agent-core/src/rpc/worker-core.ts`：

```typescript
import { remoteLLMStreamRegistry } from '../agent/turn/remote-kosong-llm';
import type { CoreRPCClient } from './client';
import { KimiCore, type KimiCoreOptions } from './core-impl';
import type {
  ChatStreamDeltaPayload,
  ChatStreamEndPayload,
  ChatStreamErrorPayload,
} from './llm-stream';

export class WorkerCoreAPI extends KimiCore {
  constructor(rpcClient: CoreRPCClient, options: KimiCoreOptions = {}) {
    super(rpcClient, options);
  }

  chatStreamDelta(payload: ChatStreamDeltaPayload): void {
    remoteLLMStreamRegistry.dispatchDelta(payload);
  }

  chatStreamEnd(payload: ChatStreamEndPayload): void {
    remoteLLMStreamRegistry.dispatchEnd(payload);
  }

  chatStreamError(payload: ChatStreamErrorPayload): void {
    remoteLLMStreamRegistry.dispatchError(payload);
  }
}
```

修改 `packages/agent-core/src/rpc/index.ts`，在 `export * from './llm-stream';` 之后追加：

```typescript
export { WorkerCoreAPI } from './worker-core';
```

- [ ] Run it and verify it PASSES.

```bash
pnpm vitest run packages/agent-core/test/rpc/worker-core.test.ts
```

Expected: 1 test passes。

- [ ] Run whole-tree typecheck.

```bash
pnpm -r typecheck
```

Expected: 全 workspace 类型检查通过。

- [ ] Commit.

```bash
git add packages/agent-core/src/rpc/worker-core.ts packages/agent-core/src/rpc/index.ts packages/agent-core/test/rpc/worker-core.test.ts
git commit -m "feat(agent-core): add WorkerCoreAPI for worker-side CoreAPI stream handlers"
```

---

### Task 8: 创建 core-worker.ts worker 入口

**Depends on:** Task 3（`createRPCEndpoint`）、Task 5（`llmFactory`）、Task 6（`RemoteKosongLLM`）、Task 7（`WorkerCoreAPI`）

**Files:**
- Create: `packages/node-sdk/src/core-worker.ts`
- Modify: `packages/node-sdk/package.json`（如需要把 `core-worker.ts` 作为独立入口导出，供 `new Worker(...)` 使用）

**Goal:** 提供 worker 线程入口文件，接收 `MessagePort`，启动 `WorkerCoreAPI` 并把 kosong LLM 请求代理回主线程。

- [ ] Write the complete code. 创建 `packages/node-sdk/src/core-worker.ts`：

```typescript
import { isMainThread, parentPort, workerData, type MessagePort } from 'node:worker_threads';

import {
  createMessagePortTransport,
  createRPCEndpoint,
  WorkerCoreAPI,
  type CoreAPI,
  type SDKAPI,
  type SDKAgentRPC,
} from '@odysseythink/agent-core';
import { RemoteKosongLLM } from '@odysseythink/agent-core';
import { initializeTelemetry, type TelemetryBootstrapOptions } from '@odysseythink/telemetry';

export interface CoreWorkerBootPayload {
  readonly homeDir?: string | undefined;
  readonly configPath?: string | undefined;
  readonly skillDirs?: readonly string[];
  readonly appVersion?: string | undefined;
  readonly telemetry?: TelemetryBootstrapOptions | undefined;
}

export function coreWorkerMain(port: MessagePort, options: CoreWorkerBootPayload): void {
  if (options.telemetry !== undefined) {
    initializeTelemetry(options.telemetry);
  }

  const endpoint = createRPCEndpoint<CoreAPI, SDKAPI>();
  const transport = createMessagePortTransport(port, endpoint.dispatch);
  endpoint.setTransport(transport);

  const core = new WorkerCoreAPI(endpoint.client, {
    homeDir: options.homeDir,
    configPath: options.configPath,
    skillDirs: options.skillDirs,
    appVersion: options.appVersion,
    llmFactory: (rpc, config) =>
      new RemoteKosongLLM({
        sdk: rpc as SDKAgentRPC,
        ...config,
      }),
  });

  // WorkerCoreAPI + endpoint 开始处理来自主线程的 RPC 请求。
  void core;
}

if (!isMainThread && parentPort !== null && workerData !== undefined) {
  coreWorkerMain(parentPort, workerData as CoreWorkerBootPayload);
}
```

修改 `packages/node-sdk/package.json`：

如果 `package.json` 的 `exports` 中没有 worker 入口，在 `exports` 对象中追加（与 `.` 同级）：

```json
  "./core-worker": {
    "types": "./src/core-worker.ts",
    "default": "./src/core-worker.ts"
  }
```

这样 `SDKRpcClient` 可以通过 `new Worker(new URL('@odysseythink/ody-code-sdk/core-worker', import.meta.url))` 或解析为绝对路径的方式启动 worker。

- [ ] Build / manual verification. 运行类型检查确认 worker 入口编译通过：

```bash
pnpm -r typecheck
```

Expected: 全 workspace 类型检查通过。

- [ ] Commit.

```bash
git add packages/node-sdk/src/core-worker.ts packages/node-sdk/package.json
git commit -m "feat(node-sdk): add core-worker entry for MessagePort worker mode"
```

---

## Local Self-Review

- [ ] 1. Spec-coverage table：

| 设计需求 | 覆盖 Task | 状态 |
|---|---|---|
| worker 内 `KimiCore` 通过 `llmFactory` 注入 `RemoteKosongLLM` | T5, T6, T8 | covered |
| `RemoteKosongLLM` 跨 RPC 流式适配 | T6 | covered |
| 主线程 delta/end/error 路由回 worker | T7 | covered |
| worker 入口 `core-worker.ts` | T8 | covered |

- [ ] 2. Placeholder scan：T5-T8 均给出完整代码、命令与预期输出，无 `TODO`/`TBD`。
- [ ] 3. No phantom tasks：每个 Task 都有 Create/Modify/Test 文件或构建验证，无 `--allow-empty`。
- [ ] 4. Dependency soundness：T6 依赖 T5；T7 依赖 T5/T6；T8 依赖 T3/T5/T6/T7。
- [ ] 5. Caller & build soundness：T5 共享签名变更（`KimiCoreOptions`/`SessionOptions`/`AgentOptions`）后用 `pnpm -r typecheck` 全树检查；T7/T8 新增导出后同样跑 `pnpm -r typecheck`。
- [ ] 6. Test-the-risk：T5 测试工厂注入状态；T6 测试 delta/end/error/abort 行为；T7 测试 stream 路由状态。
- [ ] 7. Type consistency：`llmFactory` 签名在 `KimiCoreOptions`/`SessionOptions`/`AgentOptions` 中一致；`RemoteKosongLLMConfig` 与 `LLMFactoryConfig` 字段一致。
