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
    async fn serve(&self, dispatch: Arc<ByteDispatch>) -> Result<(), TransportError>;
}

pub async fn build_transport(
    mode: TransportMode,
) -> Result<(std::sync::Arc<dyn TransportServer>, Box<dyn crate::events::EventSink>), crate::error::HostError> {
    match mode {
        TransportMode::Stdio => {
            let (server, sink) = stdio::StdioTransportServer::new();
            Ok((std::sync::Arc::new(server), sink))
        }
        TransportMode::UnixSocket { path } => {
            let (server, sink) = socket::UnixSocketTransportServer::bind(path).await?;
            Ok((std::sync::Arc::new(server), sink))
        }
        TransportMode::TcpSocket { host, port } => {
            let (server, sink) = socket::TcpSocketTransportServer::bind(host, port).await?;
            Ok((std::sync::Arc::new(server), sink))
        }
    }
}
