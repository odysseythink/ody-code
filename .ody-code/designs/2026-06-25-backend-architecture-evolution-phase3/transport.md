# Part 2 — Rust Transport Server

> Scope: Rust 侧的 transport server、wire protocol、stdio/socket 生命周期。  
> Corresponds to index: [Architecture & Data Flow](../2026-06-25-backend-architecture-evolution-phase3.md)

---

## 1. Component Overview

Transport 层是 `ody-host` 的"网络/IO 层"，负责：
1. 监听 stdio 或 socket 连接（来自 TS TUI）。
2. 完成 length-prefixed / ndjson framing 的 handshake。
3. 将收到的 request bytes 分发给 `CoreHost::dispatch`。
4. 将 `CoreHost` 的 response bytes 编码回 TUI。
5. 将 `EventSink` 产生的 `AgentEvent` 作为反向 RPC（`SDKAPI`）发送给 TUI。
6. 处理连接断开、错误传播、优雅关闭。

核心约束：**Rust 侧 wire 协议必须与 `packages/agent-core/src/rpc/transports/stream.ts` 逐字节兼容** [C:INFERRED]。

---

## 2. Typed Interfaces

### 2.1 WireMessage

```rust
enum WireMessage {
    Request {
        req_id: String,
        bytes: Vec<u8>,          // JSON-serialized RPC wrapper: { method: string, args: [payload] }
    },
    Response {
        req_id: String,
        bytes: Option<Vec<u8>>,  // JSON-serialized RPC wrapper: { ok: true, value } | { ok: false, error }
        error: Option<WireError>, // transport-level error only; RPC errors live inside bytes
    },
}

struct WireError {
    message: String,
    code: Option<String>,
}

struct HandshakeMessage {
    framing: Framing,
    token: Option<String>,
}

enum Framing { LengthPrefixed, NdJson }
```

### 2.2 TransportServer + RpcRouter

```rust
// Raw byte dispatcher: receives the full request envelope bytes,
// returns the full response envelope bytes.
type ByteDispatch = dyn Fn(&[u8]) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, RpcError>> + Send>>;

trait TransportServer: Send + Sync {
    // contract: 阻塞运行，直到连接关闭或收到 shutdown 信号
    async fn serve(&self, dispatch: Box<ByteDispatch>) -> Result<(), TransportError>;

    // contract: 返回一个 EventSink，用于向 TUI 发送反向 RPC
    fn event_sink(&self) -> Box<dyn EventSink>;
}

struct RpcRouter {
    host: Arc<CoreHost>,
}

impl RpcRouter {
    // contract: 解析 { method, args }，调用 host.dispatch(method, args[0])，
    //           包装成 { ok, value/error }
    async fn route(&self, request_bytes: &[u8]) -> Result<Vec<u8>, RpcError> {
        let wrapper: RpcRequestWrapper = serde_json::from_slice(request_bytes)?;
        let method = wrapper.method;
        let payload = wrapper.args.into_iter().next().unwrap_or(JsonValue::Null);
        let result = self.host.dispatch(&method, payload).await;
        let response = match result {
            Ok(value) => RpcResponseWrapper { ok: true, value, error: None },
            Err(e) => RpcResponseWrapper { ok: false, value: JsonValue::Null, error: Some(e.into()) },
        };
        Ok(serde_json::to_vec(&response)?)
    }
}

struct RpcRequestWrapper {
    method: String,
    args: Vec<JsonValue>,
}

struct RpcResponseWrapper {
    ok: bool,
    value: JsonValue,
    error: Option<RpcErrorJson>,
}
```

### 2.3 EventSink（transport 视角）

```rust
trait EventSink: Send + Sync {
    // contract: 将 SDKAPI method + payload 作为反向 request 发送给 TUI，
    //           并等待 TUI 返回 response。
    async fn request(&self, method: &str, payload: Vec<u8>) -> Result<Vec<u8>, RpcError>;

    // contract: 发送单向事件（emitEvent），不等待响应
    fn emit(&self, event: AgentEvent);
}
```

### 2.4 Server implementations

```rust
struct StdioTransportServer;
struct UnixSocketTransportServer { path: PathBuf }
struct TcpSocketTransportServer { host: String, port: u16 }

impl TransportServer for StdioTransportServer { ... }
impl TransportServer for UnixSocketTransportServer { ... }
impl TransportServer for TcpSocketTransportServer { ... }
```

