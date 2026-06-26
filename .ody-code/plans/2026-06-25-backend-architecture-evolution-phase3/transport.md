# Part 2 — Rust Transport Server

> Scope: `ody-host` crate 的 transport 层：wire protocol、stdio/socket server、connection lifecycle、`RpcRouter`、`EventSink` transport 实现。  
> Depends on: index §Phase B 依赖图；实现层面依赖 Part 1 (`core.md`) 的 `CoreHost::dispatch`、`EventSink`、`HostError`、`HostConfig::transport`。

## Phase B 内部依赖图

```
B1 (wire + errors)
  │
  ├──► B2 (handshake)
  │      │
  │      ▼
  │    B3 (StreamConnection)
  │      │
  ├──────┴──► B4 (stdio) ──┐
  │                        │
  └──────────► B5 (socket)─┤
                           ▼
            B6 (RpcRouter + EventSink 扩展 + main.rs 集成)
                           │
                           ▼
            B7 (cross-lang stdio integration test)
```

---

### Task B1: Wire 类型与 length-prefixed/ndjson 编解码

**Depends on:** Task A2 (`error.rs` 已存在)

**Files:**
- Modify: `rust-ody/crates/ody-host/src/error.rs:1-20`（追加 `TransportError` / `RpcError`）
- Create: `rust-ody/crates/ody-host/src/transport/mod.rs`
- Create: `rust-ody/crates/ody-host/src/transport/wire.rs`
- Test: `rust-ody/crates/ody-host/src/transport/wire.rs` 内 `#[cfg(test)]`

**Steps：**

- [ ] 写失败测试：验证 `encode_frame` / `decode_frame` 对 `length-prefixed` 与 `ndjson` 的 roundtrip，并断言 TS 侧 `Uint8Array.toJSON()` 产生的 object-with-numeric-keys 也能被 Rust 解码。

```rust
// rust-ody/crates/ody-host/src/transport/wire.rs（末尾 test 模块）
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_prefixed_roundtrip() {
        let msg = WireMessage::Request {
            req_id: "r1".to_string(),
            bytes: b"hello".to_vec(),
        };
        let frame = encode_frame(&msg, Framing::LengthPrefixed).unwrap();
        let mut offset = 0usize;
        let decoded = decode_frame(&frame, Framing::LengthPrefixed, &mut offset).unwrap();
        match decoded {
            WireMessage::Request { req_id, bytes } => {
                assert_eq!(req_id, "r1");
                assert_eq!(bytes, b"hello");
            }
            _ => panic!("expected request"),
        }
        assert_eq!(offset, frame.len());
    }

    #[test]
    fn ndjson_roundtrip() {
        let msg = WireMessage::Response {
            req_id: "r2".to_string(),
            bytes: Some(b"world".to_vec()),
            error: None,
        };
        let frame = encode_frame(&msg, Framing::NdJson).unwrap();
        assert!(frame.ends_with(b"\n"));
        let mut offset = 0usize;
        let decoded = decode_frame(&frame, Framing::NdJson, &mut offset).unwrap();
        match decoded {
            WireMessage::Response { req_id, bytes, error } => {
                assert_eq!(req_id, "r2");
                assert_eq!(bytes.unwrap(), b"world");
                assert!(error.is_none());
            }
            _ => panic!("expected response"),
        }
    }

    #[test]
    fn decodes_bytes_as_numeric_object_like_ts_uint8array() {
        // Node.js JSON.stringify(new Uint8Array([1, 2, 3])) == {"0":1,"1":2,"2":3}
        let payload = br#"{"kind":"request","reqId":"r3","bytes":{"0":1,"1":2,"2":3}}"#;
        let mut offset = 0usize;
        let decoded = decode_frame(payload, Framing::NdJson, &mut offset).unwrap();
        match decoded {
            WireMessage::Request { bytes, .. } => assert_eq!(bytes, vec![1, 2, 3]),
            _ => panic!("expected request"),
        }
    }

    #[test]
    fn rejects_frame_too_large() {
        let big = WireMessage::Request {
            req_id: "x".to_string(),
            bytes: vec![0u8; MAX_FRAME_SIZE + 1],
        };
        let err = encode_frame(&big, Framing::LengthPrefixed).unwrap_err();
        assert!(matches!(err, TransportError::InvalidFraming(_)));
    }
}
```

- [ ] 运行并验证失败：
  - `cd rust-ody && cargo test -p ody-host transport::wire::tests`
  - 预期：因 `TransportError`、`WireMessage`、`encode_frame`、`decode_frame`、`MAX_FRAME_SIZE` 不存在而编译失败。

- [ ] 写最小实现：

```rust
// rust-ody/crates/ody-host/src/error.rs（追加到文件末尾）
use std::path::PathBuf;

#[derive(Debug)]
pub enum TransportError {
    Io(std::io::Error),
    InvalidFraming(String),
    Unauthorized,
    Closed,
    SocketBind { path: PathBuf, source: std::io::Error },
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::Io(e) => write!(f, "transport io error: {e}"),
            TransportError::InvalidFraming(m) => write!(f, "invalid framing: {m}"),
            TransportError::Unauthorized => write!(f, "transport unauthorized"),
            TransportError::Closed => write!(f, "transport closed"),
            TransportError::SocketBind { path, source } => write!(f, "cannot bind socket {}: {source}", path.display()),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<std::io::Error> for TransportError {
    fn from(e: std::io::Error) -> Self { TransportError::Io(e) }
}

#[derive(Debug)]
pub enum RpcError {
    Host(HostError),
    Serialize(serde_json::Error),
    Transport(TransportError),
    MethodNotFound(String),
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RpcError::Host(e) => write!(f, "{e}"),
            RpcError::Serialize(e) => write!(f, "serialize error: {e}"),
            RpcError::Transport(e) => write!(f, "{e}"),
            RpcError::MethodNotFound(m) => write!(f, "rpc method not found: {m}"),
        }
    }
}

impl std::error::Error for RpcError {}

impl From<HostError> for RpcError {
    fn from(e: HostError) -> Self { RpcError::Host(e) }
}

impl From<serde_json::Error> for RpcError {
    fn from(e: serde_json::Error) -> Self { RpcError::Serialize(e) }
}

impl From<TransportError> for RpcError {
    fn from(e: TransportError) -> Self { RpcError::Transport(e) }
}
```

