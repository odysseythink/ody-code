use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::transport::connection::StreamConnection;
use crate::transport::wire::Framing;
use crate::transport::{ByteDispatch, TransportError, TransportServer};

pub struct StdioTransportServer {
    conn: Mutex<Option<StreamConnection>>,
}

impl StdioTransportServer {
    /// Returns server and its EventSink (ConnectionHandle).
    pub fn new() -> (Self, Box<dyn crate::events::EventSink>) {
        let ready = serde_json::json!({
            "type": "ready",
            "stdio": true,
        });
        eprintln!("{}", ready);
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
        conn.start(stdin, stdout, Framing::LengthPrefixed, dispatch)
            .await
    }
}
