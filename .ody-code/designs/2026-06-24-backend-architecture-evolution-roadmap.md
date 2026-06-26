# Phase 1-B Design: MessagePort Worker Boundary

> **对应路线图**: `.ody-code/roadmaps/backend-architecture-evolution-roadmap.md` Phase 1-B
> **设计状态**: DRAFT · **审计级别**: Deep · **目标文件**: `packages/agent-core/src/rpc/transports/message-port.ts`, `packages/node-sdk/src/core-worker.ts`, `packages/node-sdk/src/rpc.ts` 等 [C:USER]

---

## 设计决策摘要

- 默认启用 worker 模式;`SDKRpcClient` 构造选项 `transport: 'inproc' | 'worker'` 默认 `'worker'`。 [C:USER]
- `kosong` LLM 层完整留在主线程,worker 通过新增 `SDKAPI.chatStreamInit` / `chatStreamCancel` 与 `CoreAPI.chatStreamDelta` / `chatStreamEnd` / `chatStreamError` 完成流式 LLM 代理。 [C:USER]
- 崩溃语义:worker 异常退出时所有 pending RPC 返回结构化错误,UI/CLI 存活,用户手动新建会话;不自动恢复状态。 [C:USER]
- `AbortSignal` 不跨线程序列化,LLM 取消由 worker 调用 `chatStreamCancel(streamId)`,普通 RPC 取消沿用 transport 层 `cancel(callId)`。 [C:USER]
- 控制手段:`ODY_CORE_TRANSPORT=worker|inproc` 覆盖选项;`ODY_CORE_WORKER=0` 全局 kill switch;Node `resourceLimits.maxOldGenerationSizeMb` 堆上限。 [C:USER]

---

## Scope In/Out

### Scope In [C:USER]
1. `MessagePortTransport`:`packages/agent-core/src/rpc/transports/message-port.ts`,实现 `Transport` 接口,通过 `worker.postMessage` / `port.on('message')` 收发 `Uint8Array`。
2. Core worker 宿主:`packages/node-sdk/src/core-worker.ts`,在 `worker_thread` 内 boot `KimiCore`。
3. `SDKRpcClient` 增加 `transport: 'inproc' | 'worker'` 构造选项,默认 `'worker'`。
4. 崩溃语义:worker 异常退出时 pending RPC 返回结构化错误,主线程 UI/CLI 存活,用户手动重建会话。
5. 反向通道验证:`emitEvent`/`requestQuestion`/`requestApproval`/`toolCall`/`openExternal` 在 MessagePort 上行为与 InProc 一致。
6. 不可序列化 payload 审计:扫描 `CoreAPI`/`SDKAPI`,`AbortSignal` 由 transport 层 `cancel(callId)` 消息替代;函数/流不得穿越边界。
7. LLM 安全代理:`kosong` 完整留在主线程;worker 通过新增 SDKAPI/CoreAPI 流式方法调用主线程完成每次 LLM 请求。

### Scope Out [C:USER]
- **Socket transport / `ody serve`**:属于 Phase 2-B,本阶段只到 MessagePort。
- **Native Rust 模块(napi-rs)**:Phase 2-E,与 worker 边界独立。
- **agent-core 拆包**:Phase 1-C,独立轨道。
- **mode 统一**:Phase 2-D。
- **Rust Host 反转**:Phase 3,依赖本阶段产出但不在本阶段实现。
- **worker 自动重启与会话状态恢复**:崩溃后手动重建会话;自动恢复 defer。
- **多 worker / worker pool**:单 worker 单 harness。

---

## Architecture

```
主线程 (Node CLI/TUI/SDK)
  ├─ KimiHarness / Session / ClientAPI
  ├─ SDKRpcClient ──Transport──┐
  ├─ kosong ProviderManager ◄───┤  LLM 代理
  └─ MessagePort (main side)     │
                                  │
worker_thread                     │
  └─ core-worker.ts               │
       ├─ MessagePort (worker side)
       ├─ KimiCore                │
       ├─ Session / Agent / TurnFlow
       └─ RemoteKosongLLM ───────┘
```

