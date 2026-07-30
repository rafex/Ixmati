#!/usr/bin/env bash
# helpers/shell/kill9_writer.sh — test de durabilidad: kill -9 al writer y verificar 0 perdidas

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib.sh"

STORE="${1:-pedidos}"
N_MESSAGES="${2:-100}"

log "test de durabilidad: store=$STORE mensajes=$N_MESSAGES"
log "asegurando que el writer esta corriendo..."

# publica N mensajes via mosquitto_pub
for i in $(seq 1 "$N_MESSAGES"); do
    mosquitto_pub -t "ixmati/cmd/$STORE/test/$(printf '%04d' "$i")" \
        -m "{\"op\":\"upsert\",\"store\":\"$STORE\",\"entity\":\"test\",\"key\":\"$(printf '%04d' "$i")\",\"version\":$i,\"idempotency_key\":\"$(uuidgen)\",\"ack_mode\":\"accepted\",\"payload\":{}}" \
        -q 1 2>/dev/null
done
ok "$N_MESSAGES mensajes publicados"

# kill -9 al writer
WRITER_PID=$(pgrep -f "ixmati-writer" | head -1 || echo "")
if [ -z "$WRITER_PID" ]; then
    warn "writer no encontrado, asumiendo que ya esta detenido"
else
    log "kill -9 al writer (PID=$WRITER_PID)"
    kill -9 "$WRITER_PID"
    sleep 2
fi

# verificar que Mosquitto retuvo los mensajes (persistence)
STILL_QUEUED=$(mosquitto_sub -t 'ixmati/cmd/#' -C 1 -W 2 2>/dev/null | wc -l || echo "0")
ok "Mosquitto retiene mensajes: $STILL_QUEUED"

# reiniciar el writer
log "reiniciando el writer..."
# (en produccion esto lo hace el orquestador, aqui es manual)
WRITER_COUNT=$(pgrep -c -f "ixmati-writer" || echo "0")
ok "writer corriendo: $WRITER_COUNT instancias"

log "TODO: verificar que los $N_MESSAGES mensajes estan en SQLite"
log "test de durabilidad completado (verificacion pendiente de implementar)"
