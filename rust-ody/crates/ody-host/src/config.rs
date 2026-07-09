use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use serde::Deserialize;

use crate::error::HostError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportMode {
    Stdio,
    UnixSocket { path: PathBuf },
    TcpSocket { host: String, port: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogLevel {
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderConfig {
    pub provider_id: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct HostConfig {
    pub home_dir: PathBuf,
    pub config_path: Option<PathBuf>,
    pub transport: TransportMode,
    pub log_level: LogLevel,
    pub provider: ProviderConfig,
    pub mock_provider: bool,
}

#[derive(Debug, Args)]
struct SharedArgs {
    #[arg(long)]
    stdio: bool,
    #[arg(long)]
    socket_path: Option<PathBuf>,
    #[arg(long)]
    tcp_host: Option<String>,
    #[arg(long)]
    tcp_port: Option<u16>,
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long)]
    home: Option<PathBuf>,
    #[arg(long, default_value = "info")]
    log_level: String,
    #[arg(long)]
    provider: Option<String>,
    #[arg(long, hide = true)]
    mock_provider: bool,
}

#[derive(Debug, Parser)]
#[command(name = "ody-host", version)]
struct Cli {
    #[command(flatten)]
    shared: SharedArgs,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Serve(ServeArgs),
}

#[derive(Debug, Args)]
struct ServeArgs {
    #[command(flatten)]
    shared: SharedArgs,
}

#[derive(Debug, Deserialize)]
struct RawConfigFile {
    home_dir: Option<PathBuf>,
    log_level: Option<String>,
    provider: Option<RawProvider>,
}

#[derive(Debug, Deserialize)]
struct RawProvider {
    provider_id: Option<String>,
    api_key: String,
    base_url: Option<String>,
    default_model: Option<String>,
}

impl HostConfig {
    pub fn from_cli<I, T>(args: I) -> Result<Self, HostError>
    where
        I: Iterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let cli = Cli::try_parse_from(args).map_err(|e| {
            let message = e.to_string();
            match e.kind() {
                clap::error::ErrorKind::DisplayHelp => HostError::CliHelp { message },
                clap::error::ErrorKind::DisplayVersion => HostError::CliVersion { message },
                _ => HostError::config_invalid(message),
            }
        })?;
        let active = active_args(&cli)?;

        let home_dir = active.home.clone().unwrap_or_else(default_home_dir);
        let config_path = active.config.clone().or_else(|| {
            let toml = home_dir.join("ody.toml");
            if toml.exists() {
                Some(toml)
            } else {
                let json = home_dir.join("ody.json");
                if json.exists() {
                    Some(json)
                } else {
                    None
                }
            }
        });

        let file: RawConfigFile = match &config_path {
            Some(path) => load_raw_config(path)?,
            None => RawConfigFile {
                home_dir: None,
                log_level: None,
                provider: None,
            },
        };

        let transport = if let Some(path) = active.socket_path.clone() {
            TransportMode::UnixSocket { path }
        } else if let (Some(host), Some(port)) = (active.tcp_host.clone(), active.tcp_port) {
            TransportMode::TcpSocket { host, port }
        } else {
            TransportMode::Stdio
        };

        let log_level = parse_log_level(&active.log_level)?;

        let provider = ProviderConfig {
            provider_id: active
                .provider
                .clone()
                .or_else(|| file.provider.as_ref().and_then(|p| p.provider_id.clone()))
                .unwrap_or_else(|| "openai".to_string()),
            api_key: file
                .provider
                .as_ref()
                .map(|p| p.api_key.clone())
                .unwrap_or_default(),
            base_url: file.provider.as_ref().and_then(|p| p.base_url.clone()),
            default_model: Some(
                file.provider
                    .as_ref()
                    .and_then(|p| p.default_model.clone())
                    .unwrap_or_else(|| "gpt-4o-mini".to_string()),
            ),
        };

        Ok(HostConfig {
            home_dir: file.home_dir.unwrap_or(home_dir),
            config_path,
            transport,
            log_level,
            provider,
            mock_provider: active.mock_provider,
        })
    }
}

fn active_args(cli: &Cli) -> Result<&SharedArgs, HostError> {
    match &cli.command {
        Some(Command::Serve(serve)) => {
            let has_global_flags = cli.shared.stdio
                || cli.shared.socket_path.is_some()
                || cli.shared.tcp_host.is_some()
                || cli.shared.tcp_port.is_some()
                || cli.shared.config.is_some()
                || cli.shared.home.is_some()
                || cli.shared.provider.is_some()
                || cli.shared.log_level != "info";
            if has_global_flags {
                return Err(HostError::config_invalid(
                    "cannot use global flags together with the `serve` subcommand".to_string(),
                ));
            }
            Ok(&serve.shared)
        }
        None => Ok(&cli.shared),
    }
}

fn default_home_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ody")
}