---

## 3. Algorithms

### 3.1 `build_transport` — 根据 HostConfig 选择 transport

```
INPUT: config: HostConfig
OUTPUT: (Arc<dyn TransportServer>, Box<dyn EventSink>)

1. MATCH config.transport:
   - Stdio:
       server = StdioTransportServer::new()
   - UnixSocket { path }:
       server = UnixSocketTransportServer::new(path)
   - TcpSocket { host, port }:
       server = TcpSocketTransportServer::new(host, port)
2. event_sink = server.event_sink()
3. RETURN (Arc::new(server), event_sink)
```

### 3.2 `StdioTransportServer::serve`

```
INPUT: dispatch: Fn(method, bytes) -> Future<Result<bytes, error>>
OUTPUT: Result<(), TransportError>

1. stdin = tokio::io::stdin()
2. stdout = tokio::io::stdout()
3. conn = StreamConnection::new(stdin, stdout, Framing::LengthPrefixed)
       // stdio 默认已互信，跳过 handshake；见 A3
4. conn.run(dispatch).await
5. RETURN Ok(())
```

### 3.3 `UnixSocketTransportServer::serve`

```
INPUT: dispatch: Fn(method, bytes) -> Future<Result<bytes, error>>
OUTPUT: Result<(), TransportError>

1. listener = tokio::net::UnixListener::bind(path)?
2. PRINT ready message to stderr as NDJSON:
       { "type": "ready", "stdio": false, "socketPath": path }
3. LOOP:
       (socket, _) = listener.accept().await?
       spawn connection handler for this socket
       // prototype: single connection only; second connection is rejected or queued
4. On shutdown signal, remove socket file and RETURN
```

### 3.4 `StreamConnection::run` — 单连接主循环

```
INPUT: readable: AsyncRead, writable: AsyncWrite, dispatch
SIDE EFFECTS: bidirectional message exchange

1. IF framing not fixed:
       perform_handshake()
2. spawn writer_task:
       // reads from outbound channel (responses + reverse RPCs)
       WHILE let (req_id_or_method, payload, kind) = outbound.recv().await:
           IF kind == Response:
               msg = WireMessage::Response { req_id: req_id_or_method, bytes: Some(payload), error: None }
           ELSE IF kind == ReverseRequest:
               msg = WireMessage::Request { req_id: req_id_or_method, bytes: payload }
           encode_and_write(msg)
3. spawn reader_task:
       WHILE let frame = read_frame().await:
           MATCH frame:
               - Request { req_id, bytes }:
                   // This is either a CoreAPI call from TUI or a response to a reverse RPC.
                   IF pending_reverse_requests.contains(req_id):
                       resolve pending_reverse request
                   ELSE:
                       spawn dispatch_task(req_id, bytes)
               - Response { req_id, bytes, error }:
                   IF pending_outbound_requests.contains(req_id):
                       resolve pending outbound request
                   ELSE:
                       log unknown response id
4. wait for either task to finish or shutdown signal
5. close both tasks and channels
```

### 3.5 `perform_handshake`

```
1. server_hello = HandshakeMessage { framing: LengthPrefixed, token: None }
2. write_line(json(server_hello))        // ndjson line for handshake only
3. read_line() -> client_hello
4. PARSE client_hello as HandshakeMessage
5. IF client_hello.framing not in {LengthPrefixed, NdJson}:
       fail TRANSPORT_INVALID_FRAMING
6. IF required_token is set AND client_hello.token != required_token:
       fail TRANSPORT_UNAUTHORIZED
7. SET connection framing = client_hello.framing
```

### 3.6 `encode_and_write` — length-prefixed 编码

```
INPUT: msg: WireMessage
OUTPUT: bytes written to socket

1. payload = json_bytes(msg)
2. IF payload.len() > MAX_FRAME_SIZE:
       fail TRANSPORT_INVALID_FRAMING("frame too large")
3. MATCH framing:
   - LengthPrefixed:
       frame = u32le(payload.len()) + payload
   - NdJson:
       frame = payload + b'\n'
4. write_all(frame)
```

### 3.7 `read_frame` — length-prefixed 解码

