use axum::{Json, Router, routing::get};
use containerflare::{ContainerContext, RequestMetadata, run};

#[tokio::main]
async fn main() -> containerflare::Result<()> {
    let router = Router::new()
        .route("/", get(health))
        .route("/metadata", get(metadata));

    run(router).await
}

async fn health() -> &'static str {
    "ok"
}

async fn metadata(context: ContainerContext) -> Json<RequestMetadata> {
    Json(context.metadata().clone())
}
