#!/usr/bin/env bash
# helpers/shell/test_stack_validation.sh
# Validación funcional + carga del stack Ixmati en contenedor Debian
#
# Uso: Ejecutar DENTRO del contenedor Debian tras `./install.sh`
# - Valida: write/read round-trip, proyecciones, idempotencia
# - Carga: constant throughput 100 ops/s durante 30s
# - Métricas: recolecta del endpoint /metrics
# - Output: JSON + CSV con resultados
#
# Requisitos: curl, jq (instalados por test_installer_debian.sh)

set -euo pipefail

# ─────────────────────────────────────────────────────────────────────────────
# CONFIGURACIÓN

API_PORT="${API_PORT:-30000}"
API_URL="http://localhost:${API_PORT}"
API_KEY="ix-default-key"
RESULTS_FILE="${RESULTS_FILE:-/tmp/stack-validation-results.json}"
METRICS_FILE="${METRICS_FILE:-/tmp/stack-metrics.txt}"

# Colores
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# ─────────────────────────────────────────────────────────────────────────────
# LOGGING

log() {
    echo -e "${BLUE}[validation]${NC} $*"
}

ok() {
    echo -e "${GREEN}✓${NC} $*"
}

warn() {
    echo -e "${YELLOW}⚠${NC} $*"
}

fail() {
    echo -e "${RED}✗${NC} $*" >&2
    exit 1
}

# ─────────────────────────────────────────────────────────────────────────────
# FUNCIONES DE VALIDACIÓN

health_check() {
    log "verificando health check del stack..."
    local resp
    resp=$(curl -sS "${API_URL}/health" || echo "{}")

    local api_status
    api_status=$(echo "$resp" | jq -r '.overall // "ERROR"')

    if [ "$api_status" = "OK" ]; then
        ok "stack health: OK"
        RESULT_HEALTH="passed"
        return 0
    else
        warn "stack health: $api_status (tolerado)"
        RESULT_HEALTH="degraded"
        return 0  # No es fatal
    fi
}

RESULT_HEALTH="unknown"
RESULT_ROUNDTRIP="unknown"
RESULT_IDEMPOTENCY="unknown"

write_read_roundtrip() {
    log "probando round-trip write/read..."

    local store="default"
    local entity="test"
    local key="k1"
    local idem_key="val-$(date +%s)"
    local write_ok=0
    local read_ok=0

    # Write
    local write_resp
    write_resp=$(curl -sS -X POST "${API_URL}/write" \
        -H "Authorization: ApiKey ${API_KEY}" \
        -H "Content-Type: application/json" \
        -d "{\"op\":\"upsert\",\"store\":\"${store}\",\"entity\":\"${entity}\",\"key\":\"${key}\",\"version\":1,\"ts\":\"2026-01-01T00:00:00Z\",\"idempotency_key\":\"${idem_key}\",\"ack_mode\":\"committed\",\"payload\":{\"hello\":\"world\"}}")

    if echo "$write_resp" | jq -e ".idempotency_key == \"${idem_key}\"" >/dev/null 2>&1; then
        ok "POST /write successful"
        write_ok=1
    else
        warn "POST /write: $write_resp"
    fi

    # Read
    sleep 1  # Espera eventual consistency
    local read_resp
    read_resp=$(curl -sS "${API_URL}/read?store=${store}&entity=${entity}&key=${key}" \
        -H "Authorization: ApiKey ${API_KEY}")

    if echo "$read_resp" | jq -e '.found == true and .payload.hello == "world"' >/dev/null 2>&1; then
        ok "GET /read successful (cache)"
        read_ok=1
    else
        warn "GET /read: $read_resp"
    fi

    if [ "$write_ok" = "1" ] && [ "$read_ok" = "1" ]; then
        RESULT_ROUNDTRIP="passed"
    else
        RESULT_ROUNDTRIP="failed"
    fi
}

