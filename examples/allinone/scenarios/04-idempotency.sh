#!/bin/bash
# 04-idempotency.sh — Misma idempotency_key no duplica
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/../shell-helpers.sh"

echo "=== Escenario 04: Idempotency — mismo key no duplica ==="
IDEM="ixmati-idem-$(date +%s)"

echo "Enviando 3 writes con misma idempotency_key..."
for i in 1 2 3; do
    resp=$(ixmati_write "$IXMATI_STORE" "idem" "id1" $i "accepted" "$IDEM" '{"data":"idem-test"}')
    echo "  Write #$i: $(echo "$resp" | python3 -c 'import sys,json; print(json.load(sys.stdin)["status"])' 2>/dev/null)"
    sleep 0.5
done

echo ""
echo "Esperando APPLIED..."
sleep 3

status=$(ixmati_status "$IXMATI_STORE" "$IDEM")
st=$(echo "$status" | python3 -c "import sys,json; print(json.load(sys.stdin).get('status','ERROR'))" 2>/dev/null || echo "ERROR")
ver=$(echo "$status" | python3 -c "import sys,json; print(json.load(sys.stdin).get('version','?'))" 2>/dev/null || echo "?")

if [ "$st" = "APPLIED" ]; then
    green "Idempotency OK: status=$st version=$ver (solo 1 aplicado)"
else
    red "Idempotency FAIL: status=$st"
    exit 1
fi
