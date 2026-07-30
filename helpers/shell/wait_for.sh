#!/usr/bin/env bash
# helpers/shell/wait_for.sh — espera hasta que un puerto este disponible

set -euo pipefail

HOST="${1:-localhost}"
PORT="${2:-1883}"
TIMEOUT="${3:-10}"
DESCRIPTION="${4:-servicio}"

elapsed=0
while [ $elapsed -lt "$TIMEOUT" ]; do
    if nc -z "$HOST" "$PORT" 2>/dev/null; then
        echo "[wait_for] $DESCRIPTION disponible en $HOST:$PORT (${elapsed}s)"
        exit 0
    fi
    sleep 1
    elapsed=$((elapsed + 1))
done

echo "[wait_for] ERROR: $DESCRIPTION no respondio en $HOST:$PORT tras ${TIMEOUT}s"
exit 1
