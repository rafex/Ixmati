#!/usr/bin/env bash
# Ejecuta una sobrecarga MQTT controlada y captura señales para distinguir
# desconexión, errores, deferred, ACK fallidos y publisher sin PUBACK.
# No reinicia ni mata servicios; conserva snapshots y journal como evidencia.
set -euo pipefail

CONTAINER_NAME="${CONTAINER_NAME:-ixmati-load-test}"
TEST_HOST="${TEST_HOST:-192.168.3.175}"
API_PORT="${API_PORT:-30300}"
METRICS_PORT="${METRICS_PORT:-30301}"
RATE="${RATE:-1000}"
DURATION="${DURATION:-90}"
CONCURRENCY="${CONCURRENCY:-500}"
TIMEOUT="${TIMEOUT:-5}"
STORE="${STORE:-default}"
RESULT_DIR="${RESULT_DIR:-/tmp/ixmati-mqtt-stall-$(date -u +%Y%m%dT%H%M%SZ)}"
STRACE="${STRACE:-0}"
STRACE_DURATION="${STRACE_DURATION:-30}"

mkdir -p "$RESULT_DIR"
LOAD_RESULT="$RESULT_DIR/load.json"
LOAD_ERROR="$RESULT_DIR/load.err"
SAMPLES="$RESULT_DIR/samples.tsv"
JOURNAL="$RESULT_DIR/writer-journal.log"
STRACE_LOG="$RESULT_DIR/writer-strace.log"
CONTAINER_STRACE_LOG="/tmp/ixmati-writer-strace-$$.log"

metric_value() {
  local text="$1" name="$2"
  printf '%s\n' "$text" | awk -v metric="$name" '$0 ~ ("^" metric "(\\{| )") { print $NF; exit }'
}

broker_stored() {
  podman exec "$CONTAINER_NAME" bash -lc \
    "timeout 2 mosquitto_sub -h localhost -p 1883 -t '\$SYS/broker/messages/stored' -C 1 2>/dev/null" || true
}

process_ticks() {
  local pid="$1"
  podman exec "$CONTAINER_NAME" awk '{print $14" "$15}' "/proc/${pid}/stat" 2>/dev/null || true
}

printf 'unix_seconds\tqueue_depth\tack_failures\teventloop_errors\tdeferred\tcommands_acked\tlast_commit\tpuback_timeouts\toutbox_attempts\tbroker_stored\twriter_ticks\n' > "$SAMPLES"

python3 helpers/python/rate_load.py "http://${TEST_HOST}:${API_PORT}/write" \
  --rate "$RATE" --duration "$DURATION" --concurrency "$CONCURRENCY" \
  --timeout "$TIMEOUT" --store "$STORE" >"$LOAD_RESULT" 2>"$LOAD_ERROR" &
load_pid=$!

strace_pid=""
if [ "$STRACE" = "1" ]; then
  writer_pid=$(podman exec "$CONTAINER_NAME" pgrep -xo ixmati-writer 2>/dev/null || true)
  if [ -n "$writer_pid" ] && podman exec "$CONTAINER_NAME" sh -lc 'command -v strace >/dev/null'; then
    podman exec -d "$CONTAINER_NAME" sh -lc \
      "timeout '${STRACE_DURATION}s' strace -f -tt -T -p '${writer_pid}' \
        -e trace=pread64,pwrite64,fdatasync,fsync,fcntl,sendto,recvfrom,epoll_wait,futex \
        -o '${CONTAINER_STRACE_LOG}'" >/dev/null
    strace_pid="attached:${writer_pid}"
  else
    printf 'strace=unavailable\n' > "$STRACE_LOG"
  fi
fi

while kill -0 "$load_pid" 2>/dev/null; do
  now=$(date +%s)
  metrics=$(curl -fsS "http://${TEST_HOST}:${METRICS_PORT}/metrics" 2>/dev/null || \
    podman exec "$CONTAINER_NAME" curl -fsS http://127.0.0.1:9464/metrics 2>/dev/null || true)
  pid=$(podman exec "$CONTAINER_NAME" pgrep -x ixmati-writer 2>/dev/null || true)
  ticks=""
  if [ -n "$pid" ]; then
    ticks=$(process_ticks "$pid")
  fi
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$now" \
    "$(metric_value "$metrics" ixmati_consumer_queue_depth)" \
    "$(metric_value "$metrics" ixmati_mqtt_ack_failures_total)" \
    "$(metric_value "$metrics" ixmati_mqtt_eventloop_errors)" \
    "$(metric_value "$metrics" ixmati_mqtt_commands_deferred_total)" \
    "$(metric_value "$metrics" ixmati_mqtt_commands_acked_total)" \
    "$(metric_value "$metrics" ixmati_last_batch_commit_unix_seconds)" \
    "$(metric_value "$metrics" ixmati_outbox_puback_timeouts_total)" \
    "$(metric_value "$metrics" ixmati_outbox_publish_attempts_total)" \
    "$(broker_stored)" "$ticks" >> "$SAMPLES"
  sleep 5
done
wait "$load_pid" || true

if [ -n "$strace_pid" ]; then
  podman exec "$CONTAINER_NAME" sh -lc \
    "cat '${CONTAINER_STRACE_LOG}' 2>/dev/null || true" > "$STRACE_LOG"
  podman exec "$CONTAINER_NAME" rm -f "$CONTAINER_STRACE_LOG" >/dev/null 2>&1 || true
fi

podman exec "$CONTAINER_NAME" journalctl -u "ixmati-writer@${STORE}" --no-pager -n 400 > "$JOURNAL" 2>&1 || true
printf 'result_dir=%s\nload_result=%s\nsamples=%s\njournal=%s\n' \
  "$RESULT_DIR" "$LOAD_RESULT" "$SAMPLES" "$JOURNAL"
if [ -n "$strace_pid" ]; then
  printf 'strace=%s\nstrace_log=%s\n' "$strace_pid" "$STRACE_LOG"
fi
cat "$LOAD_RESULT"
