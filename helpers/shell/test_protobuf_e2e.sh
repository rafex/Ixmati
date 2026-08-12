#!/usr/bin/env bash
# Functional E2E for REST/Protobuf and gRPC against a local API stack.
#
# The script intentionally uses local processes instead of Compose so it can
# validate the protocol independently from the container image build harness.
# Build first with:
#   cargo build -p ixmati-api -p ixmati-writer -p ixmati-cache-server
#
# Optional environment:
#   API_PORT, GRPC_PORT, MQTT_PORT, API_KEY, KEEP_EVIDENCE

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
API_PORT="${API_PORT:-30311}"
GRPC_PORT="${GRPC_PORT:-30312}"
MQTT_PORT="${MQTT_PORT:-18884}"
API_KEY="${API_KEY:-protobuf-e2e-key}"
KEEP_EVIDENCE="${KEEP_EVIDENCE:-0}"
TEST_DIR="$(mktemp -d "${TMPDIR:-/tmp}/ixmati-protobuf-e2e.XXXXXX")"

API_BIN="${API_BIN:-${ROOT}/target/debug/ixmati-api}"
WRITER_BIN="${WRITER_BIN:-${ROOT}/target/debug/ixmati-writer}"
CACHE_BIN="${CACHE_BIN:-${ROOT}/target/debug/ixmati-cache-server}"
PROTO_DIR="${ROOT}/proto"

api_pid=""
writer_pid=""
cache_pid=""
mqtt_pid=""

cleanup() {
    set +e
    for pid in "$api_pid" "$writer_pid" "$cache_pid" "$mqtt_pid"; do
        if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
        fi
    done
    if [[ "$KEEP_EVIDENCE" == "1" ]]; then
        printf '[protobuf-e2e] evidencia local: %s\n' "$TEST_DIR"
    else
        rm -rf "$TEST_DIR"
    fi
}
trap cleanup EXIT INT TERM

require_command() {
    command -v "$1" >/dev/null 2>&1 || {
        printf '[protobuf-e2e] falta comando: %s\n' "$1" >&2
        exit 1
    }
}

require_file() {
    [[ -x "$1" ]] || {
        printf '[protobuf-e2e] falta binario ejecutable: %s\n' "$1" >&2
        printf '[protobuf-e2e] ejecuta: cargo build -p ixmati-api -p ixmati-writer -p ixmati-cache-server\n' >&2
        exit 1
    }
}

require_command curl
require_command mosquitto
require_command protoc
require_file "$API_BIN"
require_file "$WRITER_BIN"
require_file "$CACHE_BIN"

PROTO_INCLUDE="${PROTO_INCLUDE:-}"
if [[ -z "$PROTO_INCLUDE" ]]; then
    for candidate in /usr/include /opt/homebrew/include /opt/homebrew/opt/protobuf/include; do
        if [[ -f "$candidate/google/protobuf/struct.proto" ]]; then
            PROTO_INCLUDE="$candidate"
            break
        fi
    done
fi
[[ -n "$PROTO_INCLUDE" ]] || {
    printf '[protobuf-e2e] no se encontró google/protobuf/struct.proto; usa PROTO_INCLUDE=/ruta/include\n' >&2
    exit 1
}

decode() {
    local type="$1" schema="$2"
    protoc -I"$PROTO_DIR" -I"$PROTO_INCLUDE" \
        --decode="$type" "$schema"
}

wait_for_socket() {
    local socket="$1"
    local attempt
    for attempt in $(seq 1 100); do
        [[ -S "$socket" ]] && return 0
        sleep 0.1
    done
    printf '[protobuf-e2e] timeout esperando socket %s\n' "$socket" >&2
    return 1
}

wait_for_http() {
    local url="$1"
    local attempt
    for attempt in $(seq 1 100); do
        if curl -fsS --max-time 1 "$url" >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.1
    done
    printf '[protobuf-e2e] timeout esperando %s\n' "$url" >&2
    return 1
}

printf '[protobuf-e2e] dir=%s REST=%s gRPC=%s MQTT=%s\n' \
    "$TEST_DIR" "$API_PORT" "$GRPC_PORT" "$MQTT_PORT"

mosquitto -p "$MQTT_PORT" -v >"$TEST_DIR/mosquitto.log" 2>&1 &
mqtt_pid=$!
sleep 0.3
kill -0 "$mqtt_pid" 2>/dev/null

