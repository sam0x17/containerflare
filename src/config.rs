use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;

use containerflare_command::CommandEndpoint;
use dotenvy::Error as DotenvError;
use thiserror::Error;

use crate::platform::RuntimePlatform;

const DEFAULT_CLOUDFLARE_PORT: u16 = 8787;
const DEFAULT_CLOUD_RUN_PORT: u16 = 8080;
const CLOUD_RUN_COMMAND_REASON: &str = "host command channel is not available on Google Cloud Run";
const PORT_ENV: &str = "PORT";
const LEGACY_PORT_ENV: &str = "CF_CONTAINER_PORT";

/// Configuration consumed by the runtime before spinning up Axum/hyper.
#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    pub bind_addr: SocketAddr,
    pub platform: RuntimePlatform,
    pub command_endpoint: Option<CommandEndpoint>,
    pub command_disabled_reason: Option<String>,
}

impl RuntimeConfig {
    /// Loads configuration from Cloudflare-supplied `CF_*` environment variables.
    ///
    /// Values from a local `.env` file (parsed via [`dotenvy::dotenv_override`]) override whatever is already set in
    /// the process environment, which makes local development workflows predictable.
    pub fn from_env() -> Result<Self, ConfigError> {
        load_env_overrides()?;

        let platform = RuntimePlatform::detect();

        let port = resolve_port(&platform);

        let addr = env::var("CF_CONTAINER_ADDR")
            .ok()
            .and_then(|value| value.parse::<IpAddr>().ok())
            .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));

        let bind_addr = SocketAddr::new(addr, port);

        let command_endpoint = env::var("CF_CMD_ENDPOINT")
            .ok()
            .map(|value| {
                CommandEndpoint::from_str(&value)
                    .map_err(|_| ConfigError::InvalidCommandEndpoint(value))
            })
            .transpose()?; // convert Option<Result> -> Result<Option>

        let (command_endpoint, command_disabled_reason) = match command_endpoint {
            Some(endpoint) => (Some(endpoint), None),
            None => match platform {
                RuntimePlatform::CloudRun(_) => (None, Some(CLOUD_RUN_COMMAND_REASON.to_owned())),
                _ => (Some(CommandEndpoint::Stdio), None),
            },
        };

        Ok(Self {
            bind_addr,
            platform,
            command_endpoint,
            command_disabled_reason,
        })
    }

    /// Returns a builder for programmatic overrides.
    pub fn builder() -> RuntimeConfigBuilder {
        RuntimeConfigBuilder::default()
    }
}

impl Default for RuntimeConfig {
    /// Binds to `0.0.0.0:8787` and talks to the host over stdio.
    fn default() -> Self {
        // Default matches the local Cloudflare containers sidecar contract.
        Self {
            bind_addr: SocketAddr::new(
                IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                resolve_port(&RuntimePlatform::default()),
            ),
            platform: RuntimePlatform::default(),
            command_endpoint: Some(CommandEndpoint::Stdio),
            command_disabled_reason: None,
        }
    }
}

/// Builder type for [`RuntimeConfig`].
#[derive(Default, Clone, Debug)]
pub struct RuntimeConfigBuilder {
    bind_addr: Option<SocketAddr>,
    platform: Option<RuntimePlatform>,
    command_endpoint: Option<CommandEndpoint>,
    command_disabled_reason: Option<String>,
}

impl RuntimeConfigBuilder {
    /// Sets the address for the embedded Axum listener.
    pub fn bind_addr(mut self, addr: SocketAddr) -> Self {
        self.bind_addr = Some(addr);
        self
    }

    /// Sets the active runtime platform (Cloudflare, Cloud Run, etc.).
    pub fn platform(mut self, platform: RuntimePlatform) -> Self {
        self.platform = Some(platform);
        self
    }

    /// Sets the host command endpoint transport.
    pub fn command_endpoint(mut self, endpoint: CommandEndpoint) -> Self {
        self.command_endpoint = Some(endpoint);
        self.command_disabled_reason = None;
        self
    }

    /// Disables the host command channel entirely with an explanatory reason.
    pub fn disable_command_channel(mut self, reason: impl Into<String>) -> Self {
        self.command_endpoint = None;
        self.command_disabled_reason = Some(reason.into());
        self
    }

    /// Builds the final configuration.
    pub fn build(self) -> RuntimeConfig {
        let command_disabled_reason = self.command_disabled_reason;
        let platform = self.platform.unwrap_or_default();
        let command_endpoint = if command_disabled_reason.is_some() {
            None
        } else {
            Some(self.command_endpoint.unwrap_or_default())
        };

        RuntimeConfig {
            bind_addr: self.bind_addr.unwrap_or_else(|| {
                SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), resolve_port(&platform))
            }),
            platform,
            command_endpoint,
            command_disabled_reason,
        }
    }
}

