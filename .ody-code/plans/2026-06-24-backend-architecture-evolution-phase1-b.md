# Phase 1-B: MessagePort Worker Boundary Implementation Plan

> **Goal:** 在 `@odysseythink/ody-code-sdk` 中默认通过 `worker_threads` + `MessagePort` 运行 `KimiCore`，把 kosong LLM 层留在主线程，通过新增 `chatStream*` RPC 方法完成跨线程流式代理，并保证 worker 崩溃时主线程 UI/CLI 存活。
>
> **Architecture:** 主线程 `SDKRpcClient` 根据 `transport: 'worker' | 'inproc'` 决定是启动 worker 还是沿用进程内 `KimiCore`；worker 内通过 `MessagePortTransport` 与主线程交换 RPC 字节，`RemoteKosongLLM` 把每次 LLM 调用转发给主线程 `ClientAPI.chatStreamInit`，主线程用 `KosongLLM` 产生 delta 后通过 `CoreAPI.chatStreamDelta/End/Error` 推回 worker。崩溃语义由 transport 层 `onError` 与 worker `exit` 事件共同保证，所有 pending RPC 统一 reject 为结构化 `OdyError`。
>
> **Tech Stack:** TypeScript 5.x, Node.js `worker_threads` + `MessageChannel`, Vitest, pnpm workspace, `@odysseythink/agent-core` RPC/Transport 抽象。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| File | Responsibility |
|---|---|
| `packages/agent-core/src/errors/codes.ts` (Modify) | 新增 `WORKER_SPAWN_FAILED` / `WORKER_EXITED` / `TRANSPORT_CLOSED` 错误码及元数据。 |
| `packages/agent-core/src/rpc/transports/message-port.ts` (Create) | `MessagePortTransport`：基于 Node `MessagePort` 的 `Transport` 实现，请求-响应按 `reqId` 关联。 |
| `packages/agent-core/test/rpc/transports/message-port-transport.test.ts` (Create) | `MessagePortTransport` 单元测试：并发、close 拒绝 pending、onWire。 |
| `packages/agent-core/src/rpc/client.ts` (Modify) | 新增 `createRPCEndpoint` 与 `noopTransport`，支持单端 RPC 绑定。 |
| `packages/agent-core/test/rpc/create-rpc-endpoint.test.ts` (Create) | 单端 endpoint 与 MessagePort 集成测试。 |
| `packages/agent-core/src/rpc/llm-stream.ts` (Create) | `ChatStreamRequest` / `StreamDelta` / `ChatStreamResult` 等跨线程 LLM 流类型。 |
| `packages/agent-core/src/rpc/core-api.ts` (Modify) | 在 `CoreAPI` 中新增 `chatStreamDelta` / `chatStreamEnd` / `chatStreamError`。 |
| `packages/agent-core/src/rpc/sdk-api.ts` (Modify) | 在 `SDKAPI` 中新增 `chatStreamInit` / `chatStreamCancel`。 |
| `packages/agent-core/src/agent/turn/remote-kosong-llm.ts` (Create) | `RemoteKosongLLM`：worker 侧 LLM 实现，通过 RPC 调用主线程完成流式请求。 |
| `packages/agent-core/src/agent/turn/remote-kosong-llm.ts` (Modify, same file) | 流注册表 `RemoteLLMStreamRegistry`，供 `WorkerCoreAPI.chatStream*` 路由。 |
| `packages/agent-core/test/agent/turn/remote-kosong-llm.test.ts` (Create) | `RemoteKosongLLM` 单元测试：delta/end/error/abort。 |
| `packages/agent-core/src/rpc/core-impl.ts` (Modify) | `KimiCoreOptions` 新增 `llmFactory`；透传给 `SessionOptions`。 |
| `packages/agent-core/src/session/index.ts` (Modify) | `SessionOptions` / `Session` 透传 `llmFactory` 到 `AgentOptions`。 |
| `packages/agent-core/src/agent/index.ts` (Modify) | `AgentOptions` 新增 `llmFactory`；`Agent.llm` 优先使用工厂，默认回退 `KosongLLM`。 |
| `packages/agent-core/src/rpc/worker-core.ts` (Create) | `WorkerCoreAPI extends KimiCore`，实现 `CoreAPI` 的 `chatStream*` 反向方法并委托给流注册表。 |
| `packages/agent-core/test/rpc/worker-core.test.ts` (Create) | `WorkerCoreAPI` 单元测试：stream 方法正确路由到注册表。 |
| `packages/node-sdk/src/core-worker.ts` (Create) | worker 线程入口：反序列化 `workerData`，创建 `MessagePortTransport` + `createRPCEndpoint`，启动 `WorkerCoreAPI`。 |
| `packages/node-sdk/src/rpc.ts` (Modify) | `SDKRpcClientOptions` 增加 transport/worker 选项；`SDKRpcClient` 按模式启动 worker 或 inproc；管理 worker 生命周期。 |
| `packages/node-sdk/src/rpc.ts` (Modify, same file) | `ClientAPI` 新增 `chatStreamInit` / `chatStreamCancel`，主线程代理 kosong 流式请求。 |
| `packages/node-sdk/test/core-worker.test.ts` (Create) | 端到端：worker 内 `KimiCore` 初始化、`createSession` 往返、`emitEvent` 反向通道。 |
| `packages/node-sdk/test/llm-proxy.test.ts` (Create) | 端到端：流式 LLM 代理的 delta 顺序、响应组装、取消 abort。 |
| `packages/node-sdk/test/worker-crash-isolation.test.ts` (Create) | worker 异常退出时 pending RPC reject、主线程可清理、进程不退出。 |
| `packages/agent-core/test/rpc/transport-parity.test.ts` (Modify) | 扩展 golden parity：MessagePortTransport 与 inproc 线消息语义一致。 |
| `packages/agent-core/test/rpc/serializable-api.type.test.ts` (Create) | 类型级断言：`CoreAPI` / `SDKAPI` 所有 payload/return 可 JSON 序列化。 |

