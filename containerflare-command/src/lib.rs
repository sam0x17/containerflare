use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

#[cfg(unix)]
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::io::{self, AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tokio::time;

#[cfg(unix)]
use tokio::net::UnixStream;
use tokio::net::{
    TcpStream, tcp::OwnedReadHalf as TcpOwnedReadHalf, tcp::OwnedWriteHalf as TcpOwnedWriteHalf,
};
#[cfg(unix)]
use tokio::net::{
    unix::OwnedReadHalf as UnixOwnedReadHalf, unix::OwnedWriteHalf as UnixOwnedWriteHalf,
};

const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

/// Describes how the container establishes the host command channel transport.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum CommandEndpoint {
    #[default]
    Stdio,
    #[cfg(unix)]
    UnixSocket(PathBuf),
    Tcp(String),
    /// Marker used when the runtime intentionally disables the command channel.
    Unavailable,
}

impl FromStr for CommandEndpoint {
    type Err = CommandEndpointParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let value = s.trim();
        if value.eq_ignore_ascii_case("stdio") {
            return Ok(CommandEndpoint::Stdio);
        }

        if value.eq_ignore_ascii_case("disabled") || value.eq_ignore_ascii_case("unavailable") {
            return Ok(CommandEndpoint::Unavailable);
        }

        #[cfg(unix)]
        if let Some(path) = value.strip_prefix("unix://") {
            return Ok(CommandEndpoint::UnixSocket(PathBuf::from(path)));
        }

        if let Some(addr) = value.strip_prefix("tcp://") {
            return Ok(CommandEndpoint::Tcp(addr.to_owned()));
        }

        Err(CommandEndpointParseError::InvalidCommandEndpoint(
            value.to_owned(),
        ))
    }
}

/// Errors encountered while parsing a [`CommandEndpoint`] from a string.
#[derive(Debug, Error, Clone)]
pub enum CommandEndpointParseError {
    #[error("invalid command endpoint: {0}")]
    InvalidCommandEndpoint(String),
}

/// High-level client that talks to Cloudflare's host-managed command channel (Cloud Run does not expose one).
///
/// Commands are framed as JSON lines and travel over stdin/stdout (default), TCP, or
/// Unix sockets (when enabled). Responses are deserialized back into [`CommandResponse`]
/// instances and surfaced through async APIs.
///
/// # Transport Modes
/// - `stdio`: bidirectional pipes that the Workers container sidecar keeps open.
/// - `tcp://host:port`: an explicit TCP socket managed by the sidecar.
/// - `unix://path` *(Unix only)*: a Unix domain socket exposed by the sidecar.
///
/// # Errors
/// All async constructors and [`CommandClient::send`] return [`CommandError`] when the transport
/// cannot be established, the host drops the channel, or the host reports a failure.
///
/// # Examples
/// ```ignore
/// use containerflare_command::{CommandClient, CommandEndpoint, CommandRequest};
///
/// # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
/// let client = CommandClient::connect(CommandEndpoint::Stdio).await?;
/// let response = client.send(CommandRequest::empty("health_check")).await?;
/// assert!(response.ok);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct CommandClient {
    inner: Arc<CommandClientInner>,
}

#[derive(Debug)]
struct CommandClientInner {
    endpoint: CommandEndpoint,
    writer: CommandWriter,
    reader: CommandReader,
    timeout: Duration,
}

impl CommandClient {
    /// Connects to the configured endpoint using the default timeout.
    ///
    /// # Parameters
    /// * `endpoint` - Transport descriptor (stdio, TCP, or Unix socket).
    ///
    /// # Returns
    /// A connected [`CommandClient`] ready to issue commands.
    ///
    /// # Errors
    /// Returns [`CommandError`] if the underlying transport cannot be opened or is closed
    /// before the connection is established.
    ///
    /// # Panics
    /// Does not panic.
    pub async fn connect(endpoint: CommandEndpoint) -> Result<Self, CommandError> {
        Self::connect_with_timeout(endpoint, DEFAULT_COMMAND_TIMEOUT).await
    }

