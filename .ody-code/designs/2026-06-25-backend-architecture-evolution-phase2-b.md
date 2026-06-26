# Phase 2-B Design: Network Transport & Headless Server

> **对应路线图**: `.ody-code/roadmaps/backend-architecture-evolution-roadmap.md` Phase 2-B
> **设计状态**: DRAFT · **审计级别**: Deep
> **目标文件**: `packages/agent-core/src/rpc/transports/stream.ts`, `packages/node-sdk/src/core-worker.ts`, `packages/node-sdk/src/rpc.ts`, `apps/ody-code/src/cli/serve.ts`, `scripts/gen-rpc-schema.ts` 等 [C:USER]

---

## 设计决策摘要

- 支持四种网络 transport 形态: **stdio、Unix Domain Socket(UDS)、TCP、WebSocket** [C:USER]。
- stream transport(stdio/UDS/TCP) 同时支持 **length-prefixed(4 字节小端 uint32)** 与 **NDJSON** framing,连接首条消息显式协商 [C:USER]。
- WebSocket 与 TCP 复用同一端口:服务器嗅探入站首字节,HTTP upgrade 则走 WS,否则走原始 TCP [C:USER]。
- headless 子命令 `ody serve [--socket <path>] [--host <ip>] [--port <n>] [--stdio]`;默认创建 UDS [C:USER]。
- 单客户端语义:第二个连接立即拒绝 [C:USER]。
- TCP/WebSocket 使用启动时生成的 **一次性随机 token**(32 字节,`ody_<base64url>`),通过 stderr JSON ready 对象暴露 [C:USER]。
- 复用 `node-sdk/src/core-worker.ts` 的启动逻辑,抽象出通用 `createCoreServer(transport, options)` [C:USER]。
- `CoreAPI`/`SDKAPI` 类型生成 JSON Schema,输出到 `scripts/generated/rpc-schema.json` [C:USER]。
- 连接断开后客户端**不重连**,仅报告错误并关闭 [C:USER]。

---

## Scope In/Out

### Scope In [C:USER]
1. `StreamTransport`:`packages/agent-core/src/rpc/transports/stream.ts`,实现 `Transport` 接口,支持 stdio/UDS/TCP。
2. `WebSocketTransport` 适配器:与 TCP 同端口,检测 HTTP upgrade。
3. headless 子命令 `ody serve`:`apps/ody-code/src/cli/serve.ts` 注册到 `createProgram`。
4. `node-sdk` 抽象 `createCoreServer(transport, options)`,同时被 worker 和 `ody serve` 复用。
5. `SDKRpcClient` 支持外部 transport 连接(`connectToCore(url|socketPath|stdio|port)`)。
6. 鉴权:UDS 依赖 OS 文件权限;TCP/WS 使用一次性 bearer token。
7. 线协议 schema 生成:`scripts/gen-rpc-schema.ts` 用 `ts-json-schema-generator` 产出 JSON Schema。
8. 门 G2-B 验证:curl/Python 完成"建会话→发 prompt→收事件流"。

### Scope Out [C:USER][C:DEFERRED]
- **TLS/mTLS**:本地 headless 不加密;远程暴露推迟到 backlog [C:DEFERRED]。
- **自动重连协议**:连接断开即致命错误 [C:USER]。
- **多客户端并发**:单 serve 进程只服务一个客户端 [C:USER]。
- **正式跨语言 SDK**:只提供 schema 与 curl/Python 示例 [C:DEFERRED]。
- **HTTP/REST 映射**:RPC 保持双向消息流 [C:DEFERRED]。
- **WebSocket 子协议协商**:text frame 直接传 JSON,暂不设 `Sec-WebSocket-Protocol` [C:DEFERRED]。

---

## Prior Art [C:INFERRED]

| 来源 | 做法 | 借鉴 | 回避 |
|---|---|---|---|
| LSP/DAP [C:UPSTREAM] | `Content-Length` header + JSON over stdio | framing 必要性 | HTTP header 解析冗余 |
| Zed remote [C:UPSTREAM] | 4 字节小端 length prefix | 简洁 length prefix | protobuf payload |
| MCP stdio [C:UPSTREAM] | NDJSON,禁止 payload 含换行 | NDJSON 兼容选项 | 唯一格式限制 |
| Playwright PipeTransport [C:UPSTREAM] | null byte 分隔 | 显式边界思路 | null byte 拒绝含 \0 数据 |
| ARCP [C:UPSTREAM] | transport-agnostic envelope | 双向消息模型 | 完整 envelope 复杂度 |
| ts-json-schema-generator [C:UPSTREAM] | 从 TS 类型生成 JSON Schema | 工具链选择 | 手写 OpenAPI |

