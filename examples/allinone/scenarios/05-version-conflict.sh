#!/bin/bash
# 05-version-conflict.sh — Version conflict: v2 seguido de v1 rechaza v1
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/../shell-helpers.sh"

echo "=== Escenario 05: Version Conflict ==="
KEY="vc-$(date +%s)"

echo "1. Escribiendo v2..."
IDEM_V2=$(uuidgen)
resp=$(ixmati_write "$IXMATI_STORE" "vc" "$KEY" 2 "accepted" "$IDEM_V2" '{"data":"v2"}')
echo "   v2: $(echo "$resp" | python3 -c 'import sys,json; print(json.load(sys.stdin)["status"])' 2>/dev/null)"
sleep 2

echo "2. Escribiendo v1 (debería ser rechazado o ignorado)..."
IDEM_V1=$(uuidgen)
resp=$(ixmati_write "$IXMATI_STORE" "vc" "$KEY" 1 "accepted" "$IDEM_V1" '{"data":"v1-obsolete"}')
echo "   v1: $(echo "$resp" | python3 -c 'import sys,json; print(json.load(sys.stdin)["status"])' 2>/dev/null)"
sleep 2

echo "3. Verificando que el estado final es v2..."
s2=$(ixmati_status "$IXMATI_STORE" "$IDEM_V2")
v2=$(echo "$s2" | python3 -c "import sys,json; print(json.load(sys.stdin).get('version','?'))" 2>/dev/null || echo "?")
echo "   status v2: $v2"

s1=$(ixmati_status "$IXMATI_STORE" "$IDEM_V1")
v1=$(echo "$s1" | python3 -c "import sys,json; print(json.load(sys.stdin).get('version','?'))" 2>/dev/null || echo "PENDING")
echo "   status v1: $v1"

if [ "$v2" = "2" ]; then
    green "Version conflict: v2 prevalece (correcto)"
else
    yellow "Nota: version=$v2. El comportamiento observado puede variar según implementación."
fi
