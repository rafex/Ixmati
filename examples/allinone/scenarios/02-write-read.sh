#!/bin/bash
# 02-write-read.sh — Escribir comando y consultar status hasta APPLIED
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/../shell-helpers.sh"

echo "=== Escenario 02: Write → Status APPLIED ==="
IDEM=$(uuidgen)

echo "1. Enviando comando..."
resp=$(ixmati_write "$IXMATI_STORE" "test" "wr-1" 1 "accepted" "$IDEM" '{"total":100,"status":"pendiente"}')
echo "   Response: $resp"
status=$(echo "$resp" | python3 -c "import sys,json; print(json.load(sys.stdin)['status'])" 2>/dev/null)
[ "$status" = "ACCEPTED" ] && green "   Write ACCEPTED" || red "   Write FAILED"

echo ""
echo "2. Esperando APPLIED..."
deadline=$(($(date +%s) + 15))
while [ "$(date +%s)" -lt "$deadline" ]; do
    s=$(ixmati_status "$IXMATI_STORE" "$IDEM")
    st=$(echo "$s" | python3 -c "import sys,json; print(json.load(sys.stdin).get('status','PENDING'))" 2>/dev/null || echo "PENDING")
    if [ "$st" = "APPLIED" ]; then
        green "   Status: APPLIED"
        echo "   Full: $s"
        exit 0
    fi
    echo "   ... $st"
    sleep 1
done
red "Timeout: write no aplicado tras 15s"
exit 1
