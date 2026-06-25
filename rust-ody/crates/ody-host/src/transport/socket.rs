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
#[serde(rename_all = "camelCase")]
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
        let listener = UnixListener::bind(&path).map_err(|e| HostError::IoGeneric {
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
        let conn = self.conn.lock().unwrap().take().expect("serve() called more than once");
        let stream = self.stream.lock().unwrap().take().expect("serve() called more than once");
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
        let conn = self.conn.lock().unwrap().take().expect("serve() called more than once");
        let stream = self.stream.lock().unwrap().take().expect("serve() called more than once");
        let (read, write) = stream.into_split();
        conn.start(read, write, Framing::LengthPrefixed, dispatch).await
    }
}

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