```rust
// rust-ody/crates/ody-host/src/transport/mod.rs
pub mod connection;
pub mod rpc;
pub mod socket;
pub mod stdio;
pub mod wire;

pub use connection::StreamConnection;
pub use rpc::{RpcRouter, TransportEventSink};
pub use socket::{TcpSocketTransportServer, UnixSocketTransportServer};
pub use stdio::StdioTransportServer;
pub use wire::{Framing, HandshakeMessage, WireError, WireMessage, decode_frame, encode_frame};

pub use crate::config::TransportMode;
pub use crate::error::{RpcError, TransportError};

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type ByteDispatch = dyn Fn(&[u8]) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, RpcError>> + Send>> + Send + Sync;

#[async_trait::async_trait]
pub trait TransportServer: Send + Sync {
    /// 阻塞运行，直到连接关闭或收到 shutdown 信号。
    async fn serve(&self, dispatch: Arc<ByteDispatch>) -> Result<(), TransportError>;
}

/// 根据配置构造 transport server 及其 `EventSink`。
/// server 与 sink 在构造时同时生成，避免 `serve()` 阻塞后无法获取 sink。
pub async fn build_transport(
    mode: TransportMode,
) -> Result<(Arc<dyn TransportServer>, Box<dyn crate::events::EventSink>), HostError> {
    match mode {
        TransportMode::Stdio => {
            let (server, sink) = stdio::StdioTransportServer::new();
            Ok((Arc::new(server), sink))
        }
        TransportMode::UnixSocket { path } => {
            let (server, sink) = socket::UnixSocketTransportServer::bind(path).await?;
            Ok((Arc::new(server), sink))
        }
        TransportMode::TcpSocket { host, port } => {
            let (server, sink) = socket::TcpSocketTransportServer::bind(host, port).await?;
            Ok((Arc::new(server), sink))
        }
    }
}
```

```rust
// rust-ody/crates/ody-host/src/transport/wire.rs
use serde::{Deserialize, Serialize};

use crate::error::TransportError;

pub const MAX_FRAME_SIZE: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WireMessage {
    Request {
        req_id: String,
        #[serde(deserialize_with = "deserialize_bytes_flexible")]
        bytes: Vec<u8>,
    },
    Response {
        req_id: String,
        #[serde(deserialize_with = "deserialize_bytes_flexible")]
        bytes: Option<Vec<u8>>,
        error: Option<WireError>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireError {
    pub message: String,
    pub code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Framing {
    LengthPrefixed,
    NdJson,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeMessage {
    pub framing: Framing,
    pub token: Option<String>,
}

pub fn encode_frame(msg: &WireMessage, framing: Framing) -> Result<Vec<u8>, TransportError> {
    let payload = serde_json::to_vec(msg).map_err(|e| TransportError::InvalidFraming(e.to_string()))?;
    if payload.len() > MAX_FRAME_SIZE {
        return Err(TransportError::InvalidFraming(format!("frame too large: {}", payload.len())));
    }
    match framing {
        Framing::LengthPrefixed => {
            let mut frame = Vec::with_capacity(4 + payload.len());
            frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            frame.extend_from_slice(&payload);
            Ok(frame)
        }
        Framing::NdJson => {
            let mut frame = payload;
            frame.push(b'\n');
            Ok(frame)
        }
    }
}

pub fn decode_frame(buf: &[u8], framing: Framing, offset: &mut usize) -> Result<WireMessage, TransportError> {
    match framing {
        Framing::LengthPrefixed => {
            if buf.len() < *offset + 4 {
                return Err(TransportError::InvalidFraming("incomplete length header".to_string()));
            }
            let len = u32::from_le_bytes([
                buf[*offset], buf[*offset + 1], buf[*offset + 2], buf[*offset + 3],
            ]) as usize;
            if len > MAX_FRAME_SIZE {
                return Err(TransportError::InvalidFraming(format!("frame too large: {len}")));
            }
            if buf.len() < *offset + 4 + len {
                return Err(TransportError::InvalidFraming("incomplete payload".to_string()));
            }
            *offset += 4;
            let payload = &buf[*offset..*offset + len];
            *offset += len;
            serde_json::from_slice(payload).map_err(|e| TransportError::InvalidFraming(e.to_string()))
        }
        Framing::NdJson => {
            let start = *offset;
            let end = buf[start..].iter().position(|&b| b == b'\n')
                .map(|i| start + i)
                .ok_or_else(|| TransportError::InvalidFraming("missing newline".to_string()))?;
            let payload = &buf[start..end];
            *offset = end + 1;
            serde_json::from_slice(payload).map_err(|e| TransportError::InvalidFraming(e.to_string()))
        }
    }
}

fn deserialize_bytes_flexible<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::Array(arr) => {
            arr.into_iter()
                .map(|v| v.as_u64().map(|n| n as u8).ok_or_else(|| D::Error::custom("byte array contains non-u8")))
                .collect()
        }
        serde_json::Value::Object(mut obj) => {
            let mut pairs: Vec<(usize, u8)> = Vec::with_capacity(obj.len());
            for (k, v) in obj.drain() {
                let idx: usize = k.parse().map_err(|_| D::Error::custom("non-numeric byte object key"))?;
                let byte = v.as_u64().map(|n| n as u8).ok_or_else(|| D::Error::custom("byte object value not u8"))?;
                pairs.push((idx, byte));
            }
            pairs.sort_by_key(|(i, _)| *i);
            let len = pairs.last().map(|(i, _)| i + 1).unwrap_or(0);
            let mut bytes = vec![0u8; len];
            for (i, b) in pairs {
                bytes[i] = b;
            }
            Ok(bytes)
        }
        serde_json::Value::Null => Ok(Vec::new()),
        _ => Err(D::Error::custom("bytes field must be array or numeric object")),
    }
}
```

- [ ] 运行并验证通过：
  - `cd rust-ody && cargo test -p ody-host transport::wire::tests`
  - 预期：4 个测试全部通过。

- [ ] 提交：`git add rust-ody/crates/ody-host/src/error.rs rust-ody/crates/ody-host/src/transport/ rust-ody/crates/ody-host/src/lib.rs && git commit -m "feat(ody-host): wire protocol types and framing codec"`

---

### Task B2: Handshake 编解码与校验

**Depends on:** Task B1

**Files:**
- Modify: `rust-ody/crates/ody-host/src/transport/wire.rs`
- Test: `rust-ody/crates/ody-host/src/transport/wire.rs` 内 `#[cfg(test)]`

**Steps：**

- [ ] 写失败测试：验证 handshake 消息能按 ndjson line 编解码，并验证无效 framing 被拦截。

```rust
// rust-ody/crates/ody-host/src/transport/wire.rs（在 tests 模块内追加）
#[test]
fn handshake_roundtrip() {
    let msg = HandshakeMessage { framing: Framing::LengthPrefixed, token: Some("tok".to_string()) };
    let line = encode_handshake(&msg).unwrap();
    assert!(line.ends_with(b"\n"));
    let decoded = decode_handshake(&line).unwrap();
    assert_eq!(decoded.framing, Framing::LengthPrefixed);
    assert_eq!(decoded.token, Some("tok".to_string()));
}

#[test]
fn handshake_rejects_invalid_framing() {
    let line = br#"{"framing":"gzip","token":null}"#;
    let err = decode_handshake(line).unwrap_err();
    assert!(matches!(err, TransportError::InvalidFraming(_)));
}
```