fn parse_log_level(s: &str) -> Result<LogLevel, HostError> {
    match s.to_lowercase().as_str() {
        "debug" => Ok(LogLevel::Debug),
        "info" => Ok(LogLevel::Info),
        "warn" => Ok(LogLevel::Warn),
        "error" => Ok(LogLevel::Error),
        _ => Err(HostError::config_invalid(format!("unknown log level: {s}"))),
    }
}

fn load_raw_config(path: &PathBuf) -> Result<RawConfigFile, HostError> {
    let bytes = std::fs::read(path).map_err(|e| HostError::Io {
        source: e,
        path: path.clone(),
    })?;
    if path.extension().and_then(|s| s.to_str()) == Some("json") {
        serde_json::from_slice(&bytes).map_err(|e| HostError::config_invalid(format!("{e}")))
    } else {
        let s =
            std::str::from_utf8(&bytes).map_err(|e| HostError::config_invalid(format!("{e}")))?;
        toml::from_str(s).map_err(|e| HostError::config_invalid(format!("{e}")))
    }
}

// Include the test module from step 2 at the bottom
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_stdio_transport() {
        let args = vec!["ody-host"];
        let config = HostConfig::from_cli(args.into_iter()).unwrap();
        assert!(matches!(config.transport, TransportMode::Stdio));
        assert_eq!(config.log_level, LogLevel::Info);
    }

    #[test]
    fn socket_path_from_cli() {
        let args = vec!["ody-host", "--socket-path", "/tmp/ody.sock"];
        let config = HostConfig::from_cli(args.into_iter()).unwrap();
        assert_eq!(
            config.transport,
            TransportMode::UnixSocket {
                path: std::path::PathBuf::from("/tmp/ody.sock")
            }
        );
    }

    #[test]
    fn invalid_log_level_fails() {
        let args = vec!["ody-host", "--log-level", "verbose"];
        let err = HostConfig::from_cli(args.into_iter()).unwrap_err();
        assert!(err.to_string().contains("verbose"));
    }

    #[test]
    fn serve_subcommand_stdio() {
        let args = vec!["ody-host", "serve", "--stdio"];
        let config = HostConfig::from_cli(args.into_iter()).unwrap();
        assert!(matches!(config.transport, TransportMode::Stdio));
        assert_eq!(config.log_level, LogLevel::Info);
    }

    #[test]
    fn serve_subcommand_socket_path() {
        let args = vec!["ody-host", "serve", "--socket-path", "/tmp/ody-serve.sock"];
        let config = HostConfig::from_cli(args.into_iter()).unwrap();
        assert_eq!(
            config.transport,
            TransportMode::UnixSocket {
                path: std::path::PathBuf::from("/tmp/ody-serve.sock")
            }
        );
    }

    #[test]
    fn global_flags_stdio_still_works() {
        let args = vec!["ody-host", "--stdio"];
        let config = HostConfig::from_cli(args.into_iter()).unwrap();
        assert!(matches!(config.transport, TransportMode::Stdio));
        assert_eq!(config.log_level, LogLevel::Info);
    }

    #[test]
    fn global_flags_conflict_with_serve_rejected() {
        let args = vec![
            "ody-host",
            "--stdio",
            "serve",
            "--socket-path",
            "/tmp/ody.sock",
        ];
        let err = HostConfig::from_cli(args.into_iter()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("global flags") || msg.contains("serve"),
            "expected conflict error, got: {msg}"
        );
    }

    #[test]
    fn mock_provider_flag_is_parsed() {
        let args = vec!["ody-host", "--stdio", "--mock-provider"];
        let config = HostConfig::from_cli(args.into_iter()).unwrap();
        assert!(config.mock_provider);
    }
}

#[cfg(test)]
mod provider_config_tests {
    use super::*;

    #[test]
    fn cli_provider_overrides_default() {
        let args = vec!["ody-host", "--provider", "anthropic", "--stdio"];
        let config = HostConfig::from_cli(args.into_iter()).unwrap();
        assert_eq!(config.provider.provider_id, "anthropic");
    }

    #[test]
    fn config_file_provider_is_parsed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ody.toml");
        std::fs::write(
            &path,
            "[provider]\nprovider_id = \"kimi\"\napi_key = \"sk\"\n",
        )
        .unwrap();
        let args = vec!["ody-host", "--config", path.to_str().unwrap(), "--stdio"];
        let config = HostConfig::from_cli(args.into_iter()).unwrap();
        assert_eq!(config.provider.provider_id, "kimi");
        assert_eq!(config.provider.api_key, "sk");
    }

    #[test]
    fn default_provider_is_openai() {
        let args = vec!["ody-host", "--stdio"];
        let config = HostConfig::from_cli(args.into_iter()).unwrap();
        assert_eq!(config.provider.provider_id, "openai");
    }
}
