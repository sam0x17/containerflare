#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"

ALLOW_UNAUTH="${ALLOW_UNAUTH:-false}"
DEPLOY_ARGS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --allow-unauthenticated)
      ALLOW_UNAUTH=true
      shift
      ;;
    --no-allow-unauthenticated)
      ALLOW_UNAUTH=false
      shift
      ;;
    *)
      DEPLOY_ARGS+=("$1")
      shift
      ;;
  esac
done

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
ALLOW_FLAG=(--no-allow-unauthenticated)
if [[ "${ALLOW_UNAUTH}" == "true" ]]; then
  ALLOW_FLAG=(--allow-unauthenticated)
fi
GCLOUD_ARGS=(
  --image="${IMAGE}"
  --project="${PROJECT_ID}"
  --region="${REGION}"
  --platform=managed
  "${ALLOW_FLAG[@]}"
  --set-env-vars="RUST_LOG=${RUST_LOG_VALUE}"
)

if [[ ${#DEPLOY_ARGS[@]:-0} -gt 0 ]]; then
  GCLOUD_ARGS+=("${DEPLOY_ARGS[@]}")
fi

cd "${REPO_ROOT}"

echo "Building ${IMAGE} using examples/basic/Dockerfile..."
docker build --platform=linux/amd64 \
  -f "${REPO_ROOT}/examples/basic/Dockerfile" \
  -t "${IMAGE}" \
  "${REPO_ROOT}"

echo "Pushing ${IMAGE}..."
docker push "${IMAGE}"

echo "Deploying ${SERVICE_NAME} to region ${REGION}..."
gcloud run deploy "${SERVICE_NAME}" "${GCLOUD_ARGS[@]}"

echo "Deployment complete."
