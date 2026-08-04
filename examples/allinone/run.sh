#!/bin/bash
# run.sh — construye y levanta el all-in-one
# Uso: ./run.sh [opciones]
set -euo pipefail

API_PORT="${IXMATI_API_PORT:-30080}"
MQTT_PORT="${IXMATI_MQTT_PORT:-30200}"
STORE_NAME="${IXMATI_STORE:-default}"
API_KEY="${IXMATI_API_KEY:-smoke-test-key}"
HOST_IP="${IXMATI_HOST:-192.168.3.175}"
VOLUME="${IXMATI_VOLUME:-ixmati-allinone-data}"
BUILD=true

while [[ $# -gt 0 ]]; do
    case "$1" in
        --port) API_PORT="$2"; shift 2 ;;
        --mqtt-port) MQTT_PORT="$2"; shift 2 ;;
        --store) STORE_NAME="$2"; shift 2 ;;
        --key) API_KEY="$2"; shift 2 ;;
        --host-ip) HOST_IP="$2"; shift 2 ;;
        --volume) VOLUME="$2"; shift 2 ;;
        --no-build) BUILD=false; shift ;;
        *) echo "Opción desconocida: $1"; exit 1 ;;
    esac
done

CONTAINER="ixmati-allinone"
REPO_ROOT="$(cd "$(dirname "$0")" && git rev-parse --show-toplevel 2>/dev/null || cd "$(dirname "$0")/../.." && pwd)"

red()   { echo -e "\033[31m$1\033[0m"; }
green() { echo -e "\033[32m$1\033[0m"; }
blue()  { echo -e "\033[34m$1\033[0m"; }

if podman ps -a --format '{{.Names}}' | grep -qx "$CONTAINER" 2>/dev/null; then
    blue "[run] removiendo contenedor previo..."
    podman rm -f "$CONTAINER" >/dev/null 2>&1
fi

if $BUILD; then
    blue "[run] construyendo imagen..."
    podman build -f "$REPO_ROOT/containers/allinone/Containerfile" \
        -t localhost/ixmati-allinone:local "$REPO_ROOT"
else
    blue "[run] saltando build (--no-build)"
fi

blue "[run] iniciando contenedor..."
podman run -d --name "$CONTAINER" \
    -p "${HOST_IP}:${API_PORT}:30000" \
    -p "${HOST_IP}:${MQTT_PORT}:1883" \
    -v "${VOLUME}:/var/lib/ixmati:U" \
    -e STORE_NAME="$STORE_NAME" \
    -e IXMATI_API_KEYS="$API_KEY" \
    -e API_PORT="30000" \
    localhost/ixmati-allinone:local

blue "[run] esperando health check (timeout 60s)..."
deadline=$(($(date +%s) + 60))
ok=false
while [ "$(date +%s)" -lt "$deadline" ]; do
    if curl -sf "http://${HOST_IP}:${API_PORT}/health" >/dev/null 2>&1; then
        ok=true
        green "[run] health check OK"
        break
    fi
    sleep 1
done

if ! $ok; then
    red "[run] WARN: health check no respondió en 60s"
    echo "  Logs: podman logs $CONTAINER"
fi

curl -s "http://${HOST_IP}:${API_PORT}/health" | python3 -m json.tool 2>/dev/null || true

echo ""
green "=== Ixmati All-in-One listo ==="
echo "  API:      http://${HOST_IP}:${API_PORT}"
echo "  MQTT:     ${HOST_IP}:${MQTT_PORT}"
echo "  Store:    ${STORE_NAME}"
echo "  API key:  ${API_KEY}"
echo ""
echo "  Health:   curl http://localhost:${API_PORT}/health"
echo "  Logs:     podman logs -f $CONTAINER"
echo "  Stop:     ./stop.sh"

export IXMATI_HOST="$HOST_IP"
export IXMATI_API_PORT="$API_PORT"
export IXMATI_MQTT_PORT="$MQTT_PORT"
export IXMATI_API_KEY="$API_KEY"
export IXMATI_STORE="$STORE_NAME"