- [ ] 运行并验证失败：
  - `cd rust-ody && cargo test -p ody-host transport::wire::tests`
  - 预期：因 `encode_handshake`、`decode_handshake` 不存在而失败。

- [ ] 写最小实现（追加到 `wire.rs`）：

```rust
// rust-ody/crates/ody-host/src/transport/wire.rs（函数区追加）
pub fn encode_handshake(msg: &HandshakeMessage) -> Result<Vec<u8>, TransportError> {
    let mut payload = serde_json::to_vec(msg).map_err(|e| TransportError::InvalidFraming(e.to_string()))?;
    payload.push(b'\n');
    Ok(payload)
}

pub fn decode_handshake(line: &[u8]) -> Result<HandshakeMessage, TransportError> {
    let msg: HandshakeMessage = serde_json::from_slice(line)
        .map_err(|e| TransportError::InvalidFraming(format!("invalid handshake: {e}")))?;
    match msg.framing {
        Framing::LengthPrefixed | Framing::NdJson => Ok(msg),
    }
}
```

- [ ] 运行并验证通过：
  - `cd rust-ody && cargo test -p ody-host transport::wire::tests`
  - 预期：6 个测试全部通过。

- [ ] 提交：`git add rust-ody/crates/ody-host/src/transport/wire.rs && git commit -m "feat(ody-host): handshake line codec"`

---

### Task B3: `StreamConnection` 读写循环与反向 RPC

**Depends on:** Task B2

**Files:**
- Create: `rust-ody/crates/ody-host/src/transport/connection.rs`
- Test: `rust-ody/crates/ody-host/src/transport/connection.rs` 内 `#[cfg(test)]`

**Steps：**

- [ ] 写失败测试：使用 `tokio::io::duplex` 验证 (1) TUI→host 的请求被 dispatch 并返回 Response；(2) host→TUI 的反向请求能拿到 Response。

```rust
// rust-ody/crates/ody-host/src/transport/connection.rs（末尾 test 模块）
#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::wire::{decode_frame, encode_frame, Framing, WireMessage};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn echo_dispatch() -> Arc<ByteDispatch> {
        Arc::new(|bytes: &[u8]| {
            let response = bytes.to_vec();
            Box::pin(async move { Ok(response) })
        })
    }

    #[tokio::test]
    async fn request_response_roundtrip() {
        let (client_read, server_write) = tokio::io::duplex(1024);
        let (server_read, mut client_write) = tokio::io::duplex(1024);

        let (conn, handle) = StreamConnection::new();
        let task = tokio::spawn(conn.start(
            server_read,
            server_write,
            Framing::LengthPrefixed,
            echo_dispatch(),
        ));

        let client = tokio::spawn(async move {
            let req = WireMessage::Request {
                req_id: "c1".to_string(),
                bytes: b"hello".to_vec(),
            };
            let frame = encode_frame(&req, Framing::LengthPrefixed).unwrap();
            client_write.write_all(&frame).await.unwrap();
            client_write.flush().await.unwrap();
            drop(client_write);

            let mut buf = vec![0u8; 1024];
            let n = client_read.read(&mut buf).await.unwrap();
            let mut offset = 0usize;
            let resp = decode_frame(&buf[..n], Framing::LengthPrefixed, &mut offset).unwrap();
            match resp {
                WireMessage::Response { req_id, bytes, error } => {
                    assert_eq!(req_id, "c1");
                    assert_eq!(bytes.unwrap(), b"hello");
                    assert!(error.is_none());
                }
                _ => panic!("expected response"),
            }
        });

        client.await.unwrap();
        drop(handle);
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn reverse_request_roundtrip() {
        let (client_read, server_write) = tokio::io::duplex(1024);
        let (server_read, mut client_write) = tokio::io::duplex(1024);

        let (conn, handle) = StreamConnection::new();
        let task = tokio::spawn(conn.start(
            server_read,
            server_write,
            Framing::LengthPrefixed,
            echo_dispatch(),
        ));

        let client = tokio::spawn(async move {
            let mut buf = vec![0u8; 1024];
            let n = client_read.read(&mut buf).await.unwrap();
            let mut offset = 0usize;
            let req = decode_frame(&buf[..n], Framing::LengthPrefixed, &mut offset).unwrap();
            match req {
                WireMessage::Request { req_id, bytes } => {
                    assert_eq!(bytes, b"call");
                    let resp = WireMessage::Response {
                        req_id,
                        bytes: Some(b"ok".to_vec()),
                        error: None,
                    };
                    let frame = encode_frame(&resp, Framing::LengthPrefixed).unwrap();
                    client_write.write_all(&frame).await.unwrap();
                    client_write.flush().await.unwrap();
                }
                _ => panic!("expected request"),
            }
            drop(client_write);
        });

        let response = handle.send_request("myMethod", b"call".to_vec()).await.unwrap();
        assert_eq!(response, b"ok");

        client.await.unwrap();
        drop(handle);
        task.await.unwrap().unwrap();
    }
}
```

- [ ] 运行并验证失败：
  - `cd rust-ody && cargo test -p ody-host transport::connection::tests`
  - 预期：因 `StreamConnection`、`ConnectionHandle`、`FrameDecoder` 等不存在而编译失败。

- [ ] 写最小实现：