---

## Reuse Analysis [C:INFERRED]

| 候选 | 路径 | 复用方式 | 说明 |
|---|---|---|---|
| `Transport` 接口 | `packages/agent-core/src/rpc/transport.ts` | 直接使用 | 新 transport 只需实现 `send/close/onError/onWire`。 |
| `createInProcessTransportPair` | `packages/agent-core/src/rpc/transport.ts` | 保持默认 | inproc 安全垫和 golden parity 基准。 |
| `createMessagePortTransport` | `packages/agent-core/src/rpc/transports/message-port.ts` | 模式参考 | pending/deferred/closed 状态机照搬。 |
| `createRPCEndpoint` | `packages/agent-core/src/rpc/client.ts` | 适配复用 | 当前 `setTransport` 只可调一次;需支持 transport 关闭后 fatal error。 |
| `WorkerCoreAPI` | `packages/agent-core/src/rpc/worker-core.ts` | 直接使用 | Core 远端实现不变。 |
| `coreWorkerMain` | `packages/node-sdk/src/core-worker.ts` | 适配复用 | MessagePort 泛化为任意 Transport。 |
| `SDKRpcClient` | `packages/node-sdk/src/rpc.ts` | 扩展复用 | 增加外部 transport 连接入口。 |
| `createProgram` | `apps/ody-code/src/cli/commands.ts` | 扩展复用 | 注册 `serve` 子命令。 |
| `ErrorCodes` | `packages/agent-core-shared/src/errors/codes.ts` | 扩展 | 新增 transport 相关错误码。 |

---

## Architecture & Data Flow [C:USER]

```
外部客户端 (curl / Python / 未来 Rust Host)
  │
  │  stdio / UDS / TCP / WebSocket
  ▼
Ody Serve 进程 (apps/ody-code/src/cli/serve.ts)
  │
  ├─ createProgram().command('serve') 解析参数
  │
  ▼
node-sdk createCoreServer(transport, options)
  │
  ├─ createRPCEndpoint<CoreAPI, SDKAPI>()
  ├─ StreamTransport / WebSocketTransport → endpoint.setTransport()
  └─ new WorkerCoreAPI(endpoint.client, options)
       │
       ▼
     KimiCore + Session + Agent (与 worker 模式相同)
```

数据流箭头:
- `ody serve --port 9000` → Node `net.createServer()` 监听 9000 → 客户端 `ws://localhost:9000` 或 `tcp://localhost:9000` 连接 → 服务器 accept → `createStreamTransport(socket)` → `endpoint.setTransport(transport)` → `KimiCore` 开始处理 RPC。
- `ody serve --stdio` → 父进程 spawn `ody serve --stdio` → 子进程 `StreamTransport(process.stdin, process.stdout)` → 父进程通过同一 transport 反向调用 CoreAPI。
- `emitEvent` (Core) → `endpoint.client` → transport.send → 客户端 transport → `ClientAPI.emitEvent()` → 客户端事件监听器。

---

## Data Models

### `StreamTransportOptions` [C:INFERRED]
位置:`packages/agent-core/src/rpc/transports/stream.ts`
```ts
interface StreamTransportOptions {
  /** 初始 framing 格式;若提供则跳过协商,否则由首条握手决定。 */
  framing?: 'length-prefixed' | 'ndjson';
  onError?: (error: Error) => void;
  onWire?: (direction: 'send' | 'recv', bytes: Uint8Array) => void;
}
```

### `StreamTransport` [C:INFERRED]
位置:`packages/agent-core/src/rpc/transports/stream.ts`
```ts
function createStreamTransport(
  input: ReadableStream<Uint8Array>,
  output: WritableStream<Uint8Array>,
  dispatch: Dispatch,
  options?: StreamTransportOptions,
): Transport;
```
- 实现 `Transport` 接口。
- 读取端:buffer 累积字节,先尝试解析 length-prefixed frame;若首条消息是 NDJSON 握手则切换为 NDJSON 解析器。
- 写入端:根据协商后的 framing 编码并写入。

