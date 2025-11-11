# Containerflare Basic Example

This example is a standalone Cargo crate that depends on the root `containerflare` crate via a path dependency.

## Run via Cargo (no container)
```bash
cargo run -p containerflare-basic-example
```

## Build & Run Locally
```bash
docker build --platform=linux/amd64 -f examples/basic/Dockerfile -t containerflare-basic .
# run the amd64 image under qemu (if host != amd64) and expose Axum on all interfaces
docker run --rm --platform=linux/amd64 -e CF_CONTAINER_ADDR=0.0.0.0 -p 8787:8787 containerflare-basic
curl http://127.0.0.1:8787/
```

## Deploy to Cloudflare Containers
1. Install the Worker adapter dependencies (first run only):
   ```bash
   cd examples/basic
   npm install
   ```
2. Deploy via Wrangler (Docker must be running):
   ```bash
   npx wrangler deploy
   ```

`wrangler deploy` builds the Docker image, pushes it to Cloudflare's registry, and deploys the Worker/Durable Object that proxies requests into the Axum server running inside the container.
