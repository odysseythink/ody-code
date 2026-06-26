# Phase 2-B: 网络 Transport 与 Headless Server Implementation Plan

**Goal:** 在 `packages/agent-core` 实现可复用的 stream/WebSocket transport，在 `packages/node-sdk` 抽象 `createCoreServer`，在 `ody-code` CLI 新增 `ody serve` 子命令，并生成 RPC JSON Schema 与 G2-B 端到端验收脚本，使外部客户端可通过 stdio/UDS/TCP/WebSocket 调用 CoreAPI。

**Architecture:** Stream transport 在字节流之上支持 length-prefixed 与 NDJSON 两种 framing，首条消息完成格式协商与 token 鉴权；WebSocket transport 复用标准 WebSocket API，text frame 直接承载 JSON-RPC 消息。`createCoreServer` 接收任意 `Transport`，内部组装 `createRPCEndpoint` + `WorkerCoreAPI`，与现有 worker 模式共享同一份 Core 启动逻辑。`ody serve` 负责监听 stdio/UDS/TCP 并在 TCP 端口上同端口嗅探 HTTP upgrade 以同时服务原始 TCP 与 WebSocket；单 serve 进程只服务一个客户端，TCP/WS 通过一次性 bearer token 鉴权，UDS 依赖文件系统权限。

**Tech Stack:** TypeScript 6.0 / Node.js ≥24.15 / Vitest, pnpm 10.33.0；`ws` 用于 `ody serve` 的 WebSocket server；`ts-json-schema-generator` 用于 schema 生成；G2-B smoke test 使用构建后的 `ody serve` + `SDKRpcClient.connect()`，并通过 `vi.mock('@odysseythink/kosong')` 注入 fake provider 避免真实 LLM 调用。

> For executing workers: implement this plan task-by-task (prefer a fresh subagent/Task per task — a clean context per task avoids single-session degradation). Steps use - [ ] checkboxes for tracking.

---

## File Structure

| Path | Responsibility |
|---|---|
| `packages/agent-core-shared/src/errors/codes.ts` | 新增 transport 错误码：`TRANSPORT_UNAUTHORIZED`、`TRANSPORT_INVALID_FRAMING`、`TRANSPORT_ALREADY_CONNECTED` |
| `packages/agent-core/src/rpc/transports/stream.ts` | `createStreamTransport`：length-prefixed + NDJSON framing、handshake、token 校验 |
| `packages/agent-core/src/rpc/transports/websocket.ts` | `createWebSocketTransport`：基于标准 WebSocket API 的 transport 适配器 |
| `packages/agent-core/src/rpc/transports/bytes-buffer.ts` | 共享字节缓冲辅助类，供 stream transport 使用 |
| `packages/agent-core/src/rpc/index.ts` | 导出 `createStreamTransport` 与 `createWebSocketTransport` |
| `packages/agent-core/test/rpc/transports/stream-transport.test.ts` | Stream transport 单元测试 |
| `packages/agent-core/test/rpc/transports/websocket-transport.test.ts` | WebSocket transport 单元测试 |
| `packages/agent-core/test/rpc/transports/transport-parity.test.ts` | 扩展 parity，覆盖 stream/WebSocket 与 inproc 的 wire 语义一致性 |
| `packages/node-sdk/src/core-server.ts` | `createCoreServer(transport, options)`：通用 Core 启动器 |
| `packages/node-sdk/src/core-worker.ts` | 复用 `createCoreServer`，保持 worker entry 不变 |
| `packages/node-sdk/src/index.ts` | 导出 `createCoreServer` 与相关类型 |
| `packages/node-sdk/src/rpc.ts` | `SDKRpcClient.connect()`：外部 transport 连接入口 |
| `packages/node-sdk/test/core-server.test.ts` | `createCoreServer` 启动与 RPC 调用测试 |
| `packages/node-sdk/test/sdk-rpc-client-connect.test.ts` | `SDKRpcClient.connect()` 跨 transport 端到端测试 |
| `apps/ody-code/src/cli/serve.ts` | `registerServeCommand` 与 serve 运行时：UDS/TCP/WS/stdio、单客户端、ready message |
| `apps/ody-code/src/cli/commands.ts` | 注册 `serve` 子命令 |
| `apps/ody-code/package.json` | 新增 `ws` 运行时依赖；新增 `@odysseythink/kosong` devDependency 用于 smoke test mock |
| `apps/ody-code/test/cli/serve.test.ts` | `ody serve` CLI 测试 |
| `package.json`（根） | 新增 `ts-json-schema-generator` devDependency 与 `gen:rpc-schema` script |
| `scripts/gen-rpc-schema.ts` | 生成 `scripts/generated/rpc-schema.json` |
| `packages/agent-core/test/scripts/gen-rpc-schema.test.ts` | 验证 schema 文件与关键字段 |
| `apps/ody-code/test/e2e/g2b-smoke.e2e.test.ts` | G2-B 门控：启动 `ody serve`，`SDKRpcClient.connect()` 建会话→发 prompt→收事件流 |

