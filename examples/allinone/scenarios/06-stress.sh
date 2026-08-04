#!/bin/bash
# 06-stress.sh — 100 writes concurrentes
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/../shell-helpers.sh"

N="${1:-100}"
echo "=== Escenario 06: Stress — $N writes ==="
echo "Enviando $N comandos..."

t0=$(date +%s.%N 2>/dev/null || date +%s)

for i in $(seq 1 "$N"); do
    curl -s -X POST "${API_BASE}/write" \
        -H "Content-Type: application/json" \
        -H "$AUTH_HEADER" \
        -d "{\"op\":\"upsert\",\"store\":\"${IXMATI_STORE}\",\"entity\":\"stress\",\"key\":\"s${i}\",\"version\":1,\"ts\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"idempotency_key\":\"$(uuidgen)\",\"ack_mode\":\"accepted\",\"payload\":{\"i\":${i}}}" \
        > /dev/null &
done
wait

t1=$(date +%s.%N 2>/dev/null || date +%s)
elapsed=$(echo "$t1 - $t0" | bc 2>/dev/null || echo "?")
rate=$(echo "scale=1; $N / $elapsed" | bc 2>/dev/null || echo "?")

echo "Enviados:      $N"
echo "Tiempo total:  ${elapsed}s"
echo "Throughput:    ${rate} writes/s"
echo ""

echo "Esperando que se procesen (máx 60s)..."
sleep 5

applied=0
pending=0
deadline=$(($(date +%s) + 55))
for i in $(seq 1 "$N"); do
    # Sampleamos cada 10 writes para no sobrecargar
    if [ $((i % 10)) -eq 0 ] || [ "$i" -le 5 ]; then
        s=$(curl -s "${API_BASE}/writes/${IXMATI_STORE}/N/A" 2>/dev/null || true)
    fi
done

final_check=$(curl -s "${API_BASE}/health" 2>/dev/null)
overall=$(echo "$final_check" | python3 -c "import sys,json; print(json.load(sys.stdin)['overall'])" 2>/dev/null || echo "FAIL")

if [ "$overall" = "OK" ]; then
    green "Sistema healthy tras $N writes"
else
    red "Sistema degradado tras stress: $overall"
fi

echo ""
echo "Verifica con: curl ${API_BASE}/health | python3 -m json.tool"
