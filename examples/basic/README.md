# Containerflare Basic Example

This example is a standalone Cargo crate that depends on the root `containerflare` crate via a path dependency. It targets both Cloudflare Containers and Google Cloud Run with a single Dockerfile and two helper deploy scripts.

## Run via Cargo (no container)
```bash
cargo run -p containerflare-basic-example
```

## Build & Run Locally (Cloudflare-style container)
```bash
docker build --platform=linux/amd64 -f examples/basic/Dockerfile -t containerflare-basic .
# run the amd64 image under qemu (if host != amd64)
docker run --rm --platform=linux/amd64 -p 8787:8787 containerflare-basic
curl http://127.0.0.1:8787/      # returns the forwarded RequestMetadata JSON
```

`containerflare` binds to `0.0.0.0:8787` by default, so Cloudflare's sidecar (and your local Docker host) can reach the Axum listener without any extra env vars. Set `CF_CONTAINER_ADDR` or `CF_CONTAINER_PORT` if you need a custom binding. On Cloud Run, the runtime binds to the injected `PORT` automatically (the same Dockerfile is used for both).

## Deploy to Cloudflare Containers

From this directory:
```bash
./deploy_cloudflare.sh
```

`deploy_cloudflare.sh` runs `wrangler deploy` inside `examples/basic`, building/pushing the Docker
image and deploying the Worker/Durable Object that proxies requests into the Axum server.

## Test in Cloudflare
After deployment, Wrangler prints a `workers.dev` URL. Exercise the Worker/Container pair and watch logs:

```bash
# Fetch from the deployed Worker (replace with the URL wrangler printed)
curl https://containerflare-basic.<your-account>.workers.dev/metadata

# Stream Worker/Durable Object logs
npx wrangler tail containerflare-basic --format=pretty

# Inspect container rollout + runtime logs
npx wrangler containers list
npx wrangler containers logs --name containerflare-basic-containerflarebasic
```

When you're done, `npx wrangler deployments list` shows previous versions and `npx wrangler delete` tears everything down.

## Metadata From Workers
`containerflare` expects Cloudflare-specific request details in the `x-containerflare-metadata` header. The provided Worker populates it (see `worker/index.js`) with the request ID, colo/region/country, client IP, worker name (`CONTAINERFLARE_WORKER` from `wrangler.toml`), and the full URL/method before proxying the request into the container. Inside Rust handlers you can read those values via `ContainerContext::metadata()`. If you adjust the Worker, keep writing this header so your Axum code retains access to Cloudflare context.

## Deploy to Google Cloud Run

From this directory:
```bash
./deploy_cloudrun.sh
```

The script builds this example crate using `examples/basic/Dockerfile`, pushes it to
`gcr.io/<project>/<service>`, and deploys via `gcloud run deploy` using your gcloud defaults unless
overridden (`PROJECT_ID`, `REGION`, `SERVICE_NAME`, `TAG`, `RUST_LOG`). `PORT` is injected by Cloud
Run at runtime and the command channel remains disabled there (`CommandError::Unavailable`).