数据流箭头 [C:USER]:
- `KimiHarness.createSession()` → `SDKRpcClient` (主线程) → `MessagePortTransport.send()` → worker `MessagePortTransport` → `KimiCore.createSession()` → 创建 `Session`/`Agent`。
- `Agent.emitEvent()` (worker) → worker `MessagePortTransport.send()` → 主线程 `MessagePortTransport` → `ClientAPI.emitEvent()` → 主线程事件监听器。
- `RemoteKosongLLM.chat()` (worker) → `sdk.chatStreamInit()` → 主线程 `ClientAPI.chatStreamInit()` → `KosongLLM.chat()` → 每个 delta 通过 `core.chatStreamDelta()` 推回 worker → worker 回调 `onTextDelta/onThinkDelta/onToolCallDelta`。

---

## Data Models

### `MessagePortTransport` [C:INFERRED]
位置:`packages/agent-core/src/rpc/transports/message-port.ts`
```ts
interface MessagePortTransportOptions {
  port: MessagePort;
  onError?: (error: Error) => void;
  onWire?: (direction: 'send' | 'recv', bytes: Uint8Array) => void;
}
function createMessagePortTransport(port: MessagePort): Transport;
```

### Core worker boot payload [C:USER]
位置:`packages/node-sdk/src/core-worker.ts`
```ts
interface CoreWorkerBootPayload {
  homeDir?: string;
  configPath?: string;
  skillDirs?: string[];
  appVersion?: string;
  telemetry?: TelemetryConfig;
}
function coreWorkerMain(port: MessagePort): void;
```

### `SDKRpcClientOptions` 扩展 [C:USER]
位置:`packages/node-sdk/src/rpc.ts`
```ts
interface SDKRpcClientOptions {
  // ... existing fields ...
  transport?: 'inproc' | 'worker';
  workerPath?: string;
  workerResourceLimits?: ResourceLimits;
}
```

### 新增 RPC 契约(单独 PR,遵守契约冻结) [C:USER]
`SDKAPI` 新增:
```ts
chatStreamInit: (request: ChatStreamRequest) => Promise<{ streamId: string }>;
chatStreamCancel: (payload: { streamId: string }) => void;
```
`CoreAPI` 新增:
```ts
chatStreamDelta: (payload: { streamId: string; delta: StreamDelta }) => void;
chatStreamEnd:   (payload: { streamId: string; result: ChatStreamResult }) => void;
chatStreamError: (payload: { streamId: string; error: OdyErrorPayload }) => void;
```
类型 [C:INFERRED]:
```ts
interface ChatStreamRequest {
  modelName: string;
  systemPrompt: string;
  messages: Message[];
  tools: Tool[];
  capability?: ModelCapability;
  completionBudgetConfig?: CompletionBudgetConfig;
  requestLogContext?: LLMRequestLogContext;
}
type StreamDelta =
  | { type: 'text'; text: string }
  | { type: 'think'; think: string }
  | { type: 'tool_call_part'; toolCallId: string; name?: string; argumentsPart?: string };
interface ChatStreamResult {
  toolCalls: ToolCall[];
  providerFinishReason?: FinishReason;
  rawFinishReason?: string;
  usage: TokenUsage;
  streamTiming?: LLMStreamTiming;
}
```

### Worker 侧 Remote LLM [C:INFERRED]
位置:`packages/agent-core/src/agent/turn/remote-kosong-llm.ts`
```ts
interface RemoteKosongLLMConfig {
  sdk: SDKRPC;
  modelName: string;
  systemPrompt: string;
  capability?: ModelCapability;
  completionBudgetConfig?: CompletionBudgetConfig;
}
class RemoteKosongLLM implements LLM {
  constructor(config: RemoteKosongLLMConfig);
  chat(params: LLMChatParams): Promise<LLMChatResponse>;
  isRetryableError(error: unknown): boolean;
}
```

### 主线程 LLM 代理服务 [C:INFERRED]
位置:`packages/node-sdk/src/rpc.ts` 内 `ClientAPI`
```ts
class ClientAPI implements PromisableMethods<SDKAPI> {
  async chatStreamInit(request: ChatStreamRequest): Promise<{ streamId: string }>;
  chatStreamCancel(payload: { streamId: string }): void;
}
```

---

## Algorithms

