use axum::{Json, Router, routing::get};
use containerflare::{ContainerContext, RequestMetadata, RuntimePlatform, run};
use serde::Serialize;

#[tokio::main]
async fn main() -> containerflare::Result<()> {
    tracing_subscriber::fmt::init();

    let router = Router::new()
        .route("/", get(metadata))
        .route("/platform", get(platform));

    run(router).await
}

async fn metadata(ctx: ContainerContext) -> Json<RequestMetadata> {
    Json(ctx.metadata().clone())
}

async fn platform(ctx: ContainerContext) -> Json<PlatformSnapshot> {
    let trace = ctx
        .metadata()
        .trace_context
        .as_ref()
        .map(|context| TraceSummary {
            trace_id: context.trace_id.clone(),
            span_id: context.span_id.clone(),
            sampled: context.sampled,
        });

    let snapshot = match ctx.platform() {
        RuntimePlatform::CloudRun(run) => PlatformSnapshot {
            platform: "cloud_run",
            worker_name: None,
            service: run.service.clone(),
            revision: run.revision.clone(),
            configuration: run.configuration.clone(),
            project_id: run.project_id.clone(),
            region: run.region.clone(),
            trace,
        },
        RuntimePlatform::Cloudflare(cf) => PlatformSnapshot {
            platform: "cloudflare",
            worker_name: cf.worker_name.clone(),
            service: None,
            revision: None,
            configuration: None,
            project_id: None,
            region: None,
            trace,
        },
        RuntimePlatform::Generic => PlatformSnapshot {
            platform: "generic",
            worker_name: None,
            service: None,
            revision: None,
            configuration: None,
            project_id: None,
            region: None,
            trace,
        },
    };

    Json(snapshot)
}

#[derive(Serialize)]
struct PlatformSnapshot {
    platform: &'static str,
    worker_name: Option<String>,
    service: Option<String>,
    revision: Option<String>,
    configuration: Option<String>,
    project_id: Option<String>,
    region: Option<String>,
    trace: Option<TraceSummary>,
}

#[derive(Serialize)]
struct TraceSummary {
    trace_id: Option<String>,
    span_id: Option<String>,
    sampled: Option<bool>,
}
