#!/usr/bin/env bash
# Prolonged rate-controlled capacity test. The Debian target is expected to
# already be installed and healthy; this script never leaves the test throttle
# behind when interrupted.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CONTAINER_NAME="${CONTAINER_NAME:-ixmati-load-test}"
TEST_HOST="${TEST_HOST:-192.168.3.175}"
# The generator runs with --network host on the Debian Podman machine. Do not
# route load traffic through the operator or the LAN interface.
PODMAN_HOST_IP="${PODMAN_HOST_IP:-127.0.0.1}"
API_PORT="${API_PORT:-30300}"
WRITER_METRICS_PORT="${WRITER_METRICS_PORT:-30301}"
STORE="${STORE:-default}"
API_KEY="${API_KEY:-ix-default-key}"
DURATION="${DURATION:-3600}"
CONCURRENCY="${CONCURRENCY:-200}"
OUT_DIR="${OUT_DIR:-${ROOT}/spec-native/evidence/raw/soak-$(date -u +%Y%m%dT%H%M%SZ)}"
RATES=(${SOAK_RATES:-150 200})
GENERATOR="${GENERATOR:-${SOAK_GENERATOR:-auto}}"
GENERATOR_IMAGE="${GENERATOR_IMAGE:-ixmati-soak-generator}"
GENERATOR_CONTAINER="${GENERATOR_CONTAINER:-ixmati-soak-generator}"
SNAPSHOT_INTERVAL="${SNAPSHOT_INTERVAL:-900}"

mkdir -p "$OUT_DIR"
exec > >(tee "$OUT_DIR/run.log") 2>&1

restore_overrides() {
  podman exec "$CONTAINER_NAME" bash -lc '
    rm -f /etc/systemd/system/ixmati-api.service.d/override.conf
    systemctl daemon-reload
    systemctl restart ixmati-api >/dev/null 2>&1 || true
  ' >/dev/null 2>&1 || true
}

snapshot_pid=""
generator_container=""
cleanup() {
  if [[ -n "$snapshot_pid" ]]; then
    kill "$snapshot_pid" >/dev/null 2>&1 || true
    wait "$snapshot_pid" >/dev/null 2>&1 || true
  fi
  if [[ -n "$generator_container" ]]; then
    podman rm -f "$generator_container" >/dev/null 2>&1 || true
  fi
  restore_overrides
}
trap cleanup EXIT INT TERM

{
  echo "sha=$(git -C "$ROOT" rev-parse HEAD)"
  echo "started_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host=$TEST_HOST api_port=$API_PORT writer_metrics_port=$WRITER_METRICS_PORT"
  echo "container=$CONTAINER_NAME store=$STORE duration=$DURATION concurrency=$CONCURRENCY"
  echo "generator=$GENERATOR image=$GENERATOR_IMAGE podman_host_ip=$PODMAN_HOST_IP snapshot_interval=$SNAPSHOT_INTERVAL rates=${RATES[*]}"
  podman version
} > "$OUT_DIR/manifest.txt"

podman exec "$CONTAINER_NAME" curl -fsS "http://127.0.0.1:30000/health" > "$OUT_DIR/health.json"

for rate in "${RATES[@]}"; do
  run_dir="$OUT_DIR/rate-${rate}"
  mkdir -p "$run_dir"
  echo "=== soak rate=${rate}/s duration=${DURATION}s ==="

  # Capacity runs temporarily remove the 40/s production admission throttle;
  # outbox backpressure remains active and the override is always removed by
  # the EXIT trap.
  podman exec "$CONTAINER_NAME" bash -lc '
    mkdir -p /etc/systemd/system/ixmati-api.service.d
    cat > /etc/systemd/system/ixmati-api.service.d/override.conf <<EOF