mkdir -p "$TEST_DIR/cache"
CACHE_BACKEND=sqlite \
CACHE_DIR="$TEST_DIR/cache" \
CACHE_SOCKET_PATH="$TEST_DIR/cache.sock" \
RUST_LOG="ixmati_cache_server=warn" \
    "$CACHE_BIN" >"$TEST_DIR/cache.log" 2>&1 &
cache_pid=$!
wait_for_socket "$TEST_DIR/cache.sock"

MQTT_BROKER="tcp://127.0.0.1:${MQTT_PORT}" \
STORE_NAME=smoke \
SQLITE_PATH="$TEST_DIR/smoke.db" \
CACHE_SOCKET_PATH="$TEST_DIR/cache.sock" \
BATCH_SIZE=1 \
BATCH_INTERVAL_MS=25 \
PUBLISH_INTERVAL_MS=25 \
RUST_LOG="ixmati_writer=warn" \
    "$WRITER_BIN" >"$TEST_DIR/writer.log" 2>&1 &
writer_pid=$!
sleep 0.5
kill -0 "$writer_pid" 2>/dev/null

API_HOST=127.0.0.1 \
API_PORT="$API_PORT" \
GRPC_HOST=127.0.0.1 \
GRPC_PORT="$GRPC_PORT" \
MQTT_BROKER="tcp://127.0.0.1:${MQTT_PORT}" \
SQLITE_PATH="$TEST_DIR/smoke.db" \
CACHE_BACKEND=sqlite \
CACHE_DIR="$TEST_DIR/cache" \
CACHE_READ_MODE=socket \
CACHE_SOCKET_PATH="$TEST_DIR/cache.sock" \
IXMATI_API_KEYS="$API_KEY" \
MAX_WRITES_PER_WINDOW=1000 \
RUST_LOG="ixmati_api=warn" \
    "$API_BIN" >"$TEST_DIR/api.log" 2>&1 &
api_pid=$!
wait_for_http "http://127.0.0.1:${API_PORT}/health"

printf '[protobuf-e2e] REST health protobuf\n'
curl -fsS --max-time 5 \
    -H 'Accept: application/protobuf' \
    "http://127.0.0.1:${API_PORT}/health" \
    -o "$TEST_DIR/health.pb"
decode ixmati.v1.HealthCheckResponse "$PROTO_DIR/ixmati/v1/read.proto" \
    <"$TEST_DIR/health.pb" | tee "$TEST_DIR/health.txt"
grep -q 'overall: STATUS_OK\|overall: STATUS_DEGRADED' "$TEST_DIR/health.txt"

printf '[protobuf-e2e] REST/JSON compatibility\n'
curl -fsS --max-time 5 -H 'Accept: application/json' \
    "http://127.0.0.1:${API_PORT}/health" \
    -o "$TEST_DIR/health.json"
grep -q '"overall"' "$TEST_DIR/health.json"

order_key="protobuf-rest-$(date +%s)-$$"
idempotency_key="protobuf-rest-idem-$(date +%s)-$$"
cat >"$TEST_DIR/write.txtproto" <<EOF
envelope {
  op: "upsert"
  store: "smoke"
  entity: "order"
  key: "$order_key"
  version: 1
  ts: "2026-08-12T00:00:00Z"
  idempotency_key: "$idempotency_key"
  ack_mode: "committed"
  payload { fields { key: "order_id" value { string_value: "$order_key" } } fields { key: "total" value { number_value: 42.5 } } }
}
EOF
protoc -I"$PROTO_DIR" -I"$PROTO_INCLUDE" \
    --encode=ixmati.v1.WriteRequest "$PROTO_DIR/ixmati/v1/write.proto" \
    <"$TEST_DIR/write.txtproto" >"$TEST_DIR/write.pb"

printf '[protobuf-e2e] REST write/status/read protobuf\n'
curl -fsS --max-time 10 -X POST \
    -H 'Content-Type: application/protobuf' \
    -H "Authorization: ApiKey ${API_KEY}" \
    --data-binary @"$TEST_DIR/write.pb" \
    "http://127.0.0.1:${API_PORT}/write" \
    -o "$TEST_DIR/write-response.pb"
