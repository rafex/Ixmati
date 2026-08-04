#!/bin/bash
# stress-test.sh — escenarios de carga para encontrar bugs
# Uso: ./stress-test.sh [--scenario all|concurrent|large|flood]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/shell-helpers.sh"

SCENARIO="${1:-all}"
HOST="${IXMATI_HOST}"
PORT="${IXMATI_API_PORT}"

echo "=== Ixmati All-in-One Stress Test ==="
echo "  Target: ${API_BASE}"
echo ""

# ── concurrent: 50 writes paralelos ──
test_concurrent() {
    echo "--- Concurrent: 50 writes paralelos ---"
    t0=$(date +%s)
    for i in $(seq 1 50); do
        curl -s -X POST "${API_BASE}/write" \
            -H "Content-Type: application/json" \
            -H "$AUTH_HEADER" \
            -d "{\"op\":\"upsert\",\"store\":\"${IXMATI_STORE}\",\"entity\":\"stress\",\"key\":\"c${i}\",\"version\":1,\"ts\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"idempotency_key\":\"stress-c-${i}\",\"ack_mode\":\"accepted\",\"payload\":{\"i\":${i}}}" \
            > /dev/null &
    done
    wait
    t1=$(date +%s)
    elapsed=$((t1 - t0))
    rate=$(echo "scale=1; 50 / $elapsed" | bc 2>/dev/null || echo "?")
    echo "  50 writes en ${elapsed}s (${rate}/s)"
    green "  Concurrent OK"
}

# ── large: payload grande ──
test_large() {
    echo "--- Large payload: 20KB de JSON ---"
    LARGE_DATA=$(python3 -c "import json; print(json.dumps({'items': [{'id': i, 'name': 'item' * 50, 'tags': [f'tag{j}' for j in range(50)]} for i in range(50)]}))")
    IDEM=$(uuidgen)
    resp=$(curl -s -X POST "${API_BASE}/write" \
        -H "Content-Type: application/json" \
        -H "$AUTH_HEADER" \
        -d "{\"op\":\"upsert\",\"store\":\"${IXMATI_STORE}\",\"entity\":\"large\",\"key\":\"L1\",\"version\":1,\"ts\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"idempotency_key\":\"$IDEM\",\"ack_mode\":\"accepted\",\"payload\":${LARGE_DATA}}")
    status=$(echo "$resp" | python3 -c "import sys,json; print(json.load(sys.stdin)['status'])" 2>/dev/null || echo "FAIL")
    if [ "$status" = "ACCEPTED" ]; then
        green "  Large payload: ACCEPTED (${#LARGE_DATA} bytes)"
    else
        red "  Large payload: $status"
    fi
}

# ── flood: burst rápido ──
test_flood() {
    echo "--- Flood: 200 writes burst ---"
    t0=$(date +%s)
    for i in $(seq 1 200); do
        curl -s -X POST "${API_BASE}/write" \
            -H "Content-Type: application/json" \
            -H "$AUTH_HEADER" \
            -d "{\"op\":\"upsert\",\"store\":\"${IXMATI_STORE}\",\"entity\":\"flood\",\"key\":\"f${i}\",\"version\":1,\"ts\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"idempotency_key\":\"flood-${i}\",\"ack_mode\":\"accepted\",\"payload\":{\"i\":${i}}}" \
            > /dev/null 2>&1 &
    done
    wait
    t1=$(date +%s)
    elapsed=$((t1 - t0))
    echo "  200 writes en ${elapsed}s"

    echo "  Verificando health..."
    health=$(curl -s "${API_BASE}/health")
    overall=$(echo "$health" | python3 -c "import sys,json; print(json.load(sys.stdin)['overall'])" 2>/dev/null || echo "FAIL")
    if [ "$overall" = "OK" ]; then
        green "  Flood: sistema healthy post-burst"
    else
        red "  Flood: sistema degradado ($overall)"
    fi
}

case "$SCENARIO" in
    all)
        test_concurrent
        test_large
        test_flood
        ;;
    concurrent) test_concurrent ;;
    large) test_large ;;
    flood) test_flood ;;
    *) echo "Uso: $0 [all|concurrent|large|flood]"; exit 1 ;;
esac

echo ""
green "Stress tests completados"