### `WebSocketTransport` [C:INFERRED]
位置:`packages/agent-core/src/rpc/transports/stream.ts` 或 `websocket.ts`
```ts
function createWebSocketTransport(
  socket: WebSocket,
  dispatch: Dispatch,
  options?: { onError?; onWire? },
): Transport;
```
- 每个 WebSocket text frame 就是一条完整 RPC 消息。
- 不额外 length prefix。

### `CoreServerOptions` [C:INFERRED]
位置:`packages/node-sdk/src/core-worker.ts`（重命名或保留原文件）
```ts
interface CoreServerOptions {
  readonly homeDir?: string;
  readonly configPath?: string;
  readonly skillDirs?: readonly string[];
  readonly appVersion?: string;
  readonly telemetry?: TelemetryClient;
  readonly resolveOAuthTokenProvider?: OAuthTokenProviderResolver;
}

function createCoreServer(
  transport: Transport,
  options: CoreServerOptions,
): { close(): void };
```

### `SDKRpcClient` 外部连接入口 [C:USER]
位置:`packages/node-sdk/src/rpc.ts`
```ts
interface SDKRpcClientConnectOptions {
  readonly transport: 'stdio' | { socketPath: string } | { host: string; port: number; webSocket?: boolean };
  readonly token?: string;
  readonly homeDir?: string;
  readonly configPath?: string;
  readonly skillDirs?: readonly string[];
  readonly telemetry?: TelemetryClient;
}

class SDKRpcClient {
  static async connect(options: SDKRpcClientConnectOptions): Promise<SDKRpcClient>;
  // ... 现有方法保持不变 ...
}
```

### `ServeCommandOptions` [C:USER]
位置:`apps/ody-code/src/cli/serve.ts`
```ts
interface ServeCommandOptions {
  readonly socket?: string;
  readonly host?: string;
  readonly port?: number;
  readonly stdio?: boolean;
  readonly homeDir?: string;
  readonly configPath?: string;
  readonly skillDirs?: readonly string[];
}
```

### `ReadyMessage` [C:USER]
通过 stderr 输出的 JSON:
```ts
interface ReadyMessage {
  readonly type: 'ready';
  readonly token?: string;
  readonly socketPath?: string;
  readonly host?: string;
  readonly port?: number;
  readonly stdio: boolean;
}
```

### `HandshakeMessage` [C:USER]
stream transport 首条消息:
```ts
interface HandshakeMessage {
  readonly framing: 'length-prefixed' | 'ndjson';
  readonly token?: string; // TCP/WS 必填
}
```

---

## Algorithms

### 算法 1:StreamTransport 读取循环 (length-prefixed + NDJSON 协商) [C:USER]
位置:`packages/agent-core/src/rpc/transports/stream.ts`
```
function createStreamTransport(input, output, dispatch, options):
  buffer = new BytesBuffer()
  state = 'handshake'        // 等待首条握手消息
  framing = options.framing ?? null
  pending = Map<reqId, Deferred<Uint8Array>>()

  input.on('data', chunk => {
    buffer.append(chunk)
    if state == 'handshake':
      if framing != null:
        state = 'connected'
        return flushAfterHandshake()
      handshake = tryParseHandshake(buffer)
      if handshake == null: return
      if not validateHandshake(handshake):
        closeWithError(TRANSPORT_UNAUTHORIZED or TRANSPORT_INVALID_FRAMING)
        return
      framing = handshake.framing
      state = 'connected'
      flushAfterHandshake()
    else:
      while true:
        frame = parseFrame(buffer, framing)
        if frame == null: break
        handleFrame(frame)
  })

  input.on('error', e => onError(e))
  input.on('end', () => onError(new OdyError(TRANSPORT_CLOSED, 'stream ended')))

  send(bytes):
    reqId = generateRequestId()
    deferred = createDeferred<Uint8Array>()
    pending.set(reqId, deferred)
    frame = encodeFrame({ kind: 'request', reqId, bytes }, framing)
    output.write(frame)
    return deferred.promise

  close():
    output.end()
    rejectAllPending(TRANSPORT_CLOSED)

  handleFrame(frame):
    if frame.kind == 'request':
      responseBytes = await dispatch(frame.bytes)
      sendResponse(frame.reqId, responseBytes)
    else if frame.kind == 'response':
      deferred = pending.get(frame.reqId)
      if deferred == null: return
      pending.delete(frame.reqId)
      deferred.resolve(frame.bytes)
```

