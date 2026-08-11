#!/usr/bin/env bash
# helpers/shell/kill9_writer.sh — prueba de durabilidad durante un crash real.
#
# Ejecutar desde el host que tiene la conexión Podman al contenedor instalado:
#   CONTAINER_NAME=ixmati-load-test TEST_HOST=192.168.3.175 \
#     helpers/shell/kill9_writer.sh default 100
#
# MQTT QoS 1 y el outbox hacen que los duplicados sean posibles y aceptables;
# una escritura o evento perdido es fallo. El contenedor Debian mínimo no
# necesita el binario sqlite3: la verificación usa el módulo sqlite3 de Python.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/lib.sh"

STORE="${1:-default}"
N_MESSAGES="${2:-100}"
CONTAINER="${CONTAINER_NAME:-ixmati-load-test}"
TEST_HOST="${TEST_HOST:-192.168.3.175}"
API_PORT="${API_PORT:-30300}"
DB_PATH="${DB_PATH:-/var/lib/ixmati/stores/${STORE}.db}"
KILL_AFTER="${KILL_AFTER:-$((N_MESSAGES / 2))}"
PUBLISH_DELAY_MS="${PUBLISH_DELAY_MS:-10}"
RECOVERY_TIMEOUT="${RECOVERY_TIMEOUT:-60}"
OUT="${OUT:-/tmp/ixmati-kill9-${STORE}-$(date +%Y%m%dT%H%M%S).tsv}"
EVENTS="${OUT%.tsv}.events"
REQUESTS="${OUT%.tsv}.requests"

require podman "la prueba necesita acceso al contenedor remoto"
require curl "la prueba consulta el estado durable de la API"

if ! podman container exists "$CONTAINER"; then
    die "el contenedor $CONTAINER no existe"
fi
if (( N_MESSAGES < 2 )); then
    die "N_MESSAGES debe ser >= 2"
fi
if (( KILL_AFTER < 1 || KILL_AFTER >= N_MESSAGES )); then
    die "KILL_AFTER debe estar entre 1 y N_MESSAGES-1"
fi

mkdir -p "$(dirname "$OUT")"
: > "$EVENTS"
: > "$REQUESTS"
: > "$OUT"
printf 'idempotency_key\tentity_key\tapi_status\tsqlite_present\tevent_id\n' > "$OUT"

run_in_container() {
    podman exec "$CONTAINER" "$@"
}

sqlite_query() {
    local query="$1"
    run_in_container python3 -c '
import sqlite3
import sys

db = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
row = db.execute(sys.argv[2]).fetchone()
print("" if row is None or row[0] is None else row[0])
' "$DB_PATH" "$query"
}

log "crash durability: store=$STORE mensajes=$N_MESSAGES container=$CONTAINER"
run_in_container systemctl is-active --quiet "ixmati-writer@${STORE}" \
    || die "ixmati-writer@${STORE} no está activo"

writer_pid_before="$(run_in_container systemctl show -p MainPID --value "ixmati-writer@${STORE}")"
if [[ "$writer_pid_before" == "0" || -z "$writer_pid_before" ]]; then
    die "no se pudo obtener el PID del writer"
fi

# Capturar el bus antes de publicar: una línea contiene topic + envelope JSON.
run_in_container bash -c \
    "timeout $((RECOVERY_TIMEOUT + 30)) mosquitto_sub -q 1 -t 'ixmati/evt/${STORE}/#' -v" \
    > "$EVENTS" 2>&1 &
subscriber_pid=$!
cleanup() {
    kill "$subscriber_pid" 2>/dev/null || true
    wait "$subscriber_pid" 2>/dev/null || true
}
trap cleanup EXIT
sleep 1

run_id="kill9-$(date +%s)-$$"
declare -a keys
declare -a entity_keys

for i in $(seq 1 "$N_MESSAGES"); do
    key="${run_id}-key-${i}"
    entity_key="${run_id}-entity-${i}"
    keys+=("$key")
    entity_keys+=("$entity_key")
    printf '%s\t%s\n' "$key" "$entity_key" >> "$REQUESTS"
done

publish_message() {
    local i="$1"
    local key="${keys[$((i - 1))]}"
    local entity_key="${entity_keys[$((i - 1))]}"
    local payload
    payload=$(printf '{"op":"upsert","store":"%s","entity":"kill9","key":"%s","version":1,"ts":"2026-01-01T00:00:00Z","idempotency_key":"%s","ack_mode":"committed","payload":{"crash_test":"%s"}}' \
        "$STORE" "$entity_key" "$key" "$run_id")
    run_in_container mosquitto_pub -q 1 \
        -t "ixmati/cmd/${STORE}/kill9/${entity_key}" -m "$payload"
}

log "publicando $KILL_AFTER mensajes antes del crash"
for i in $(seq 1 "$KILL_AFTER"); do
    publish_message "$i"