```rust
// rust-ody/crates/ody-host/src/transport/connection.rs
use std::collections::HashMap;
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{Mutex, mpsc, oneshot};

use crate::error::{RpcError, TransportError};
use crate::events::{AgentEvent, EventSink};
use crate::transport::wire::{decode_frame, encode_frame, Framing, WireError, WireMessage};
use crate::transport::ByteDispatch;

#[derive(Debug, Clone)]
enum OutboundItem {
    Response { req_id: String, result: Result<Vec<u8>, RpcError> },
    Request { req_id: String, payload: Vec<u8> },
}

#[derive(Clone)]
pub struct ConnectionHandle {
    inner: Arc<ConnectionInner>,
}

impl Clone for ConnectionHandle {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

struct ConnectionInner {
    outbound_tx: mpsc::Sender<OutboundItem>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<Result<Vec<u8>, RpcError>>>>>,
}

impl ConnectionHandle {
    pub async fn send_response(
        &self,
        req_id: String,
        result: Result<Vec<u8>, RpcError>,
    ) -> Result<(), TransportError> {
        self.inner
            .outbound_tx
            .send(OutboundItem::Response { req_id, result })
            .await
            .map_err(|_| TransportError::Closed)
    }

    pub async fn send_request(
        &self,
        _method: &str,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, RpcError> {
        let req_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.inner.pending.lock().await.insert(req_id.clone(), tx);
        self.inner
            .outbound_tx
            .send(OutboundItem::Request { req_id, payload })
            .await
            .map_err(|_| RpcError::Transport(TransportError::Closed))?;
        rx.await
            .map_err(|_| RpcError::Transport(TransportError::Closed))?
    }
}

impl EventSink for ConnectionHandle {
    fn emit(&self, event: AgentEvent) {
        let payload = serde_json::to_vec(&event).unwrap_or_default();
        let req_id = uuid::Uuid::new_v4().to_string();
        let item = OutboundItem::Request { req_id, payload };
        if let Err(e) = self.inner.outbound_tx.try_send(item) {
            tracing::warn!("event emit dropped: {e}");
        }
    }
}

pub struct StreamConnection {
    inner: Arc<ConnectionInner>,
    outbound_rx: Option<mpsc::Receiver<OutboundItem>>,
}

impl StreamConnection {
    /// 预先创建 handle 与内部通道，允许在 `serve()` 拿到 `dispatch` 之前先把 sink 交给 `CoreHost`。
    pub fn new() -> (Self, ConnectionHandle) {
        let (outbound_tx, outbound_rx) = mpsc::channel::<OutboundItem>(128);
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let inner = Arc::new(ConnectionInner { outbound_tx, pending });
        let conn = Self {
            inner: Arc::clone(&inner),
            outbound_rx: Some(outbound_rx),
        };
        let handle = ConnectionHandle { inner };
        (conn, handle)
    }

    pub async fn start<R, W>(
        mut self,
        reader: R,
        writer: W,
        framing: Framing,
        dispatch: Arc<ByteDispatch>,
    ) -> Result<(), TransportError>
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let outbound_rx = self
            .outbound_rx
            .take()
            .expect("StreamConnection::start called more than once");
        let outbound_tx = self.inner.outbound_tx.clone();
        let pending = Arc::clone(&self.inner.pending);
        let writer_handle = tokio::spawn(writer_loop(writer, outbound_rx, framing));
        let reader_result = reader_loop(reader, framing, dispatch, outbound_tx, pending).await;
        writer_handle.abort();
        reader_result
    }
}

async fn writer_loop<W>(
    mut writer: W,
    mut rx: mpsc::Receiver<OutboundItem>,
    framing: Framing,
) -> Result<(), TransportError>
where
    W: AsyncWrite + Unpin + Send,
{
    while let Some(item) = rx.recv().await {
        let msg = match item {
            OutboundItem::Response { req_id, result } => match result {
                Ok(bytes) => WireMessage::Response {
                    req_id,
                    bytes: Some(bytes),
                    error: None,
                },
                Err(e) => WireMessage::Response {
                    req_id,
                    bytes: None,
                    error: Some(WireError {
                        message: e.to_string(),
                        code: None,
                    }),
                },
            },
            OutboundItem::Request { req_id, payload } => WireMessage::Request {
                req_id,
                bytes: payload,
            },
        };
        let frame = encode_frame(&msg, framing)?;
        writer.write_all(&frame).await?;
        writer.flush().await?;
    }
    Ok(())
}

async fn reader_loop<R>(
    mut reader: R,
    framing: Framing,
    dispatch: Arc<ByteDispatch>,
    outbound_tx: mpsc::Sender<OutboundItem>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<Result<Vec<u8>, RpcError>>>>>,
) -> Result<(), TransportError>
where
    R: AsyncRead + Unpin + Send,
{
    let mut decoder = FrameDecoder::new(framing);
    let mut buf = [0u8; 8192];
    loop {
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        decoder.push(&buf[..n]);
        while let Some(msg) = decoder.try_parse()? {
            match msg {
                WireMessage::Request { req_id, bytes } => {
                    let dispatch = Arc::clone(&dispatch);
                    let outbound_tx = outbound_tx.clone();
                    tokio::spawn(async move {
                        let result = dispatch(&bytes).await;
                        let _ = outbound_tx
                            .send(OutboundItem::Response { req_id, result })
                            .await;
                    });
                }
                WireMessage::Response { req_id, bytes, error } => {
                    let result = match error {
                        Some(e) => Err(RpcError::MethodNotFound(e.message)),
                        None => Ok(bytes.unwrap_or_default()),
                    };
                    if let Some(tx) = pending.lock().await.remove(&req_id) {
                        let _ = tx.send(result);
                    } else {
                        tracing::warn!("response for unknown req_id: {req_id}");
                    }
                }
            }
        }
    }
    Ok(())
}

struct FrameDecoder {
    buf: Vec<u8>,
    framing: Framing,
}

impl FrameDecoder {
    fn new(framing: Framing) -> Self {
        Self {
            buf: Vec::new(),
            framing,
        }
    }

    fn push(&mut self, chunk: &[u8]) {
        self.buf.extend_from_slice(chunk);
    }

    fn try_parse(&mut self) -> Result<Option<WireMessage>, TransportError> {
        match self.framing {
            Framing::LengthPrefixed => {
                if self.buf.len() < 4 {
                    return Ok(None);
                }
                let len = u32::from_le_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
                if self.buf.len() < 4 + len {
                    return Ok(None);
                }
                let mut offset = 0usize;
                let msg = decode_frame(&self.buf, self.framing, &mut offset)?;
                self.buf.drain(..4 + len);
                Ok(Some(msg))
            }
            Framing::NdJson => {
                let Some(pos) = self.buf.iter().position(|&b| b == b'\n') else {
                    return Ok(None);
                };
                let mut offset = 0usize;
                let msg = decode_frame(&self.buf, self.framing, &mut offset)?;
                self.buf.drain(..pos + 1);
                Ok(Some(msg))
            }
        }
    }
}
```

- [ ] 添加 `uuid` 依赖确认：已在 A1 `Cargo.toml` 中声明 `uuid = { version = "1", features = ["v7", "serde"] }`。反向 RPC 使用 `Uuid::new_v4()`， features 已足够；如需 `v4` 可改为 `"v4"` 或 `uuid::Uuid::now_v7().to_string()`。这里保持 `new_v4()` 并在 Cargo.toml 追加 `"v4"` feature：

```toml
# rust-ody/crates/ody-host/Cargo.toml [dependencies] 中修改 uuid 行
uuid = { version = "1", features = ["v4", "v7", "serde"] }
```

- [ ] 运行并验证通过：
  - `cd rust-ody && cargo test -p ody-host transport::connection::tests`
  - 预期：两个测试全部通过。

- [ ] 提交：`git add rust-ody/crates/ody-host/src/transport/connection.rs rust-ody/crates/ody-host/Cargo.toml && git commit -m "feat(ody-host): StreamConnection with bidirectional RPC"`

---

### Task B4: `StdioTransportServer`

**Depends on:** Task B3

**Files:**
- Create: `rust-ody/crates/ody-host/src/transport/stdio.rs`

**Steps：**

- [ ] 写最小实现（stdio 属于 wiring，无法单元测试；end-to-end 验证在 B7）：