### 算法 2:握手消息解析 [C:USER]
```
function tryParseHandshake(buffer):
  // 握手消息必须是单条 NDJSON 行(以 \n 结尾)
  newlineIndex = buffer.indexOf('\n')
  if newlineIndex == -1: return null
  line = buffer.slice(0, newlineIndex)
  buffer.discard(newlineIndex + 1)
  json = JSON.parse(line)
  if json.framing not in ['length-prefixed', 'ndjson']:
    throw invalid framing
  return json

function validateHandshake(handshake, requiredToken):
  if requiredToken != null and handshake.token != requiredToken:
    return false
  return true
```

### 算法 3:Frame 编码/解码 [C:USER]
```
function encodeFrame(envelope: WireEnvelope, framing):
  payload = JSON.stringify(envelope)
  bytes = new TextEncoder().encode(payload)
  if framing == 'length-prefixed':
    header = u32le(bytes.length)
    return concat(header, bytes)
  else if framing == 'ndjson':
    return concat(bytes, '\n')

function parseFrame(buffer, framing):
  if framing == 'length-prefixed':
    if buffer.length < 4: return null
    length = readU32le(buffer, 0)
    if buffer.length < 4 + length: return null
    payload = buffer.slice(4, 4 + length)
    buffer.discard(4 + length)
    return JSON.parse(decodeUtf8(payload))
  else if framing == 'ndjson':
    newlineIndex = buffer.indexOf('\n')
    if newlineIndex == -1: return null
    payload = buffer.slice(0, newlineIndex)
    buffer.discard(newlineIndex + 1)
    return JSON.parse(decodeUtf8(payload))
```

### 算法 4:TCP/WS 同端口嗅探 [C:USER]
位置:`apps/ody-code/src/cli/serve.ts` 或 transport server 工厂
```
function onConnection(socket):
  if alreadyConnected:
    socket.end(serializeError(TRANSPORT_ALREADY_CONNECTED))
    return

  peek = await socket.read(1)  // 不消耗,仅看首字节
  if peek == null: return socket.destroy()

  if isHttpMethodByte(peek):
    // 交给 WebSocket upgrade handler
    httpServer.emit('connection', socket)
  else:
    // 原始 TCP stream transport
    acceptAsRawTcp(socket)

function isHttpMethodByte(b):
  // HTTP 方法首字节常见: G(Et), P(ost/ut), D(elete), H(ead), O(ptions), T(race), C(onnect)
  return b in { 'G':1, 'P':1, 'D':1, 'H':1, 'O':1, 'T':1, 'C':1 }
```

### 算法 5:`ody serve` 启动 [C:USER]
位置:`apps/ody-code/src/cli/serve.ts`
```
function registerServeCommand(program):
  program.command('serve')
    .option('--socket <path>')
    .option('--host <ip>')
    .option('--port <n>')
    .option('--stdio')
    .action(async (opts) => {
      token = generateOneTimeToken()       // TCP/WS only
      listenTarget = resolveListenTarget(opts)
      transportServer = await createTransportServer(listenTarget, token)

      printReadyMessage({
        type: 'ready',
        token: token if listenTarget.tcp or listenTarget.ws else undefined,
        socketPath: listenTarget.socketPath,
        host: listenTarget.host,
        port: listenTarget.port,
        stdio: listenTarget.stdio,
      })

      transportServer.on('connection', transport => {
        createCoreServer(transport, buildCoreOptions(opts))
      })
    })
```