> **File path notes:** 当前仓库中 `SDKRpcClient` 与 `ClientAPI` 都位于 `packages/node-sdk/src/rpc.ts`，因此 Part 3 的所有修改都针对该文件（不新建 `sdk-rpc-client.ts`）；`packages/node-sdk/test/` 与 `packages/agent-core/test/` 是现有测试目录，新增测试应放在对应目录下。

---

## Dependency Overview

```text
Part 1: transport.md
  ├─ T1 错误码
  ├─ T2 MessagePortTransport
  ├─ T3 createRPCEndpoint/noopTransport
  └─ T4 ChatStream 类型 + API 扩展

Part 2: worker-core.md
  ├─ T5 llmFactory 注入 (KimiCore → Session → Agent)
  ├─ T6 RemoteKosongLLM + 流注册表
  ├─ T7 WorkerCoreAPI
  └─ T8 core-worker.ts
        └─ depends T3, T5, T6, T7

Part 3: sdk-client.md
  ├─ T9 ClientAPI.chatStream* + 主线程 kosong 代理
  └─ T10 SDKRpcClient transport/worker 模式
        └─ depends T8, T9

Part 4: integration.md
  ├─ T12 MessagePortTransport golden parity + 可序列化 payload 审计
  ├─ T13 Agent llmFactory 注入路径测试
  ├─ T14 core-worker LLM 代理端到端测试（含 ready 信号）
  ├─ T15 worker crash 隔离与自动降级测试
  └─ T16 全仓库类型检查 / 测试 / changeset / 文档
        └─ depends T4, T10, T12, T13, T14, T15
```

Part 1 内部：T1 可被 T2/T3 并行依赖（错误码在单元测试中引用），T4 可被 T2/T3 并行阅读，但实际代码不强制依赖。为顺序清晰，Part 1 按 T1→T2→T3→T4 串行执行。
Part 2 内部：T5 与 T6 可并行；T7 依赖 T5/T6；T8 依赖 T3/T7。
Part 3 内部：T9 可先于 T10；T10 依赖 T8/T9。
Part 4 全部依赖 Part 2/3 完成后进行。

---

## Risks & Open Questions