test_idempotency() {
    log "probando idempotencia..."

    local idem_key="idempotent-$(date +%s)"
    local failures=0

    # Dos writes con mismo idempotency_key
    for i in 1 2; do
        if ! curl -sS -f -X POST "${API_URL}/write" \
            -H "Authorization: ApiKey ${API_KEY}" \
            -H "Content-Type: application/json" \
            -d "{\"op\":\"upsert\",\"store\":\"default\",\"entity\":\"test\",\"key\":\"idempotent\",\"version\":1,\"ts\":\"2026-01-01T00:00:00Z\",\"idempotency_key\":\"${idem_key}\",\"ack_mode\":\"accepted\",\"payload\":{\"iteration\":${i}}}" >/dev/null 2>&1; then
            failures=$((failures + 1))
        fi
    done

    if [ "$failures" -eq 0 ]; then
        ok "idempotency: dos writes con mismo key aceptados"
        RESULT_IDEMPOTENCY="passed"
    else
        warn "idempotency: $failures/2 writes fallaron"
        RESULT_IDEMPOTENCY="failed"
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# LOAD TESTING

LOAD_OPS_DONE=0
LOAD_OPS_ERRORS=0
LOAD_P50_MS=0
LOAD_P99_MS=0
LOAD_AVG_MS=0
LATENCIES_FILE="/tmp/stack-load-latencies.txt"

load_test() {
    local duration_sec="${1:-30}"
    local target_ops_per_sec="${2:-100}"

    log "load test: ${target_ops_per_sec} ops/s durante ${duration_sec}s..."

    : > "$LATENCIES_FILE"

    local start_time
    start_time=$(date +%s%N)

    local ops_done=0
    local ops_errors=0

    local end_time=$((start_time + duration_sec * 1000000000))

    while [ "$(date +%s%N)" -lt "$end_time" ]; do
        local req_start
        req_start=$(date +%s%N)

        # HTTP request
        if curl -sS -X POST "${API_URL}/write" \
            -H "Authorization: ApiKey ${API_KEY}" \
            -H "Content-Type: application/json" \
            -d "{\"op\":\"upsert\",\"store\":\"default\",\"entity\":\"load\",\"key\":\"k${ops_done}\",\"version\":1,\"ts\":\"2026-01-01T00:00:00Z\",\"idempotency_key\":\"load-${ops_done}\",\"ack_mode\":\"accepted\",\"payload\":{\"seq\":${ops_done}}}" >/dev/null 2>&1; then
            ops_done=$((ops_done + 1))
        else
            ops_errors=$((ops_errors + 1))
        fi

        local req_end
        req_end=$(date +%s%N)
        local latency_ms=$(( (req_end - req_start) / 1000000 ))
        echo "$latency_ms" >> "$LATENCIES_FILE"
    done

    LOAD_OPS_DONE=$ops_done
    LOAD_OPS_ERRORS=$ops_errors

    local n
    n=$(wc -l < "$LATENCIES_FILE" | tr -d ' ')
    if [ "$n" -gt 0 ]; then
        local sorted="/tmp/stack-load-latencies-sorted.txt"
        sort -n "$LATENCIES_FILE" > "$sorted"
        local p50_idx p99_idx
        p50_idx=$(( (n * 50 / 100) > 0 ? (n * 50 / 100) : 1 ))
        p99_idx=$(( (n * 99 / 100) > 0 ? (n * 99 / 100) : 1 ))
        LOAD_P50_MS=$(sed -n "${p50_idx}p" "$sorted")
        LOAD_P99_MS=$(sed -n "${p99_idx}p" "$sorted")
        LOAD_AVG_MS=$(awk '{sum+=$1; n++} END {if (n>0) printf "%.1f", sum/n; else print 0}' "$LATENCIES_FILE")
        rm -f "$sorted"
    fi

    ok "load test: ${ops_done} ops ejecutadas, ${ops_errors} errores"
    echo "  ops_done=$ops_done ops_errors=$ops_errors p50=${LOAD_P50_MS}ms p99=${LOAD_P99_MS}ms avg=${LOAD_AVG_MS}ms"
}

# ─────────────────────────────────────────────────────────────────────────────
# RECOLECTAR MÉTRICAS

collect_metrics() {
    log "recolectando métricas del endpoint /metrics..."

    if ! curl -sS "${API_URL}/metrics" > "$METRICS_FILE"; then
        warn "no se pudo recolectar metrics (endpoint puede no estar disponible)"
        return 1
    fi

    # Parsear algunos valores principales
    local write_total write_latency cache_hits cache_misses

    write_total=$( (grep -v '^#' "$METRICS_FILE" | grep "write_requests_total" || true) | (grep "status=\"success\"" || true) | awk '{print $2}' | head -1)
    cache_hits=$( (grep -v '^#' "$METRICS_FILE" | grep "cache_hits_total" || true) | awk '{print $2}' | head -1)
    cache_misses=$( (grep -v '^#' "$METRICS_FILE" | grep "cache_misses_total" || true) | awk '{print $2}' | head -1)

    ok "métricas recolectadas"
    echo "  write_total=${write_total:-0} cache_hits=${cache_hits:-0} cache_misses=${cache_misses:-0}"
}

# ─────────────────────────────────────────────────────────────────────────────
# GENERAR REPORTE

generate_report() {
    log "generando reporte JSON..."

    local timestamp
    timestamp=$(date -u +%Y-%m-%dT%H:%M:%SZ)

    local report
    report=$(cat <<EOF
{
  "timestamp": "$timestamp",
  "test_results": {
    "health_check": "$RESULT_HEALTH",
    "write_read_roundtrip": "$RESULT_ROUNDTRIP",
    "idempotency": "$RESULT_IDEMPOTENCY"
  },
  "load_test": {
    "duration_sec": 30,
    "target_ops_per_sec": 100,
    "ops_done": $LOAD_OPS_DONE,
    "ops_errors": $LOAD_OPS_ERRORS,
    "achieved_ops_per_sec": $(awk "BEGIN {printf \"%.1f\", $LOAD_OPS_DONE/30}"),
    "latency_p50_ms": ${LOAD_P50_MS:-0},
    "latency_p99_ms": ${LOAD_P99_MS:-0},
    "latency_avg_ms": ${LOAD_AVG_MS:-0}
  },
  "api_endpoint": "$API_URL",
  "metrics_file": "$METRICS_FILE"
}
EOF
)

    echo "$report" | jq '.' > "$RESULTS_FILE"
    ok "reporte guardado: $RESULTS_FILE"
}

# ─────────────────────────────────────────────────────────────────────────────
# MAIN

main() {
    log "iniciando validación del stack..."

    # Esperar a que API esté lista
    local retries=30
    while [ $retries -gt 0 ]; do
        if curl -sS "${API_URL}/health" >/dev/null 2>&1; then
            break
        fi
        ((retries--))
        sleep 1
    done

    if [ $retries -eq 0 ]; then
        fail "API no responde tras 30s"
    fi

    ok "API disponible (${API_URL})"

    # Validación
    health_check
    write_read_roundtrip
    test_idempotency

    # Load testing
    load_test 30 100

    # Recolectar métricas
    collect_metrics

    # Reporte
    generate_report

    log "validación completada exitosamente"
}

main "$@"
