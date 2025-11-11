# containerflare

Rust runtime helpers for building Cloudflare Containers Workers with Axum. The crate wires up an Axum router inside the Containers runtime, injects Cloudflare request metadata, and provides a command channel stub for reaching host-managed capabilities (KV, D1, queues, etc.).

## Status
- Architecture + scaffolding in place (`RuntimeConfig`, `ContainerContext`, `ContainerflareRuntime`).
- `CommandClient` now supports JSON-over-STDIO (default) plus TCP/Unix sockets for local dev proxies, with configurable timeouts.

See `ARCHITECTURE.md` for the full iteration plan.

## Examples
`examples/basic` hosts a standalone Axum crate (it depends on this repo via `path = "../.."`) plus its container + Worker scaffolding. Building requires Rust 1.84+ (edition 2024).

Run it directly without containers:
```bash
cargo run -p containerflare-basic-example
```

Build the container locally:
```bash
docker build --platform=linux/amd64 -f examples/basic/Dockerfile .
```

Smoke test the image:
```bash
docker run --rm --platform=linux/amd64 -p 8787:8787 containerflare-basic
curl http://127.0.0.1:8787/           # entire RequestMetadata payload
```

Deploy via Cloudflare Containers by running `npm install` inside `examples/basic` (installs Wrangler v4) and executing `npx wrangler deploy` after logging in with `npx wrangler login` or setting `CLOUDFLARE_API_TOKEN`. The example’s `wrangler.toml` sets `image_build_context = "../.."` so Docker sees the whole workspace (see `examples/basic/README.md` for the full flow).

Verify the deployment from your machine:
```bash
# Tail Worker + Durable Object logs (Ctrl+C to stop)
npx wrangler tail containerflare-basic --format=pretty

# Inspect container rollout + health
npx wrangler containers list
npx wrangler containers logs --name containerflare-basic-containerflarebasic

# Hit the deployed Worker route printed by wrangler deploy
curl https://containerflare-basic.<your-account>.workers.dev/
```

## Metadata bridge
The Worker shim adds an `x-containerflare-metadata` header before proxying each request into the container. That JSON payload contains the request ID (`cf-ray`), colo/region, country, client IP, worker name (set via the `CONTAINERFLARE_WORKER` var in `wrangler.toml`), and the full URL/method. On the Rust side you can read those fields via `ContainerContext::metadata()` (see `RequestMetadata` in `src/context.rs`). If you customize the Worker, keep forwarding that header so handlers continue to receive Cloudflare-specific context.

## Target triple & container expectations
Cloudflare’s official Containers docs state that “containers should be built for the `linux/amd64` architecture” (`cloudflare-docs/src/content/docs/containers/platform-details/architecture.mdx:79`).

To match that requirement we build binaries for `x86_64-unknown-linux-musl` and ship them in an Alpine-based OCI image. That keeps the runtime statically linked, easy to distribute, and compatible with Cloudflare’s VM isolation model. A typical build command:

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

The runtime now binds to `0.0.0.0:8787` by default so Cloudflare’s sidecar (which connects from `10.0.0.1`) can reach the Axum server without extra configuration. Override `CF_CONTAINER_ADDR`/`CF_CONTAINER_PORT` if you need a different address or port locally.

If you need glibc, you can swap Alpine for a Debian/Ubuntu base image and target `x86_64-unknown-linux-gnu`, but the runtime must still be packaged as an amd64 container image.

## Quick start
```rust
use axum::{routing::get, Router};
use containerflare::run;

#[tokio::main]
async fn main() -> containerflare::Result<()> {
    let router = Router::new().route("/", get(|| async { "ok" }));
    run(router).await
}
```

Run the binary inside your container image. Cloudflare will proxy requests from the worker/DO into the Axum listener bound by `containerflare` (defaults to `0.0.0.0:8787`). Use `CF_CMD_ENDPOINT` to point the command client at a different IPC channel (for example `tcp://127.0.0.1:9000` when testing against a local shim).
