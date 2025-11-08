# containerflare

Rust runtime helpers for building Cloudflare Containers Workers with Axum. The crate wires up an Axum router inside the Containers runtime, injects Cloudflare request metadata, and provides a command channel stub for reaching host-managed capabilities (KV, D1, queues, etc.).

## Status
- Architecture + scaffolding in place (`RuntimeConfig`, `ContainerContext`, `ContainerflareRuntime`).
- Command channel is a stub that logs attempts; future work will add the JSON-over-STDIO transport Cloudflare exposes.

See `ARCHITECTURE.md` for the full iteration plan.

## Target triple & container expectations
Cloudflare’s official Containers docs state that “containers should be built for the `linux/amd64` architecture” (`cloudflare-docs/src/content/docs/containers/platform-details/architecture.mdx:79`).

To match that requirement we build binaries for `x86_64-unknown-linux-musl` and ship them in an Alpine-based OCI image. That keeps the runtime statically linked, easy to distribute, and compatible with Cloudflare’s VM isolation model. A typical build command:

```bash
cargo build --release --target x86_64-unknown-linux-musl
```

If you need glibc, you can swap Alpine for a Debian/Ubuntu base image and target `x86_64-unknown-linux-gnu`, but the runtime must still be packaged as an amd64 container image.

## Quick start
```rust
use axum::{routing::get, Router};
use containerflare::{serve, RuntimeConfig};

#[tokio::main]
async fn main() -> containerflare::Result<()> {
    let router = Router::new().route("/", get(|| async { "ok" }));
    serve(router, RuntimeConfig::from_env()?).await
}
```

Run the binary inside your container image. Cloudflare will proxy requests from the worker/DO into the Axum listener bound by `containerflare` (defaults to `127.0.0.1:8787`).