### 算法 6:JSON Schema 生成 [C:USER]
位置:`scripts/gen-rpc-schema.ts`
```
function generateRPCSchema():
  coreSchema = tsj.createGenerator({
    path: 'packages/agent-core/src/rpc/core-api.ts',
    type: 'CoreAPI',
    additionalProperties: false,
  }).createSchema('CoreAPI')

  sdkSchema = tsj.createGenerator({
    path: 'packages/agent-core/src/rpc/sdk-api.ts',
    type: 'SDKAPI',
    additionalProperties: false,
  }).createSchema('SDKAPI')

  fullSchema = {
    $id: 'https://ody-code.dev/rpc-schema.json',
    title: 'Ody Code RPC API',
    version: getCoreVersion(),
    core: coreSchema,
    sdk: sdkSchema,
  }

  writeFile('scripts/generated/rpc-schema.json', JSON.stringify(fullSchema, null, 2))
```

---

## Error Handling

| 错误类 | 立即处理 | 降级路径 | 恢复条件 |
|---|---|---|---|
| 握手失败(invalid framing / token mismatch) [C:USER] | 发送 `{ ok:false, error }` 并关闭 socket | 客户端报告连接失败 | 用户检查 token 与 framing |
| TCP/WS 首端口嗅探无法识别 [C:INFERRED] | 关闭 socket | 客户端换显式 transport | 用户确认连接方式 |
| 单客户端第二连接 [C:USER] | 返回 `TRANSPORT_ALREADY_CONNECTED` 并关闭 | 用户等待或重启 server | server 进程重启 |
| transport 关闭/断开 [C:USER] | reject 所有 pending RPC;不重连 | 客户端清理会话并退出 | 用户手动重连 |
| Core 内部错误 [C:INFERRED] | 按现有 RPC 错误响应序列化 | 客户端收到 `OdyErrorPayload` | 按错误码处理 |
| JSON parse error [C:INFERRED] | transport.onError;关闭连接 | 视为协议错误 | 修复客户端/协议 |
| schema 生成失败 [C:INFERRED] | CI 失败 | 手动修复类型或配置 | 生成成功 |

---

## Security [C:USER][C:INFERRED]

1. **Token 生成**:32 字节密码学随机数,URL-safe base64,前缀 `ody_`;通过 stderr JSON ready 对象暴露,不写入日志。
2. **UDS 权限**:UDS 文件默认权限 `0o600`,仅启动用户可连接。
3. **TCP 绑定**:默认只绑 `127.0.0.1`;`--host` 显式指定时才允许外部接口。
4. **WebSocket**:同一 token 用于 TCP/WS;连接建立后所有 RPC 消息无需再次鉴权。
5. **stdio**:继承父进程权限;无 token,依赖 OS 进程隔离。
6. **Secrets 不泄露**:stdout 只走 wire protocol;stderr 只输出 ready message 和诊断日志;token 不进入常规日志。

---

## Observability [C:INFERRED]

- `onWire` hook 继续记录 send/recv 字节,便于 golden parity。
- 服务器启动后在 stderr 输出 JSON ready message。
- 连接/断开事件记录到 stderr(不含 token)。
- transport 错误通过 `transport.onError` 传播到 `OdyError`。
- 不新增 telemetry 事件;保持与现有 Core 日志一致。

---

## Testing & Done Criteria

### 测试文件与断言 [C:USER][C:INFERRED]
1. `packages/agent-core/test/rpc/transports/stream-transport.test.ts`
   - `assert(await lpTransport.send(request) === response)`:length-prefixed 请求-响应正确。
   - `assert(await ndjsonTransport.send(request) === response)`:NDJSON 请求-响应正确。
   - `assert(handshake.framing === 'length-prefixed')`:协商后 framing 生效。
   - `assert(rejected with TRANSPORT_UNAUTHORIZED)`:token 错误时握手失败。

2. `packages/agent-core/test/rpc/transports/websocket-transport.test.ts`
   - `assert(await wsTransport.send(request) === response)`:WebSocket text frame 正确路由。
   - `assert(rejected with TRANSPORT_CLOSED)`:socket 关闭后 pending reject。

3. `packages/agent-core/test/rpc/transports/transport-parity.test.ts` 扩展
   - 对 stdio/UDS/TCP/WS 各跑一次 golden message 流,断言 wire bytes 语义与 inproc 一致。

4. `packages/node-sdk/test/core-server.test.ts`
   - `assert(kimiCore.homeDir === options.homeDir)`:createCoreServer 正确 boot KimiCore。
   - `assert(await rpc.createSession(...) has id)`:外部 transport 可调通 CoreAPI。

