#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"

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
IMAGE="gcr.io/${PROJECT_ID}/${SERVICE_NAME}"
TAG="${TAG:-latest}"
FULL_IMAGE="${IMAGE}:${TAG}"

RUST_LOG_VALUE="${RUST_LOG:-info}"

echo "Building ${FULL_IMAGE} from ${REPO_ROOT}..."
docker build --platform=linux/amd64 \
  -f "${SCRIPT_DIR}/Dockerfile" \
  -t "${FULL_IMAGE}" \
  "${REPO_ROOT}"

echo "Pushing ${FULL_IMAGE}..."
docker push "${FULL_IMAGE}"

echo "Deploying ${SERVICE_NAME} to region ${REGION}..."
gcloud run deploy "${SERVICE_NAME}" \
  --image="${FULL_IMAGE}" \
  --region="${REGION}" \
  --platform=managed \
  --allow-unauthenticated \
  --set-env-vars="RUST_LOG=${RUST_LOG_VALUE}" \
  "$@"

echo "Deployment complete."
