use std::env;

/// Describes the runtime platform the container is executing inside.
#[derive(Clone, Debug)]
pub enum RuntimePlatform {
    Cloudflare(CloudflarePlatform),
    CloudRun(CloudRunPlatform),
    Generic,
}

impl Default for RuntimePlatform {
    fn default() -> Self {
        Self::Cloudflare(CloudflarePlatform::default())
    }
}

impl RuntimePlatform {
    /// Attempts to infer the current platform from environment variables that Cloudflare or
    /// Google Cloud Run automatically inject.
    pub fn detect() -> Self {
        if let Some(platform) = CloudflarePlatform::from_env() {
            return Self::Cloudflare(platform);
        }

        if let Some(platform) = CloudRunPlatform::from_env() {
            return Self::CloudRun(platform);
        }

        Self::Generic
    }

    /// Returns the Cloudflare platform details when active.
    pub fn as_cloudflare(&self) -> Option<&CloudflarePlatform> {
        match self {
            RuntimePlatform::Cloudflare(platform) => Some(platform),
            _ => None,
        }
    }

    /// Returns the Cloud Run platform details when active.
    pub fn as_cloud_run(&self) -> Option<&CloudRunPlatform> {
        match self {
            RuntimePlatform::CloudRun(platform) => Some(platform),
            _ => None,
        }
    }

    /// Indicates whether the runtime is executing inside Cloudflare Containers.
    pub fn is_cloudflare(&self) -> bool {
        matches!(self, RuntimePlatform::Cloudflare(_))
    }

    /// Indicates whether the runtime is executing inside Google Cloud Run.
    pub fn is_cloud_run(&self) -> bool {
        matches!(self, RuntimePlatform::CloudRun(_))
    }
}

/// Cloudflare-specific platform configuration gleaned from environment variables.
#[derive(Clone, Debug, Default)]
pub struct CloudflarePlatform {
    pub worker_name: Option<String>,
}

impl CloudflarePlatform {
    fn from_env() -> Option<Self> {
        let worker_name = env::var("CONTAINERFLARE_WORKER").ok();
        let has_cf_env = worker_name.is_some()
            || env::var("CF_CONTAINER_PORT").is_ok()
            || env::var("CF_CONTAINER_ADDR").is_ok()
            || env::var("CF_CMD_ENDPOINT").is_ok();

        if has_cf_env {
            Some(Self { worker_name })
        } else {
            None
        }
    }
}

/// Google Cloud Run platform configuration.
#[derive(Clone, Debug, Default)]
pub struct CloudRunPlatform {
    pub service: Option<String>,
    pub revision: Option<String>,
    pub configuration: Option<String>,
    pub project_id: Option<String>,
    pub region: Option<String>,
}

impl CloudRunPlatform {
    fn from_env() -> Option<Self> {
        let service = env::var("K_SERVICE").ok();
        let revision = env::var("K_REVISION").ok();
        let configuration = env::var("K_CONFIGURATION").ok();
        let project_id = env::var("GOOGLE_CLOUD_PROJECT")
            .ok()
            .or_else(|| env::var("GCLOUD_PROJECT").ok());
        let region = env::var("GOOGLE_CLOUD_REGION")
            .ok()
            .or_else(|| env::var("REGION").ok());

        let has_run_env = service.is_some()
            || revision.is_some()
            || env::var("PORT").is_ok()
            || project_id.is_some();

        if has_run_env {
            Some(Self {
                service,
                revision,
                configuration,
                project_id,
                region,
            })
        } else {
            None
        }
    }
}
