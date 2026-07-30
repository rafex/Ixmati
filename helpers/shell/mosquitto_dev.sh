#!/usr/bin/env bash
# helpers/shell/mosquitto_dev.sh — broker Mosquitto para desarrollo local

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib.sh"

MOSQUITTO_DATA="${MOSQUITTO_DATA:-/tmp/ixmati-mosquitto}"

case "${1:-start}" in
    start)
        log "iniciando Mosquitto dev (persistence + QoS 1)"
        mkdir -p "$MOSQUITTO_DATA"/{data,log}
        mosquitto -d \
            -p 1883 \
            --persistence true \
            --persistence_location "$MOSQUITTO_DATA/data" \
            --log_dest file "$MOSQUITTO_DATA/log/mosquitto.log"
        sleep 1
        "$SCRIPT_DIR/wait_for.sh" localhost 1883 5 "Mosquitto"
        ok "Mosquitto corriendo en :1883"
        ;;
    stop)
        log "deteniendo Mosquitto dev"
        pkill -f "mosquitto.*1883" || true
        ok "Mosquitto detenido"
        ;;
    clean)
        log "limpiando datos de Mosquitto dev"
        pkill -f "mosquitto.*1883" || true
        rm -rf "$MOSQUITTO_DATA"
        ok "Mosquitto limpio ($MOSQUITTO_DATA)"
        ;;
    *)
        echo "uso: $0 {start|stop|clean}"
        exit 1
        ;;
esac
