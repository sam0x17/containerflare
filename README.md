# containerflare

[![Crates.io](https://img.shields.io/crates/v/containerflare.svg)](https://crates.io/crates/containerflare)
[![Docs](https://docs.rs/containerflare/badge.svg)](https://docs.rs/containerflare)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

`containerflare` lets you run Axum inside [Cloudflare Containers](https://developers.cloudflare.com/containers) without re-implementing the platform glue. It exposes a tiny runtime that:

- boots an Axum router on the container’s loopback listener
- forwards Cloudflare request metadata into your handlers
- keeps a command channel open so you can reach host-managed capabilities (KV, D1, Queues,
  etc.)

The result feels like developing any other Axum app—only now it runs next to your Worker.

## Highlights

- **Axum-first runtime** – bring your own router, tower layers, extractors, etc.
- **Cloudflare metadata bridge** – request ID, colo/region/country, client IP, worker name, and
  URLs are injected via `ContainerContext`.
- **Command channel client** – talk JSON-over-STDIO (default), TCP, or Unix sockets to the host
  when Cloudflare publishes more capabilities.
- **Production-ready example** – `examples/basic` demonstrates a full Worker + Durable Object +
  container deployment using Wrangler v4.

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

- `ContainerContext` is injected via Axum’s extractor system.
- `RequestMetadata` contains everything Cloudflare knows about the request (worker name, colo,
  region, `cf-ray`, client IP, method/path/url, etc.).
- `ContainerContext::command_client()` provides the low-level JSON command channel; call
  `invoke` whenever Cloudflare documents a capability.

Run the binary inside your container image. Cloudflare will proxy HTTP traffic from the
Worker/Durable Object to the listener bound by `containerflare` (defaults to `0.0.0.0:8787`).
Override `CF_CONTAINER_ADDR`/`CF_CONTAINER_PORT` if you need something else locally. Use
`CF_CMD_ENDPOINT` when pointing the command client at a TCP or Unix socket shim.

## Running locally

```bash
# build and run the example container (amd64)
docker build --platform=linux/amd64 -f examples/basic/Dockerfile -t containerflare-basic .
docker run --rm --platform=linux/amd64 -p 8787:8787 containerflare-basic

# curl echoes the RequestMetadata JSON – easy proof the bridge works
curl http://127.0.0.1:8787/
```

## Deploying to Cloudflare Containers

```bash
cd examples/basic
npm install                         # installs Wrangler v4 and @cloudflare/containers
npx wrangler login                  # or export CLOUDFLARE_API_TOKEN=...
npx wrangler deploy                 # builds + pushes the Docker image and Worker
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

## Example project

`examples/basic` is a real Cargo crate that depends on `containerflare` via `path = "../.."`.
It ships with:

- a Dockerfile that builds for `x86_64-unknown-linux-musl`
- a Worker/Durable Object that forwards metadata and proxies requests
- deployment scripts and docs for Wrangler v4

Use it as a template for your own containerized Workers.

## Platform expectations

- Cloudflare currently expects Containers to be built for the `linux/amd64` architecture, so we
  target `x86_64-unknown-linux-musl` by default. You could just as easily use a debian/ubuntu
  based image, however alpine/musl is great for small container sizes
- The runtime binds to `0.0.0.0:8787` so the Cloudflare sidecar (which connects from
  `10.0.0.1`) can reach your Axum listener. Override `CF_CONTAINER_ADDR` / `CF_CONTAINER_PORT`
  for custom setups.
- The `CommandClient` speaks JSON-over-STDIO for now. When Cloudflare documents additional
  transports we can add typed helpers on top of it.

Contributions are welcome—file issues or PRs with ideas!
