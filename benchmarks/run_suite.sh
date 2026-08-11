#!/usr/bin/env bash
# Ejecuta la comparativa directa y, opcionalmente, el camino HTTP de Ixmati.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${OUT_DIR:-${ROOT}/spec-native/evidence/raw/db-comparison-$(date -u +%Y%m%dT%H%M%SZ)}"
PG_CONTAINER="${PG_CONTAINER:-ixmati-benchmark-postgres}"
PG_IMAGE="${PG_IMAGE:-docker.io/library/postgres:18}"
PG_PORT="${PG_PORT:-30432}"
PG_DSN="host=127.0.0.1 port=${PG_PORT} dbname=ixmati_bench user=postgres password=benchmark"
RUN_IXMATI="${RUN_IXMATI:-1}"
START_IXMATI="${START_IXMATI:-1}"
RUN_DIRECT="${RUN_DIRECT:-1}"
IXMATI_URL="${IXMATI_URL:-http://127.0.0.1:30000}"
UV_ARGS=(uv run --with 'psycopg[binary]==3.2.9' python)
RATES_WRITE=(20 40 60 80 100 150 200)
RATES_READ=(100 250 500 1000)
REPETITIONS="${BENCH_REPETITIONS:-3}"
BENCH_USERS_VALUE="${BENCH_USERS:-1000}"
BENCH_ORDERS_VALUE="${BENCH_ORDERS:-10000}"
SEED_CONCURRENCY="${SEED_CONCURRENCY:-8}"

mkdir -p "$OUT_DIR"
exec > >(tee "$OUT_DIR/run.log") 2>&1

