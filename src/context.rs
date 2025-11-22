use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use thiserror::Error;

use containerflare_command::{CommandClient, CommandError, CommandRequest, CommandResponse};

use crate::platform::{CloudRunPlatform, CloudflarePlatform, RuntimePlatform};

/// Header set by the Worker shim that carries Cloudflare-specific request metadata.
const METADATA_HEADER: &str = "x-containerflare-metadata";

/// Request-scoped handle that exposes platform-specific request metadata plus the host command
/// client.
#[derive(Clone, Debug)]
pub struct ContainerContext {
    metadata: RequestMetadata,
    command_client: CommandClient,
    platform: RuntimePlatform,
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

    /// Returns the runtime platform detected from the environment.
    pub fn platform(&self) -> &RuntimePlatform {
        &self.platform
    }

    /// Issues an IPC command over the host-managed channel.
    pub async fn invoke(&self, request: CommandRequest) -> Result<CommandResponse, CommandError> {
        self.command_client.send(request).await
    }
}

/// Cloudflare metadata forwarded by the Worker shim plus additional Cloud Run details inferred
/// from headers and environment variables.
///
/// For Cloudflare Containers this mirrors the fields documented in Cloudflare's `cf` object:
/// <https://developers.cloudflare.com/workers/runtime-apis/request/#incomingrequestcfproperties>.
/// When running on Google Cloud Run the `cloud_run_*`, `project_id`, and `trace_context` fields
/// are populated automatically from the platform metadata and `x-cloud-trace-context` header.
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
    pub project_id: Option<String>,
    pub cloud_run_service: Option<String>,
    pub cloud_run_revision: Option<String>,
    pub cloud_run_configuration: Option<String>,
    pub cloud_run_region: Option<String>,
    pub trace_context: Option<TraceContext>,
    pub forwarded_for: Vec<String>,
    pub forwarded_proto: Option<String>,
    pub forwarded: Option<String>,
    pub user_agent: Option<String>,
    pub accept: Option<String>,
    pub accept_language: Option<String>,
    pub accept_encoding: Option<String>,
    pub sec_gpc: Option<String>,
    pub client_hints: Option<ClientHints>,
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
            project_id: None,
            cloud_run_service: None,
            cloud_run_revision: None,
            cloud_run_configuration: None,
            cloud_run_region: None,
            trace_context: None,
            forwarded_for: Vec::new(),
            forwarded_proto: None,
            forwarded: None,
            user_agent: None,
            accept: None,
            accept_language: None,
            accept_encoding: None,
            sec_gpc: None,
            client_hints: None,
            method: "GET".to_owned(),
            path: "/".to_owned(),
            raw_url: None,
        }
    }
}

impl RequestMetadata {
    /// Builds metadata from either the shim header or fallbacks for local testing.
    fn from_parts(parts: &Parts, platform: &RuntimePlatform) -> Self {
        if let Some(metadata) = Self::from_metadata_header(parts) {
            return metadata;
        }

        let mut metadata = Self::from_headers(parts);

        if let Some(cf) = platform.as_cloudflare() {
            metadata.apply_cloudflare_defaults(cf);
        }

        if let Some(run) = platform.as_cloud_run() {
            metadata.apply_cloud_run_defaults(parts, run);
        }

        metadata
    }

    fn from_metadata_header(parts: &Parts) -> Option<Self> {
        let header = parts.headers.get(METADATA_HEADER)?;
        let raw = header.to_str().ok()?;
        serde_json::from_str(raw).ok()
    }

    fn from_headers(parts: &Parts) -> Self {
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
            .and_then(|value| value.to_str().ok().map(|v| v.to_owned()))
            .or_else(|| pick_client_ip_from_xff(headers));
        let host = headers
            .get("x-forwarded-host")
            .or_else(|| headers.get(axum::http::header::HOST))
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_owned());

