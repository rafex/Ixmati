#!/bin/bash
# 07-crash.sh — Kill writer y verificar recuperación
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/../shell-helpers.sh"

echo "=== Escenario 07: Crash Recovery — kill writer ==="
CONTAINER="ixmati-allinone"

echo "1. Escribiendo comando pre-crash..."
IDEM_PRE=$(uuidgen)
resp=$(ixmati_write "$IXMATI_STORE" "crash" "pre" 1 "accepted" "$IDEM_PRE" '{"data":"pre-crash"}')
echo "   $(echo "$resp" | python3 -c 'import sys,json; print(json.load(sys.stdin)["status"])' 2>/dev/null)"

echo "2. Matando writer (supervisor lo reinicia)..."
podman exec "$CONTAINER" pkill -9 ixmati-writer 2>/dev/null || \
    yellow "   no se pudo matar el writer (prueba manual: podman exec $CONTAINER pkill -9 ixmati-writer)"

echo "3. Esperando recuperación (10s)..."
sleep 10

echo "4. Escribiendo comando post-crash..."
IDEM_POST=$(uuidgen)
resp=$(ixmati_write "$IXMATI_STORE" "crash" "post" 1 "accepted" "$IDEM_POST" '{"data":"post-crash"}')
echo "   $(echo "$resp" | python3 -c 'import sys,json; print(json.load(sys.stdin)["status"])' 2>/dev/null)"

echo "5. Verificando ambos..."
sleep 3
for idem in "$IDEM_PRE" "$IDEM_POST"; do
    s=$(ixmati_status "$IXMATI_STORE" "$idem")
    st=$(echo "$s" | python3 -c "import sys,json; print(json.load(sys.stdin).get('status','ERROR'))" 2>/dev/null || echo "ERROR")
    if [ "$st" = "APPLIED" ]; then
        green "   $idem → APPLIED"
    else
        red "   $idem → $st"
    fi
done

echo ""
podman logs --tail=5 "$CONTAINER" 2>/dev/null | grep -E 'writer|error' || true
green "Crash recovery completado"
