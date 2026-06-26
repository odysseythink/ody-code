use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum RpcError {
    Parse { message: String },
    Transport { message: String },
    Handler { message: String },
    MethodNotFound(String),
    Serialize(serde_json::Error),
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RpcError::Parse { message } => write!(f, "rpc parse error: {message}"),
            RpcError::Transport { message } => write!(f, "rpc transport error: {message}"),
            RpcError::Handler { message } => write!(f, "rpc handler error: {message}"),
            RpcError::MethodNotFound(m) => write!(f, "rpc method not found: {m}"),
            RpcError::Serialize(e) => write!(f, "rpc serialize error: {e}"),
        }
    }
}

impl std::error::Error for RpcError {}

impl From<serde_json::Error> for RpcError {
    fn from(e: serde_json::Error) -> Self {
        RpcError::Serialize(e)
    }
}

#[derive(Debug)]
pub enum HostError {
    ConfigInvalid { message: String },
    Io { source: std::io::Error, path: PathBuf },
    IoGeneric { message: String },
    CliHelp { message: String },
    CliVersion { message: String },
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostError::ConfigInvalid { message } => write!(f, "invalid config: {message}"),
            HostError::Io { source, path } => write!(f, "io error at {}: {source}", path.display()),
            HostError::IoGeneric { message } => write!(f, "io error: {message}"),
            HostError::CliHelp { message } | HostError::CliVersion { message } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for HostError {}

impl HostError {
    pub fn config_invalid(message: impl Into<String>) -> Self {
        HostError::ConfigInvalid { message: message.into() }
    }
}

// --- TransportError ---
#[derive(Debug)]
pub enum TransportError {
    Io(std::io::Error),
    InvalidFraming(String),
    Unauthorized,
    Closed,
    SocketBind { path: std::path::PathBuf, source: std::io::Error },
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

impl From<TransportError> for RpcError {
    fn from(e: TransportError) -> Self {
        RpcError::Transport { message: e.to_string() }
    }
}

impl From<TransportError> for HostError {
    fn from(e: TransportError) -> Self {
        HostError::Io { source: std::io::Error::other(e.to_string()), path: std::path::PathBuf::new() }
    }
}
