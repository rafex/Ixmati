#!/bin/bash
# entrypoint.sh — inicializa el container all-in-one
# Genera defaults desde env vars y arranca supervisord

set -e

export STORE_NAME="${STORE_NAME:-default}"
export SQLITE_PATH="${SQLITE_PATH:-/var/lib/ixmati/stores/${STORE_NAME}.db}"
export IXMATI_API_KEYS="${IXMATI_API_KEYS:-ix-default-key}"
export API_PORT="${API_PORT:-30000}"
export GRPC_HOST="${GRPC_HOST:-0.0.0.0}"
export GRPC_PORT="${GRPC_PORT:-30100}"

# Crear directorios de datos si no existen
mkdir -p "$(dirname "$SQLITE_PATH")"
mkdir -p /var/lib/ixmati/cache
mkdir -p /var/run/ixmati
chown -R ixmati:ixmati /var/lib/ixmati /var/log/ixmati /var/run/ixmati 2>/dev/null || true

echo "[ixmati] all-in-one starting"
echo "  store:     ${STORE_NAME}"
echo "  db:        ${SQLITE_PATH}"
echo "  api port:  ${API_PORT}"
echo "  grpc port: ${GRPC_PORT}"
echo "  mqtt:      tcp://localhost:1883"

exec "$@"