    /// Connects to the endpoint and enforces a custom read timeout.
    ///
    /// # Parameters
    /// * `endpoint` - Transport descriptor (stdio, TCP, or Unix socket).
    /// * `timeout` - Maximum duration to wait for each response before failing.
    ///
    /// # Returns
    /// A connected [`CommandClient`] that enforces the provided timeout for every command.
    ///
    /// # Errors
    /// Returns [`CommandError`] if the underlying transport cannot be opened or the timeout
    /// elapses while establishing the connection.
    ///
    /// # Panics
    /// Does not panic.
    pub async fn connect_with_timeout(
        endpoint: CommandEndpoint,
        timeout: Duration,
    ) -> Result<Self, CommandError> {
        let (writer, reader) = match &endpoint {
            CommandEndpoint::Stdio => (
                CommandWriter::Stdio(Mutex::new(tokio::io::stdout())),
                CommandReader::Stdio(Mutex::new(BufReader::new(tokio::io::stdin()))),
            ),
            CommandEndpoint::Tcp(addr) => {
                let stream = TcpStream::connect(addr).await?;
                let (read_half, write_half) = stream.into_split();
                (
                    CommandWriter::Tcp(Mutex::new(write_half)),
                    CommandReader::Tcp(Mutex::new(BufReader::new(read_half))),
                )
            }
            #[cfg(unix)]
            CommandEndpoint::UnixSocket(path) => {
                let stream = UnixStream::connect(path).await?;
                let (read_half, write_half) = stream.into_split();
                (
                    CommandWriter::Unix(Mutex::new(write_half)),
                    CommandReader::Unix(Mutex::new(BufReader::new(read_half))),
                )
            }
            CommandEndpoint::Unavailable => {
                return Err(CommandError::Unavailable(
                    "command endpoint marked unavailable".into(),
                ));
            }
        };

        Ok(Self {
            inner: Arc::new(CommandClientInner {
                endpoint,
                writer,
                reader,
                timeout,
            }),
        })
    }

    /// Creates a [`CommandClient`] that always reports an unavailable channel.
    ///
    /// This is useful for runtimes (Google Cloud Run, local testing, etc.) that do not expose
    /// a host-managed command bus but still want to share the API surface.
    pub fn unavailable(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        let shared = Arc::new(reason);
        Self {
            inner: Arc::new(CommandClientInner {
                endpoint: CommandEndpoint::Unavailable,
                writer: CommandWriter::Unavailable(shared.clone()),
                reader: CommandReader::Unavailable(shared),
                timeout: DEFAULT_COMMAND_TIMEOUT,
            }),
        }
    }

    /// Returns the endpoint backing this client.
    pub fn endpoint(&self) -> &CommandEndpoint {
        &self.inner.endpoint
    }

    /// Sends a command request and waits for a response (or timeout).
    ///
    /// # Parameters
    /// * `request` - Structured command for the Workers sidecar.
    ///
    /// # Returns
    /// The [`CommandResponse`] emitted by the sidecar.
    ///
    /// # Errors
    /// Returns [`CommandError`] if the channel closes, the response payload cannot be
    /// deserialized, the command reports a failure, or the read timeout elapses.
    ///
    /// # Panics
    /// Does not panic.
    pub async fn send(&self, request: CommandRequest) -> Result<CommandResponse, CommandError> {
        self.inner.writer.send(&request).await?;

        let response = time::timeout(self.inner.timeout, self.inner.reader.read()).await;
        let response = match response {
            Ok(result) => result?,
            Err(_) => return Err(CommandError::Timeout(self.inner.timeout)),
        };

        if response.ok {
            Ok(response)
        } else {
            let diagnostic = response
                .diagnostic
                .clone()
                .unwrap_or_else(|| "host returned failure".to_owned());
            Err(CommandError::CommandFailure {
                diagnostic,
                payload: response.payload.clone(),
            })
        }
    }
}

/// JSON payload describing a command issued to the host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRequest {
    /// Command verb recognized by the Workers sidecar.
    pub command: String,
    /// Structured JSON payload to accompany the command (defaults to `null`).
    #[serde(default)]
    pub payload: serde_json::Value,
}