        let method = parts.method.to_string();
        let path_and_query = parts.uri.path_and_query().map(|pq| pq.as_str().to_owned());
        let path = path_and_query
            .clone()
            .unwrap_or_else(|| parts.uri.path().to_owned());
        let raw_url = Some(parts.uri.to_string()).filter(|value| !value.is_empty());
        let forwarded_proto = header_to_string(headers.get("x-forwarded-proto"));
        let scheme = forwarded_proto
            .clone()
            .or_else(|| parts.uri.scheme_str().map(|value| value.to_owned()));
        let forwarded = header_to_string(headers.get("forwarded"));
        let forwarded_for = header_to_string(headers.get("x-forwarded-for"))
            .map(|value| {
                value
                    .split(',')
                    .map(|v| v.trim().to_owned())
                    .filter(|v| !v.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let user_agent = header_to_string(headers.get(axum::http::header::USER_AGENT));
        let accept = header_to_string(headers.get(axum::http::header::ACCEPT));
        let accept_language = header_to_string(headers.get(axum::http::header::ACCEPT_LANGUAGE));
        let accept_encoding = header_to_string(headers.get(axum::http::header::ACCEPT_ENCODING));
        let sec_gpc = header_to_string(headers.get("sec-gpc"));
        let client_hints = ClientHints::from_headers(headers);

        Self {
            request_id,
            colo,
            region,
            country,
            client_ip,
            host,
            scheme,
            worker_name: None,
            project_id: None,
            cloud_run_service: None,
            cloud_run_revision: None,
            cloud_run_configuration: None,
            cloud_run_region: None,
            trace_context: None,
            forwarded_for,
            forwarded_proto,
            forwarded,
            user_agent,
            accept,
            accept_language,
            accept_encoding,
            sec_gpc,
            client_hints,
            method,
            path,
            raw_url,
        }
    }

    fn apply_cloudflare_defaults(&mut self, platform: &CloudflarePlatform) {
        if self.worker_name.is_none() {
            self.worker_name = platform.worker_name.clone();
        }
    }

    fn apply_cloud_run_defaults(&mut self, parts: &Parts, platform: &CloudRunPlatform) {
        if self.cloud_run_service.is_none() {
            self.cloud_run_service = platform.service.clone();
        }
        if self.cloud_run_revision.is_none() {
            self.cloud_run_revision = platform.revision.clone();
        }
        if self.cloud_run_configuration.is_none() {
            self.cloud_run_configuration = platform.configuration.clone();
        }
        if self.project_id.is_none() {
            self.project_id = platform.project_id.clone();
        }
        if self.cloud_run_region.is_none() {
            self.cloud_run_region = platform.region.clone();
        }

        if self.cloud_run_region.is_none() {
            self.cloud_run_region = self
                .host
                .as_ref()
                .and_then(|host| extract_region_from_host(host));
        }

        if self.project_id.is_none() {
            self.project_id = platform
                .project_id
                .clone()
                .or_else(|| std::env::var("GOOGLE_CLOUD_PROJECT").ok())
                .or_else(|| std::env::var("GCLOUD_PROJECT").ok())
                .or_else(|| {
                    self.host
                        .as_ref()
                        .and_then(|host| extract_project_from_host(host))
                });
        }

        if self.region.is_none() {
            self.region = self.cloud_run_region.clone();
        }

        if self.worker_name.is_none() {
            self.worker_name = self.cloud_run_service.clone();
        }

        if let Some(value) = parts
            .headers
            .get("x-cloud-trace-context")
            .and_then(|header| header.to_str().ok())
        {
            let trace =
                TraceContext::from_cloud_trace_header(value, platform.project_id.as_deref());
            if self.request_id.is_none() {
                self.request_id = trace.trace_id.clone();
            }
            self.trace_context = Some(trace);
        }
    }

    /// Attempts to rebuild the raw URL using scheme + host + path when only a path was available.
    fn rebuild_raw_url_if_needed(&mut self) {
        let needs_rebuild = self
            .raw_url
            .as_ref()
            .map(|url| url.starts_with('/') || !url.contains("://"))
            .unwrap_or(true);

        if needs_rebuild
            && let (Some(host), Some(scheme)) = (self.host.as_ref(), self.scheme.as_ref()) {
                self.raw_url = Some(format!("{}://{}{}", scheme, host, self.path));
            }
    }
}

/// Google Cloud Trace context parsed from `x-cloud-trace-context` headers.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct TraceContext {
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub sampled: Option<bool>,
    pub project_id: Option<String>,
    pub raw: Option<String>,
}

impl TraceContext {
    fn from_cloud_trace_header(header: &str, project_id: Option<&str>) -> Self {
        let mut trace_id = None;
        let mut span_id = None;
        let mut sampled = None;

        let mut parts = header.split('/');
        if let Some(trace) = parts.next()
            && !trace.is_empty() {
                trace_id = Some(trace.to_owned());
            }

        if let Some(rest) = parts.next() {
            let mut rest_parts = rest.split(';');
            if let Some(span) = rest_parts.next()
                && !span.is_empty() {
                    span_id = Some(span.to_owned());
                }
            for section in rest_parts {
                if let Some(flag) = section
                    .strip_prefix('o')
                    .and_then(|value| value.strip_prefix('='))
                {
                    sampled = match flag.trim() {
                        "1" => Some(true),
                        "0" => Some(false),
                        _ => None,
                    };
                }
            }
        }

        Self {
            trace_id,
            span_id,
            sampled,
            project_id: project_id.map(|value| value.to_owned()),
            raw: Some(header.to_owned()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ClientHints {
    pub ua: Option<String>,
    pub ua_mobile: Option<String>,
    pub ua_platform: Option<String>,
    pub ua_arch: Option<String>,
    pub ua_platform_version: Option<String>,
    pub ua_model: Option<String>,
    pub ua_bitness: Option<String>,
    pub ua_wow64: Option<String>,
    pub ua_full_version_list: Option<String>,
}

impl ClientHints {
    fn from_headers(headers: &axum::http::HeaderMap) -> Option<Self> {
        let ua = header_to_string(headers.get("sec-ch-ua"));
        let ua_mobile = header_to_string(headers.get("sec-ch-ua-mobile"));
        let ua_platform = header_to_string(headers.get("sec-ch-ua-platform"));
        let ua_arch = header_to_string(headers.get("sec-ch-ua-arch"));
        let ua_platform_version = header_to_string(headers.get("sec-ch-ua-platform-version"));
        let ua_model = header_to_string(headers.get("sec-ch-ua-model"));
        let ua_bitness = header_to_string(headers.get("sec-ch-ua-bitness"));
        let ua_wow64 = header_to_string(headers.get("sec-ch-ua-wow64"));
        let ua_full_version_list = header_to_string(headers.get("sec-ch-ua-full-version-list"));

        if ua.is_none()
            && ua_mobile.is_none()
            && ua_platform.is_none()
            && ua_arch.is_none()
            && ua_platform_version.is_none()
            && ua_model.is_none()
            && ua_bitness.is_none()
            && ua_wow64.is_none()
            && ua_full_version_list.is_none()
        {
            None
        } else {
            Some(Self {
                ua,
                ua_mobile,
                ua_platform,
                ua_arch,
                ua_platform_version,
                ua_model,
                ua_bitness,
                ua_wow64,
                ua_full_version_list,
            })
        }
    }
}

fn header_to_string(value: Option<&axum::http::HeaderValue>) -> Option<String> {
    value.and_then(|v| v.to_str().ok().map(|s| s.to_owned()))
}

fn pick_client_ip_from_xff(headers: &axum::http::HeaderMap) -> Option<String> {
    let xff = headers.get("x-forwarded-for")?.to_str().ok()?;
    let mut first = None;
    for part in xff.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if first.is_none() {
            first = Some(part.to_owned());
        }
        if let Ok(ip) = part.parse::<IpAddr>()
            && is_public_ip(&ip) {
                return Some(part.to_owned());
            }
    }
    first
}

fn is_public_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast())
        }
        IpAddr::V6(v6) => {
            !(v6.is_loopback()
                || v6.is_multicast()
                || v6.is_unspecified()
                || v6.is_unique_local()
                || v6.is_unicast_link_local())
        }
    }
}