### 算法 1:SDKRpcClient 启动(worker 模式) [C:USER]
位置:`packages/node-sdk/src/rpc.ts:136` 附近
```
function bootSDKRpcClient(options):
  transportMode = resolveTransportMode(options.transport)
  if transportMode == 'inproc':
    return bootInProc(options)

  channel = new MessageChannel()
  workerPath = resolveWorkerPath(options.workerPath)
  worker = new Worker(workerPath, {
    workerData: buildWorkerBootPayload(options),
    transferList: [channel.port2],
    resourceLimits: options.workerResourceLimits ?? defaultResourceLimits(),
  })
  mainTransport = createMessagePortTransport(channel.port1)
  [coreRpcUnused, sdkRpc] = createRPC<CoreAPI, SDKAPI>({
    transport: [noopTransport, mainTransport],
  })
  core = null  // KimiCore 在 worker 内
  ready = sdkRpc(new ClientAPI(this, worker)).then(rpc => this.rpc = rpc)
  attachWorkerExitHandlers(worker, mainTransport)
  return { core, ready }
```

### 算法 2:MessagePortTransport 请求-响应关联 [C:INFERRED]
位置:`packages/agent-core/src/rpc/transports/message-port.ts`
```
class MessagePortTransport:
  pending = Map<reqId, Deferred<Uint8Array>>()

  constructor(port, dispatch: Dispatch):
    port.on('message', msg => this.handleMessage(msg, dispatch))

  send(bytes):
    reqId = generateRequestId()
    deferred = createDeferred<Uint8Array>()
    pending.set(reqId, deferred)
    port.postMessage({ kind: 'request', reqId, bytes })
    return deferred.promise

  async handleMessage(msg, dispatch):
    if msg.kind == 'request':
      responseBytes = await dispatch(msg.bytes)
      port.postMessage({ kind: 'response', reqId: msg.reqId, bytes: responseBytes })
    else if msg.kind == 'response':
      deferred = pending.get(msg.reqId)
      pending.delete(msg.reqId)
      if deferred == null: return
      deferred.resolve(msg.bytes)

  close():
    port.close()
    for deferred in pending.values():
      deferred.reject(new OdyError(TRANSPORT_CLOSED, 'MessagePort closed'))
    pending.clear()
```

### 算法 3:Core worker 启动 [C:USER]
位置:`packages/node-sdk/src/core-worker.ts`
```
function coreWorkerMain():
  port = workerData.port
  options = workerData.options
  workerTransport = createMessagePortTransport(port)
  [coreRpc, sdkRpcUnused] = createRPC<CoreAPI, SDKAPI>({
    transport: [workerTransport, noopTransport],
  })
  kimiCore = new KimiCore(coreRpc, {
    ...options,
    llmFactory: remoteLlmFactory,
  })
```

### 算法 4:流式 LLM 代理(worker ↔ 主线程) [C:INFERRED]
Worker 侧 `RemoteKosongLLM.chat()`:
```
async chat(params):
  streamId = null
  try:
    { streamId } = await this.sdk.chatStreamInit(buildRequest(params))
    if params.signal != null:
      params.signal.throwIfAborted()
      onAbort = () => this.sdk.chatStreamCancel({ streamId })
      params.signal.addEventListener('abort', onAbort, { once: true })

    result = await new Promise((resolve, reject) => {
      const cleanup = () => { /* 注销 delta/end/error 监听 */ }
      registerCoreHandler('chatStreamDelta', { streamId }, (delta) => {
        forwardDelta(delta, params)
      })
      registerCoreHandler('chatStreamEnd', { streamId }, (result) => {
        cleanup(); resolve(result)
      })
      registerCoreHandler('chatStreamError', { streamId }, (error) => {
        cleanup(); reject(fromOdyErrorPayload(error))
      })
    })
    return buildLLMChatResponse(result)
  finally:
    if streamId != null: params.signal?.removeEventListener('abort', onAbort)
```
主线程 `ClientAPI.chatStreamInit()`:
```
async chatStreamInit(request):
  streamId = generateStreamId()
  provider = this.providerManager.resolveProviderConfig(request.modelName)
  llm = new KosongLLM({
    provider,
    modelName: request.modelName,
    systemPrompt: request.systemPrompt,
    capability: request.capability,
    completionBudgetConfig: request.completionBudgetConfig,
  })
  abortController = new AbortController()
  this.streams.set(streamId, { abortController, llm })

  llm.chat({
    messages: request.messages,
    tools: request.tools,
    signal: abortController.signal,
    requestLogContext: request.requestLogContext,
    onTextDelta: text => this.coreRpc.chatStreamDelta({
      streamId, delta: { type: 'text', text }
    }),
    onThinkDelta: think => this.coreRpc.chatStreamDelta({
      streamId, delta: { type: 'think', think }
    }),
    onToolCallDelta: delta => this.coreRpc.chatStreamDelta({
      streamId, delta: { type: 'tool_call_part', ...delta }
    }),
  }).then(
    response => this.coreRpc.chatStreamEnd({ streamId, result: response }),
    error => this.coreRpc.chatStreamError({ streamId, error: toOdyErrorPayload(error) }),
  )

  return { streamId }
```

