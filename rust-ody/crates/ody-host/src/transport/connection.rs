use std::collections::HashMap;
use std::sync::Arc;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::error::{RpcError, TransportError};
use crate::events::{AgentEvent, EventSink};
use crate::transport::wire::{decode_frame, encode_frame, Framing, WireError, WireMessage};
use crate::transport::ByteDispatch;

#[derive(Debug)]
enum OutboundItem {
    Response {
        req_id: String,
        result: Result<Vec<u8>, RpcError>,
    },
    Request {
        req_id: String,
        payload: Vec<u8>,
    },
}

#[derive(Clone)]
pub struct ConnectionHandle {
    inner: Arc<ConnectionInner>,
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

    pub async fn send_request(&self, _method: &str, payload: Vec<u8>) -> Result<Vec<u8>, RpcError> {
        let req_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.inner.pending.lock().await.insert(req_id.clone(), tx);
        self.inner
            .outbound_tx
            .send(OutboundItem::Request { req_id, payload })
            .await
            .map_err(|_| RpcError::Transport {
                message: "transport closed".to_string(),
            })?;
        rx.await.map_err(|_| RpcError::Transport {
            message: "transport closed".to_string(),
        })?
    }
}

#[async_trait::async_trait]
impl EventSink for ConnectionHandle {
    async fn request(&self, method: &str, payload: Vec<u8>) -> Result<Vec<u8>, RpcError> {
        let rpc_payload = wrap_rpc_request(method, payload);
        self.send_request(method, rpc_payload).await
    }

    fn emit(&self, event: AgentEvent) {
        let event_bytes = serde_json::to_vec(&event).unwrap_or_default();
        let payload = wrap_rpc_request("emitEvent", event_bytes);
        let req_id = uuid::Uuid::new_v4().to_string();
        let item = OutboundItem::Request { req_id, payload };
        if let Err(e) = self.inner.outbound_tx.try_send(item) {
            tracing::warn!("event emit dropped: {e}");
        }
    }
}

fn wrap_rpc_request(method: &str, payload: Vec<u8>) -> Vec<u8> {
    let payload_value: serde_json::Value =
        serde_json::from_slice(&payload).unwrap_or(serde_json::Value::Null);
    let wrapper = serde_json::json!({
        "method": method,
        "args": [payload_value],
    });
    serde_json::to_vec(&wrapper).unwrap_or_default()
}

pub struct StreamConnection {
    inner: Arc<ConnectionInner>,
    outbound_rx: Option<mpsc::Receiver<OutboundItem>>,
}

impl StreamConnection {
    pub fn new() -> (Self, ConnectionHandle) {
        let (outbound_tx, outbound_rx) = mpsc::channel::<OutboundItem>(128);
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let inner = Arc::new(ConnectionInner {
            outbound_tx,
            pending,
        });
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
                WireMessage::Response {
                    req_id,
                    bytes,
                    error,
                } => {
                    let result = match error {
                        Some(e) => Err(RpcError::Handler { message: e.message }),
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
                let len = u32::from_le_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]])
                    as usize;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::wire::{decode_frame, encode_frame, Framing, WireMessage};
    use std::sync::Arc;

    fn echo_dispatch() -> Arc<ByteDispatch> {
        Arc::new(|bytes: &[u8]| {
            let response = bytes.to_vec();
            Box::pin(async move { Ok(response) })
        })
    }

    #[tokio::test]
    async fn request_response_roundtrip() {
        let (mut client_read, server_write) = tokio::io::duplex(1024);
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

            // Read the response before closing the write end,
            // so the server's reader loop has time to respond.
            let mut buf = vec![0u8; 1024];
            let n = client_read.read(&mut buf).await.unwrap();
            let mut offset = 0usize;
            let resp = decode_frame(&buf[..n], Framing::LengthPrefixed, &mut offset).unwrap();
            match resp {
                WireMessage::Response {
                    req_id,
                    bytes,
                    error,
                } => {
                    assert_eq!(req_id, "c1");
                    assert_eq!(bytes.unwrap(), b"hello");
                    assert!(error.is_none());
                }
                _ => panic!("expected response"),
            }

            drop(client_write);
        });

        client.await.unwrap();
        drop(handle);
        task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn reverse_request_roundtrip() {
        let (mut client_read, server_write) = tokio::io::duplex(1024);
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

        let response = handle
            .send_request("myMethod", b"call".to_vec())
            .await
            .unwrap();
        assert_eq!(response, b"ok");

        client.await.unwrap();
        drop(handle);
        task.await.unwrap().unwrap();
    }
}