| # | Risk | Mitigation in Plan |
|---|---|---|
| R1 | `createRPCEndpoint` 单端绑定破坏 `createRPC` 内部 pending/error 语义 | T3 单端 endpoint 测试覆盖 call/return/throw/onError；T14 parity 重跑 golden 消息流。 |
| R2 | `llmFactory` 注入路径遗漏某个 `new Agent`/`new Session`/`new KimiCore` 调用方 | T5 使用 `grep -rn` 更新所有调用方，并以 `pnpm -r typecheck` 全树检查收尾。 |
| R3 | Worker 启动失败或 SEA/pkg 打包时 `core-worker.ts` 入口丢失 | T10 实现可配置 `workerPath` + spawn 失败自动降级 inproc；T13 覆盖崩溃路径；CI 后续补充 SEA smoke test。 |
| R4 | 新增 `chatStream*` RPC 契约不可序列化 | T4 类型定义全部使用 plain JSON 对象；T15 类型级审计扫描 `Function`/`AbortSignal`/`ReadableStream`。 |
| R5 | 流式取消竞态：cancel 后仍有 delta 到达 | T6/T9 按 `streamId` 过滤，cancel 后忽略该 stream 后续消息；T12 断言 cancel 触发 abort。 |
| R6 | worker OOM 或内存泄漏 | T10 设置默认 `resourceLimits.maxOldGenerationSizeMb`；T13 崩溃测试验证 onError 传播。 |
| R7 | 主线程 auth/wrapper 与现有 `Agent.generate` 行为不一致 | T9 在 `ClientAPI.chatStreamInit` 中复用 `resolveOAuthTokenProvider` 生成 auth 包装器，与 `Agent.generate` 等价。 |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-24-backend-architecture-evolution-phase1-b/transport.md` | MessagePort Transport + RPC endpoint + ChatStream 契约 | done |
| 2 | `2026-06-24-backend-architecture-evolution-phase1-b/worker-core.md` | Remote LLM + WorkerCoreAPI + core-worker 入口 | done |
| 3 | `2026-06-24-backend-architecture-evolution-phase1-b/sdk-client.md` | ClientAPI LLM 代理 + SDKRpcClient worker 模式 | done |
| 4 | `2026-06-24-backend-architecture-evolution-phase1-b/integration.md` | 集成测试 + parity + payload 审计 + 最终验证 | done |

---

## Spec-Coverage Table

| 设计 Scope In / 需求 | 覆盖 Part:Task | 状态 |
|---|---|---|
| `MessagePortTransport` 实现 `Transport` 接口，通过 `postMessage` 收发 `Uint8Array` | Part 1:T2 | covered |
| Core worker 宿主 `core-worker.ts` 在 worker 线程 boot `KimiCore` | Part 2:T8 | covered |
| `SDKRpcClient` 增加 `transport: 'inproc' \| 'worker'`，默认 `'worker'` | Part 3:T10 | covered |
| Worker 崩溃语义：pending RPC 返回结构化错误，UI/CLI 存活 | Part 4:T15 | covered |
| 反向通道验证：`emitEvent`/`requestQuestion`/`requestApproval`/`toolCall`/`openExternal` 跨 worker 行为一致 | Part 4:T14 | covered |
| 可序列化 payload 审计：`AbortSignal` 由 `chatStreamCancel(streamId)` 替代，函数/流/类实例不穿越边界 | Part 1:T4 / Part 4:T12 | covered |
| LLM 安全代理：kosong 留在主线程，worker 通过新增 `chatStream*` RPC 完成每次请求 | Part 2:T6,T7 / Part 3:T9 / Part 4:T14 | covered |
| 控制手段：`ODY_CORE_TRANSPORT`、`ODY_CORE_WORKER=0`、堆上限 | Part 3:T10 | covered |

---

## Self-Review

- [ ] 1. Spec-coverage table：每条设计 In-Scope 需求都已映射到 Part/Task，无 GAP。
- [ ] 2. Placeholder scan：所有 Part 文件中无 `TODO`/`TBD`/`implement later`/`add appropriate error handling` 等占位。
- [ ] 3. No phantom tasks：每个 Task 都有明确的 Create/Modify/Test 文件、可运行的测试或手动验证步骤、commit 动作；无 `--allow-empty` 或 "already done in Task N"。
- [ ] 4. Dependency soundness：每个 `Depends on:` 都指向更早的 Task/Part；无引用后续才定义的符号。
- [ ] 5. Caller & build soundness：共享签名变更（`KimiCoreOptions`/`SessionOptions`/`AgentOptions` 新增 `llmFactory`、`SDKRpcClientOptions` 扩展）在对应 Task 中搜索并更新所有调用方（含测试），并以 `pnpm -r typecheck` 全树检查收尾。
- [ ] 6. Test-the-risk：每个状态变更 Task 都有行为断言（pending Map、streams Map、abortController、worker exit）；过滤/正则类测试枚举 must-survive 输入并确认不被误杀。
- [ ] 7. Type consistency：跨 Part 复用的类型（`ChatStreamRequest`、`StreamDelta`、`ChatStreamResult`、`llmFactory` 签名、`ResourceLimits` 字段名）与早期 Task 定义完全一致。
