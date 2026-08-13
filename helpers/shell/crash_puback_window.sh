#!/usr/bin/env bash
# Fuerza SIGKILL exactamente después de PUBACK y antes de published_at.
#
# El failpoint sólo existe cuando IXMATI_TEST_MODE=1. El override temporal de
# systemd se elimina antes del restart para que la barrera no pueda afectar al
# proceso recuperado.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib.sh"

STORE="${1:-default}"
N_MESSAGES="${2:-20}"
CONTAINER="${CONTAINER_NAME:-ixmati-load-test}"
TEST_HOST="${TEST_HOST:-192.168.3.175}"
API_PORT="${API_PORT:-30300}"
API_KEY="${API_KEY:-ix-default-key}"
DB_PATH="${DB_PATH:-/var/lib/ixmati/stores/${STORE}.db}"
TIMEOUT="${TIMEOUT:-60}"
RUN_ID="puback-window-$(date +%s)-$$"
OUT="${OUT:-/tmp/ixmati-${RUN_ID}.tsv}"
EVENTS="${OUT%.tsv}.events"
BARRIER="/run/ixmati/${STORE}-${RUN_ID}.barrier.json"
OVERRIDE_DIR="/etc/systemd/system/ixmati-writer@${STORE}.service.d"
OVERRIDE_FILE="${OVERRIDE_DIR}/90-crash-puback-window.conf"

require podman "la prueba necesita acceso al contenedor remoto"
require curl "la prueba consulta el estado durable de la API"
podman container exists "$CONTAINER" || die "el contenedor $CONTAINER no existe"
(( N_MESSAGES > 0 )) || die "N_MESSAGES debe ser positivo"

run_in_container() { podman exec "$CONTAINER" "$@"; }
sqlite_query() {
    run_in_container python3 -c '
import sqlite3, sys
db = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
print(db.execute(sys.argv[2]).fetchone()[0] or 0)
' "$DB_PATH" "$1"
}

mkdir -p "$(dirname "$OUT")"
: > "$EVENTS"
printf 'idempotency_key\tentity_key\tapi_status\tidempotency_applied_at\tevent_id\tevent_occurrences\n' > "$OUT"

cleanup() {
    kill "${subscriber_pid:-}" 2>/dev/null || true
    wait "${subscriber_pid:-}" 2>/dev/null || true
    run_in_container rm -f "$BARRIER" 2>/dev/null || true
    run_in_container rm -f "$OVERRIDE_FILE" 2>/dev/null || true
    run_in_container systemctl daemon-reload 2>/dev/null || true
}
trap cleanup EXIT

run_in_container systemctl is-active --quiet "ixmati-writer@${STORE}" \
    || die "ixmati-writer@${STORE} no está activo"

log "instalando failpoint temporal para store=$STORE mensajes=$N_MESSAGES"
run_in_container mkdir -p "$OVERRIDE_DIR" /run/ixmati
run_in_container sh -c "printf '%s\\n' '[Service]' 'Environment=IXMATI_TEST_MODE=1' 'Environment=IXMATI_TEST_PUBACK_BARRIER=${BARRIER}' > '${OVERRIDE_FILE}'"
run_in_container rm -f "$BARRIER"
run_in_container systemctl daemon-reload
run_in_container systemctl restart "ixmati-writer@${STORE}"
sleep 2

run_in_container sh -c \
    "timeout $((TIMEOUT + 20)) mosquitto_sub -q 1 -t 'ixmati/evt/${STORE}/#' -v" \
    > "$EVENTS" 2>&1 &
subscriber_pid=$!
sleep 1

