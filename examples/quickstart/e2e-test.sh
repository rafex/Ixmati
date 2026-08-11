#!/bin/bash
# e2e-test.sh — smoke test automatizado para ixmati-quickstart
# Uso: cd examples/quickstart && ./e2e-test.sh

set -euo pipefail

API="${API_URL:-http://localhost:8080}"
API_KEY="${API_KEY:-ix-quickstart-key}"
PASS=0
FAIL=0

red()   { echo -e "\033[31m$1\033[0m"; }
green() { echo -e "\033[32m$1\033[0m"; }

check() {
    local name="$1" expected="$2" actual="$3"
    if [ "$actual" = "$expected" ]; then
        green "  [PASS] $name"
        PASS=$((PASS + 1))
    else
        red "  [FAIL] $name (expected=$expected actual=$actual)"
        FAIL=$((FAIL + 1))
    fi
}

echo "=== Ixmati Quickstart E2E Test ==="
echo ""

# 1. Health check
health=$(curl -s "$API/health")
overall=$(echo "$health" | python3 -c "import sys,json; print(json.load(sys.stdin)['overall'])" 2>/dev/null || echo "FAIL")
check "Health check" "OK" "$overall"

# 2. POST /write sin auth → 401
code=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$API/write" \
    -H "Content-Type: application/json" \
    -d '{"op":"upsert","store":"pedidos","entity":"pedido","key":"e2e-1","version":1,"ts":"2026-07-30T00:00:00Z","idempotency_key":"00000000-0000-0000-0000-000000000001","ack_mode":"accepted","payload":{}}')
check "POST /write sin auth → 401" "401" "$code"

# 3. POST /write pedido 1
IDEM1="$(uuidgen)"
resp=$(curl -s -X POST "$API/write" \
    -H "Authorization: ApiKey $API_KEY" \
    -H "Content-Type: application/json" \
    -d "{\"op\":\"upsert\",\"store\":\"pedidos\",\"entity\":\"pedido\",\"key\":\"e2e-1\",\"version\":1,\"ts\":\"2026-07-30T00:00:00Z\",\"idempotency_key\":\"$IDEM1\",\"ack_mode\":\"accepted\",\"payload\":{\"total\":1500,\"estado\":\"pendiente\"}}")
status=$(echo "$resp" | python3 -c "import sys,json; print(json.load(sys.stdin)['status'])" 2>/dev/null || echo "FAIL")
check "POST /write e2e-1 → APPLIED" "APPLIED" "$status"

# 4. POST /write pedido 2
IDEM2="$(uuidgen)"
resp=$(curl -s -X POST "$API/write" \
    -H "Authorization: ApiKey $API_KEY" \
    -H "Content-Type: application/json" \
    -d "{\"op\":\"upsert\",\"store\":\"pedidos\",\"entity\":\"pedido\",\"key\":\"e2e-2\",\"version\":1,\"ts\":\"2026-07-30T00:00:00Z\",\"idempotency_key\":\"$IDEM2\",\"ack_mode\":\"accepted\",\"payload\":{\"total\":2500,\"estado\":\"confirmado\"}}")
status=$(echo "$resp" | python3 -c "import sys,json; print(json.load(sys.stdin)['status'])" 2>/dev/null || echo "FAIL")
check "POST /write e2e-2 → APPLIED" "APPLIED" "$status"

# 5. Wait for writer to process and query status
echo ""
echo "  [INFO] Esperando al writer (3s)..."
sleep 3

# Query e2e-1
resp=$(curl -s "$API/writes/pedidos/$IDEM1")
applied=$(echo "$resp" | python3 -c "import sys,json; print(json.load(sys.stdin).get('status','PENDING'))" 2>/dev/null || echo "PENDING")
check "GET /writes/pedidos/$IDEM1 → APPLIED" "APPLIED" "$applied"

# Query e2e-2
resp=$(curl -s "$API/writes/pedidos/$IDEM2")
applied=$(echo "$resp" | python3 -c "import sys,json; print(json.load(sys.stdin).get('status','PENDING'))" 2>/dev/null || echo "PENDING")
check "GET /writes/pedidos/$IDEM2 → APPLIED" "APPLIED" "$applied"

echo ""
echo "=== Resultado: $PASS/$((PASS + FAIL)) tests ==="
if [ "$FAIL" -gt 0 ]; then
    red "Algunos tests fallaron. Verifica los logs: docker compose logs"
    exit 1
else
    green "Todos los tests pasaron!"
fi
