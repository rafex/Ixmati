#!/bin/bash
# subscribe-events.sh — monitor de eventos MQTT en tiempo real
# Uso: ./subscribe-events.sh [--store default] [--host localhost] [--port 30200] [--topic custom/topic]
set -euo pipefail

STORE="${IXMATI_STORE:-default}"
HOST="${IXMATI_HOST:-localhost}"
PORT="${IXMATI_MQTT_PORT:-30200}"
TOPIC=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --store) STORE="$2"; shift 2 ;;
        --host) HOST="$2"; shift 2 ;;
        --port) PORT="$2"; shift 2 ;;
        --topic) TOPIC="$2"; shift 2 ;;
        *) echo "Uso: $0 [--store NAME] [--host HOST] [--port PORT] [--topic TOPIC]"; exit 1 ;;
    esac
done

TOPIC="${TOPIC:-ixmati/evt/${STORE}/#}"

echo "=== Ixmati Event Monitor ==="
echo "  Broker: ${HOST}:${PORT}"
echo "  Topic:  ${TOPIC}"
echo "  Ctrl+C para salir"
echo ""

if command -v mosquitto_sub &>/dev/null; then
    mosquitto_sub -h "$HOST" -p "$PORT" -t "$TOPIC" -q 1 -v | while read -r topic payload; do
        echo "[$(date +%H:%M:%S)] $topic"
        echo "$payload" | python3 -m json.tool 2>/dev/null || echo "  $payload"
        echo ""
    done
elif python3 -c "import paho.mqtt.client" 2>/dev/null; then
    python3 -c "
import json, uuid, paho.mqtt.client as mqtt, sys
from datetime import datetime

def on_connect(c, u, f, rc, p):
    if rc == 0:
        c.subscribe('${TOPIC}', qos=1)
        print(f'Conectado a ${HOST}:${PORT}', flush=True)

def on_message(c, u, msg):
    now = datetime.now().strftime('%H:%M:%S')
    print(f'[{now}] {msg.topic}')
    try:
        print(json.dumps(json.loads(msg.payload.decode()), indent=2))
    except:
        print(f'  {msg.payload.decode()}')
    print('')

c = mqtt.Client(client_id=f'ixmati-mon-{uuid.uuid4().hex[:8]}')
c.on_connect = on_connect
c.on_message = on_message
c.connect('${HOST}', int('${PORT}'))
c.loop_forever()
" 2>/dev/null
else
    echo "ERROR: Necesitas mosquitto-clients o paho-mqtt"
    echo "  apt install mosquitto-clients"
    echo "  pip install paho-mqtt"
    exit 1
fi