cleanup() {
  podman rm -f "$PG_CONTAINER" >/dev/null 2>&1 || true
  if [[ "$RUN_IXMATI" == 1 && "$START_IXMATI" == 1 ]]; then
    podman compose -f "$ROOT/containers/compose/multi-store.yaml" down -v >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

{
  echo "sha=$(git -C "$ROOT" rev-parse HEAD)"
  echo "date=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "host=$(hostname)"
  uname -a
  nproc || true
  free -h || true
  df -T "$OUT_DIR" || true
  podman version
  podman image inspect "$PG_IMAGE" --format 'postgres_image={{.Id}}' 2>/dev/null || true
} > "$OUT_DIR/manifest.txt"

if [[ "$RUN_DIRECT" == 1 ]]; then
  SQLITE_DB="$OUT_DIR/direct.sqlite"
  "${UV_ARGS[@]}" "$ROOT/benchmarks/runner.py" init-sqlite "$SQLITE_DB"
  "${UV_ARGS[@]}" "$ROOT/benchmarks/runner.py" seed-sqlite "$SQLITE_DB" \
    --users "$BENCH_USERS_VALUE" --orders "$BENCH_ORDERS_VALUE"

podman run -d --name "$PG_CONTAINER" \
  --cpus="${BENCH_CPUS:-2}" --memory="${BENCH_MEMORY:-2g}" \
  -p "127.0.0.1:${PG_PORT}:5432" \
  -e POSTGRES_PASSWORD=benchmark -e POSTGRES_DB=ixmati_bench "$PG_IMAGE" >/dev/null
for _ in $(seq 1 60); do
  if podman exec "$PG_CONTAINER" pg_isready -U postgres -d ixmati_bench >/dev/null 2>&1; then
    break
  fi
  sleep 1
done
podman exec "$PG_CONTAINER" pg_isready -U postgres -d ixmati_bench

"${UV_ARGS[@]}" "$ROOT/benchmarks/runner.py" init-postgres "$PG_DSN"
"${UV_ARGS[@]}" "$ROOT/benchmarks/runner.py" seed-postgres "$PG_DSN" \
  --users "$BENCH_USERS_VALUE" --orders "$BENCH_ORDERS_VALUE"

run_direct() {
  local engine="$1" target="$2" operation="$3" rate="$4" concurrency="$5" batch_size="$6" cache_state="${7:-warm}" repeat="${8:-1}"
  local name="${engine}-${operation}-${rate}-c${concurrency}-b${batch_size}-${cache_state}-r${repeat}.json"
  local warmup="${BENCH_WARMUP:-15}"
  [[ "$cache_state" == "cold-first-pass" ]] && warmup=0
  "${UV_ARGS[@]}" "$ROOT/benchmarks/runner.py" load \
    --engine "$engine" --target "$target" --operation "$operation" \
    --rate "$rate" --duration "${BENCH_DURATION:-30}" \
    --warmup "$warmup" --cache-state "$cache_state" \
    --concurrency "$concurrency" --batch-size "$batch_size" > "$OUT_DIR/$name" || true
}

for rate in "${RATES_READ[@]}"; do
  for cache_state in cold-first-pass warm; do
    for repeat in $(seq 1 "$REPETITIONS"); do
      run_direct sqlite "$SQLITE_DB" read_point "$rate" 16 1 "$cache_state" "$repeat"
      run_direct sqlite "$SQLITE_DB" read_join "$rate" 16 1 "$cache_state" "$repeat"
      run_direct postgres "$PG_DSN" read_point "$rate" 16 1 "$cache_state" "$repeat"
      run_direct postgres "$PG_DSN" read_join "$rate" 16 1 "$cache_state" "$repeat"
    done
  done
done
for concurrency in 1 4 16 32 64; do
  for repeat in $(seq 1 "$REPETITIONS"); do
    run_direct sqlite "$SQLITE_DB" read_point "${BENCH_CEILING_RATE:-100000}" "$concurrency" 1 warm "$repeat"
    run_direct postgres "$PG_DSN" read_point "${BENCH_CEILING_RATE:-100000}" "$concurrency" 1 warm "$repeat"
  done
done
for rate in "${RATES_WRITE[@]}"; do
  for repeat in $(seq 1 "$REPETITIONS"); do
    run_direct sqlite "$SQLITE_DB" write "$rate" 1 1 warm "$repeat"
    run_direct sqlite "$SQLITE_DB" write "$rate" 1 100 warm "$repeat"
    run_direct postgres "$PG_DSN" write "$rate" 16 1 warm "$repeat"
    run_direct postgres "$PG_DSN" write "$rate" 16 100 warm "$repeat"
  done
done
for operation in update idempotency mixed; do
  for repeat in $(seq 1 "$REPETITIONS"); do
    run_direct sqlite "$SQLITE_DB" "$operation" 100 16 1 warm "$repeat"
    run_direct postgres "$PG_DSN" "$operation" 100 16 1 warm "$repeat"
  done
done
fi

if [[ "$RUN_IXMATI" == 1 ]]; then
  capture_ixmati_snapshot() {
    local label="$1"
    curl -fsS "$IXMATI_URL/metrics" > "$OUT_DIR/ixmati-metrics-${label}.prom" || true
    podman compose -f "$ROOT/containers/compose/multi-store.yaml" ps > "$OUT_DIR/ixmati-services-${label}.txt" || true
  }
  if [[ "$START_IXMATI" == 1 ]]; then
    podman compose -f "$ROOT/containers/compose/multi-store.yaml" down -v || true
    if [[ "${IXMATI_BUILD:-0}" == 1 ]]; then
      podman compose -f "$ROOT/containers/compose/multi-store.yaml" up -d --build
    else
      podman compose -f "$ROOT/containers/compose/multi-store.yaml" up -d
    fi
    for _ in $(seq 1 60); do
      if curl -fsS "$IXMATI_URL/health" >/dev/null 2>&1; then break; fi
      sleep 1
    done
    curl -fsS "$IXMATI_URL/health"
    capture_ixmati_snapshot pre-load
  fi
  "${UV_ARGS[@]}" "$ROOT/benchmarks/seed_ixmati.py" "$IXMATI_URL" \
    --users "$BENCH_USERS_VALUE" --orders "$BENCH_ORDERS_VALUE" \
    --concurrency "$SEED_CONCURRENCY"
  for rate in "${RATES_READ[@]}"; do
    for cache_state in cold-first-pass warm; do
      warmup="${BENCH_WARMUP:-15}"
      [[ "$cache_state" == "cold-first-pass" ]] && warmup=0
      for repeat in $(seq 1 "$REPETITIONS"); do
        for operation in read_point read_join; do
          "${UV_ARGS[@]}" "$ROOT/benchmarks/runner.py" load \
            --engine http --target "$IXMATI_URL" --operation "$operation" \
            --rate "$rate" --duration "${BENCH_DURATION:-30}" \
            --warmup "$warmup" --cache-state "$cache_state" --concurrency 64 \
            > "$OUT_DIR/ixmati-${operation}-${rate}-${cache_state}-r${repeat}.json" || true
          capture_ixmati_snapshot "${operation}-${rate}-${cache_state}-r${repeat}"
        done
      done
    done
  done
  for rate in "${RATES_WRITE[@]}"; do
    for repeat in $(seq 1 "$REPETITIONS"); do
      "${UV_ARGS[@]}" "$ROOT/benchmarks/runner.py" load \
        --engine http --target "$IXMATI_URL" --operation write \
        --rate "$rate" --duration "${BENCH_DURATION:-30}" \
        --warmup "${BENCH_WARMUP:-15}" --concurrency 200 \
        > "$OUT_DIR/ixmati-write-${rate}-r${repeat}.json" || true
      capture_ixmati_snapshot "write-${rate}-r${repeat}"
    done
  done
  for operation in update idempotency mixed; do
    for repeat in $(seq 1 "$REPETITIONS"); do
      "${UV_ARGS[@]}" "$ROOT/benchmarks/runner.py" load \
        --engine http --target "$IXMATI_URL" --operation "$operation" \
        --rate 100 --duration "${BENCH_DURATION:-30}" \
        --warmup "${BENCH_WARMUP:-15}" --cache-state warm --concurrency 64 \
        > "$OUT_DIR/ixmati-${operation}-100-r${repeat}.json" || true
      capture_ixmati_snapshot "${operation}-100-r${repeat}"
    done
  done
  if [[ "$START_IXMATI" == 1 ]]; then
    podman compose -f "$ROOT/containers/compose/multi-store.yaml" down -v || true
  fi
fi

echo "results=$OUT_DIR"