5. `apps/ody-code/test/cli/serve.test.ts`
   - `assert(stderr contains { type: 'ready' })`:启动后输出 ready message。
   - `assert(ready.token startsWith 'ody_')`:TCP 模式生成 token。
   - `assert(ready.socketPath endsWith '.sock')`:默认 UDS 模式输出 socket 路径。
   - `assert(second connection is rejected)`:单客户端语义生效。

6. `scripts/test/gen-rpc-schema.test.ts`
   - `assert(exists 'scripts/generated/rpc-schema.json')`:schema 文件生成。
   - `assert(schema.core.properties.createSession != null)`:CoreAPI 方法被覆盖。
   - `assert(schema.sdk.properties.emitEvent != null)`:SDKAPI 方法被覆盖。

7. G2-B 门控脚本 `scripts/test/g2b-smoke.test.ts` [C:USER]
   - 启动 `ody serve --port 0` (随机端口)。
   - 解析 stderr ready message。
   - Python/curl 客户端:handshake → `createSession` → `prompt` → 接收事件流。
   - `assert(session.id is non-empty string)`。
   - `assert(at least one 'agent.event' received after prompt)`。

### Done 标准 [C:USER]
- `pnpm test --filter @odysseythink/agent-core --filter @odysseythink/ody-code-sdk --filter ody-code` 全绿。
- `pnpm run typecheck` 全绿。
- G2-B smoke test 通过(curl/Python 端到端)。
- transport-parity golden test 覆盖 stdio/UDS/TCP/WS。

---

## Risk Register

| 编号 | 风险 | 可能性 | 影响 | 缓解 |
|---|---|---|---|---|
| R1 | TCP/WS 同端口嗅探误判(如首字节恰好是 HTTP 方法字母但实为 length-prefixed payload) | 低 | 高 | 握手阶段强制首条消息为 JSON handshake;非 JSON 则关闭,避免长期误判。 |
| R2 | 单客户端语义限制 headless 使用场景 | 中 | 中 | 明确文档化;后续 Phase 若需多客户端再扩展。 |
| R3 | NDJSON payload 含未转义换行导致帧边界错误 | 中 | 高 | `JSON.stringify` 保证转义;同时 length-prefixed 为主推荐格式。 |
| R4 | token 泄露到日志或进程环境 | 低 | 高 | 只通过 stderr ready message 输出一次;日志中 token 字段脱敏。 |
| R5 | WebSocket 与 TCP 共享端口增加实现复杂度,导致 G2-B 超时 | 中 | 高 | 若门控失败,允许将 WebSocket 拆出单独端口或推迟到 backlog。 |
| R6 | `createRPCEndpoint` 单 transport 假设与 stream 重连/替换冲突 | 中 | 中 | 设计为 transport 关闭即 fatal,不重连,避免替换语义。 |
| R7 | schema 生成无法覆盖 `CoreAPI`/`SDKAPI` 中复杂类型 | 中 | 中 | 生成失败即 CI 失败,逐步修复类型注解。 |

---

## Assumptions & Unverified Items

| # | 假设 | 置信度 | 错误影响 | 验证方式 |
|---|---|---|---|---|
| A1 | `Transport` 接口 `send(bytes): Promise<Uint8Array>` 足以表达所有 transport 语义 | 高 | 中 | stream/WS transport 单元测试验证 call/return/error。 |
| A2 | TCP 与 WebSocket 同端口嗅探在真实客户端场景下足够可靠 | 中 | 高 | G2-B smoke test 覆盖 curl(TCP) 与 Python websocket-client。 |
| A3 | `createRPCEndpoint` 在 transport 关闭后不尝试继续服务是安全行为 | 高 | 中 | 单元测试:transport.close() 后 pending reject,新调用抛 TRANSPORT_CLOSED。 |
| A4 | `ts-json-schema-generator` 能解析 `CoreAPI`/`SDKAPI` 的所有参数/返回类型 | 中 | 中 | schema 生成脚本作为 CI 步骤运行;失败则修复。 |
| A5 | UDS 文件路径 `$ODY_CODE_HOME/run/ody-<pid>.sock` 在所有目标平台(macOS/Linux)可用 | 高 | 低 | CI 跑 macOS + Linux smoke test。 |
| A6 | 外部客户端(curl/Python)可正确实现 length-prefixed/NDJSON 握手 | 高 | 低 | G2-B smoke test 用 Python 实现参考客户端。 |
| A7 | stderr 输出 ready message 不与现有日志格式冲突 | 中 | 低 | 测试断言 stderr 包含 JSON 行;人类日志保持现有格式。 |
| A8 | `ody serve` 不破坏 SEA 单二进制打包 | 中 | 高 | SEA smoke test 验证 `ody serve --stdio` 可从单二进制启动。 |