fn extract_region_from_host(host: &str) -> Option<String> {
    // Cloud Run hosts look like:
    // - <service>-<hash>-<region>.a.run.app  (legacy)
    // - <service>-<projectNumber>.<region>.run.app (modern)
    let labels: Vec<&str> = host.split('.').collect();

    // Prefer the label immediately before "run".
    let mut region_part: Option<&str> = None;
    for window in labels.windows(2) {
        if window[1] == "run" {
            region_part = Some(window[0]);
            break;
        }
    }

    // Fallback to the second label (<service>.<region>.run.app).
    if region_part.is_none() && labels.len() >= 3 {
        region_part = Some(labels[labels.len().saturating_sub(3)]);
    }

    let region = region_part?;
    if region.is_empty() {
        return None;
    }

    let mapped = match region {
        "uc" => "us-central1",
        "ue" => "us-east1",
        "uw1" => "us-west1",
        other => other,
    };

    Some(mapped.to_owned())
}

fn extract_project_from_host(host: &str) -> Option<String> {
    // Modern Cloud Run domains embed the project number in the first label:
    // <service>-<projectNumber>.<region>.run.app
    let first_label = host.split('.').next()?;
    let mut parts = first_label.rsplitn(2, '-');
    let numeric = parts.next()?;
    if !numeric.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(numeric.to_owned())
}

