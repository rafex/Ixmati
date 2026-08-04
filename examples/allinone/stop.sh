#!/bin/bash
# stop.sh — detiene y limpia el all-in-one
# Uso: ./stop.sh [--purge]
set -euo pipefail

CONTAINER="ixmati-allinone"
PURGE=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --purge) PURGE=true; shift ;;
        *) echo "Uso: ./stop.sh [--purge]"; exit 1 ;;
    esac
done

echo "[stop] deteniendo $CONTAINER..."
podman stop "$CONTAINER" 2>/dev/null || echo "  (ya estaba detenido)"
podman rm "$CONTAINER" 2>/dev/null || echo "  (ya estaba removido)"

if $PURGE; then
    echo "[stop] purgando volumen..."
    podman volume rm "${IXMATI_VOLUME:-ixmati-allinone-data}" 2>/dev/null || \
        echo "  (volumen no encontrado)"
fi

echo "[stop] listo."
