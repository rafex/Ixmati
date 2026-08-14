#!/usr/bin/env bash
set -Eeuo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
compose_file="${repo_root}/benchmarks/java-live/compose.yaml"
duration="${1:-60}"
write_rate="${2:-20}"
read_rate="${3:-20}"
clients="${4:-3}"
duration="${duration#duration=}"
write_rate="${write_rate#write_rate=}"
read_rate="${read_rate#read_rate=}"
clients="${clients#clients=}"
timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
evidence_dir="${OUTPUT_DIR:-${repo_root}/spec-native/evidence/raw/java-live-${timestamp}}"
mkdir -p "${evidence_dir}"

if [[ "${clients}" != "3" ]]; then
  echo "java-live-demo requires clients=3 (the topology is deliberately 3 vs 3)" >&2
  exit 2
fi
command -v podman >/dev/null || { echo "podman is required" >&2; exit 1; }
command -v mvn >/dev/null || { echo "mvn is required" >&2; exit 1; }

podman_connection="${PODMAN_CONNECTION:-}"
podman_cli() {
  if [[ -n "${podman_connection}" ]]; then
    podman --connection "${podman_connection}" "$@"
  else
    podman "$@"
  fi
}
compose() { podman_cli compose -f "${compose_file}" "$@"; }
cleanup() { compose down -v --remove-orphans >/dev/null 2>&1 || true; }
trap cleanup EXIT INT TERM

{
  echo "sha=$(git -C "${repo_root}" rev-parse HEAD)"
  echo "duration=${duration}"
  echo "write_rate_per_client=${write_rate}"
  echo "read_rate_per_client=${read_rate}"
  echo "clients_per_side=${clients}"
  echo "topology=3 direct SQLite clients vs 3 Ixmati gRPC clients"
  echo "warning=concurrent visual demo; shared CPU/memory/filesystem; not isolated capacity"
} > "${evidence_dir}/manifest.txt"

echo "[java-live] compiling Java client"
mvn -q -f "${repo_root}/benchmarks/java-live/pom.xml" test
echo "[java-live] building Ixmati and infrastructure images"
if [[ -n "${podman_connection}" ]]; then
  CONTAINER_CONNECTION="${podman_connection}" make -C "${repo_root}" containers-build
else
  make -C "${repo_root}" containers-build
fi
echo "[java-live] building Java and dashboard images"
podman_cli build -f "${repo_root}/benchmarks/java-live/Containerfile" -t localhost/ixmati-java-live-client:local "${repo_root}"
podman_cli build -f "${repo_root}/benchmarks/java-live/dashboard-Containerfile" -t localhost/ixmati-java-live-dashboard:local "${repo_root}"

compose down -v --remove-orphans >/dev/null 2>&1 || true
export LIVE_DURATION="${duration}" LIVE_WRITE_RATE="${write_rate}" LIVE_READ_RATE="${read_rate}"
echo "[java-live] starting isolated Podman project"
compose up -d mosquitto cache-server api writer projector litestream sqlite-init dashboard
ready=0
for _ in $(seq 1 30); do
  if compose exec -T dashboard python -c 'from urllib.request import urlopen; urlopen("http://api:30000/health", timeout=1).read()' >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 1
done
if [[ "${ready}" != "1" ]]; then
  echo "Ixmati API did not become ready inside the demo network" >&2
  compose logs --no-color api writer >&2 || true
  exit 1
fi
compose up -d sqlite-java-1 sqlite-java-2 sqlite-java-3 ixmati-java-1 ixmati-java-2 ixmati-java-3
dashboard_host="${PODMAN_DASHBOARD_HOST:-127.0.0.1}"
echo "podman_connection=${podman_connection:-local}" >> "${evidence_dir}/manifest.txt"
echo "dashboard=http://${dashboard_host}:30450" | tee -a "${evidence_dir}/manifest.txt"
echo "[java-live] dashboard: http://${dashboard_host}:30450"
echo "[java-live] terminal view runs inside the dashboard container; Ctrl-C cleans the project"
compose exec -T dashboard python /app/tui.py --url http://dashboard:8080/state --duration "${duration}" \
  | tee "${evidence_dir}/terminal.log"

echo "[java-live] collecting evidence"
compose ps > "${evidence_dir}/services.txt" || true
compose logs --no-color > "${evidence_dir}/services.log" 2>&1 || true
for service in sqlite-java-1 sqlite-java-2 sqlite-java-3 ixmati-java-1 ixmati-java-2 ixmati-java-3; do
  compose cp "$service:/snapshots/." "${evidence_dir}/snapshots-$service" >/dev/null 2>&1 || true
done
compose exec -T writer sqlite3 /data/default.db "PRAGMA integrity_check; SELECT COUNT(*) AS outbox_pending FROM _outbox WHERE published_at IS NULL; SELECT COUNT(*) AS idempotency_rows FROM _idempotency;" \
  > "${evidence_dir}/ixmati-integrity.txt" 2>&1 || true
compose exec -T dashboard python -c 'import json; print(json.dumps(__import__("urllib.request",fromlist=["urlopen"]).urlopen("http://127.0.0.1:8080/state").read().decode()))' \
  > "${evidence_dir}/final-state.json" 2>/dev/null || true
echo "evidence=${evidence_dir}"