done

log "publicando los mensajes restantes mientras se fuerza SIGKILL"
(
    for i in $(seq "$((KILL_AFTER + 1))" "$N_MESSAGES"); do
        publish_message "$i"
        sleep "0.${PUBLISH_DELAY_MS}s"
    done
) &
publisher_pid=$!
sleep "0.${PUBLISH_DELAY_MS}s"

writer_pid="$(run_in_container systemctl show -p MainPID --value "ixmati-writer@${STORE}")"
log "SIGKILL al writer PID=$writer_pid (PID inicial=$writer_pid_before)"
run_in_container systemctl kill -s SIGKILL --kill-who=main "ixmati-writer@${STORE}"
wait "$publisher_pid" 2>/dev/null || true

log "reiniciando ixmati-writer@${STORE} mediante systemd"
run_in_container systemctl restart "ixmati-writer@${STORE}"

deadline=$((SECONDS + RECOVERY_TIMEOUT))
while (( SECONDS < deadline )); do
    if run_in_container systemctl is-active --quiet "ixmati-writer@${STORE}"; then
        current_pid="$(run_in_container systemctl show -p MainPID --value "ixmati-writer@${STORE}")"
        if [[ "$current_pid" != "0" && "$current_pid" != "$writer_pid" ]]; then
            break
        fi
    fi
    sleep 1
done
run_in_container systemctl is-active --quiet "ixmati-writer@${STORE}" \
    || die "el writer no se recuperó en ${RECOVERY_TIMEOUT}s"

log "esperando confirmación durable de las claves"
pending=0
sqlite_count=0
while IFS=$'\t' read -r key entity_key; do
    status="$(curl -sS --max-time 5 "http://${TEST_HOST}:${API_PORT}/writes/${STORE}/${key}" || true)"
    if grep -q '"status":"APPLIED"' <<< "$status"; then
        api_status=APPLIED
    else
        api_status=PENDING
        pending=$((pending + 1))
    fi
    sqlite_present="$(sqlite_query \
        "SELECT COUNT(*) FROM _idempotency WHERE store='${STORE}' AND idempotency_key='${key}';" 2>/dev/null || echo 0)"
    sqlite_present="${sqlite_present//$'\n'/}"
    if [[ "$sqlite_present" == "1" ]]; then
        sqlite_count=$((sqlite_count + 1))
    fi
    event_id="$(sqlite_query \
        "SELECT o.event_id FROM _outbox o JOIN _idempotency i ON i.store=o.store AND i.entity=o.entity AND i.key=o.key WHERE i.store='${STORE}' AND i.idempotency_key='${key}' ORDER BY o.id DESC LIMIT 1;" 2>/dev/null || true)"
    event_id="${event_id//$'\n'/}"
    printf '%s\t%s\t%s\t%s\t%s\n' "$key" "$entity_key" "$api_status" \
        "$sqlite_present" "$event_id" >> "$OUT"
done < "$REQUESTS"

outbox_pending="$(sqlite_query \
    "SELECT COUNT(*) FROM _outbox o JOIN _idempotency i ON i.store=o.store AND i.entity=o.entity AND i.key=o.key WHERE i.store='${STORE}' AND i.idempotency_key LIKE '${run_id}-%' AND o.published_at IS NULL;" 2>/dev/null || echo 0)"
outbox_pending="${outbox_pending//$'\n'/}"

log "resultado: sqlite_applied=$sqlite_count/$N_MESSAGES api_pending=$pending outbox_pending=$outbox_pending"
if (( sqlite_count != N_MESSAGES || pending != 0 )); then
    err "se perdió o no se recuperó al menos una escritura; evidencia: $OUT"
    exit 1
fi

log "esperando eventos recuperados y comprobando el outbox"
sleep 3
missing_events=0
while IFS=$'\t' read -r key entity_key api_status sqlite_present event_id; do
    if [[ -z "$event_id" ]] || ! grep -Fq "$event_id" "$EVENTS"; then
        missing_events=$((missing_events + 1))
    fi
done < <(tail -n +2 "$OUT")

if (( missing_events != 0 )); then
    err "eventos no observados=$missing_events/$N_MESSAGES; evidencia: $EVENTS"
    exit 1
fi

outbox_pending_after="$(sqlite_query \
    "SELECT COUNT(*) FROM _outbox o JOIN _idempotency i ON i.store=o.store AND i.entity=o.entity AND i.key=o.key WHERE i.store='${STORE}' AND i.idempotency_key LIKE '${run_id}-%' AND o.published_at IS NULL;")"
outbox_pending_after="${outbox_pending_after//$'\n'/}"
ok "durabilidad verificada: $N_MESSAGES escrituras, $N_MESSAGES eventos, outbox pendiente=$outbox_pending_after"
log "manifiesto: $OUT"
log "eventos MQTT: $EVENTS"