```rust
// rust-ody/crates/ody-host/src/transport/stdio.rs
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::transport::connection::StreamConnection;
use crate::transport::wire::Framing;
use crate::transport::{ByteDispatch, TransportError, TransportServer};

pub struct StdioTransportServer {
    conn: Mutex<Option<StreamConnection>>,
}

impl StdioTransportServer {
    /// 返回 server 及其 `EventSink`（`ConnectionHandle`）。
    pub fn new() -> (Self, Box<dyn crate::events::EventSink>) {
        let (conn, handle) = StreamConnection::new();
        (
            Self {
                conn: Mutex::new(Some(conn)),
            },
            Box::new(handle),
        )
    }
}

#[async_trait]
impl TransportServer for StdioTransportServer {
    async fn serve(&self, dispatch: Arc<ByteDispatch>) -> Result<(), TransportError> {
        let conn = self
            .conn
            .lock()
            .unwrap()
            .take()
            .expect("serve() called more than once");
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        // stdio 默认已互信，跳过 handshake，固定 length-prefixed。
        conn.start(stdin, stdout, Framing::LengthPrefixed, dispatch).await
    }
}
```

- [ ] 运行构建验证：
  - `cd rust-ody && cargo build -p ody-host`
  - 预期：编译通过（`main.rs` 仍为 A7 placeholder，不引用 `StdioTransportServer`）。

- [ ] Manual verification（仅验证进程可启动）：
  1. `cd rust-ody && cargo run -p ody-host -- --stdio`
  2. 按 `Ctrl+C` 后进程应正常退出，stderr 出现 A7 placeholder 日志 `ody-host core ready (transport placeholder)`。

- [ ] 提交：`git add rust-ody/crates/ody-host/src/transport/stdio.rs && git commit -m "feat(ody-host): StdioTransportServer"`

---

### Task B5: Unix/TCP Socket Transport Servers

**Depends on:** Task B3

**Files:**
- Create: `rust-ody/crates/ody-host/src/transport/socket.rs`
- Modify: `rust-ody/crates/ody-host/src/error.rs`（追加 `IoGeneric` 变体）
- Test: `rust-ody/crates/ody-host/src/transport/socket.rs` 内 `#[cfg(test)]`

**Steps：**

- [ ] 写失败测试：验证 ready message JSON 字段与 TS `ReadyMessage` 接口一致。

```rust
// rust-ody/crates/ody-host/src/transport/socket.rs（末尾 test 模块）
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn ready_message_unix_matches_ts_shape() {
        let msg = ReadyMessage::unix(PathBuf::from("/tmp/ody.sock"));
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "ready");
        assert_eq!(json["stdio"], false);
        assert_eq!(json["socketPath"], "/tmp/ody.sock");
        assert!(json.get("host").is_none());
    }

    #[test]
    fn ready_message_tcp_matches_ts_shape() {
        let msg = ReadyMessage::tcp("127.0.0.1".to_string(), 9000);
        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "ready");
        assert_eq!(json["stdio"], false);
        assert_eq!(json["host"], "127.0.0.1");
        assert_eq!(json["port"], 9000);
        assert!(json.get("socketPath").is_none());
    }
}
```

- [ ] 运行并验证失败：
  - `cd rust-ody && cargo test -p ody-host transport::socket::tests`
  - 预期：因 `ReadyMessage` 等类型不存在而失败。

- [ ] 写最小实现：

```rust
// rust-ody/crates/ody-host/src/transport/socket.rs
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::net::{TcpListener, TcpStream, UnixListener, UnixStream};

use crate::error::HostError;
use crate::transport::connection::StreamConnection;
use crate::transport::wire::Framing;
use crate::transport::{ByteDispatch, TransportError, TransportServer};

#[derive(Debug, serde::Serialize)]
struct ReadyMessage {
    r#type: &'static str,
    stdio: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    socket_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    port: Option<u16>,
}

impl ReadyMessage {
    fn unix(path: PathBuf) -> Self {
        Self {
            r#type: "ready",
            stdio: false,
            socket_path: Some(path.to_string_lossy().to_string()),
            host: None,
            port: None,
        }
    }

    fn tcp(host: String, port: u16) -> Self {
        Self {
            r#type: "ready",
            stdio: false,
            socket_path: None,
            host: Some(host),
            port: Some(port),
        }
    }
}

fn print_ready(msg: ReadyMessage) {
    eprintln!("{}", serde_json::to_string(&msg).unwrap_or_default());
}

pub struct UnixSocketTransportServer {
    conn: Mutex<Option<StreamConnection>>,
    stream: Mutex<Option<UnixStream>>,
}

impl UnixSocketTransportServer {
    pub async fn bind(path: PathBuf) -> Result<(Self, Box<dyn crate::events::EventSink>), HostError> {
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| HostError::IoGeneric {
                message: format!("cannot remove stale socket {}: {e}", path.display()),
            })?;
        }
        let listener = UnixListener::bind(&path).await.map_err(|e| HostError::IoGeneric {
            message: format!("cannot bind unix socket {}: {e}", path.display()),
        })?;
        print_ready(ReadyMessage::unix(path));
        let (stream, _) = listener.accept().await.map_err(|e| HostError::IoGeneric {
            message: format!("unix socket accept failed: {e}"),
        })?;
        let (conn, handle) = StreamConnection::new();
        Ok((
            Self {
                conn: Mutex::new(Some(conn)),
                stream: Mutex::new(Some(stream)),
            },
            Box::new(handle),
        ))
    }
}

#[async_trait]
impl TransportServer for UnixSocketTransportServer {
    async fn serve(&self, dispatch: Arc<ByteDispatch>) -> Result<(), TransportError> {
        let conn = self
            .conn
            .lock()
            .unwrap()
            .take()
            .expect("serve() called more than once");
        let stream = self
            .stream
            .lock()
            .unwrap()
            .take()
            .expect("serve() called more than once");
        let (read, write) = stream.into_split();
        conn.start(read, write, Framing::LengthPrefixed, dispatch).await
    }
}

pub struct TcpSocketTransportServer {
    conn: Mutex<Option<StreamConnection>>,
    stream: Mutex<Option<TcpStream>>,
}

impl TcpSocketTransportServer {
    pub async fn bind(host: String, port: u16) -> Result<(Self, Box<dyn crate::events::EventSink>), HostError> {
        let addr: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|e| HostError::IoGeneric { message: format!("invalid tcp address: {e}") })?;
        let listener = TcpListener::bind(&addr).await.map_err(|e| HostError::IoGeneric {
            message: format!("cannot bind tcp socket {addr}: {e}"),
        })?;
        let local_addr = listener.local_addr().map_err(|e| HostError::IoGeneric {
            message: format!("cannot get local addr: {e}"),
        })?;
        print_ready(ReadyMessage::tcp(local_addr.ip().to_string(), local_addr.port()));
        let (stream, _) = listener.accept().await.map_err(|e| HostError::IoGeneric {
            message: format!("tcp socket accept failed: {e}"),
        })?;
        let (conn, handle) = StreamConnection::new();
        Ok((
            Self {
                conn: Mutex::new(Some(conn)),
                stream: Mutex::new(Some(stream)),
            },
            Box::new(handle),
        ))
    }
}

#[async_trait]
impl TransportServer for TcpSocketTransportServer {
    async fn serve(&self, dispatch: Arc<ByteDispatch>) -> Result<(), TransportError> {
        let conn = self
            .conn
            .lock()
            .unwrap()
            .take()
            .expect("serve() called more than once");
        let stream = self
            .stream
            .lock()
            .unwrap()
            .take()
            .expect("serve() called more than once");
        let (read, write) = stream.into_split();
        conn.start(read, write, Framing::LengthPrefixed, dispatch).await
    }
}
```