/// Errors emitted when a handler requests [`ContainerContext`] but extensions were not set up.
#[derive(Debug, Error)]
pub enum ContainerContextRejection {
    #[error("command client missing from request extensions")]
    MissingCommandClient,
    #[error("runtime platform missing from request extensions")]
    MissingRuntimePlatform,
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

        let platform = parts
            .extensions
            .get::<RuntimePlatform>()
            .cloned()
            .ok_or(ContainerContextRejection::MissingRuntimePlatform)?;

        let mut metadata = RequestMetadata::from_parts(parts, &platform);
        metadata.rebuild_raw_url_if_needed();

        Ok(Self {
            metadata,
            command_client,
            platform,
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
        let metadata = RequestMetadata::from_parts(&parts, &RuntimePlatform::default());

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
        let parsed = RequestMetadata::from_parts(&parts, &RuntimePlatform::default());

        assert_eq!(parsed.request_id, metadata.request_id);
        assert_eq!(parsed.colo, metadata.colo);
        assert_eq!(parsed.worker_name, metadata.worker_name);
        assert_eq!(parsed.path, metadata.path);
        assert_eq!(parsed.raw_url, metadata.raw_url);
    }

    #[test]
    fn cloud_run_metadata_from_headers() {
        let platform = RuntimePlatform::CloudRun(CloudRunPlatform {
            service: Some("svc".into()),
            revision: Some("rev".into()),
            configuration: Some("cfg".into()),
            project_id: Some("proj-123".into()),
            region: Some("us-central1".into()),
        });

        let request = Request::builder()
            .method("GET")
            .uri("http://127.0.0.1/hello")
            .header("x-forwarded-for", "198.51.100.1")
            .header("x-forwarded-host", "example.run.app")
            .header("x-forwarded-proto", "https")
            .header(
                "x-cloud-trace-context",
                "105445aa7843bc8bf206b120001000/123;o=1",
            )
            .header("user-agent", "test-agent")
            .header("accept-language", "en-US")
            .header("sec-ch-ua", "\"Chromium\";v=\"1\"")
            .body(())
            .unwrap();

        let (parts, _) = request.into_parts();
        let metadata = RequestMetadata::from_parts(&parts, &platform);

        assert_eq!(metadata.cloud_run_service.as_deref(), Some("svc"));
        assert_eq!(metadata.cloud_run_revision.as_deref(), Some("rev"));
        assert_eq!(metadata.cloud_run_configuration.as_deref(), Some("cfg"));
        assert_eq!(metadata.project_id.as_deref(), Some("proj-123"));
        assert_eq!(metadata.cloud_run_region.as_deref(), Some("us-central1"));
        assert_eq!(metadata.region.as_deref(), Some("us-central1"));
        assert_eq!(metadata.scheme.as_deref(), Some("https"));
        assert_eq!(metadata.host.as_deref(), Some("example.run.app"));
        assert_eq!(metadata.client_ip.as_deref(), Some("198.51.100.1"));
        assert_eq!(metadata.worker_name.as_deref(), Some("svc"));
        assert_eq!(metadata.user_agent.as_deref(), Some("test-agent"));
        assert_eq!(metadata.accept_language.as_deref(), Some("en-US"));
        assert!(metadata.client_hints.is_some());
        assert_eq!(
            metadata.request_id.as_deref(),
            Some("105445aa7843bc8bf206b120001000")
        );
        assert!(metadata.trace_context.is_some());
        assert_eq!(
            metadata.trace_context.as_ref().unwrap().span_id.as_deref(),
            Some("123")
        );
    }
}