decode ixmati.v1.WriteResponse "$PROTO_DIR/ixmati/v1/write.proto" \
    <"$TEST_DIR/write-response.pb" | tee "$TEST_DIR/write-response.txt"
grep -q 'status: "COMMITTED"' "$TEST_DIR/write-response.txt"
grep -q "idempotency_key: \"${idempotency_key}\"" "$TEST_DIR/write-response.txt"

curl -fsS --max-time 5 -H 'Accept: application/protobuf' \
    "http://127.0.0.1:${API_PORT}/writes/smoke/${idempotency_key}" \
    -o "$TEST_DIR/status.pb"
decode ixmati.v1.GetWriteStatusResponse "$PROTO_DIR/ixmati/v1/write.proto" \
    <"$TEST_DIR/status.pb" | tee "$TEST_DIR/status.txt"
grep -q 'status: WRITE_STATUS_COMMITTED' "$TEST_DIR/status.txt"

curl -fsS --max-time 5 -H 'Accept: application/protobuf' \
    "http://127.0.0.1:${API_PORT}/read?store=smoke&entity=order&key=${order_key}" \
    -o "$TEST_DIR/read-get.pb"
decode ixmati.v1.ReadResponse "$PROTO_DIR/ixmati/v1/read.proto" \
    <"$TEST_DIR/read-get.pb" | tee "$TEST_DIR/read-get.txt"
grep -q 'found: true' "$TEST_DIR/read-get.txt"
grep -q 'source: "cache"\|source: "sqlite"' "$TEST_DIR/read-get.txt"

cat >"$TEST_DIR/read.txtproto" <<EOF
store: "smoke"
entity: "order"
key: "$order_key"
EOF
protoc -I"$PROTO_DIR" -I"$PROTO_INCLUDE" \
    --encode=ixmati.v1.ReadRequest "$PROTO_DIR/ixmati/v1/read.proto" \
    <"$TEST_DIR/read.txtproto" >"$TEST_DIR/read.pb"
curl -fsS --max-time 5 -X POST \
    -H 'Content-Type: application/protobuf' \
    --data-binary @"$TEST_DIR/read.pb" \
    "http://127.0.0.1:${API_PORT}/read" \
    -o "$TEST_DIR/read-post.pb"
decode ixmati.v1.ReadResponse "$PROTO_DIR/ixmati/v1/read.proto" \
    <"$TEST_DIR/read-post.pb" | tee "$TEST_DIR/read-post.txt"
grep -q 'found: true' "$TEST_DIR/read-post.txt"

printf '[protobuf-e2e] REST error negotiation\n'
invalid_status="$(curl -sS --max-time 5 -o "$TEST_DIR/invalid-error.pb" -w '%{http_code}' \
    -X POST -H 'Content-Type: application/protobuf' \
    -H "Authorization: ApiKey ${API_KEY}" \
    --data-binary 'not-a-protobuf-message' \
    "http://127.0.0.1:${API_PORT}/write")"
[[ "$invalid_status" == "400" ]]
decode ixmati.v1.ErrorDetail "$PROTO_DIR/ixmati/v1/common.proto" \
    <"$TEST_DIR/invalid-error.pb" | tee "$TEST_DIR/invalid-error.txt"
grep -q 'error: "INVALID_ARGUMENT"' "$TEST_DIR/invalid-error.txt"

unauth_status="$(curl -sS --max-time 5 -o "$TEST_DIR/unauth-error.pb" -w '%{http_code}' \
    -X POST -H 'Content-Type: application/protobuf' \
    --data-binary @"$TEST_DIR/write.pb" \
    "http://127.0.0.1:${API_PORT}/write")"
[[ "$unauth_status" == "401" ]]
decode ixmati.v1.ErrorDetail "$PROTO_DIR/ixmati/v1/common.proto" \
    <"$TEST_DIR/unauth-error.pb" | tee "$TEST_DIR/unauth-error.txt"
grep -q 'error: "UNAUTHORIZED"' "$TEST_DIR/unauth-error.txt"

printf '[protobuf-e2e] gRPC unary + replay/live stream\n'
IXMATI_E2E_GRPC="http://127.0.0.1:${GRPC_PORT}" \
IXMATI_E2E_API_KEY="$API_KEY" \
    cargo test -p ixmati-api --test protobuf_e2e -- --nocapture

printf '[protobuf-e2e] PASS: REST/Protobuf y gRPC unary/stream validados\n'
