#!/bin/bash
# 01-health.sh — Verificar health check del all-in-one
# Uso: ./01-health.sh
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/../shell-helpers.sh"

echo "=== Escenario 01: Health Check ==="
echo ""

ixmati_health
echo ""

overall=$(echo "$(ixmati_health)" | python3 -c "import sys,json; print(json.load(sys.stdin)['overall'])" 2>/dev/null || echo "FAIL")
components=$(echo "$(ixmati_health)" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d.get('components',[])))" 2>/dev/null || echo "0")

echo "Resultados:"
echo "  Overall:   $overall"
echo "  Componentes: $components"
echo ""

if [ "$overall" = "OK" ] && [ "$components" -ge 3 ]; then
    green "Health check completo: todos los componentes OK"
else
    red "Health check incompleto: overall=$overall componentes=$components"
    exit 1
fi
