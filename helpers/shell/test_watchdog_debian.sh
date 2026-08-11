#!/usr/bin/env bash
# Valida el watchdog contra una pérdida controlada de progreso del writer.
# No pretende reproducir la causa MQTT: mantiene una transacción SQLite
# exclusiva para bloquear el commit, publica una orden y verifica que systemd
# reinicia el writer con código 42 y que la orden se recupera después.
set -euo pipefail

CONTAINER_NAME="${CONTAINER_NAME:-ixmati-load-test}"
TEST_HOST="${TEST_HOST:-192.168.3.175}"
API_PORT="${API_PORT:-30300}"
WATCHDOG_TIMEOUT_MS="${WATCHDOG_TIMEOUT_MS:-2500}"
LOCK_HOLD_SECONDS="${LOCK_HOLD_SECONDS:-7}"
STORE="${STORE:-default}"
DB_PATH="${DB_PATH:-/var/lib/ixmati/stores/${STORE}.db}"
SERVICE="ixmati-writer@${STORE}"
IDEM="watchdog-${STORE}-$(date -u +%Y%m%dT%H%M%SZ)"
LOCK_READY="/tmp/ixmati-watchdog-lock-ready"

exec_c() { podman exec "$CONTAINER_NAME" bash -c "$1"; }

cleanup() {
  exec_c "rm -f /etc/systemd/system/ixmati-writer@.service.d/91-watchdog.conf ${LOCK_READY}; systemctl daemon-reload; systemctl restart ${SERVICE}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

exec_c "test -S /var/run/ixmati/cache.sock && systemctl is-active --quiet ${SERVICE}"
before_restarts="$(exec_c "systemctl show ${SERVICE} -p NRestarts --value")"

exec_c "mkdir -p /etc/systemd/system/ixmati-writer@.service.d && printf '%s\\n' '[Service]' 'Environment=MQTT_WATCHDOG_TIMEOUT_MS=${WATCHDOG_TIMEOUT_MS}' > /etc/systemd/system/ixmati-writer@.service.d/91-watchdog.conf && systemctl daemon-reload && systemctl restart ${SERVICE}"

# El lock se mantiene el tiempo suficiente para superar el timeout del
# watchdog; al liberarse, la instancia reiniciada puede terminar el trabajo.
exec_c "rm -f ${LOCK_READY}; DB_PATH='${DB_PATH}' LOCK_READY='${LOCK_READY}' LOCK_HOLD_SECONDS='${LOCK_HOLD_SECONDS}' python3 -c 'import os,sqlite3,time; c=sqlite3.connect(os.environ[\"DB_PATH\"], timeout=1); c.execute(\"BEGIN EXCLUSIVE\"); open(os.environ[\"LOCK_READY\"],\"w\").close(); time.sleep(int(os.environ[\"LOCK_HOLD_SECONDS\"])); c.rollback(); c.close()' >/tmp/ixmati-watchdog-lock.log 2>&1 &"

for _ in $(seq 1 30); do
  if exec_c "test -f ${LOCK_READY}"; then break; fi
  sleep 0.2
done
exec_c "test -f ${LOCK_READY}"

payload="{\"op\":\"upsert\",\"store\":\"${STORE}\",\"entity\":\"watchdog\",\"key\":\"${IDEM}\",\"version\":1,\"ts\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"idempotency_key\":\"${IDEM}\",\"ack_mode\":\"committed\",\"payload\":{\"watchdog\":true}}"
curl -sS --max-time 8 -X POST "http://${TEST_HOST}:${API_PORT}/write" \
  -H 'Authorization: ApiKey ix-default-key' \
  -H 'Content-Type: application/json' \
  --data "$payload" >/tmp/ixmati-watchdog-write.json 2>/tmp/ixmati-watchdog-write.err || true

for _ in $(seq 1 30); do
  if exec_c "journalctl -u ${SERVICE} --no-pager -n 100 2>/dev/null | grep -q 'no durable progress'"; then break; fi
  sleep 0.5
done
exec_c "journalctl -u ${SERVICE} --no-pager -n 200 2>/dev/null | grep -q 'no durable progress'"
exec_c "journalctl -u ${SERVICE} --no-pager -n 200 2>/dev/null | grep -q 'status=42'"

for _ in $(seq 1 40); do
  current_restarts="$(exec_c "systemctl show ${SERVICE} -p NRestarts --value")"
  if [ "${current_restarts:-0}" -gt "${before_restarts:-0}" ]; then break; fi
  sleep 0.5
done
[ "${current_restarts:-0}" -gt "${before_restarts:-0}" ]

for _ in $(seq 1 30); do
  status="$(curl -sS "http://${TEST_HOST}:${API_PORT}/writes/${STORE}/${IDEM}" -H 'Authorization: ApiKey ix-default-key' || true)"
  if [[ "$status" == *'"status":"APPLIED"'* ]]; then break; fi
  sleep 0.5
done
[[ "$status" == *'"status":"APPLIED"'* ]]

printf 'watchdog=triggered service=%s before_restarts=%s after_restarts=%s idempotency=%s status=APPLIED\n' \
  "$SERVICE" "$before_restarts" "$current_restarts" "$IDEM"
