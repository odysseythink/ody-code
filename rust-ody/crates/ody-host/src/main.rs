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
