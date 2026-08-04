#!/bin/bash
# 03-outbox.sh — Write produce evento MQTT (outbox transaccional)
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/../shell-helpers.sh"

echo "=== Escenario 03: Outbox — write produce evento MQTT ==="
IDEM=$(uuidgen)

echo "1. Suscribiendo a ixmati/evt/${IXMATI_STORE}/#..."
echo "2. Enviando comando..."

received=$(python3 -c "
import json, time, uuid, paho.mqtt.client as mqtt

evts = []
def on_msg(c, u, msg):
    evts.append(json.loads(msg.payload.decode()))

c = mqtt.Client(client_id=f's03-{uuid.uuid4().hex[:8]}')
c.on_message = on_msg
c.connect('${IXMATI_HOST}', ${IXMATI_MQTT_PORT})
c.subscribe('ixmati/evt/${IXMATI_STORE}/#', qos=1)
c.loop_start()

import urllib.request
payload = json.dumps({
    'op': 'upsert', 'store': '${IXMATI_STORE}', 'entity': 'outbox',
    'key': 'o1', 'version': 1, 'ts': '2026-08-01T00:00:00Z',
    'idempotency_key': '$IDEM', 'ack_mode': 'accepted',
    'payload': {'data': 'outbox-test'}
}).encode()
req = urllib.request.Request('http://${IXMATI_HOST}:${IXMATI_API_PORT}/write', data=payload,
    headers={'Content-Type': 'application/json', 'Authorization': 'Bearer ${IXMATI_API_KEY}'})
urllib.request.urlopen(req, timeout=5)

deadline = time.monotonic() + 15
while time.monotonic() < deadline and len(evts) < 1:
    time.sleep(0.1)
c.loop_stop()
c.disconnect()
print(len(evts))
for e in evts:
    print(json.dumps(e, indent=2))
" 2>/dev/null)

count=$(echo "$received" | head -1)
if [ "${count:-0}" -ge 1 ]; then
    green "Evento MQTT recibido en < 15s"
    echo "$received" | tail -n +2
    exit 0
else
    red "No se recibió evento MQTT"
    exit 1
fi