```
OUTPUT: WireMessage or TransportError

1. IF framing == LengthPrefixed:
       read_exact(4) -> length_bytes
       length = u32le(length_bytes)
       IF length > MAX_FRAME_SIZE: fail TRANSPORT_INVALID_FRAMING
       payload = read_exact(length)
   ELSE:
       read_until(b'\n') -> payload
2. msg = json_parse(payload)
3. IF msg.bytes field is a JSON object with numeric keys:
       revive to Vec<u8> / Uint8Array        // 兼容 TS Uint8Array.toJSON()
4. RETURN msg
```

### 3.8 `dispatch_task` — 处理单个 request envelope

```
INPUT: req_id: String, bytes: Vec<u8>   // bytes = { method, args }

1. result = byte_dispatch(bytes).await   // byte_dispatch is the closure from RpcRouter
2. MATCH result:
   - Ok(response_bytes):                 // response_bytes = { ok, value/error }
       send Response { req_id, bytes: Some(response_bytes), error: None }
   - Err(e):
       send Response { req_id, bytes: None, error: Some({ message, code }) }
```

---

## 4. Call-Site Integration

### 4.1 `rust-ody/crates/ody-host/src/main.rs`

```rust
let (server, event_sink) = transport::build_transport(config.transport).await?;
let host = Arc::new(CoreHost::new(config, event_sink)?);
let router = RpcRouter { host };
let byte_dispatch = Box::new(move |bytes: &[u8]| {
    let router = router.clone();
    Box::pin(async move { router.route(bytes).await }) as Pin<Box<dyn Future<Output = _> + Send>>
});
server.serve(byte_dispatch).await?;
```

### 4.2 `packages/node-sdk/src/rpc.ts:180-213`

TS 侧 `createExternalTransport` 已支持 spawn `ody serve --stdio` 并等待 ready message。原型中：
- 将 spawn 命令从 `ody` 改为 `ody-host`（或 SEA 提取后的路径）。
- 保持 `createStreamTransport(proc.stdout, proc.stdin, dispatch, { framing: 'length-prefixed' })` 不变。

### 4.3 `packages/agent-core/src/rpc/client.ts`

`createRPCEndpoint<SDKAPI, CoreAPI>` 与 `createRPC` 保持 TS 侧调用约定；Rust 侧需实现同样的 request/response 匹配语义。

---

## 5. Error Handling（局部）

| Error class | Immediate handling | Degradation path | Recovery condition |
|---|---|---|---|
| `TransportError::InvalidFraming` | Close connection, log error | TUI 看到 host 退出 | 重启 host |
| `TransportError::Unauthorized` | Close connection | TUI 无法连接 | 提供正确 token |
| `TransportError::Io` | Close connection, propagate to main | Host 退出 | 检查 socket/stdio 可用性 |
| `RpcError::MethodNotImplemented` | Return error response for that `reqId` | TUI 收到明确错误 | 不调用未实现方法 |
| `SdkError::ReverseRpcTimeout` | Cancel pending reverse request | Tool/approval 失败 | TUI 及时响应 |

---

## 6. Local Test Notes

### Must-pass assertions

1. `cargo test -p ody-host`:
   - `length_prefixed_roundtrip` — 编码再解码 1000 条随机消息，payload 逐字节相等。
   - `handshake_length_prefixed_agreed` — 客户端发 `{framing:"length-prefixed"}`，服务端进入 length-prefixed 模式。
   - `handshake_rejects_invalid_framing` — 客户端发 `{framing:"gzip"}`，服务端关闭连接并返回 `TRANSPORT_INVALID_FRAMING`。
2. Cross-language integration test:
   - 启动 `ody-host --stdio`，TS `SDKRpcClient.connect({ transport: 'stdio' })` 成功。
   - 调用 `getCoreInfo` 返回 version/capabilities。
   - 调用 `createSession`，Rust 侧落盘 `state.json`，TS 侧收到 `SessionSummary`。
3. Socket mode test:
   - 启动 `ody-host --socket-path /tmp/ody-test.sock`。
   - TS 连接 `{ socketPath: '/tmp/ody-test.sock' }`。
   - 重复上述 API 调用并通过。

### Must-reject assertions

1. TS client sends ndjson framing to a length-prefixed-only Rust server → connection closed.
2. Rust server sends response with mismatched `reqId` → TS client rejects/ignores.
3. Socket path already in use → `ody-host` exits with non-zero and clear stderr.