- [ ] 扩展 `HostError` 支持无 path 的 IO 错误（追加到 `error.rs`）：

```rust
// rust-ody/crates/ody-host/src/error.rs
#[derive(Debug)]
pub enum HostError {
    ConfigInvalid { message: String },
    Io { source: std::io::Error, path: PathBuf },
    IoGeneric { message: String },
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::ConfigInvalid { message } => write!(f, "invalid config: {message}"),
            HostError::Io { source, path } => write!(f, "io error at {}: {source}", path.display()),
            HostError::IoGeneric { message } => write!(f, "io error: {message}"),
        }
    }
}
```

- [ ] 运行并验证通过：
  - `cd rust-ody && cargo test -p ody-host transport::socket::tests`
  - 预期：两个测试全部通过。

- [ ] 运行构建：
  - `cd rust-ody && cargo build -p ody-host`
  - 预期：编译通过。

- [ ] Manual verification（仅验证进程可启动，真正端到端在 B7）：
  1. `cd rust-ody && cargo run -p ody-host -- --socket-path /tmp/ody-test.sock`
  2. 在另一个终端 `nc -U /tmp/ody-test.sock` 连接后，进程应保持运行。
  3. 断开连接后进程退出。

- [ ] 提交：`git add rust-ody/crates/ody-host/src/transport/socket.rs rust-ody/crates/ody-host/src/error.rs && git commit -m "feat(ody-host): Unix/TCP socket transport servers"`

---

### Task B6: `RpcRouter` + `EventSink` 反向 RPC 实现（共享签名变更任务）

**Depends on:** Task A7 (`CoreHost::dispatch`、`EventSink`)、Task B3 (`StreamConnection`)、Task B4/B5 (`TransportServer` 实现)

**Files：**
- Create: `rust-ody/crates/ody-host/src/transport/rpc.rs`
- Modify: `rust-ody/crates/ody-host/src/events.rs`（扩展 `EventSink` trait）
- Modify: `rust-ody/crates/ody-host/src/tools/mod.rs`（`ApprovalRequest`/`ApprovalResponse` 加 serde）
- Modify: `rust-ody/crates/ody-host/src/host.rs`（`CoreHostApprovalClient` 使用反向 RPC；tests 中 `MockSink` 实现新 trait）
- Modify: `rust-ody/crates/ody-host/src/main.rs`（替换 A7 placeholder，接入 `build_transport` + `RpcRouter`）
- Modify: `rust-ody/crates/ody-host/src/transport/connection.rs`（在 `EventSink` impl 中实现 `request`）

**Steps：**

- [ ] 写失败测试：验证 `RpcRouter` 能把 JSON request wrapper 路由到 `CoreHost::dispatch` 并返回 JSON response wrapper。

```rust
// rust-ody/crates/ody-host/src/transport/rpc.rs（末尾 test 模块）
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HostConfig, ProviderConfig, TransportMode};
    use crate::events::{AgentEvent, EventSink};
    use crate::host::CoreHost;
    use crate::llm::{ChatDelta, ChatRequest, FinishReason, LlmProvider, Message, Role};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    struct MockProvider;
    #[async_trait::async_trait]
    impl LlmProvider for MockProvider {
        async fn chat_stream(
            &self,
            _request: ChatRequest,
            on_delta: &mut dyn FnMut(ChatDelta),
        ) -> Result<FinishReason, crate::llm::LlmError> {
            on_delta(ChatDelta { index: 0, content: Some("ok".to_string()), tool_call: None });
            Ok(FinishReason::Stop)
        }
    }

    struct MockSink;
    #[async_trait::async_trait]
    impl EventSink for MockSink {
        async fn request(&self, _method: &str, _payload: Vec<u8>) -> Result<Vec<u8>, RpcError> {
            Err(RpcError::MethodNotFound("mock".to_string()))
        }
        fn emit(&self, _event: AgentEvent) {}
    }

    fn make_host() -> Arc<CoreHost> {
        let config = HostConfig {
            home_dir: tempfile::tempdir().unwrap().into_path(),
            config_path: None,
            transport: TransportMode::Stdio,
            log_level: crate::config::LogLevel::Info,
            provider: ProviderConfig {
                provider_id: "mock".to_string(),
                api_key: "".to_string(),
                base_url: None,
                default_model: "mock".to_string(),
            },
        };
        Arc::new(CoreHost::new(config, Box::new(MockSink), Box::new(MockProvider)).unwrap())
    }

    #[tokio::test]
    async fn routes_get_core_info() {
        let router = RpcRouter::new(make_host());
        let request = br#"{"method":"getCoreInfo","args":[{}]}"#;
        let response = router.route(request).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&response).unwrap();
        assert_eq!(json["ok"], true);
        assert!(json["value"]["version"].is_string());
    }

    #[tokio::test]
    async fn returns_error_for_unknown_method() {
        let router = RpcRouter::new(make_host());
        let request = br#"{"method":"unknown","args":[{}]}"#;
        let response = router.route(request).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&response).unwrap();
        assert_eq!(json["ok"], false);
        assert!(json["error"]["message"].as_str().unwrap().contains("unknown"));
    }
}
```

- [ ] 运行并验证失败：
  - `cd rust-ody && cargo test -p ody-host transport::rpc::tests`
  - 预期：因 `RpcRouter`、`EventSink::request` 未实现等原因编译失败。

- [ ] 写最小实现：

