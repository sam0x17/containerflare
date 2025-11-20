# Containerflare Cloud Run Example

This example demonstrates running the `containerflare` runtime on **Google Cloud Run**. It exposes
`/` to echo the enriched `RequestMetadata` (including Cloud Run fields) and `/platform` to show the
detected platform + service metadata.

## Run locally with Cargo

```bash
cargo run -p containerflare-cloudrun-example
curl http://127.0.0.1:8787/
```

Outside of Cloud Run the runtime defaults to the Cloudflare-style `0.0.0.0:8787` binding. The
extended metadata is still available, but Cloud Run specific fields are `null`.

## Container image

```bash
docker build --platform=linux/amd64 -f examples/cloudrun/Dockerfile -t containerflare-cloudrun .
PORT=8080 docker run --rm -p 8080:8080 -e PORT=8080 containerflare-cloudrun
curl http://127.0.0.1:8080/platform
```

Setting `PORT` mirrors the Cloud Run runtime contract and exercises the automatic Cloud Run
platform detection.

## Deploy to Google Cloud Run

```bash
PROJECT_ID="$(gcloud config get-value project)"
REGION="us-central1"
IMAGE="gcr.io/${PROJECT_ID}/containerflare-cloudrun"
SERVICE="containerflare-cloudrun"

docker build --platform=linux/amd64 -f examples/cloudrun/Dockerfile -t "${IMAGE}" .
docker push "${IMAGE}"

gcloud run deploy "${SERVICE}" \
  --image="${IMAGE}" \
  --region="${REGION}" \
  --platform=managed \
  --allow-unauthenticated \
  --set-env-vars=RUST_LOG=info
```

Cloud Run assigns the `PORT` environment variable at runtime, which `containerflare` now detects
and binds automatically. The host command channel is disabled on Cloud Run, so
`ContainerContext::command_client()` will return a stub that reports
`CommandError::Unavailable` if invoked. `RequestMetadata` will include `cloud_run_*` fields along
with the parsed `x-cloud-trace-context` header.