### 算法 5:worker 崩溃检测 [C:USER]
位置:`packages/node-sdk/src/rpc.ts`
```
attachWorkerExitHandlers(worker, transport):
  worker.on('error', error => {
    transport.onError?.(new OdyError(WORKER_ERROR, error.message, { cause: error }))
  })
  worker.on('exit', exitCode => {
    if exitCode != 0:
      transport.onError?.(new OdyError(WORKER_EXITED, `Core worker exited with ${exitCode}`))
  })
```

---

## Error Handling

| 错误类 | 立即处理 | 降级路径 | 恢复条件 |
|---|---|---|---|
| Worker 启动失败 [C:USER] | 抛出 `WORKER_SPAWN_FAILED` | 自动降级为 `inproc` 并打印 warning | 修复 worker 构建或显式设 `transport='inproc'` |
| Worker 异常退出 [C:USER] | `transport.onError` reject 所有 pending RPC | UI/CLI 存活;用户手动重建会话 | 用户重新调用 `createSession` |
| MessagePort 消息损坏 [C:INFERRED] | `transport.onError`;关闭 port | pending RPC 全部 reject | 视为 worker 崩溃 |
| LLM 流式取消 [C:USER] | worker 调用 `chatStreamCancel` → 主线程 `abortController.abort()` | 流停止;worker 抛出 `AbortError` | 同会话可重试 prompt |
| 主线程 kosong 错误 [C:INFERRED] | 主线程调用 `core.chatStreamError` | worker 内 RemoteKosongLLM 抛出错误;外层 retry 处理 | 按现有 retry 策略 |
| 检测到不可序列化 payload [C:USER] | CI/运行期断言失败 | 阻断 transport 激活 | 修复 payload 类型 |

---

## Testing & Done Criteria

### 测试文件与断言 [C:USER][C:INFERRED]
1. `packages/agent-core/test/rpc/message-port-transport.test.ts`
   - `assert(await transport.send(request) === response)`:请求-响应关联正确。
   - `assert(rejected with TRANSPORT_CLOSED)`:close() 后 pending 全部拒绝。
   - `assert(onWire('send') && onWire('recv'))`:wire trace 工作。
2. `packages/node-sdk/test/core-worker.test.ts`
   - `assert(kimiCore.homeDir === options.homeDir)`:worker 内 KimiCore 正确初始化。
   - `assert(await rpc.createSession(...) has id)`:CoreAPI 调用跨 worker 往返成功。
   - `assert(event received === event sent)`:emitEvent 反向通道等价。
3. `packages/node-sdk/test/llm-proxy.test.ts`
   - `assert(typeof streamId === 'string')`:chatStreamInit 返回 streamId。
   - `assert(deltas in order)`:delta 按 kosong 产出顺序到达 worker。
   - `assert(response.usage.totalTokens > 0)`:chatStreamEnd 后正确组装响应。
   - `assert(kosongGenerate aborted)`:chatStreamCancel 触发 abort。
4. `packages/node-sdk/test/worker-crash-isolation.test.ts`
   - `assert(pendingCreateSession rejects with WORKER_EXITED)`:worker 退出 pending RPC 返回结构化错误。
   - `assert(sdkRpcClient.close() does not throw)`:主线程对象可正常清理。
   - `assert(process.exitCode is undefined)`:CLI 进程不因 worker 崩溃退出。
5. Golden parity:`packages/agent-core/test/rpc/transport-parity.test.ts` 扩展,worker transport 下重跑 golden message 流,断言 wire bytes 与 inproc 一致。
6. 不可序列化 payload 审计:静态扫描 `CoreAPI`/`SDKAPI` 类型,断言无 `Function`/`AbortSignal`/`ReadableStream` 字段。

### Done 标准 [C:USER]
- `pnpm test --filter @odysseythink/agent-core --filter @odysseythink/ody-code-sdk` 全绿。
- `ODY_CORE_TRANSPORT=worker pnpm test --filter @odysseythink/ody-code-sdk` 全绿。
- G1-B 基准:MessagePort 单次往返 P95 < 单次 LLM 首字节延迟 1%(阈值由 G1-B 门运行期确定)。
- `ODY_CORE_WORKER=0` 强制 inproc 生效,旧 inproc 路径仍绿。