```rust
// rust-ody/crates/ody-host/src/transport/rpc.rs
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::RpcError;
use crate::host::CoreHost;
use crate::transport::ByteDispatch;

pub struct RpcRouter {
    host: Arc<CoreHost>,
}

impl RpcRouter {
    pub fn new(host: Arc<CoreHost>) -> Self {
        Self { host }
    }

    pub async fn route(&self, request_bytes: &[u8]) -> Result<Vec<u8>, RpcError> {
        let wrapper: RpcRequestWrapper = serde_json::from_slice(request_bytes)?;
        let payload = wrapper.args.into_iter().next().unwrap_or(serde_json::Value::Null);
        let result = self.host.dispatch(&wrapper.method, payload).await;
        let response = match result {
            Ok(value) => RpcResponseWrapper {
                ok: true,
                value,
                error: None,
            },
            Err(e) => RpcResponseWrapper {
                ok: false,
                value: serde_json::Value::Null,
                error: Some(RpcErrorJson {
                    message: e.to_string(),
                    code: None,
                }),
            },
        };
        Ok(serde_json::to_vec(&response)?)
    }

    pub fn into_byte_dispatch(self) -> Arc<ByteDispatch> {
        let router = Arc::new(self);
        Arc::new(move |bytes: &[u8]| {
            let router = Arc::clone(&router);
            Box::pin(async move { router.route(bytes).await })
        })
    }
}

#[derive(Debug, Deserialize)]
struct RpcRequestWrapper {
    method: String,
    args: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct RpcResponseWrapper {
    ok: bool,
    value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<RpcErrorJson>,
}

#[derive(Debug, Serialize)]
struct RpcErrorJson {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
}
```

- [ ] 扩展 `EventSink` trait（`events.rs`）：

```rust
// rust-ody/crates/ody-host/src/events.rs
use crate::error::RpcError;

#[async_trait::async_trait]
pub trait EventSink: Send + Sync {
    async fn request(&self, method: &str, payload: Vec<u8>) -> Result<Vec<u8>, RpcError>;
    fn emit(&self, event: AgentEvent);
}
```

- [ ] 为 `ApprovalRequest`/`ApprovalResponse`/`ApprovalDecision` 添加 serde 与 camelCase（`tools/mod.rs`）：

```rust
// rust-ody/crates/ody-host/src/tools/mod.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalDecision {
    Approved,
    Rejected,
    Cancelled,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub tool_call_id: String,
    pub tool_name: String,
    pub action: String,
    pub display: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApprovalResponse {
    pub decision: ApprovalDecision,
}
```

- [ ] 更新 `ConnectionHandle` 的 `EventSink` impl（`connection.rs`）：

```rust
// 在 rust-ody/crates/ody-host/src/transport/connection.rs 中 ConnectionHandle impl EventSink 段
#[async_trait::async_trait]
impl crate::events::EventSink for ConnectionHandle {
    async fn request(&self, method: &str, payload: Vec<u8>) -> Result<Vec<u8>, RpcError> {
        self.send_request(method, payload).await
    }

    fn emit(&self, event: AgentEvent) {
        let payload = serde_json::to_vec(&event).unwrap_or_default();
        let req_id = uuid::Uuid::new_v4().to_string();
        let item = OutboundItem::Request { req_id, payload };
        if let Err(e) = self.inner.outbound_tx.try_send(item) {
            tracing::warn!("event emit dropped: {e}");
        }
    }
}
```

- [ ] 更新 `host.rs` 中 `CoreHostApprovalClient` 使用反向 RPC：

```rust
// rust-ody/crates/ody-host/src/host.rs（替换原 emit-and-cancel 实现）
#[async_trait::async_trait]
impl ApprovalClient for CoreHostApprovalClient<'_> {
    async fn request(&self, request: ApprovalRequest) -> Result<ApprovalResponse, ToolError> {
        let payload = serde_json::to_vec(&request)
            .map_err(|e| ToolError::ApprovalFailed { source: Box::new(e) })?;
        let response_bytes = self
            .sink
            .request("requestApproval", payload)
            .await
            .map_err(|e| ToolError::ApprovalFailed { source: Box::new(e) })?;
        let response = serde_json::from_slice::<ApprovalResponse>(&response_bytes)
            .map_err(|e| ToolError::ApprovalFailed { source: Box::new(e) })?;
        Ok(response)
    }
}
```

- [ ] 更新 `host.rs` tests 中 `MockSink` 实现新 `EventSink`：

```rust
// rust-ody/crates/ody-host/src/host.rs tests 模块
struct MockSink(Arc<Mutex<Vec<AgentEvent>>>);

#[async_trait::async_trait]
impl EventSink for MockSink {
    async fn request(&self, _method: &str, _payload: Vec<u8>) -> Result<Vec<u8>, RpcError> {
        // 测试中不触发真正的反向 RPC，返回 cancelled 决策。
        let resp = crate::tools::ApprovalResponse {
            decision: crate::tools::ApprovalDecision::Cancelled,
        };
        Ok(serde_json::to_vec(&resp).unwrap())
    }
    fn emit(&self, event: AgentEvent) {
        self.0.lock().unwrap().push(event);
    }
}
```

- [ ] 查找所有 `EventSink` 实现，确认无遗漏：
  - `grep -rn "impl EventSink" rust-ody/crates/ody-host/src/`
  - 预期命中：`connection.rs` 的 `ConnectionHandle`、`host.rs` 的 `MockSink`；A7 的 `StderrSink` 在 `main.rs` 中将被删除，故无需更新。

- [ ] 替换 `main.rs`：

```rust
// rust-ody/crates/ody-host/src/main.rs
use std::sync::Arc;

use ody_host::config::{HostConfig, LogLevel};
use ody_host::host::CoreHost;
use ody_host::llm::openai::OpenAiProvider;
use ody_host::transport::{build_transport, RpcRouter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = HostConfig::from_cli(std::env::args()).map_err(|e| e.to_string())?;
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(match config.log_level {
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Error => tracing::Level::ERROR,
        })
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let (server, event_sink) = build_transport(config.transport.clone()).await?;
    let provider = Box::new(OpenAiProvider::new(config.provider.clone()));
    let host = Arc::new(CoreHost::new(config, event_sink, provider)?);
    let router = RpcRouter::new(host);
    let dispatch = router.into_byte_dispatch();

    tracing::info!("ody-host ready");
    server.serve(dispatch).await?;
    Ok(())
}
```

- [ ] 运行全 crate 测试与构建（共享签名变更必须整树 typecheck）：
  - `cd rust-ody && cargo test -p ody-host`
  - 预期：所有测试通过，包括 `transport::rpc::tests`、`host::tests`、`tools::bash::tests` 等。
  - `cd rust-ody && cargo build -p ody-host`
  - 预期：二进制编译通过。

- [ ] 提交：`git add rust-ody/crates/ody-host/src/ rust-ody/crates/ody-host/Cargo.toml && git commit -m "feat(ody-host): RpcRouter and EventSink reverse RPC"`

---

### Task B7: 跨语言 stdio 集成测试

**Depends on:** Task B4 (`StdioTransportServer`)、Task B6 (`RpcRouter`、`main.rs` 集成)

**Files：**
- Create: `packages/node-sdk/src/__tests__/rust-host-connect.test.ts`

**Steps：**