impl CommandRequest {
    /// Creates a new request with the provided command name and payload.
    pub fn new(command: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            command: command.into(),
            payload,
        }
    }

    /// Creates a request whose payload is `null`.
    pub fn empty(command: impl Into<String>) -> Self {
        Self::new(command, serde_json::Value::Null)
    }
}

/// Response returned by the host for a previously issued command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResponse {
    /// Indicates whether the host executed the command successfully.
    pub ok: bool,
    /// JSON payload returned by the host (defaults to `null`).
    #[serde(default)]
    pub payload: serde_json::Value,
    /// Optional diagnostic string supplied by the host when `ok == false`.
    #[serde(default)]
    pub diagnostic: Option<String>,
}

impl CommandResponse {
    /// Constructs a success response with an empty payload.
    pub fn ok() -> Self {
        Self {
            ok: true,
            payload: serde_json::Value::Null,
            diagnostic: None,
        }
    }
}
/// Errors emitted by [`CommandClient`] when transport or payload handling fails.
#[derive(Debug, Error)]
pub enum CommandError {
    #[error("command failed: {diagnostic}")]
    CommandFailure { diagnostic: String, payload: Value },
    #[error("command transport closed")]
    TransportClosed,
    #[error("command timed out after {0:?}")]
    Timeout(Duration),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid command payload: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("command channel unavailable: {0}")]
    Unavailable(String),
}

#[derive(Debug)]
enum CommandWriter {
    Stdio(Mutex<tokio::io::Stdout>),
    Tcp(Mutex<TcpOwnedWriteHalf>),
    #[cfg(unix)]
    Unix(Mutex<UnixOwnedWriteHalf>),
    Unavailable(Arc<String>),
}

#[derive(Debug)]
enum CommandReader {
    Stdio(Mutex<BufReader<tokio::io::Stdin>>),
    Tcp(Mutex<BufReader<TcpOwnedReadHalf>>),
    #[cfg(unix)]
    Unix(Mutex<BufReader<UnixOwnedReadHalf>>),
    Unavailable(Arc<String>),
}

impl CommandWriter {
    async fn send(&self, request: &CommandRequest) -> Result<(), CommandError> {
        let line = serde_json::to_string(request)?;
        match self {
            CommandWriter::Stdio(writer) => Self::write_line(writer, &line).await,
            CommandWriter::Tcp(writer) => Self::write_line(writer, &line).await,
            #[cfg(unix)]
            CommandWriter::Unix(writer) => Self::write_line(writer, &line).await,
            CommandWriter::Unavailable(reason) => {
                Err(CommandError::Unavailable(reason.as_ref().clone()))
            }
        }
    }

    async fn write_line<W>(writer: &Mutex<W>, line: &str) -> Result<(), CommandError>
    where
        W: AsyncWrite + Unpin + Send,
    {
        let mut guard = writer.lock().await;
        guard.write_all(line.as_bytes()).await?;
        guard.write_all(b"\n").await?;
        guard.flush().await?;
        Ok(())
    }
}

impl CommandReader {
    async fn read(&self) -> Result<CommandResponse, CommandError> {
        match self {
            CommandReader::Stdio(reader) => Self::read_line(reader).await,
            CommandReader::Tcp(reader) => Self::read_line(reader).await,
            #[cfg(unix)]
            CommandReader::Unix(reader) => Self::read_line(reader).await,
            CommandReader::Unavailable(reason) => {
                Err(CommandError::Unavailable(reason.as_ref().clone()))
            }
        }
    }

    async fn read_line<R>(reader: &Mutex<BufReader<R>>) -> Result<CommandResponse, CommandError>
    where
        R: AsyncRead + Unpin + Send,
    {
        let mut guard = reader.lock().await;
        let mut buf = String::new();
        let read = guard.read_line(&mut buf).await?;
        if read == 0 {
            return Err(CommandError::TransportClosed);
        }
        let response = serde_json::from_str(&buf)?;
        Ok(response)
    }
}
