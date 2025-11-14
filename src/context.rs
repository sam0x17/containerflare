use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use containerflare_command::{CommandClient, CommandError, CommandRequest, CommandResponse};

/// Header set by the Worker shim that carries Cloudflare-specific request metadata.
const METADATA_HEADER: &str = "x-containerflare-metadata";

/// Request-scoped handle that exposes Cloudflare request metadata plus the host command client.
#[derive(Clone, Debug)]
pub struct ContainerContext {
    metadata: RequestMetadata,
    command_client: CommandClient,
}

impl ContainerContext {
    /// Returns the request metadata parsed from Cloudflare headers.
    pub fn metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    /// Returns the low-level command client for host-managed capabilities.
    pub fn command_client(&self) -> &CommandClient {
        &self.command_client
    }

    /// Issues an IPC command over the host-managed channel.
    pub async fn invoke(&self, request: CommandRequest) -> Result<CommandResponse, CommandError> {
        self.command_client.send(request).await
    }
}

/// Cloudflare metadata forwarded by the Worker shim.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct RequestMetadata {
    pub request_id: Option<String>,
    pub colo: Option<String>,
    pub region: Option<String>,
    pub country: Option<String>,
    pub client_ip: Option<String>,
    pub host: Option<String>,
    pub scheme: Option<String>,
    pub worker_name: Option<String>,
    pub method: String,
    pub path: String,
    pub raw_url: Option<String>,
}

impl Default for RequestMetadata {
    fn default() -> Self {
        Self {
            request_id: None,
            colo: None,
            region: None,
            country: None,
            client_ip: None,
            host: None,
            scheme: None,
            worker_name: None,
            method: "GET".to_owned(),
            path: "/".to_owned(),
            raw_url: None,
        }
    }
}

impl RequestMetadata {
    /// Builds metadata from either the shim header or fallbacks for local testing.
    fn from_parts(parts: &Parts) -> Self {
        if let Some(metadata) = Self::from_metadata_header(parts) {
            return metadata;
        }

        let headers = &parts.headers;
        let request_id = headers
            .get("cf-ray")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_owned());
        let colo = headers
            .get("cf-colo")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_owned());
        let country = headers
            .get("cf-ipcountry")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_owned());
        let region = headers
            .get("cf-region")
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_owned());
        let client_ip = headers
            .get("cf-connecting-ip")
            .or_else(|| headers.get("x-forwarded-for"))
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_owned());
        let host = headers
            .get(axum::http::header::HOST)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_owned());

        let method = parts.method.to_string();
        let path_and_query = parts.uri.path_and_query().map(|pq| pq.as_str().to_owned());
        let path = path_and_query
            .clone()
            .unwrap_or_else(|| parts.uri.path().to_owned());
        let raw_url = Some(parts.uri.to_string()).filter(|value| !value.is_empty());
        let scheme = parts.uri.scheme_str().map(|value| value.to_owned());

        Self {
            request_id,
            colo,
            region,
            country,
            client_ip,
            host,
            scheme,
            worker_name: None,
            method,
            path,
            raw_url,
        }
    }

    fn from_metadata_header(parts: &Parts) -> Option<Self> {
        let header = parts.headers.get(METADATA_HEADER)?;
        let raw = header.to_str().ok()?;
        serde_json::from_str(raw).ok()
    }
}

/// Errors emitted when a handler requests [`ContainerContext`] but extensions were not set up.
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    #[test]
    fn metadata_defaults_to_headers() {
        let request = Request::builder()
            .method("GET")
            .uri("https://example.com/foo?bar=baz")
            .header("cf-ray", "ray123")
            .header("cf-colo", "iad")
            .header("cf-ipcountry", "US")
            .header("cf-region", "na")
            .header("cf-connecting-ip", "203.0.113.1")
            .body(())
            .unwrap();

        let (parts, _) = request.into_parts();
        let metadata = RequestMetadata::from_parts(&parts);

        assert_eq!(metadata.request_id.as_deref(), Some("ray123"));
        assert_eq!(metadata.colo.as_deref(), Some("iad"));
        assert_eq!(metadata.country.as_deref(), Some("US"));
        assert_eq!(metadata.region.as_deref(), Some("na"));
        assert_eq!(metadata.client_ip.as_deref(), Some("203.0.113.1"));
        assert_eq!(metadata.path, "/foo?bar=baz");
    }

    #[test]
    fn metadata_header_overrides_values() {
        let mut metadata = RequestMetadata::default();
        metadata.request_id = Some("abc".into());
        metadata.colo = Some("sfo".into());
        metadata.region = Some("us-west".into());
        metadata.country = Some("US".into());
        metadata.client_ip = Some("203.0.113.9".into());
        metadata.host = Some("example.com".into());
        metadata.scheme = Some("https".into());
        metadata.worker_name = Some("test-worker".into());
        metadata.method = "POST".into();
        metadata.path = "/foo?bar=baz".into();
        metadata.raw_url = Some("https://example.com/foo?bar=baz".into());

        let metadata_header = serde_json::to_string(&metadata).unwrap();
        let request = Request::builder()
            .method("POST")
            .uri("https://placeholder.invalid/")
            .header(METADATA_HEADER, metadata_header)
            .body(())
            .unwrap();

        let (parts, _) = request.into_parts();
        let parsed = RequestMetadata::from_parts(&parts);

        assert_eq!(parsed.request_id, metadata.request_id);
        assert_eq!(parsed.colo, metadata.colo);
        assert_eq!(parsed.worker_name, metadata.worker_name);
        assert_eq!(parsed.path, metadata.path);
        assert_eq!(parsed.raw_url, metadata.raw_url);
    }
}