- [ ] 写失败测试：启动 `ody-host --stdio`，用 TS `createStreamTransport` 发送 `getCoreInfo` 与 `createSession`，断言 Rust 侧返回正确 JSON wrapper。

```typescript
// packages/node-sdk/src/__tests__/rust-host-connect.test.ts
import { spawn, execSync } from 'node:child_process';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

import { createStreamTransport } from '@odysseythink/agent-core';
import { afterEach, describe, expect, test } from 'vitest';

const workspaceRoot = resolve(__dirname, '../../../');
const cargoDir = join(workspaceRoot, 'rust-ody');
const binaryPath = join(cargoDir, 'target/debug/ody-host');

function buildHost(): void {
  execSync('cargo build -p ody-host', { cwd: cargoDir, stdio: 'pipe' });
}

function makeTempConfig(): { configPath: string; homeDir: string } {
  const homeDir = mkdtempSync(join(tmpdir(), 'ody-host-test-'));
  const configPath = join(homeDir, 'ody.toml');
  const escaped = homeDir.replace(/\\/g, '\\\\');
  writeFileSync(
    configPath,
    `home_dir = "${escaped}"\nlog_level = "info"\n\n[provider]\napi_key = ""\ndefault_model = "mock"\n`,
  );
  return { configPath, homeDir };
}

function encodeRequest(method: string, payload: unknown): Uint8Array {
  return new TextEncoder().encode(JSON.stringify({ method, args: [payload] }));
}

describe('rust host stdio transport', () => {
  let proc: ReturnType<typeof spawn> | undefined;
  let transport: ReturnType<typeof createStreamTransport> | undefined;
  let cleanup: (() => void) | undefined;

  afterEach(() => {
    transport?.close();
    proc?.kill();
    cleanup?.();
  });

  test(
    'getCoreInfo and createSession roundtrip',
    async () => {
      buildHost();
      const { configPath, homeDir } = makeTempConfig();
      cleanup = () => rmSync(homeDir, { recursive: true, force: true });

      proc = spawn(binaryPath, ['--config', configPath, '--stdio'], {
        stdio: ['pipe', 'pipe', 'pipe'],
      });

      // 等待 host 完成初始化（读取第一条 stderr 日志）。
      await new Promise<void>((resolve, reject) => {
        const onData = (): void => {
          proc!.stderr.off('data', onData);
          resolve();
        };
        proc!.stderr.on('data', onData);
        proc!.once('error', reject);
        proc!.once('exit', (code) => {
          reject(new Error(`ody-host exited with ${String(code)}`));
        });
      });

      transport = createStreamTransport(
        proc.stdout,
        proc.stdin,
        async () => {
          // host 的反向 RPC / emitEvent 走到这里；测试无需处理，返回空成功。
          return new TextEncoder().encode(JSON.stringify({ ok: true, value: null }));
        },
        { framing: 'length-prefixed' },
      );

      const infoBytes = await transport.send(encodeRequest('getCoreInfo', {}));
      const info = JSON.parse(new TextDecoder().decode(infoBytes));
      expect(info.ok).toBe(true);
      expect(info.value.version).toMatch(/^\d+\.\d+\.\d+/);

      const createBytes = await transport.send(
        encodeRequest('createSession', { workDir: process.cwd() }),
      );
      const create = JSON.parse(new TextDecoder().decode(createBytes));
      expect(create.ok).toBe(true);
      expect(create.value.id).toBeDefined();
      expect(create.value.workDir).toBe(process.cwd());
    },
    60000,
  );
});
```

- [ ] 运行并验证失败：
  - `cd /Users/ranwei/workspace/ody-code && pnpm vitest run packages/node-sdk/src/__tests__/rust-host-connect.test.ts`
  - 预期：若 `ody-host` 尚未构建或 `RpcRouter` 未实现，测试启动失败；若 wire 协议不兼容，则 `transport.send` 超时或解析失败。

- [ ] 写最小实现：B7 本身已是测试代码，无额外实现；其通过依赖于 B6 的 `main.rs` 集成。确认 B6 已完成后直接运行测试。

- [ ] 运行并验证通过：
  - `cd /Users/ranwei/workspace/ody-code && pnpm vitest run packages/node-sdk/src/__tests__/rust-host-connect.test.ts`
  - 预期：测试通过，`getCoreInfo` 返回 version，`createSession` 返回 session id。

- [ ] 提交：`git add packages/node-sdk/src/__tests__/rust-host-connect.test.ts && git commit -m "test(node-sdk): cross-language stdio integration with ody-host"`

---

## Local Self-Review

- [ ] 1. Spec-coverage table: 本 Part 覆盖 design `transport.md` §2.1-2.4（Wire/TransportServer/RpcRouter/EventSink）与 §3.1-3.8（framing/encoding/handshake/connection/dispatch 算法）。B1-B3 覆盖 wire 与 connection；B4/B5 覆盖 stdio/socket server；B6 覆盖 RpcRouter 与 EventSink 反向 RPC；B7 覆盖跨语言集成。
- [ ] 2. Placeholder scan: 无 `TODO`/`TBD`；B4/B5 的 manual verification 明确标注仅验证进程启动，端到端验证由 B7 完成，这不是 deferred dependency，而是任务边界声明。
- [ ] 3. No phantom tasks: 每个任务产生可编译/可测试的代码变更。B4/B5 虽无单元测试，但有 `cargo build` 与 manual verification；B7 是完整测试。
- [ ] 4. Dependency soundness: B1→B2→B3；B4/B5 依赖 B3；B6 依赖 A7+B3+B4+B5；B7 依赖 B4+B6。无反向依赖。
- [ ] 5. Caller & build soundness: B6 是共享签名变更任务，扩展 `EventSink` trait 并更新 `events.rs`、`connection.rs`、`host.rs`（含 tests）和 `main.rs`；以 `cargo test -p ody-host` 全 crate 测试 + `cargo build -p ody-host` 结束。B5 扩展 `HostError` 加 `IoGeneric`，仅影响本地 pattern 匹配，无外部 caller。
- [ ] 6. Test-the-risk: B1 测试 length-prefixed/ndjson roundtrip 与 TS `Uint8Array.toJSON()` 兼容；B2 测试 handshake 拒绝无效 framing；B3 测试请求-响应与反向请求双回路；B5 测试 ready message JSON shape；B6 测试 RpcRouter 路由与错误包装；B7 跨语言断言 `getCoreInfo`/`createSession` 字节级兼容。
- [ ] 7. Type consistency: `RpcResponseWrapper` 的 `ok`/`value`/`error` 形状与 TS `RpcResponse` 一致；`ApprovalRequest` 使用 `camelCase` 序列化匹配 TS `ApprovalRequest`；`ApprovalDecision` 使用 `lowercase` 匹配 TS `ApprovalDecision`；`ReadyMessage` 字段名与 TS `ReadyMessage` 一致。