---

## Risk Register

| 编号 | 风险 | 可能性 | 影响 | 缓解 |
|---|---|---|---|---|
| R1 | createRPC 单端使用(no-op transport)hacky | 中 | 中 | 文档说明;后续可重构为 `createRPCEndpoint` |
| R2 | LLM 每次请求多一次 RPC 往返,增加首字节延迟 | 高 | 中 | G1-B 量化;超阈值可改为批量推送 |
| R3 | SEA/pkg 打包时 worker 入口文件丢失 | 中 | 高 | CI 加 SEA smoke test;worker 路径可配置 |
| R4 | 遗漏不可序列化 payload | 中 | 高 | CI 静态扫描 + golden parity + 运行时 JSON-safe 断言 |
| R5 | 流式取消竞态:cancel 后仍有 delta 到达 | 中 | 中 | 按 streamId 过滤;cancel 后忽略该 stream 后续消息 |
| R6 | worker 内存泄漏/OOM | 低 | 高 | `resourceLimits.maxOldGenerationSizeMb`;长会话压力测试 |

---

## Reuse Analysis

| 候选组件 | 文件路径 | 复用方式 | 说明 |
|---|---|---|---|
| `Transport` 接口与 `createInProcessTransportPair` | `packages/agent-core/src/rpc/transport.ts` | 直接使用 + 新增实现 | `Transport` 接口(`send(bytes): Promise<Uint8Array>`)已存在;`MessagePortTransport` 是其新实现,`InProcessTransport` 保留为默认回退。 [C:UPSTREAM] |
| `createRPC` 与 RPC 客户端工厂 | `packages/agent-core/src/rpc/client.ts` | 适配使用 | 跨 worker 使用时每个进程只取 createRPC 返回的一对 client 中的一个;未使用侧用 no-op transport 占位。 [C:INFERRED] |
| `SDKRpcClient` / `ClientAPI` | `packages/node-sdk/src/rpc.ts` | 扩展 | 增加 transport 选项、worker 生命周期管理、LLM 代理方法。 [C:USER] |
| `KosongLLM` | `packages/agent-core/src/agent/turn/kosong-llm.ts` | 主线程保留 | worker 内不再直接实例化;主线程 `ClientAPI.chatStreamInit` 继续使用。 [C:USER] |
| `KimiCore` / `Session` / `Agent` | `packages/agent-core/src/rpc/core-impl.ts` 等 | 整体移入 worker | 逻辑不变,仅 boot 位置与 LLM 实现方式改变。 [C:USER] |
| `LLM` 接口 | `packages/agent-core/src/loop/llm.ts` | 新增实现 | worker 内新增 `RemoteKosongLLM` 实现 `LLM` 接口。 [C:INFERRED] |

无可用现成组件之处(greenfield):
- `MessagePortTransport` 请求-响应关联实现。
- `RemoteKosongLLM` 跨 RPC 流式适配器。
- `core-worker.ts` worker 入口文件。

---

## Assumptions & Unverified Items

| # | 假设 | 置信度 | 错误影响 | 验证方式 |
|---|---|---|---|---|
| A1 | `CoreAPI`/`SDKAPI` 现有 payload 已 JSON-safe(无函数/流/AbortSignal) | 高 | 低 | 静态扫描 + 运行时断言;explore 已确认无函数/流,`AbortSignal` 在本地处理。 |
| A2 | `createRPC` 可通过 no-op transport 实现单端使用,不破坏内部 pending/error 处理 | 中 | 中 | 写单元测试验证：单端 createRPC + MessagePortTransport 可正常 call/return/throw error。 |
| A3 | `worker_thread` 支持运行完整的 `KimiCore` + `Agent` + `TurnFlow`,包括其所有同步/异步初始化 | 高 | 高 | `core-worker.test.ts` 端到端 boot 并创建会话。 |
| A4 | `MessagePort` 传输 `Uint8Array` 的性能满足 G1-B 阈值(< LLM 首字节 1%) | 中 | 中 | G1-B 基准测试测量往返 latency。 |
| A5 | 主线程代理 kosong 时,流式 delta 的 RPC 往返不会显著劣化首字节/流式体验 | 中 | 高 | `llm-proxy.test.ts` 端到端测试 + G1-B 主观/客观评估。 |
| A6 | SDK 选项对象(`homeDir`, `configPath`, `skillDirs`, `telemetry` config 等)可被 `workerData` 结构化克隆 | 高 | 中 | 实现时序列化/反序列化测试;复杂对象(如 telemetry client)只传配置不整个传递。 |
| A7 | worker 入口文件路径可通过 `__dirname`/相对路径在构建后可靠解析 | 中 | 高 | SEA smoke test + 可配置 `workerPath` 兜底。 |
| A8 | 新增 `chatStream*` RPC 方法可被单独评审并入 `CoreAPI`/`SDKAPI`,不违反契约冻结纪律 | 高 | 中 | 设计文档已声明单独 PR;用户确认。 |

