use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::config::CommandEndpoint;

#[derive(Clone, Debug)]
pub struct CommandClient {
    inner: Arc<CommandClientInner>,
}

#[derive(Debug)]
struct CommandClientInner {
    endpoint: CommandEndpoint,
}

impl CommandClient {
    pub fn new(endpoint: CommandEndpoint) -> Self {
        Self {
            inner: Arc::new(CommandClientInner { endpoint }),
        }
    }

    pub fn endpoint(&self) -> &CommandEndpoint {
        &self.inner.endpoint
    }

    pub async fn send(&self, request: CommandRequest) -> Result<CommandResponse, CommandError> {
        tracing::warn!(
            command = %request.command,
            "command transport is not wired up yet; returning unsupported error"
        );

        Err(CommandError::TransportNotReady(format!(
            "transport {:?} is not implemented yet",
            self.inner.endpoint
        )))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRequest {
    pub command: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

impl CommandRequest {
    pub fn new(command: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            command: command.into(),
            payload,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResponse {
    pub ok: bool,
    #[serde(default)]
    pub payload: serde_json::Value,
    #[serde(default)]
    pub diagnostic: Option<String>,
}

impl CommandResponse {
    pub fn ok() -> Self {
        Self {
            ok: true,
            payload: serde_json::Value::Null,
            diagnostic: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum CommandError {
    #[error("command transport not available: {0}")]
    TransportNotReady(String),
    #[error("command failed: {0}")]
    CommandFailure(String),
}
