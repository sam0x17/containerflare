# containerflare

[![Crates.io](https://img.shields.io/crates/v/containerflare.svg)](https://crates.io/crates/containerflare)
[![Docs](https://docs.rs/containerflare/badge.svg)](https://docs.rs/containerflare)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

`containerflare` lets you run Axum inside [Cloudflare Containers](https://developers.cloudflare.com/containers) without re-implementing the platform glue, and now auto-detects when it is executing on [Google Cloud Run](https://cloud.google.com/run). It exposes a tiny runtime that:

- boots an Axum router on the container’s loopback listener (or Cloud Run’s `PORT` binding)
- forwards Cloudflare/Cloud Run request metadata into your handlers
- keeps a command channel open so you can reach host-managed capabilities (KV, D1, Queues,
  etc.) whenever the platform exposes one

The result feels like developing any other Axum app—only now it runs next to your Worker.

## Highlights

- **Axum-first runtime** – bring your own router, tower layers, extractors, etc.
- **Cloudflare metadata bridge** – request ID, colo/region/country, client IP, worker name, and
  URLs are injected via `ContainerContext`.
- **Cloud Run aware** – automatically binds to the Cloud Run `PORT`, populates service /
  revision / project / trace fields, and disables the host command channel when unavailable.
- **Command channel client** – talk JSON-over-STDIO (default), TCP, or Unix sockets to the host;
  the IPC layer now ships as the standalone `containerflare-command` crate for direct use.
- **Production-ready examples** – `examples/basic` demonstrates a full Cloudflare deployment
  (Worker + Durable Object + container). `examples/cloudrun` mirrors the same Axum app but targets
  Google Cloud Run with its own Dockerfile + deployment guide.

## Installation

```bash
cargo add containerflare
```

The crate targets Rust 1.90+ (edition 2024).

## Quick start

```rust
use axum::{routing::get, Json, Router};
use containerflare::{run, ContainerContext, RequestMetadata};

#[tokio::main]
async fn main() -> containerflare::Result<()> {
    let router = Router::new().route("/", get(metadata));
    run(router).await
}

async fn metadata(ctx: ContainerContext) -> Json<RequestMetadata> {
    Json(ctx.metadata().clone())
}
```

- `ContainerContext` is injected via Axum’s extractor system and surfaces
  `ContainerContext::platform()` so you can differentiate between Cloudflare and Cloud Run.
- `RequestMetadata` contains everything Cloudflare knows about the request (worker name, colo,
  region, `cf-ray`, client IP, method/path/url, etc.) plus Cloud Run service/revision/
  configuration/project information and the parsed `x-cloud-trace-context` header when present.
- `ContainerContext::command_client()` provides the low-level JSON command channel; call
  `invoke` whenever Cloudflare documents a capability. On Cloud Run the channel is disabled and
  the client reports `CommandError::Unavailable` so you can log or fall back gracefully.

Run the binary inside your container image. Cloudflare will proxy HTTP traffic from the
Worker/Durable Object to the listener bound by `containerflare` (defaults to `0.0.0.0:8787`).
Override `CF_CONTAINER_ADDR`/`CF_CONTAINER_PORT` if you need something else locally. On Cloud Run
the runtime automatically binds to the provided `PORT` (defaulting to 8080 when running the sample
Dockerfile locally). Use `CF_CMD_ENDPOINT` when pointing the command client at a TCP or Unix socket
shim.

## Standalone command crate

If you only need access to the host-managed command bus (KV, R2, Queues, etc.), depend on
[`containerflare-command`](https://crates.io/crates/containerflare-command) directly. Cloud Run
does not expose this bus so commands immediately return `CommandError::Unavailable`, but the same
API works on Cloudflare Containers:

```bash
cargo add containerflare-command
```

It exposes `CommandClient`, `CommandRequest`, `CommandResponse`, and the `CommandEndpoint`
parsers without pulling in the runtime/router pieces.

## Running locally

```bash
# build and run the example container (amd64)
docker build --platform=linux/amd64 -f examples/basic/Dockerfile -t containerflare-basic .
docker run --rm --platform=linux/amd64 -p 8787:8787 containerflare-basic

# curl echoes the RequestMetadata JSON – easy proof the bridge works
curl http://127.0.0.1:8787/
```

## Deploying to Cloudflare Containers

From `examples/basic`, run:

```bash
./deploy_cloudflare.sh     # runs wrangler deploy from examples/basic
```

The example’s `wrangler.toml` sets `image_build_context = "../.."`, so the Docker build sees
the entire workspace (the example crate depends on this repo via `path = "../.."`). After
deploy Wrangler prints a `workers.dev` URL that proxies into your container:

```bash
npx wrangler tail containerflare-basic --format=pretty
npx wrangler containers list
npx wrangler containers logs --name containerflare-basic-containerflarebasic
curl https://containerflare-basic.<your-account>.workers.dev/
```

## Deploying to Google Cloud Run

The same example crate can target Cloud Run. From `examples/basic`:

```bash
./deploy_cloudrun.sh       # builds with Dockerfile and runs gcloud run deploy
```

It uses your gcloud defaults for project/region unless overridden (`PROJECT_ID`, `REGION`,
`SERVICE_NAME`, `TAG`, `RUST_LOG`). When `containerflare` detects Cloud Run it binds to the
injected `PORT`, captures `K_SERVICE`/`K_REVISION`/`K_CONFIGURATION`/`GOOGLE_CLOUD_PROJECT`,
parses `x-cloud-trace-context`, and disables the host command channel. Handlers can inspect that
state via `ContainerContext::platform()` and the new Cloud Run fields on `RequestMetadata`.

When `containerflare` detects Cloud Run it binds to the injected `PORT`, captures
`K_SERVICE`/`K_REVISION`/`K_CONFIGURATION`/`GOOGLE_CLOUD_PROJECT`, parses
`x-cloud-trace-context`, and disables the host command channel. Handlers can inspect that state via
`ContainerContext::platform()` and the new Cloud Run fields on `RequestMetadata`.

## Metadata bridge

The Worker shim (see `examples/basic/worker/index.js`) adds an `x-containerflare-metadata`
header before proxying every request into the container. That JSON payload includes:

- request identifier (`cf-ray`)
- colo / region / country codes
- client IP
- worker name (derived from the `CONTAINERFLARE_WORKER` Wrangler variable)
- HTTP method, path, and full URL

On the Rust side you can read all of those fields via `ContainerContext::metadata()` (see
`RequestMetadata` in `src/context.rs`). If you customize the Worker, keep writing this header
so your Axum handlers continue to receive Cloudflare context.

On Cloud Run the runtime infers metadata directly from HTTP headers + environment variables. It
records the service, revision, configuration, project ID, region, trace/span IDs, and whether the
request is sampled based on the `x-cloud-trace-context` header. These new fields appear on
`RequestMetadata` alongside the existing Cloudflare values.

## Example project

`examples/basic` is a real Cargo crate that depends on `containerflare` via `path = "../.."`.
It ships with:

- a Dockerfile that builds for `x86_64-unknown-linux-musl`
- a Worker/Durable Object that forwards metadata and proxies requests
- deployment scripts and docs for Wrangler v4

Use it as a template for your own containerized Workers.

`examples/cloudrun` mirrors the same pattern but targets Google Cloud Run. It echoes both the
standard metadata and the detected platform (`ContainerContext::platform()`) so you can see exactly
which fields are available when running outside Cloudflare.

## Platform expectations

- Cloudflare currently expects Containers to be built for the `linux/amd64` architecture, so we
  target `x86_64-unknown-linux-musl` by default. You could just as easily use a debian/ubuntu
  based image, however alpine/musl is great for small container sizes.
- The runtime binds to `0.0.0.0:8787` so the Cloudflare sidecar (which connects from
  `10.0.0.1`) can reach your Axum listener. Override `CF_CONTAINER_ADDR` / `CF_CONTAINER_PORT`
  for custom setups. On Cloud Run the runtime binds to the injected `PORT` (usually 8080 when
  running locally).
- The `CommandClient` speaks JSON-over-STDIO for now. When Cloudflare documents additional
  transports we can add typed helpers on top of it. Cloud Run disables the channel, so the client
  immediately returns `CommandError::Unavailable`.

Contributions are welcome—file issues or PRs with ideas!
