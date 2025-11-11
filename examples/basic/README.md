# Containerflare Basic Example

This example is a standalone Cargo crate that depends on the root `containerflare` crate via a path dependency.

## Run via Cargo (no container)
```bash
cargo run -p containerflare-basic-example
```

## Build & Run Locally
```bash
docker build --platform=linux/amd64 -f examples/basic/Dockerfile -t containerflare-basic .
# run the amd64 image under qemu (if host != amd64)
docker run --rm --platform=linux/amd64 -p 8787:8787 containerflare-basic
curl http://127.0.0.1:8787/
```

`containerflare` binds to `0.0.0.0:8787` by default, so Cloudflare's sidecar (and your local Docker host) can reach the Axum listener without any extra env vars. Set `CF_CONTAINER_ADDR` or `CF_CONTAINER_PORT` if you need a custom binding.

## Deploy to Cloudflare Containers
1. Install the Worker adapter dependencies (first run only):
   ```bash
   cd examples/basic
   npm install
   ```
2. Authenticate Wrangler (only needed once per machine/account):
   ```bash
   npx wrangler login           # or export CLOUDFLARE_API_TOKEN=...
   ```
3. Deploy via Wrangler (Docker must be running). The config sets `image_build_context = "../.."` so the full workspace is available to the Docker build:
   ```bash
   npx wrangler deploy
   ```

`wrangler deploy` builds the Docker image, pushes it to Cloudflare's registry, and deploys the Worker/Durable Object that proxies requests into the Axum server running inside the container.