/// Errors that can occur while building [`RuntimeConfig`].
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid command endpoint: {0}")]
    InvalidCommandEndpoint(String),
    #[error("failed to load .env overrides: {0}")]
    Dotenv(#[from] DotenvError),
}

fn load_env_overrides() -> Result<(), ConfigError> {
    match dotenvy::dotenv_override() {
        Ok(_) => Ok(()),
        Err(err) if err.not_found() => Ok(()),
        Err(err) => Err(ConfigError::Dotenv(err)),
    }
}

fn resolve_port(platform: &RuntimePlatform) -> u16 {
    env::var(PORT_ENV)
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .or_else(|| {
            env::var(LEGACY_PORT_ENV)
                .ok()
                .and_then(|value| value.parse::<u16>().ok())
        })
        .unwrap_or_else(|| match platform {
            RuntimePlatform::CloudRun(_) => DEFAULT_CLOUD_RUN_PORT,
            _ => DEFAULT_CLOUDFLARE_PORT,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use containerflare_command::CommandEndpoint;
    #[cfg(unix)]
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        ENV_LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn builder_overrides_defaults() {
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 8)), 9999);
        let config = RuntimeConfig::builder()
            .bind_addr(addr)
            .command_endpoint(CommandEndpoint::Tcp("127.0.0.1:9998".into()))
            .build();

        assert_eq!(config.bind_addr, addr);
        assert!(matches!(
            config.command_endpoint,
            Some(CommandEndpoint::Tcp(_))
        ));
        assert!(config.command_disabled_reason.is_none());
    }

    #[test]
    fn builder_disables_command_channel() {
        let config = RuntimeConfig::builder()
            .disable_command_channel("disabled for tests")
            .build();

        assert!(config.command_endpoint.is_none());
        assert_eq!(
            config.command_disabled_reason.as_deref(),
            Some("disabled for tests")
        );
    }

    #[test]
    fn parses_command_endpoint_strings() {
        assert!(matches!(
            "stdio".parse::<CommandEndpoint>(),
            Ok(CommandEndpoint::Stdio)
        ));
        assert!(matches!(
            "tcp://127.0.0.1:1111".parse::<CommandEndpoint>(),
            Ok(CommandEndpoint::Tcp(addr)) if addr == "127.0.0.1:1111"
        ));
        assert!(matches!(
            "disabled".parse::<CommandEndpoint>(),
            Ok(CommandEndpoint::Unavailable)
        ));

        #[cfg(unix)]
        {
            let endpoint = "unix:///tmp/socket".parse::<CommandEndpoint>();
            assert!(
                matches!(endpoint, Ok(CommandEndpoint::UnixSocket(path)) if path == PathBuf::from("/tmp/socket"))
            );
        }
    }

    #[test]
    fn reads_env_configuration() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            std::env::set_var("CF_CONTAINER_PORT", "9000");
            std::env::set_var("CF_CONTAINER_ADDR", "127.0.0.2");
            std::env::set_var("CF_CMD_ENDPOINT", "tcp://127.0.0.1:7878");
        }

        let config = RuntimeConfig::from_env().expect("config");
        assert_eq!(
            config.bind_addr,
            SocketAddr::new("127.0.0.2".parse().unwrap(), 9000)
        );
        assert!(matches!(
            config.command_endpoint,
            Some(CommandEndpoint::Tcp(_))
        ));
        assert!(config.command_disabled_reason.is_none());

        unsafe {
            std::env::remove_var("CF_CONTAINER_PORT");
            std::env::remove_var("CF_CONTAINER_ADDR");
            std::env::remove_var("CF_CMD_ENDPOINT");
        }
    }

    #[test]
    fn infers_cloud_run_defaults() {
        let _guard = env_lock().lock().unwrap();
        unsafe {
            std::env::remove_var("CF_CONTAINER_PORT");
            std::env::remove_var("CF_CONTAINER_ADDR");
            std::env::remove_var("CF_CMD_ENDPOINT");
            std::env::set_var("PORT", "1234");
            std::env::set_var("K_SERVICE", "test-service");
        }

        let config = RuntimeConfig::from_env().expect("config");
        assert_eq!(
            config.bind_addr,
            SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 1234)
        );
        assert!(config.command_endpoint.is_none());
        assert_eq!(
            config.command_disabled_reason.as_deref(),
            Some(CLOUD_RUN_COMMAND_REASON)
        );

        unsafe {
            std::env::remove_var("PORT");
            std::env::remove_var("K_SERVICE");
            std::env::remove_var("CF_CONTAINER_PORT");
            std::env::remove_var("CF_CONTAINER_ADDR");
            std::env::remove_var("CF_CMD_ENDPOINT");
        }
    }
}