---

## Self-Review

**高赌注决策 scrutiny**

1. **createRPC 单端使用(no-op transport)**: [C:INFERRED]
   - 正常输入:主线程 `createRPC({ transport: [noop, mainTransport] })` 只使用 `sdkRpc`;worker `createRPC({ transport: [workerTransport, noop] })` 只使用 `coreRpc`。
   - 期望:正常调用走 MessagePort,响应正确返回。
   - 对抗输入:代码意外调用了未使用侧的 client(如主线程调用 `coreRpc`)。
   - 期望:立即抛出 "unused transport side",在测试/开发期暴露,不静默失败。

2. **MessagePortTransport 请求-响应关联**: [C:INFERRED]
   - 正常输入:并发发送 req-1、req-2,响应按 reqId 返回。
   - 期望:req-1 的响应不会进入 req-2 的 promise。
   - 对抗输入:收到未知 reqId 的响应(超时后到达、恶意/损坏消息)。
   - 期望:忽略该响应,不污染其他 pending;记录 warning。

3. **worker 启动失败回退 inproc**: [C:USER]
   - 正常输入:`workerPath` 不存在或 `new Worker()` 抛出。
   - 期望:降级为 inproc 并打印 warning,CLI 仍可启动。
   - 对抗输入:用户同时设置 `transport='worker'` 与 `ODY_CORE_WORKER=0`。
   - 期望:`ODY_CORE_WORKER=0` 优先,kill switch 必须绝对生效。

**四透镜扫描**

- **Security**:检查 worker 不得长期持有 API key/secrets;`buildWorkerBootPayload` 必须只传可结构化克隆的纯配置,函数/Provider/Client 实例留在主线程。发现 `SDKRpcClientOptions` 中的 `resolveOAuthTokenProvider` 是函数,必须留在主线程由 `ClientAPI` 使用,不能序列化到 worker。已在 Scope/Data 模型中明确。
- **Test**:每个行为都有 must-pass 断言;补充 must-reject 用例——静态扫描若发现 `CoreAPI`/`SDKAPI` 出现 `Function`/`AbortSignal`/`ReadableStream` 字段则 CI 失败。
- **Ops**:`generateRequestId()` 需保证唯一性,建议使用 `crypto.randomUUID()` 或自增大整数 + worker 启动时间戳,避免响应路由错乱;`resourceLimits` 必须设置堆上限,并在压力测试中观测 OOM。
- **Integration**:已验证 `Transport` 接口(`packages/agent-core/src/rpc/transport.ts:1`)、`createInProcessTransportPair`(`transport.ts:31`)、`SDKRpcClient`/`ClientAPI`(`packages/node-sdk/src/rpc.ts:125`)、`KimiCore`(`packages/agent-core/src/rpc/core-impl.ts:137`)、`KosongLLM`(`packages/agent-core/src/agent/turn/kosong-llm.ts:68`)、`LLM` 接口(`packages/agent-core/src/loop/llm.ts:71`) 均存在。需新增 `KimiCoreOptions.llmFactory` 字段以支持 worker 内 RemoteKosongLLM 注入。
- **Scope**:本设计仍围绕单一子系统"MessagePort worker 边界"展开;LLM 代理是因安全模型衍生的必要子模块,不拆分为独立设计。

---

## User Final Approval

- **审计级别**: Deep
- **Section key claims**: 全部确认 [C:USER]
- **Assumptions A1-A4**: 全部接受 [C:USER]
- **Assumptions A5-A8**: 全部接受 [C:USER]
- **最终状态**: ✅ 设计已批准
- **批准时间**: 2026-06-24
- **下一步**: 建议运行 `/plan` 将本设计转化为具体实施计划。