---

## Dependency Overview

```text
Part 1: Transport Primitives (agent-core)
  ├─ Task 1: Transport error codes + bytes buffer
  ├─ Task 2: StreamTransport (length-prefixed + NDJSON + handshake)
  ├─ Task 3: StreamTransport unit tests
  ├─ Task 4: WebSocketTransport
  └─ Task 5: WebSocketTransport unit tests + transport parity

Part 2: Core Server & SDK Client (node-sdk)
  ├─ Task 6: createCoreServer abstraction (depends on Part 1)
  ├─ Task 7: createCoreServer tests
  ├─ Task 8: SDKRpcClient.connect() (depends on Task 6)
  └─ Task 9: SDKRpcClient.connect() tests

Part 3: CLI Serve (ody-code)
  ├─ Task 10: `ody serve` stdio + UDS 实现与 CLI 注册 (depends on Part 2)
  ├─ Task 11: TCP + 一次性 token (depends on Task 10)
  └─ Task 12: TCP/WebSocket 同端口共享 (depends on Task 11)

Part 4: Schema & G2-B Gate
  ├─ Task 13: gen-rpc-schema.ts + test (depends on Part 1-3 已稳定导出的类型表面)
  └─ Task 14: G2-B smoke test (depends on Task 12)
```

Part 1 完成后，Part 2 可开始；Part 2 完成后，Part 3 可开始；Part 3 完成后，Part 4 可开始。Task 13 主要依赖类型已导出，可在 Part 3 稳定后并行开始。

---

## Risks & Open Questions

| Risk | Mitigation in Plan |
|---|---|
| R1: TCP/WS 同端口嗅探误判（首字节恰好为 HTTP 方法字母） | Task 10 强制首条消息必须是 JSON handshake；非 JSON 立即关闭，避免长期误判 |
| R2: 单客户端语义限制 headless 使用场景 | Task 10/11 用测试固定该语义；文档化，后续 Phase 再扩展 |
| R3: NDJSON payload 含未转义换行导致帧边界错误 | Task 2 使用 `JSON.stringify` 转义；length-prefixed 为主推荐格式 |
| R4: token 泄露到日志或进程环境 | Task 10 仅通过 stderr ready message 输出一次；日志中 token 字段脱敏 |
| R5: WebSocket 与 TCP 共享端口增加复杂度，导致 G2-B 超时 | Task 14 使用 `SDKRpcClient.connect()` 同时覆盖 TCP 与 WebSocket 路径；若失败则回退到单独 WS 端口 |
| R6: `createRPCEndpoint` 单 transport 假设与替换冲突 | 设计为 transport 关闭即 fatal、不重连；Task 6/8 用关闭后 pending reject 验证 |
| R7: schema 生成无法覆盖 `CoreAPI`/`SDKAPI` 复杂类型 | Task 12 将 schema 生成作为测试；失败即 CI 失败，逐步修复类型注解 |

---

## Parts

| # | File | Scope | Status |
|---|---|---|---|
| 1 | `2026-06-25-backend-architecture-evolution-phase2-b/transports.md` | agent-core transport primitives | done |
| 2 | `2026-06-25-backend-architecture-evolution-phase2-b/sdk.md` | node-sdk core server & client connection | done |
| 3 | `2026-06-25-backend-architecture-evolution-phase2-b/cli.md` | `ody serve` CLI command | done |
| 4 | `2026-06-25-backend-architecture-evolution-phase2-b/schema.md` | JSON schema generation + G2-B smoke test | done |

---

