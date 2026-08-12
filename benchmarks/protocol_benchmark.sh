#!/usr/bin/env bash
# Run the JSON/REST-Protobuf/gRPC comparison from a Podman host.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
connection="${PODMAN_CONNECTION:-debian-server-wifi}"
image="${BENCHMARK_IMAGE:-localhost/ixmati-builder:local}"
api_url="${API_URL:-http://${PODMAN_HOST_IP:-127.0.0.1}:30000}"
grpc_url="${GRPC_URL:-http://${PODMAN_HOST_IP:-127.0.0.1}:30100}"
metrics_url="${METRICS_URL:-${api_url}/metrics}"
api_key="${IXMATI_API_KEY:-ix-default-key}"
store="${IXMATI_BENCH_STORE:-pedidos}"
entity="${IXMATI_BENCH_ENTITY:-pedido}"
duration="${DURATION:-30}"
warmup="${WARMUP:-5}"
cooldown="${COOLDOWN:-2}"
concurrency="${CONCURRENCY:-200}"
rates=( ${RATES:-40 100 150} )
protocols=( ${PROTOCOLS:-json protobuf grpc} )
timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
output_dir="${OUTPUT_DIR:-${repo_root}/spec-native/evidence/raw/protobuf-bench-${timestamp}}"

mkdir -p "${output_dir}"
{
  printf 'sha=%s\n' "$(git -C "${repo_root}" rev-parse HEAD)"
  printf 'connection=%s\nimage=%s\n' "${connection}" "${image}"
  printf 'api_url=%s\ngrpc_url=%s\nmetrics_url=%s\n' \
    "${api_url}" "${grpc_url}" "${metrics_url}"
  printf 'duration=%s\nwarmup=%s\ncooldown=%s\nconcurrency=%s\n' \
    "${duration}" "${warmup}" "${cooldown}" "${concurrency}"
  printf 'rates=%s\nprotocols=%s\n' "${rates[*]}" "${protocols[*]}"
} > "${output_dir}/manifest.txt"

snapshot() {
  local label="$1"
  curl -fsS "${metrics_url}" > "${output_dir}/${label}-api.prom" || true
  podman --connection "${connection}" ps --format '{{.Names}} {{.Status}}' \
    > "${output_dir}/${label}-services.txt"
  podman --connection "${connection}" stats --no-stream \
    --format '{{.Name}} {{.CPU}} {{.MemUsage}} {{.PIDS}}' \
    > "${output_dir}/${label}-stats.txt" || true
}

snapshot start
for rate in "${rates[@]}"; do
  for protocol in "${protocols[@]}"; do
    output_file="${output_dir}/rate-${rate}-${protocol}.json"
    podman --connection "${connection}" run --rm --network host \
      --entrypoint /usr/local/bin/ixmati-protocol-bench "${image}" \
      --protocol "${protocol}" \
      --url "${api_url}" \
      --grpc-url "${grpc_url}" \
      --rate "${rate}" \
      --duration "${duration}" \
      --warmup "${warmup}" \
      --cooldown "${cooldown}" \
      --concurrency "${concurrency}" \
      --api-key "${api_key}" \
      --store "${store}" \
      --entity "${entity}" \
      > "${output_file}"
    snapshot "rate-${rate}-${protocol}"
  done
done

printf 'evidence=%s\n' "${output_dir}"
