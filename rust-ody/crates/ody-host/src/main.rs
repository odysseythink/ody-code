use std::sync::Arc;

use ody_host::config::{HostConfig, LogLevel};
use ody_host::error::HostError;
use ody_host::host::CoreHost;
use ody_host::llm::chat_provider_adapter::ChatProviderLlmAdapter;
use ody_host::llm::mock::MockProvider;
use ody_host::provider_factory::create_host_provider;
use ody_host::transport::{build_transport, RpcRouter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = match HostConfig::from_cli(std::env::args()) {
        Ok(config) => config,
        Err(HostError::CliHelp { message }) => {
            println!("{message}");
            return Ok(());
        }
        Err(HostError::CliVersion { message }) => {
            println!("{message}");
            return Ok(());
        }
        Err(e) => return Err(e.to_string().into()),
    };
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_max_level(match config.log_level {
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Error => tracing::Level::ERROR,
        })
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let (server, event_sink) = build_transport(config.transport.clone()).await?;
    let event_sink: Arc<dyn ody_host::events::EventSink> = Arc::from(event_sink);
    let provider: Arc<dyn ody_host::llm::LlmProvider> = if config.mock_provider {
        Arc::new(MockProvider::new())
    } else {
        let chat_provider = create_host_provider(&config.provider).map_err(|e| e.to_string())?;
        Arc::new(ChatProviderLlmAdapter::new(chat_provider))
    };
    let host = Arc::new(CoreHost::new(config, event_sink, provider)?);
    let router = RpcRouter::new(host);
    let dispatch = router.into_byte_dispatch();

    tracing::info!("ody-host ready");
    server.serve(dispatch).await?;
    Ok(())
}