## Spec-Coverage Table

| 设计章节/需求 | 覆盖方式 | 状态 |
|---|---|---|
| StreamTransport：stdio/UDS/TCP，length-prefixed + NDJSON | Part 1 Task 2-3 | covered |
| WebSocketTransport 与 TCP 同端口 | Part 1 Task 4-5、Part 3 Task 10 | covered |
| `ody serve` 子命令（stdio/UDS/TCP/WS） | Part 3 Task 10-11 | covered |
| `createCoreServer` 抽象复用 | Part 2 Task 6-7 | covered |
| `SDKRpcClient.connect()` 外部连接 | Part 2 Task 8-9 | covered |
| 鉴权：UDS OS 文件权限；TCP/WS 一次性 token | Part 1 Task 2、Part 3 Task 10-11 | covered |
| 单客户端语义 | Part 1 Task 2、Part 3 Task 11 | covered |
| 线协议 schema 生成 | Part 4 Task 13 | covered |
| G2-B 门控：`ody serve` + `SDKRpcClient.connect()` 端到端 | Part 4 Task 14 | covered |
| Scope Out: TLS/mTLS、自动重连、多客户端、正式跨语言 SDK、HTTP/REST、WS 子协议 | — | no-op |

---

## Self-Review

- [ ] 1. Spec-coverage table: 每一条设计 In-Scope 需求都已映射到 Part/Task，无 GAP。
- [ ] 2. Placeholder scan: 所有 Part 文件中无 `TODO`/`TBD`/"implement later"/"add appropriate error handling" 等占位；每个任务给出完整代码、命令与预期输出。
- [ ] 3. No phantom tasks: 每个 Task 都有明确的 Create/Modify/Test 文件、可运行的测试或手动验证步骤、以及 commit 动作；无 `--allow-empty` 或 "already done in Task N"。
- [ ] 4. Dependency soundness: 每个 `Depends on:` 都指向更早的 Task/Part；Part 4 Task 14 仅依赖 Part 3 Task 12，Task 13 依赖 Part 1-3 已稳定导出的类型表面。
- [ ] 5. Caller & build soundness: 共享签名变更（`ErrorCodes` 新增码、`Transport` 未变更、`createCoreServer` 新增导出、`SDKRpcClient.connect` 新增静态方法、`ody serve` 新增子命令）在对应 Task 中搜索并更新所有调用方（含测试），并以 `pnpm -r typecheck` 全树检查收尾。
- [ ] 6. Test-the-risk: 每个 transport 有请求-响应 round-trip 测试；单客户端有第二连接拒绝测试；token 错误有握手失败测试；frame 边界有含换行 payload 测试；schema 生成有字段覆盖测试；G2-B 有真实 `ody serve` 进程启动、`SDKRpcClient.connect()` 连接与事件流断言。
- [ ] 7. Type consistency: `StreamTransportOptions`、`CoreServerOptions`、`SDKRpcClientConnectOptions`、`ReadyMessage`、`HandshakeMessage` 的类型与属性名在定义后所有调用方严格复用，无重命名漂移。
<!-- e2e-enriched -->

### Task 1: Generate and run E2E tests

Based on the changed files, validate the following areas:
- /Users/ranwei/workspace/ody-code/apps/ody-code/src/cli (priority: important)
- /Users/ranwei/workspace/ody-code/apps/ody-code/src (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core-shared/src/errors (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/compaction (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/config (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/context (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/cron (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/injection (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/permission (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/permission/policies (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/records (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/replay (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/session-mode (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/skill (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/tool (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/turn (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/agent/usage (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/rpc (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/session/checkpoint (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/session (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/collaboration (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/file (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/src/tools/builtin/idea (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/test/agent/cron/harness (priority: important)
- /Users/ranwei/workspace/ody-code/packages/agent-core/test/agent/harness (priority: important)
- /Users/ranwei/workspace/ody-code/packages/node-sdk/src (priority: important)

For any externally-facing interface you changed (HTTP endpoint/handler, RPC, or
CLI command), add a test that drives it through that interface and asserts on the
response (status code + parsed body), then run the suite. If the interface
requires authentication, supply a valid credential so the authorized path is
exercised and also assert the unauthorized case (401/403). You may also use the
RunE2ETests tool to scaffold and run E2E tests.

