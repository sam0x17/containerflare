#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

PROJECT_ID="${PROJECT_ID:-$(gcloud config get-value project 2>/dev/null | tr -d '\n')}"
if [[ -z "${PROJECT_ID}" ]]; then
  echo >&2 "error: PROJECT_ID not set and gcloud config has no project"
  exit 1
fi

REGION="${REGION:-$(gcloud config get-value run/region 2>/dev/null | tr -d '\n')}"
if [[ -z "${REGION}" ]]; then
  REGION="$(gcloud config get-value compute/region 2>/dev/null | tr -d '\n')"
fi
if [[ -z "${REGION}" ]]; then
  echo >&2 "error: REGION not set and gcloud config has no default region"
  exit 1
fi

SERVICE_NAME="${SERVICE_NAME:-containerflare-cloudrun}"
TAG="${TAG:-latest}"
IMAGE="gcr.io/${PROJECT_ID}/${SERVICE_NAME}:${TAG}"
RUST_LOG_VALUE="${RUST_LOG:-info}"

cd "${REPO_ROOT}"

echo "Building ${IMAGE} using examples/basic/Dockerfile.cloudrun..."
docker build --platform=linux/amd64 \
  -f "examples/basic/Dockerfile.cloudrun" \
  -t "${IMAGE}" \
  "${REPO_ROOT}"

echo "Pushing ${IMAGE}..."
docker push "${IMAGE}"

echo "Deploying ${SERVICE_NAME} to region ${REGION}..."
gcloud run deploy "${SERVICE_NAME}" \
  --image="${IMAGE}" \
  --project="${PROJECT_ID}" \
  --region="${REGION}" \
  --platform=managed \
  --allow-unauthenticated \
  --set-env-vars="RUST_LOG=${RUST_LOG_VALUE}" \
  "$@"

echo "Deployment complete."
