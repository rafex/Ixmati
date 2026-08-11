#!/usr/bin/env bash
# helpers/wrk/staircase.sh — escalera de carga (DEC-0055/0058, P1.5) para
# encontrar el throughput sostenible real en vez de asumir un techo de
# commits/s aislado. Para cada escalón: fija MAX_WRITES_PER_WINDOW al
# valor objetivo (vía override de systemd en ixmati-api dentro del
# contenedor de test), corre wrk con ack_mode:committed (latencia real
# end-to-end, no solo "la API aceptó"), captura aceptadas/comprometidas/429
# + latencia + profundidad de cola antes/después de cada escalón.
#
# IMPORTANTE (ver DEC-0058): se prefiere wrk2 cuando está instalado, porque
# `-R` fija la tasa de llegada y evita confundir el techo del generador con
# el del sistema. Si sólo existe wrk, el script conserva una ruta de
# compatibilidad con concurrencia configurable, pero etiqueta el resultado
# como no rate-controlled.
#
# Uso: helpers/wrk/staircase.sh <host> <api_port> <writer_metrics_port>
#   ej: helpers/wrk/staircase.sh 192.168.3.175 30012 30013
# Requiere: un contenedor de test corriendo (nombre fijo abajo), con
# METRICS_PORT habilitado en ixmati-writer@default.
set -euo pipefail

HOST="${1:-127.0.0.1}"
API_PORT="${2:-30000}"
METRICS_PORT="${3:-9464}"
CONTAINER="${CONTAINER_NAME:-ixmati-load-test}"
DURATION="${DURATION:-30s}"
RATES=(20 40 60 80 100 150 200)
CONCURRENCY="${CONCURRENCY:-200}"

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${OUT:-/tmp/staircase-results.txt}"
: > "$OUT"

scrape_gauge() {
  # $1=url $2=metric_name -> suma todas las series (por store)
  # Si la serie aún no existe, devuelve NA. Nunca convierte ausencia de datos
  # en cero: eso falseaba la profundidad de cola de DEC-0058.
  curl -sS "$1/metrics" 2>/dev/null | awk -v metric="$2" '
    $0 ~ ("^" metric "(\\{| )") { sum += $NF; found = 1 }
    END { if (found) print sum; else print "NA" }
  '
}

if command -v wrk2 >/dev/null 2>&1; then
  LOAD_GENERATOR="wrk2"
elif command -v wrk >/dev/null 2>&1; then
  LOAD_GENERATOR="wrk"
else
  echo "ERROR: se requiere wrk2 o wrk" >&2
  exit 1
fi

echo "generator=$LOAD_GENERATOR concurrency=$CONCURRENCY duration=$DURATION" | tee -a "$OUT"

for rate in "${RATES[@]}"; do
  echo "=== escalon: ${rate}/s ===" | tee -a "$OUT"

  podman exec "$CONTAINER" bash -c "mkdir -p /etc/systemd/system/ixmati-api.service.d && cat > /etc/systemd/system/ixmati-api.service.d/override.conf <<EOF
[Service]
Environment=MAX_WRITES_PER_WINDOW=${rate}
Environment=THROTTLE_WINDOW_SECS=1
EOF
systemctl daemon-reload
systemctl restart ixmati-api"
  sleep 2

  outbox_before=$(scrape_gauge "http://${HOST}:${API_PORT}" "ixmati_outbox_size")
  qdepth_before=$(scrape_gauge "http://${HOST}:${METRICS_PORT}" "ixmati_consumer_queue_depth")

  if [[ "$LOAD_GENERATOR" == "wrk2" ]]; then
    result=$(wrk2 -t4 -c"$CONCURRENCY" -R"$rate" -d"$DURATION" --timeout 5s -s "$REPO/helpers/wrk/write_committed.lua" "http://${HOST}:${API_PORT}/write" 2>&1)
  else
    result=$(wrk -t4 -c"$CONCURRENCY" -d"$DURATION" --timeout 5s -s "$REPO/helpers/wrk/write_committed.lua" "http://${HOST}:${API_PORT}/write" 2>&1)
  fi
  echo "$result" | tee -a "$OUT"

  sleep 2
  outbox_after=$(scrape_gauge "http://${HOST}:${API_PORT}" "ixmati_outbox_size")
  qdepth_after=$(scrape_gauge "http://${HOST}:${METRICS_PORT}" "ixmati_consumer_queue_depth")

  echo "outbox_size before=$outbox_before after=$outbox_after | consumer_queue_depth before=$qdepth_before after=$qdepth_after" | tee -a "$OUT"
  echo "" | tee -a "$OUT"
done

echo "=== listo, resultados en $OUT ==="