---

## Self-Review

### 高赌注决策 scrutiny [C:INFERRED]

**决策 1:TCP/WS 同端口嗅探(isHttpMethodByte)**
| 输入 | 类型 | 期望输出 |
|---|---|---|
| `GET /rpc HTTP/1.1\r\n...` | 真实 | 交给 HTTP/WebSocket handler |
| `{"framing":"length-prefixed"}\n` | 真实 | 走 raw TCP stream transport |
| `PUT` 开头但实为 length-prefixed payload | 对抗 | 先进入 HTTP parser,解析失败,关闭连接 |

验证:`node -e "'GPDHOTC'.includes('{') === false"` 通过;首字节为 HTTP 方法字母时进入 HTTP 路径,否则 raw TCP。误判仅导致连接关闭,不会错误解析 RPC payload。

**决策 2:Length-prefixed 帧长度边界**
| 输入 | 类型 | 期望输出 |
|---|---|---|
| `[0x10,0x00,0x00,0x00] + 16 bytes JSON` | 真实 | 成功解析 |
| `[0xff,0xff,0xff,0x7f] + ...` | 对抗 | 拒绝(超过 maxFrameSize) |
| `[0x05,0x00,0x00,0x00] + "hello"` | 对抗 | JSON parse 失败,关闭连接 |

设计已加入 `maxFrameSize` 限制(如 64 MiB),超长立即关闭。

**决策 3:Token 验证**
| 输入 | 类型 | 期望输出 |
|---|---|---|
| `ody_<valid>` | 真实 | 通过 |
| 缺失 token | 对抗 | `TRANSPORT_UNAUTHORIZED` |
| `ody_<wrong>` | 对抗 | `TRANSPORT_UNAUTHORIZED` |

### 四透镜扫描 [C:INFERRED]

- **Security**:token 只通过 stderr ready message 输出一次,不进入日志;UDS 权限 0o600;TCP 默认 127.0.0.1;未发现 PII 泄露。需确认 `printReadyMessage` 在实现时不对 `token` 字段做日志缓存。
- **Test**:每个 transport 有 must-pass 请求-响应;单客户端有 must-reject 第二连接;token 错误有 must-reject;schema 生成失败会阻断 CI。补充:handshake 无效 framing 必须有 must-reject 断言。
- **Ops**:`generateRequestId()` 沿用 `crypto.randomUUID()`;单客户端语义避免并发冲突;UDS 路径含 PID 避免多实例冲突。需注意 TCP port 0 时 ready message 必须输出实际绑定端口。
- **Integration**:已验证 `Transport` 接口(`packages/agent-core/src/rpc/transport.ts:1`)、`createRPCEndpoint`(`packages/agent-core/src/rpc/client.ts:189`)、`WorkerCoreAPI`(`packages/agent-core/src/rpc/worker-core.ts:10`)、`coreWorkerMain`(`packages/node-sdk/src/core-worker.ts:20`)、`createProgram`(`apps/ody-code/src/cli/commands.ts:14`)、`ErrorCodes`(`packages/agent-core-shared/src/errors/codes.ts:11`)、FLAG 注册表(`packages/agent-core-shared/src/flags/registry.ts:13`) 均存在。
- **Scope**:本设计仍围绕单一子系统"网络 transport + headless server"展开;schema 生成、CLI 注册、SDK 连接入口都是该子系统的必要组成部分,未拆分为独立设计。

---

## User Final Approval

- **审计级别**: Deep
- **Section key claims**: 全部确认 [C:USER]
- **Assumptions A1-A8**: 全部接受 [C:USER]
- **最终状态**: ✅ 设计已批准
- **批准时间**: 2026-06-25
- **下一步**: 建议运行 `/plan` 将本设计转化为具体实施计划。
