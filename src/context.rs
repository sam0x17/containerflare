use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::command::{CommandClient, CommandError, CommandRequest, CommandResponse};

#[derive(Clone, Debug)]
pub struct ContainerContext {
    metadata: RequestMetadata,
    command_client: CommandClient,
}

impl ContainerContext {
    pub fn metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    pub fn command_client(&self) -> &CommandClient {
        &self.command_client
    }

    pub async fn invoke(&self, request: CommandRequest) -> Result<CommandResponse, CommandError> {
        self.command_client.send(request).await
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestMetadata {
    pub request_id: Option<String>,
    pub colo: Option<String>,
    pub region: Option<String>,
    pub method: String,
    pub path: String,
}

impl Default for RequestMetadata {
    fn default() -> Self {
        Self {
            request_id: None,
            colo: None,
            region: None,
            method: "GET".to_owned(),
            path: "/".to_owned(),
        }
    }
}

impl RequestMetadata {
    fn from_parts(parts: &Parts) -> Self {
        let headers = &parts.headers;
        let request_id = headers
            .get("cf-ray")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_owned());
        let colo = headers
            .get("cf-ipcountry")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_owned());
        let region = headers
            .get("cf-region")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_owned());

        let method = parts.method.to_string();
        let path = parts
            .uri
            .path_and_query()
            .map(|pq| pq.as_str().to_owned())
            .unwrap_or_else(|| parts.uri.path().to_owned());

        Self {
            request_id,
            colo,
            region,
            method,
            path,
        }
    }
}

#[derive(Debug, Error)]
pub enum ContainerContextRejection {
    #[error("command client missing from request extensions")]
    MissingCommandClient,
}

impl IntoResponse for ContainerContextRejection {
    fn into_response(self) -> Response {
        let status = StatusCode::INTERNAL_SERVER_ERROR;
        let message = self.to_string();
        (status, message).into_response()
    }
}

#[async_trait]
impl<S> FromRequestParts<S> for ContainerContext
where
    S: Send + Sync,
{
    type Rejection = ContainerContextRejection;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let command_client = parts
            .extensions
            .get::<CommandClient>()
            .cloned()
            .ok_or(ContainerContextRejection::MissingCommandClient)?;

        let metadata = RequestMetadata::from_parts(parts);

        Ok(Self {
            metadata,
            command_client,
        })
    }
}
