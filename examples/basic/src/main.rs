use axum::{Json, Router, routing::get};
use containerflare::{ContainerContext, RequestMetadata, run};

#[tokio::main]
async fn main() -> containerflare::Result<()> {
    let router = Router::new().route("/", get(metadata));

    run(router).await
}

async fn metadata(context: ContainerContext) -> Json<(&'static str, RequestMetadata)> {
    Json(("it works!", context.metadata().clone()))
}