[Service]
Environment=MAX_WRITES_PER_WINDOW=1000000
Environment=THROTTLE_WINDOW_SECS=1
EOF
    systemctl daemon-reload
    systemctl restart ixmati-api
  '

  if [[ "$GENERATOR" == "container" ]]; then
    generator_container="$GENERATOR_CONTAINER"
    podman rm -f "$generator_container" >/dev/null 2>&1 || true
  else
    generator_container=""
  fi

  (
    while true; do
      timestamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
      {
        echo "timestamp=$timestamp"
        podman exec "$CONTAINER_NAME" curl -fsS "http://127.0.0.1:30000/metrics" || true
        echo
      } > "$run_dir/api-${timestamp}.prom"
      podman exec "$CONTAINER_NAME" curl -fsS "http://127.0.0.1:${WRITER_METRICS_PORT}/metrics" > "$run_dir/writer-${timestamp}.prom" || true
      podman ps --filter "name=${CONTAINER_NAME}" --format '{{.Names}} {{.Status}}' > "$run_dir/container-${timestamp}.txt" || true
      podman logs --since "${SNAPSHOT_INTERVAL}s" "$CONTAINER_NAME" > "$run_dir/container-log-${timestamp}.log" 2>&1 || true
      podman exec "$CONTAINER_NAME" journalctl -u ixmati-api -u "ixmati-writer@${STORE}" --since "${SNAPSHOT_INTERVAL} seconds ago" --no-pager > "$run_dir/journal-${timestamp}.log" 2>/dev/null || true
      if [[ -n "$generator_container" ]]; then
        podman logs --tail 200 "$generator_container" > "$run_dir/generator-log-${timestamp}.log" 2>&1 || true
        podman exec "$generator_container" cat /tmp/rate-load.jsonl > "$run_dir/rate-load-${timestamp}.jsonl" 2>/dev/null || true
      fi
      sleep "$SNAPSHOT_INTERVAL"
    done
  ) &
  snapshot_pid=$!

  if [[ "$GENERATOR" == "container" ]]; then
    podman run -d --name "$generator_container" --network host \
      "$GENERATOR_IMAGE" "http://${PODMAN_HOST_IP}:${API_PORT}/write" \
      --rate "$rate" --duration "$DURATION" --concurrency "$CONCURRENCY" \
      --api-key "$API_KEY" --store "$STORE" --entity soak-order \
      --sample-interval 10 --snapshot-file /tmp/rate-load.jsonl \
      > "$run_dir/generator-id.txt"
    # Do not use `podman wait` here: it would keep one SSH/API connection
    # open for the entire soak. The generator is detached; the local process
    # sleeps and snapshots it with short-lived Podman calls instead.
    sleep "$DURATION"
    podman inspect --format '{{.State.Status}} {{.State.ExitCode}}' \
      "$generator_container" > "$run_dir/generator-exit.txt" 2>&1 || true
    podman exec "$generator_container" cat /tmp/rate-load.jsonl > "$run_dir/rate-load.jsonl" 2>/dev/null || true
    podman logs "$generator_container" > "$run_dir/result.json" 2>&1 || true
  elif [[ "$GENERATOR" == "jmeter" || ( "$GENERATOR" == "auto" && -n "$(command -v jmeter || true)" ) ]]; then
    jmeter -n -t "$ROOT/benchmarks/ixmati-soak.jmx" \
      -Jhost="$TEST_HOST" -Jport="$API_PORT" -Jstore="$STORE" -Japi_key="$API_KEY" \
      -Jrate="$rate" -Jduration="$DURATION" -Jconcurrency="$CONCURRENCY" \
      -l "$run_dir/results.jtl" -j "$run_dir/jmeter.log" \
      > "$run_dir/jmeter.stdout" 2>&1
  else
    python3 "$ROOT/helpers/python/rate_load.py" "http://${TEST_HOST}:${API_PORT}/write" \
      --rate "$rate" --duration "$DURATION" --concurrency "$CONCURRENCY" \
      --api-key "$API_KEY" --store "$STORE" --entity soak-order \
      --sample-interval 10 --snapshot-file "$run_dir/rate-load.jsonl" \
      > "$run_dir/result.json"
  fi

  kill "$snapshot_pid" >/dev/null 2>&1 || true
  wait "$snapshot_pid" >/dev/null 2>&1 || true
  snapshot_pid=""
  echo "load finished; drain for ${DRAIN_SECONDS:-300}s"
  sleep "${DRAIN_SECONDS:-300}"
  podman exec "$CONTAINER_NAME" curl -fsS "http://127.0.0.1:30000/metrics" > "$run_dir/api-final.prom" || true
  podman exec "$CONTAINER_NAME" curl -fsS "http://127.0.0.1:${WRITER_METRICS_PORT}/metrics" > "$run_dir/writer-final.prom" || true
  podman exec "$CONTAINER_NAME" bash -lc "python3 -c 'import sqlite3; c=sqlite3.connect(\"/var/lib/ixmati/stores/${STORE}.db\"); print(c.execute(\"PRAGMA integrity_check\").fetchone()[0]); print(c.execute(\"SELECT COUNT(*) FROM _outbox WHERE published_at IS NULL\").fetchone()[0])'" \
    > "$run_dir/sqlite-final.txt" 2>&1 || true
done

restore_overrides
echo "evidence=$OUT_DIR"