declare -a keys=()
for i in $(seq 1 "$N_MESSAGES"); do
    key="${RUN_ID}-key-${i}"
    entity_key="${RUN_ID}-entity-${i}"
    keys+=("$key")
    payload=$(printf '{"op":"upsert","store":"%s","entity":"puback_window","key":"%s","version":1,"ts":"2026-01-01T00:00:00Z","idempotency_key":"%s","ack_mode":"committed","payload":{"run":"%s","n":%s}}' \
        "$STORE" "$entity_key" "$key" "$RUN_ID" "$i")
    run_in_container mosquitto_pub -q 1 \
        -t "ixmati/cmd/${STORE}/puback_window/${entity_key}" -m "$payload"
    printf '%s\t%s\n' "$key" "$entity_key" >> "${OUT%.tsv}.requests"
done

log "esperando manifiesto atómico de PUBACK antes de published_at"
deadline=$((SECONDS + TIMEOUT))
while (( SECONDS < deadline )); do
    if run_in_container test -s "$BARRIER"; then
        break
    fi
    sleep 1
done
run_in_container test -s "$BARRIER" \
    || die "no se alcanzó la barrera PUBACK en ${TIMEOUT}s; revisar eventos y logs"
run_in_container cat "$BARRIER" > "${OUT%.tsv}.barrier.json"
log "barrera alcanzada; terminando writer con SIGKILL"
run_in_container systemctl kill -s SIGKILL --kill-who=main "ixmati-writer@${STORE}"
run_in_container rm -f "$BARRIER"

# Es obligatorio quitar el override antes de que systemd vuelva a arrancar el
# writer. Así la recuperación prueba el binario normal y no el failpoint.
run_in_container rm -f "$OVERRIDE_FILE"
run_in_container systemctl daemon-reload
run_in_container systemctl restart "ixmati-writer@${STORE}"

deadline=$((SECONDS + TIMEOUT))
while (( SECONDS < deadline )); do
    if run_in_container systemctl is-active --quiet "ixmati-writer@${STORE}"; then
        break
    fi
    sleep 1
done
run_in_container systemctl is-active --quiet "ixmati-writer@${STORE}" \
    || die "el writer no se recuperó"

log "verificando idempotencia, APPLIED, outbox y eventos"
missing=0
while IFS=$'\t' read -r key entity_key; do
    api_status="$(curl -fsS --max-time 5 -H "Authorization: ApiKey ${API_KEY}" "http://${TEST_HOST}:${API_PORT}/writes/${STORE}/${key}" || true)"
    if grep -q '"status":"APPLIED"' <<< "$api_status"; then api=APPLIED; else api=PENDING; fi
    idem="$(sqlite_query "SELECT applied_at FROM _idempotency WHERE store='${STORE}' AND idempotency_key='${key}';")"
    event_id="$(sqlite_query "SELECT event_id FROM _outbox WHERE store='${STORE}' AND entity='puback_window' AND key='${entity_key}' ORDER BY id DESC LIMIT 1;")"
    occurrences="$(grep -F -o "$event_id" "$EVENTS" 2>/dev/null | wc -l | tr -d ' ')"
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$key" "$entity_key" "$api" "$idem" "$event_id" "$occurrences" >> "$OUT"
    if [[ "$api" != APPLIED || -z "$idem" || "$idem" == 0 || -z "$event_id" || "$occurrences" -lt 1 ]]; then
        missing=$((missing + 1))
    fi
done < "${OUT%.tsv}.requests"

outbox_pending="$(sqlite_query "SELECT COUNT(*) FROM _outbox WHERE store='${STORE}' AND entity='puback_window' AND event_id LIKE '${RUN_ID}%' AND published_at IS NULL;")"
duplicates="$(awk -F '\t' 'NR > 1 {sum += ($6 > 1 ? $6 - 1 : 0)} END {print sum + 0}' "$OUT")"
if (( missing != 0 )); then
    die "${missing}/${N_MESSAGES} claves no se recuperaron; evidencia=$OUT eventos=$EVENTS"
fi
ok "ventana PUBACK verificada: ${N_MESSAGES}/${N_MESSAGES} APPLIED, outbox_pending=${outbox_pending}, duplicados_observados=${duplicates}"
log "evidencia: $OUT, ${OUT%.tsv}.barrier.json, $EVENTS"
