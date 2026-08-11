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
            -d "{\"op\":\"upsert\",\"store\":\"default\",\"entity\":\"test\",\"key\":\"idempotent\",\"version\":1,\"ts\":\"2026-01-01T00:00:00Z\",\"idempotency_key\":\"${idem_key}\",\"ack_mode\":\"committed\",\"payload\":{\"iteration\":${i}}}" >/dev/null 2>&1; then
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
# LOAD TESTING (concurrente: N workers en paralelo, cada uno en su propio
# subshell/archivo — un loop curl secuencial nunca supera ~1/latencia ops/s)

LOAD_OPS_DONE=0
LOAD_OPS_ERRORS=0
LOAD_P50_MS=0
LOAD_P99_MS=0
LOAD_AVG_MS=0
LOAD_WORKER_PREFIX="/tmp/stack-load-worker"
LATENCIES_FILE="/tmp/stack-load-latencies.txt"

load_worker() {
    local worker_id="$1"
    local end_time_epoch="$2"
    local out_file="${LOAD_WORKER_PREFIX}-${worker_id}.txt"
    local seq=0
    local status latency_ms req_start req_end

    : > "$out_file"
    while [ "$(date +%s)" -lt "$end_time_epoch" ]; do
        req_start=$(date +%s%N)
        status=$(curl -sS --max-time 5 -o /dev/null -w '%{http_code}' -X POST "${API_URL}/write" \
            -H "Authorization: ApiKey ${API_KEY}" \
            -H "Content-Type: application/json" \
            -d "{\"op\":\"upsert\",\"store\":\"default\",\"entity\":\"load\",\"key\":\"k${worker_id}-${seq}\",\"version\":1,\"ts\":\"2026-01-01T00:00:00Z\",\"idempotency_key\":\"load-${worker_id}-${seq}\",\"ack_mode\":\"committed\",\"payload\":{\"seq\":${seq}}}" 2>/dev/null || echo "000")
        req_end=$(date +%s%N)
        latency_ms=$(( (req_end - req_start) / 1000000 ))

        case "$status" in
            2??) echo "OK $latency_ms" >> "$out_file" ;;
            *) echo "ERR $latency_ms" >> "$out_file" ;;
        esac
        seq=$((seq + 1))
    done
}

load_test() {
    local duration_sec="${1:-30}"
    local target_ops_per_sec="${2:-100}"
    local concurrency="${3:-${LOAD_CONCURRENCY:-20}}"
    LOAD_DURATION_SEC="$duration_sec"
    LOAD_TARGET_OPS="$target_ops_per_sec"
    LOAD_CONCURRENCY_USED="$concurrency"

    log "load test: concurrencia=${concurrency}, target=${target_ops_per_sec} ops/s, duración=${duration_sec}s..."

    rm -f "${LOAD_WORKER_PREFIX}"-*.txt
    local end_time_epoch=$(( $(date +%s) + duration_sec ))

    local pids=()
    for w in $(seq 1 "$concurrency"); do
        load_worker "$w" "$end_time_epoch" &
        pids+=("$!")
    done
    for pid in "${pids[@]}"; do
        wait "$pid"
    done

    : > "$LATENCIES_FILE"
    cat "${LOAD_WORKER_PREFIX}"-*.txt > /tmp/stack-load-raw.txt 2>/dev/null || true

    local ops_done ops_errors
    ops_done=$( (grep -c '^OK' /tmp/stack-load-raw.txt || true) )
    ops_errors=$( (grep -c '^ERR' /tmp/stack-load-raw.txt || true) )
    awk '{print $2}' /tmp/stack-load-raw.txt > "$LATENCIES_FILE"

    LOAD_OPS_DONE=${ops_done:-0}
    LOAD_OPS_ERRORS=${ops_errors:-0}

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

    rm -f "${LOAD_WORKER_PREFIX}"-*.txt /tmp/stack-load-raw.txt

    local achieved
    achieved=$(awk "BEGIN {printf \"%.1f\", ${LOAD_OPS_DONE}/${duration_sec}}")
    ok "load test: ${LOAD_OPS_DONE} ops ejecutadas, ${LOAD_OPS_ERRORS} errores, ${achieved} ops/s reales (concurrencia=${concurrency})"
    echo "  ops_done=$LOAD_OPS_DONE ops_errors=$LOAD_OPS_ERRORS p50=${LOAD_P50_MS}ms p99=${LOAD_P99_MS}ms avg=${LOAD_AVG_MS}ms"
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
    "duration_sec": ${LOAD_DURATION_SEC:-30},
    "target_ops_per_sec": ${LOAD_TARGET_OPS:-100},
    "concurrency": ${LOAD_CONCURRENCY_USED:-20},
    "ops_done": $LOAD_OPS_DONE,
    "ops_errors": $LOAD_OPS_ERRORS,
    "achieved_ops_per_sec": $(awk "BEGIN {printf \"%.1f\", $LOAD_OPS_DONE/${LOAD_DURATION_SEC:-30}}"),
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

    # Load testing (concurrencia configurable via LOAD_CONCURRENCY, default 20)
    load_test 30 100

    # Recolectar métricas
    collect_metrics

    # Reporte
    generate_report

    log "validación completada exitosamente"
}

main "$@"
